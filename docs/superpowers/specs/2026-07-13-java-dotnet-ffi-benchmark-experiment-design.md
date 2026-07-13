# Java/.NET FFI Benchmark Experiment

**Date:** 2026-07-13
**Status:** Approved (spike)
**Branch:** `experiment/java-dotnet-bindings` — nothing merges to `main` unless the
benchmark report justifies it.

## Purpose

Answer one question with real numbers: **is jsonata-core, consumed over a C-ABI FFI
boundary, significantly faster than the native JSONata implementations Java and .NET
users already have** (dashjoin/jsonata-java and Jsonata.Net.Native)?

Java and .NET users pay a real adoption cost for a native-library binding (platform-
specific artifacts instead of portable bytecode/IL). They will only pay it for a large
performance win. This experiment produces the evidence; the user makes the merge call
from the report — no pre-committed threshold.

This is a scoped-down spike of Phases 4–6 of the 2026-07-09 multi-language design
(`2026-07-09-multi-language-and-agentic-study-design.md`). That spec's settled
decisions (hand-written C ABI, FFM not JNI, `[LibraryImport]` P/Invoke, competitor
choices) are inherited here, not re-litigated. What this spike defers: native-lib
bundling, public API polish, READMEs, CI wiring, registry packaging.

## Components

### 1. Minimal C ABI (`src/capi.rs`, new `capi` cargo feature)

The Phase-4 surface trimmed to what benchmarking needs:

```c
typedef struct JsonataExpr JsonataExpr;
JsonataExpr* jsonata_compile(const char* expr_utf8);      // NULL on parse error
char*        jsonata_evaluate(JsonataExpr*, const char* json_utf8); // NULL on error; "undefined" result → NULL with empty error
void         jsonata_free_expr(JsonataExpr*);
void         jsonata_free_string(char*);
char*        jsonata_last_error_message(void);            // thread-local slot
const char*  jsonata_version(void);
```

- JSON crosses the boundary as UTF-8 C strings in both directions. No structured
  value model in C.
- Thread-local last-error slot. One `JsonataExpr*` per thread (engine is `Rc`-based /
  non-`Send`; both benchmark harnesses are single-threaded).
- Written to the approved Phase-4 shape so it can be promoted to the real Phase 4
  later regardless of the Java/.NET outcome. `jsonata_bind_var`,
  `jsonata_last_error_code`/`_position`, the hand-maintained header file, the C smoke
  test, and consumption docs are deferred to full Phase 4.
- Built as the existing `cdylib` with `--features capi` (must coexist with the PyO3
  `extension-module` feature arrangement; the plan resolves the exact feature/crate-type
  mechanics).

### 2. Java spike (`benchmarks/java/`)

Maven JMH project. Contains a package-private FFM (`java.lang.foreign`) wrapper over
the C ABI, ~100 lines: loads the `.so` from a path given by system property or env var
(no resource bundling), `Arena`-managed strings, `AutoCloseable` handle.

Competitor: `com.dashjoin:jsonata` (latest release) from Maven Central, evaluating the
same corpus.

Toolchain: FFM is final in Java 22+; install a user-local Temurin JDK (current LTS) —
system JDK 21 is only a fallback via `--enable-preview`.

### 3. .NET spike (`benchmarks/dotnet/`)

BenchmarkDotNet project, net8.0, internal `[LibraryImport]` source-generated P/Invoke
wrapper. Competitor: `Jsonata.Net.Native` (latest) from NuGet — despite the name it is
a pure-C# port, the correct head-to-head. Toolchain: user-local .NET SDK via
`dotnet-install.sh`.

### 4. Methodology

- **Corpus:** the same expression/data scenarios used by the existing `benchmarks/`
  rust/python/javascript suites, so numbers are comparable project-wide.
- **Correctness gate before timing:** for every scenario, our-binding output and
  competitor output must be semantically equal (JSON-equal, ignoring key order and
  formatting). Mismatched scenarios are excluded from timing and flagged in the
  report. Benchmarking wrong answers is worthless.
- **Two measurement modes, reported separately:**
  1. *String→string* — JSON text in, JSON text out, for all parties (each pays its
     own parse/serialize). Symmetric; our best case.
  2. *Home-turf* — the competitor evaluates pre-parsed native objects (Jackson tree
     for jsonata-java, Jsonata.Net.Native's native tree for C#), the way real users
     hold data, while our binding still pays string serialization at the boundary.
     Our worst realistic case.
- **Two lifecycle variants:** compile-once-evaluate-many (steady-state, the realistic
  server pattern) and compile+evaluate per call.
- JMH defaults (forked, warmup + measurement iterations); BenchmarkDotNet defaults.
  Single-threaded.

### 5. Deliverable

A report (markdown, `benchmarks/results/`) with per-scenario tables for both modes
and both lifecycle variants, geomean speedups per language, best/worst cases, and a
boundary-cost analysis (how much of each result is FFI serialize/parse vs engine
time). The user decides from this report whether Java and/or .NET graduate to full
Phase 5/6 (independently — one can win and the other lose).

## Error handling

Spike-level: compile/evaluate failures in the wrapper throw (Java) / raise (.NET)
with the thread-local error message. The correctness gate treats an error where the
competitor succeeds as a mismatch (excluded + flagged). Engine `undefined` results
map to null/absent and the comparison treats competitor-idiomatic equivalents
(e.g. jsonata-java's `null` vs absent) leniently, with any such leniency noted in
the report.

## Testing

- Rust: unit test for `capi.rs` (compile/evaluate/free round-trip, error paths,
  UTF-8 with multibyte chars).
- Java/.NET: a smoke test per wrapper (a handful of expressions with known outputs)
  that must pass before any benchmark run.
- The correctness gate itself doubles as the cross-implementation test.
- Existing suites (1682 reference tests, cargo tests) must stay green — all changes
  are additive behind the `capi` feature.

## Definition of done

The report exists in `benchmarks/results/` on the experimental branch with numbers
for both languages, both modes, both lifecycle variants, plus the correctness-gate
outcome — presented to the user for the merge decision. Merging anything to `main`
is explicitly not part of this spike.
