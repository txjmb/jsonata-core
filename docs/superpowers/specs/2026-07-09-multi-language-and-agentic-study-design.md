# Multi-language surface, agentic CLI, and the JSONata-vs-jq agentic study

## Context

jsonatapy is currently a Python-only Rust extension (PyO3), 1258/1258 reference-suite
compatible, published as `jsonatapy` on PyPI and `jsonata-core` on crates.io. The next phase
has two goals that turned out, through discussion, to be sequenced rather than parallel:

1. **Long-term:** make the Rust JSONata core usable from Java, C#, and C, as a standalone
   CLI binary usable in place of `jq`, and as a Python `uvx`-runnable tool (both as a direct
   CLI and as a FastMCP server) for agentic workflows.
2. **Near-term and higher priority:** before investing in Java/.NET bindings, run a real
   empirical study — using the CLI as the instrument — testing whether coding agents
   actually complete JSON-wrangling tasks more successfully and with fewer tokens using
   JSONata than using jq, Python one-liners, or grep/sed/awk. If the result supports it, the
   study becomes a Medium article. If it doesn't, that's a legitimate finding too and it
   changes what's worth building next.

The study depends on the CLI existing and being jq-familiar enough that agents can use it
without extensive onboarding. So the CLI (and its Python `uvx` sibling) moves to the front of
the work, and Java/.NET move to the back, gated on the study not being a dead end.

## Scope

This spec covers six phases. Each phase gets its own implementation plan; this document
establishes the architecture and decisions that span all of them so later phases don't
re-litigate settled questions.

1. Rust CLI binary (`jsonata`), jq-familiar interface
2. Python CLI + FastMCP server (`jsonatapy` entry point, `jsonatapy mcp` subcommand)
3. The agentic study (task suite, multi-model trials, analysis, article)
4. C ABI (`jsonata.h`) — the C deliverable and the foundation for 5 and 6
5. Java binding (FFM, Java 22+)
6. .NET binding (P/Invoke)

**Non-goals for this spec:**
- Actually writing the Medium article's prose — the study's analysis output feeds it, but
  article-writing is a separate downstream step not planned here.
- Maven Central / NuGet.org publishing — phases 5–6 ship as GitHub Release artifacts with
  build instructions; registry publishing is a later, separate decision.
- Supporting Java <22 (JNI) or .NET Framework — explicitly out of scope, see Decisions.
- Changing jsonatapy's existing Python API surface (`jsonatapy.transform()` etc.) — the CLI
  and MCP server are new, additive entry points.

## Decisions

These were settled during brainstorming and apply across all phases:

- **Repo structure:** everything stays in this repo (approach A: monorepo, feature-gated).
  One version number, one CI, one release workflow. New top-level dirs: `bindings/c/`,
  `bindings/java/`, `bindings/dotnet/`, `study/`. `benchmarks/` gains `java/`, `dotnet/`,
  `jq/` subdirs in their respective phases.
- **CLI interface:** jq-familiar, not jq-syntax — JSONata expressions, jq's ergonomics
  (stdin/file input, `-c`/`-r`/`-n`, `--arg`/`--argjson`, `-f`). One shared flag spec used by
  both the Rust binary and the Python entry point, tested against the same case list.
- **FFI approach:** hand-written C ABI (~8 functions), not UniFFI or a CLI shim. Small
  surface, no third-party codegen dependency, and it doubles as the C deliverable itself.
- **Java target:** Java 22+ via `java.lang.foreign` (FFM/Panama), not JNI or JNA. Pure-Java
  binding, no compiled glue per platform.
- **MCP packaging:** `fastmcp` is an optional extra (`jsonatapy[mcp]`), not a hard dependency
  — jsonatapy stays zero-runtime-dependency for embedders; the CLI prints the
  `uvx --from "jsonatapy[mcp]" jsonatapy mcp` hint when fastmcp is missing.
- **Study rigor:** real agent trials (not static analysis), forced-tool conditions (not
  free-choice-only), multi-model, with a documentation cheatsheet as an explicit
  experimental variable rather than always-on or never-on.

## Design

### Phase 1: Rust CLI (`jsonata` binary)

New `[[bin]]` target in the existing `jsonata-core` crate, gated behind a `cli` feature so
the `clap` dependency doesn't leak into library/PyO3 builds:

```toml
[[bin]]
name = "jsonata"
path = "src/bin/jsonata.rs"
required-features = ["cli"]

[features]
cli = ["dep:clap"]
```

**Flags** (jq-familiar):
```
jsonata [OPTIONS] <expression> [file]
  -c, --compact         compact JSON output (default: pretty-printed)
  -r, --raw-output       print strings without quotes
  -n, --null-input       don't read input; $ is undefined
  -f, --from-file <f>    read expression from file instead of argv
      --arg <name=val>   bind $name to a string value
      --argjson <n=val>  bind $name to a parsed JSON value
  -V, --version
  -h, --help
```
Input: file argument if given, else stdin. Multiple JSON values on stdin are not supported
in v1 (jq's `--slurp`/streaming semantics don't map cleanly onto JSONata's single-document
model — explicitly deferred, not silently mishandled).

**Semantics:**
- JSONata "undefined" result → print nothing, exit 0 (mirrors jq's empty-output behavior;
  distinct from an explicit `null` result, which prints `null`).
- Evaluation errors → stderr, formatted as `<code> at position <pos>: <message>` (reusing the
  existing error type's fields), exit 1.
- Expression parse errors → same stderr format, exit 1 (parse errors are a subset of
  evaluation errors from the CLI's perspective — both come from `JsonataExpression::new`
  failing before or during evaluation).
- Invalid input JSON → stderr `error: invalid JSON input: <serde message>`, exit 3.
- Usage errors (bad flags, missing expression) → clap's standard stderr + exit 2.

These three exit codes are chosen deliberately for the study: an agent (or the study's
grading script) can distinguish "my expression was wrong" (1) from "my input was malformed"
(3) from "I misused the tool" (2) without parsing stderr text.

**Distribution:** existing `release.yml` gains a step building `--features cli` release
binaries for Linux/macOS/Windows × x86_64/aarch64, attached to the GitHub Release. Also
installable via `cargo install jsonata-core --features cli`.

**Testing:** `tests/cli/` using `assert_cmd`, covering the flag matrix, exit codes, and
undefined-vs-null-vs-error output distinctions above.

### Phase 2: Python CLI + FastMCP server

`pyproject.toml` gains:
```toml
[project.scripts]
jsonatapy = "jsonatapy.__main__:main"

[project.optional-dependencies]
mcp = ["fastmcp>=2.0"]
```

`python/jsonatapy/__main__.py` implements the **identical flag surface** to the Rust binary
(same flag spec doc, same exit codes), calling the existing `jsonatapy` Python API
internally rather than shelling out to the Rust binary. A shared `study/cli_spec.md` (or
similar) documents the flags once; both implementations' tests reference the same fixture
cases so they can't silently drift.

Dispatch: `jsonatapy <expr> [file]` behaves as the CLI above; `jsonatapy mcp [--http]
[--port N]` launches the MCP server (stdio by default). If `fastmcp` isn't importable,
`jsonatapy mcp` prints the install hint and exits 2 rather than a raw `ImportError`.

**MCP tools** (`jsonatapy mcp`, richer toolset as decided):
- `evaluate(expression: str, data: str, bindings: dict | None) -> str` — evaluate against a
  JSON document, optional variable bindings, returns the JSON-encoded result (or an MCP
  tool-error with the error code/position on failure).
- `validate(expression: str) -> {ok: bool, error?: str, position?: int}` — parse-only check.
- `explain(topic: str | None) -> str` — returns curated JSONata reference material (function
  index if no topic, specific section if given). This content is authored once and reused
  as the study's Phase 3 cheatsheet, so it's written to be concise (token cost matters for
  both uses).
- `evaluate_batch(expressions: list[str], data: str) -> list[result | error]` — run several
  expressions against one document in one call, avoiding N round-trips.

**Testing:** FastMCP's in-memory client for the four tools; CLI tests mirror Phase 1's
`assert_cmd` suite via subprocess or in-process argv invocation.

### Phase 3: The agentic study

Lives in `study/` in this repo — task fixtures, harness, results, analysis script, kept
separate from the library's own test suite.

**Task suite (~20 tasks):** hand-authored, spanning extraction/filtering, reshaping,
aggregation/grouping, multi-file joins, deep nested access, and messy-data edge cases
(missing fields, nulls, heterogeneous array shapes). Each task is a directory:
```
study/tasks/<NN>-<slug>/
  input/*.json          # one or more input files
  prompt.md             # natural-language instruction given to the agent
  check.py              # returns pass/fail given the agent's stdout/output file
```
`check.py` compares canonical JSON (order-insensitive where the task allows it) rather than
exact string match, since agents may format output differently. Task difficulty is
calibrated during the Phase 3 pilot (below) so at least some tasks show <100% success in at
least one condition — a suite where every condition scores 100% has no signal.

**Conditions (6):**
1. `jsonata-bare` — only the `jsonata` CLI available, no reference docs in the prompt.
2. `jsonata+docs` — `jsonata` CLI + the `explain` cheatsheet content injected into the
   system/task prompt (its token count is charged to this condition's total).
3. `jq` — only `jq` available.
4. `python` — only a Python interpreter (one-off scripts/one-liners) available.
5. `grep-sed-awk` — only those three tools available.
6. `free-choice` — all of the above available; measures organic tool adoption, not a
   controlled comparison arm.

Each condition is enforced by restricting the agent's available tools/binaries in its
sandbox (not just prompt instruction) so results reflect actual capability, not compliance.

**Harness:** `study/harness/` with a pluggable agent-runner interface (`run_trial(task,
condition, model) -> TrialResult`), so a second backend (e.g. a different CLI or the raw API)
can be added later without redesigning the analysis pipeline. First implementation drives
`claude -p` non-interactively: JSON output mode for token/turn counts, `--model` to select
Sonnet vs Haiku, an isolated temp working directory per run, restricted bash permissions
matching the condition's allowed tools.

**Metrics per run:** pass/fail (from `check.py`), total input+output tokens, number of
turns, number of tool calls, wall-clock seconds, and a failure-mode tag (wrong-answer /
gave-up / error-loop / timeout) assigned by inspecting the transcript.

**Scale:** 20 tasks × 6 conditions × 3 trials × 2 models ≈ 720 runs. Results append to
`study/results/runs.jsonl`; the harness is resumable — re-running skips `(task, condition,
trial, model)` tuples already present in the file, so a partial or interrupted run is cheap
to continue. Before committing to the full matrix, a **calibration pilot** (5 tasks × all 6
conditions × 1 trial × 1 model, ~30 runs) validates that the harness works end-to-end and
that task difficulty is calibrated (not universally trivial or universally impossible) —
this is a checkpoint inside this phase, not a separate phase.

**Analysis:** `study/analysis/report.py` reads `runs.jsonl` and produces: success rate per
condition (overall and per-task), token cost conditional on success (median/p90), tool
adoption distribution in `free-choice`, and failure-mode breakdown per condition. Output is
tables/CSVs plus matplotlib figures suitable for dropping into the article.

### Phase 4: C ABI

`capi` feature on the existing `cdylib`, adding `src/capi.rs`:

```c
// bindings/c/jsonata.h
typedef struct JsonataExpr JsonataExpr;

JsonataExpr* jsonata_compile(const char* expr_utf8);
char* jsonata_evaluate(JsonataExpr* expr, const char* json_utf8);
int   jsonata_bind_var(JsonataExpr* expr, const char* name, const char* json_value_utf8);
void  jsonata_free_expr(JsonataExpr* expr);
void  jsonata_free_string(char* s);
int   jsonata_last_error_code(JsonataExpr* expr);
char* jsonata_last_error_message(JsonataExpr* expr);
int   jsonata_last_error_position(JsonataExpr* expr);
const char* jsonata_version(void);
```

JSON strings cross the boundary as UTF-8 C strings (no C-visible structured value model) —
simplest possible surface, and symmetric parsing cost keeps the Java/.NET benchmarks
comparable to the Rust-internal path. `jsonata_compile` returns `NULL` on parse failure;
callers check `jsonata_last_error_*` via a small thread-local error slot (documented as
**not thread-safe across handles from different threads** — one `JsonataExpr*` per thread,
matching the core's existing `Rc`-based non-`Send` design noted in project memory).

`bindings/c/jsonata.h` is hand-maintained (not bindgen-generated, to keep it readable) and
validated by a CI-compiled C smoke test (`bindings/c/examples/smoke.c`) that compiles and
links against the built `cdylib` on every PR touching `src/capi.rs` or the header.

### Phase 5: Java binding

`bindings/java/` — a Maven project, pure-Java `java.lang.foreign` binding over the C ABI
(no JNI, no compiled glue). Native libraries bundled under `src/main/resources/native/
{os}-{arch}/` and extracted to a temp file at class-load time, `System.load()`'d once. A
`Jsonata` class wraps compile/evaluate/close with `AutoCloseable` for the expression handle.

Benchmark: JMH suite in `benchmarks/java/` running the same expression/data corpus used
elsewhere in this repo's benchmarks, compared against dashjoin/jsonata-java.

Deliverable: build instructions in `bindings/java/README.md` + a prebuilt jar (with bundled
natives for the CI-built platforms) attached to GitHub Releases. Maven Central publishing is
explicitly deferred (non-goal).

### Phase 6: .NET binding

`bindings/dotnet/` — a net8.0 class library using `[LibraryImport]` source-generated
P/Invoke over the C ABI. Native libraries laid out under `runtimes/{rid}/native/` (standard
NuGet convention, so packaging later is a non-event even though publishing itself is
deferred). A `JsonataExpression` class implements `IDisposable`.

Benchmark: BenchmarkDotNet suite in `benchmarks/dotnet/`, compared against
Jsonata.Net.Native.

Deliverable: same shape as Java — build instructions + prebuilt package artifact on GitHub
Releases, registry publishing deferred.

## Testing / verification strategy (all phases)

- Phases 1–2 share one flag-behavior fixture set (`study/cli_spec.md` + a JSON case list)
  consumed by both `assert_cmd` (Rust) and pytest (Python), so the two CLIs can't silently
  diverge in flag semantics.
- Phase 3's harness is validated by the calibration pilot before the full matrix runs —
  cheap to catch a broken checker or miscalibrated task before spending the full budget.
- Phase 4's C smoke test and Phases 5–6's benchmark suites are CI-gated to only run when
  their respective `bindings/*` or `src/capi.rs` paths change, keeping normal PR CI fast.
- Existing 1258-case reference suite and Rust unit tests are unaffected by any of this —
  all new code is additive (new bin target, new optional feature, new bindings dirs).

## Definition of done

- Phase 1: `jsonata` binary builds via `cargo build --release --features cli`, is attached
  to GitHub Releases for all target platforms, and passes the shared CLI fixture suite.
- Phase 2: `uvx jsonatapy '<expr>' file.json` works with no prior install; `uvx --from
  "jsonatapy[mcp]" jsonatapy mcp` serves the four MCP tools, verified against a FastMCP
  client.
- Phase 3: the full 720-run matrix is complete, `study/analysis/report.py` produces the
  success-rate/token/failure-mode breakdown, and a written finding (positive or negative)
  is documented in `study/RESULTS.md` — this is the actual go/no-go gate for Phases 5–6.
- Phase 4: `jsonata.h` + smoke test compile and pass in CI on every relevant PR.
- Phase 5: Java benchmark numbers exist against jsonata-java; prebuilt jar attached to a
  release.
- Phase 6: .NET benchmark numbers exist against Jsonata.Net.Native; prebuilt package
  attached to a release.
