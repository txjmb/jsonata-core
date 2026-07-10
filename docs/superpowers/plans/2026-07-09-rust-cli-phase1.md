# Rust CLI Binary (`jsonata`) — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a `jsonata` command-line binary — a jq-familiar way to evaluate JSONata expressions against JSON from stdin or a file — built from the existing `jsonata-core` crate, distributed via GitHub Releases and `cargo install`, with an exit-code contract precise enough for scripts and agents to act on programmatically.

**Architecture:** A new `[[bin]]` target (`src/bin/jsonata/main.rs` + two small sibling modules) gated behind a `cli` Cargo feature so the `clap` dependency never reaches library or PyO3 builds. The binary calls the crate's existing public Rust API directly (`parser::parse`, `evaluator::{Context, Evaluator}`, `value::JValue`) — no new evaluation logic, just argument parsing, I/O, output formatting, and error-message translation. A shared JSON fixture file (`study/cli_fixtures.json`) captures the full flag/exit-code contract so Phase 2's Python CLI can be tested against the exact same cases.

**Tech Stack:** Rust (clap 4 with derive macros for argument parsing), `assert_cmd` + `predicates` for black-box CLI integration tests (dev-dependencies), existing `jsonata-core` public API.

## Global Constraints

- `clap` is an **optional** dependency, enabled only by the new `cli` feature — library and `python`-feature builds must not gain it. Verify with `cargo build --release` (no features) after Task 1 still succeeds without pulling in clap as a direct build requirement.
- Flag surface (final, must match exactly — Phase 2's Python CLI mirrors this 1:1):
  `jsonata [OPTIONS] [EXPRESSION] [FILE]`
  `-c, --compact` · `-r, --raw-output` · `-n, --null-input` · `-f, --from-file <FILE>` (expression comes from this file; the remaining single positional argument, if any, becomes the input data file instead of the expression) · `--arg NAME=VALUE` (repeatable) · `--argjson NAME=JSON` (repeatable) · `-V, --version` · `-h, --help`.
- Input source: the (possibly re-slotted, see `-f` above) file positional argument if present, else stdin. `-n`/`--null-input` skips reading input entirely (`$` is `Undefined`) and is incompatible with also supplying a data-file positional argument.
- No slurp / multi-JSON-document streaming in this phase — out of scope, not silently mishandled (a fixture case asserts multi-document stdin currently just parses the first JSON value via `serde_json`, i.e. whatever `JValue::from_json_str` does today with trailing content — verify and document the actual behavior in Task 2, don't guess).
- Exit codes (verified against actual error types in `src/evaluator.rs`/`src/parser.rs` during planning — see below):
  - `0` — success, including a JSONata `Undefined` result (prints nothing) and clap's own `--version`/`--help` handling.
  - `1` — expression parse error or evaluation error.
  - `2` — usage/invocation error: malformed CLI flags (clap's own default behavior), malformed `--arg`/`--argjson` (`NAME=VALUE` syntax violated), incompatible flag combination (`-n` + data-file argument, `-f` + two positional arguments), or an expression/input file that cannot be read (I/O error, e.g. not found).
  - `3` — the input was read successfully but is not valid JSON.
- **Error message convention — do not use `EvaluatorError`'s `Display`/`.to_string()` directly, and do not duplicate the unwrap logic.** `src/lib.rs`'s existing `evaluator_error_to_py` unwraps each variant's inner `String` payload directly (`TypeError(msg) => msg`, etc.) because that inner string already carries the JSONata spec code prefix (e.g. `"T2002: ..."`) that `tests/python/test_reference_suite.py::extract_error_code` depends on — but the enum's own `thiserror`-derived `Display` wraps it in an outer `"Type error: "`/`"Reference error: "`/`"Evaluation error: "` prefix that would bury the code. Task 6 extracts this unwrap into a `pub fn message(&self) -> &str` method directly on `EvaluatorError` (in `src/evaluator.rs`, not gated behind the `python` feature) so both `src/lib.rs`'s Python conversion and the CLI call the same code — do not write a second, CLI-local copy of the match arms. `ParserError` is the opposite case: its `Display` (via the existing logic that Task 6 also relocates, as `ParserError::display_message()` in `src/parser.rs`) already puts the code first for `ParserError::Coded`, so `.to_string()`-based formatting is correct there, with a `"Parse error: "` prefix added only for non-coded variants — again, one shared method, not a duplicate.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` must stay clean after every task (matches `.github/workflows/lint.yml`).
- Distribution matrix must match the **existing** `build-wheels` job matrix in `.github/workflows/release.yml` exactly — do not invent additional platforms: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` (both `ubuntu-latest`), `x86_64-pc-windows-msvc` (`windows-latest`), `aarch64-apple-darwin` (self-hosted macOS runner, no Windows-aarch64 or macOS-x86_64 legs exist today).
- Design source of truth: `docs/superpowers/specs/2026-07-09-multi-language-and-agentic-study-design.md` (Phase 1 section + the cross-phase Decisions section).
- Work directly on `main` unless told otherwise — no feature branch has been created for this plan (check `git status`/`git branch` before starting if that assumption seems stale).

---

### Task 1: Cargo scaffolding + `--version`/`--help`

**Files:**
- Modify: `Cargo.toml`
- Create: `src/bin/jsonata/main.rs`
- Create: `tests/cli_test.rs`

**Interfaces:**
- Produces: a `jsonata` binary target buildable via `cargo build --release --features cli`; nothing else in the crate depends on this yet.
- Consumes: nothing new from elsewhere in the crate at this step.

- [ ] **Step 1: Add the `cli` feature, `clap` dependency, `[[bin]]` target, and test dependencies to `Cargo.toml`**

In `Cargo.toml`, add `clap` to `[dependencies]` (it already appears transitively at version `4.6.1` per `Cargo.lock`, pulled in by `criterion`; pin compatibly):

```toml
[dependencies]
pyo3 = { version = "0.29", optional = true }
serde = { version = "1.0", features = ["derive"] }
serde_json = { version = "1.0", features = ["preserve_order"] }
thiserror = "2.0"
regex = "1.12"
chrono = "0.4"
num-traits = "0.2"
base64 = "0.22"
indexmap = "2.2"
percent-encoding = "2.3"
rand = "0.10"
stacker = "0.1"
simd-json = { version = "0.17", optional = true }
clap = { version = "4.6", features = ["derive"], optional = true }
```

Add the new feature (keep existing features unchanged):

```toml
[features]
default = ["simd"]
simd = ["dep:simd-json"]
python = ["dep:pyo3"]
cli = ["dep:clap"]
bench = []   # exposes _bench facade for Criterion benchmarks
```

Add the binary target (after `[lib]`):

```toml
[[bin]]
name = "jsonata"
path = "src/bin/jsonata/main.rs"
required-features = ["cli"]
```

Add test-only dependencies:

```toml
[dev-dependencies]
criterion = "0.8"
proptest = "1.10"
assert_cmd = "2.0"
predicates = "3.1"
```

- [ ] **Step 2: Create a minimal binary that only handles `--version`/`--help`**

Create `src/bin/jsonata/main.rs`:

```rust
use clap::Parser;

/// Evaluate JSONata expressions against JSON data.
#[derive(Parser, Debug)]
#[command(name = "jsonata", version, about = "Evaluate JSONata expressions against JSON data")]
struct Cli {
    /// Compact JSON output (default: pretty-printed)
    #[arg(short = 'c', long)]
    compact: bool,

    /// Print string results without surrounding quotes
    #[arg(short = 'r', long = "raw-output")]
    raw_output: bool,

    /// Don't read input; $ is undefined
    #[arg(short = 'n', long = "null-input")]
    null_input: bool,

    /// Read the expression from a file instead of the first positional argument
    #[arg(short = 'f', long = "from-file", value_name = "FILE")]
    from_file: Option<String>,

    /// Bind $NAME to a string value: --arg NAME=VALUE
    #[arg(long = "arg", value_name = "NAME=VALUE", action = clap::ArgAction::Append)]
    arg: Vec<String>,

    /// Bind $NAME to a parsed JSON value: --argjson NAME=JSON
    #[arg(long = "argjson", value_name = "NAME=JSON", action = clap::ArgAction::Append)]
    argjson: Vec<String>,

    /// The JSONata expression (or, with --from-file, the input data file)
    #[arg(value_name = "EXPRESSION_OR_FILE")]
    positional1: Option<String>,

    /// The input data file (used only when --from-file supplies the expression)
    #[arg(value_name = "FILE")]
    positional2: Option<String>,
}

fn main() {
    let _cli = Cli::parse();
}
```

- [ ] **Step 3: Write the failing smoke test**

Create `tests/cli_test.rs`:

```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn version_flag_prints_version_and_exits_zero() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("jsonata"));
}

#[test]
fn help_flag_lists_known_options() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--compact"))
        .stdout(contains("--raw-output"))
        .stdout(contains("--null-input"))
        .stdout(contains("--from-file"));
}
```

- [ ] **Step 4: Run the tests to verify they fail (or rather, that the binary doesn't exist yet)**

Run: `cargo test --release --features cli --test cli_test`
Expected: builds the crate and `jsonata` binary, then both tests should actually PASS already at this point since Step 2's stub is enough for `--version`/`--help` (clap generates both automatically). If they fail, read the failure output — most likely cause is a typo in the `Cli` struct's `#[arg]` attributes; fix and re-run before proceeding.

- [ ] **Step 5: Confirm the `cli` feature doesn't leak into default/library builds**

Run: `cargo build --release`
Expected: succeeds, and `clap` does not appear as a compiled dependency (check with `cargo tree --release -e normal | grep -i clap` — expect no output, since `clap` is `optional = true` and `cli` is not in `default`).

Run: `cargo build --release --features cli`
Expected: succeeds, and `target/release/jsonata` (or `.exe` on Windows) exists.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock src/bin/jsonata/main.rs tests/cli_test.rs
git commit -m "feat(cli): scaffold jsonata binary with --version/--help"
```

---

### Task 2: Core evaluation — expression argument, stdin/file input, default pretty output

**Files:**
- Modify: `src/bin/jsonata/main.rs`
- Modify: `tests/cli_test.rs`

**Interfaces:**
- Consumes: `jsonata_core::parser::parse`, `jsonata_core::evaluator::{Context, Evaluator}`, `jsonata_core::value::JValue::{from_json_str, to_json_string_pretty, is_undefined}` — all existing public crate API, verified present during planning (`src/parser.rs:1829`, `src/evaluator.rs:2911`/`2925`/`3214`, `src/value.rs:52`/`564`/`569`/`587`).
- Produces: `run(cli: Cli) -> std::process::ExitCode`, the function all later tasks extend. Errors from this task are placeholder-formatted (`"error: {e}"` via `.to_string()`) — Task 6 replaces this with the correct code-preserving formatter; don't worry about the exact wording yet, only the exit code.

- [ ] **Step 1: Write the failing tests**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn evaluates_expression_against_stdin_json() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("name")
        .write_stdin(r#"{"name": "Alice"}"#)
        .assert()
        .success()
        .stdout("\"Alice\"\n");
}

#[test]
fn evaluates_expression_against_file_argument() {
    let dir = std::env::temp_dir();
    let path = dir.join("jsonata_cli_test_input.json");
    std::fs::write(&path, r#"{"name": "Bob"}"#).unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("name")
        .arg(path.to_str().unwrap())
        .assert()
        .success()
        .stdout("\"Bob\"\n");

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn pretty_prints_object_results_by_default() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("{\"x\": a}")
        .write_stdin(r#"{"a": 1}"#)
        .assert()
        .success()
        .stdout(contains("{\n"));
}

#[test]
fn undefined_result_prints_nothing_and_exits_zero() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("nonexistent_field")
        .write_stdin(r#"{"a": 1}"#)
        .assert()
        .success()
        .stdout("");
}

#[test]
fn null_result_prints_literal_null() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("nullField")
        .write_stdin(r#"{"nullField": null}"#)
        .assert()
        .success()
        .stdout("null\n");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --release --features cli --test cli_test`
Expected: FAIL — `main()` currently ignores `_cli` and does nothing, so stdout is empty for all of these (the `undefined_result_prints_nothing_and_exits_zero` test will spuriously pass; the rest fail).

- [ ] **Step 3: Implement expression evaluation, stdin/file input resolution, and default output**

Replace `src/bin/jsonata/main.rs`'s `fn main()` with:

```rust
use jsonata_core::evaluator::{Context, Evaluator};
use jsonata_core::parser;
use jsonata_core::value::JValue;
use std::io::Read;
use std::process::ExitCode;

fn main() -> ExitCode {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> ExitCode {
    let expression = match &cli.positional1 {
        Some(expr) => expr.clone(),
        None => {
            eprintln!("error: missing required argument: EXPRESSION");
            return ExitCode::from(2);
        }
    };

    let data_source = cli.positional2.clone();

    let data = match read_input(data_source.as_deref()) {
        Ok(data) => data,
        Err((msg, code)) => {
            eprintln!("{}", msg);
            return ExitCode::from(code);
        }
    };

    let ast = match parser::parse(&expression) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    let mut evaluator = Evaluator::with_context(Context::new());
    let result = match evaluator.evaluate(&ast, &data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    if result.is_undefined() {
        return ExitCode::SUCCESS;
    }

    match result.to_json_string_pretty() {
        Ok(s) => println!("{}", s),
        Err(e) => {
            eprintln!("error: could not serialize result: {}", e);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Reads and JSON-parses the input document from a file path, or stdin if
/// `path` is `None`. Returns `(stderr message, exit code)` on failure.
fn read_input(path: Option<&str>) -> Result<JValue, (String, u8)> {
    let raw = match path {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| (format!("error: could not read input file {}: {}", p, e), 2))?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| (format!("error: could not read stdin: {}", e), 2))?;
            buf
        }
    };
    JValue::from_json_str(&raw)
        .map_err(|e| (format!("error: invalid JSON input: {}", e), 3))
}
```

Keep the existing `Cli` struct and its `use clap::Parser;` import at the top of the file, above these additions.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --release --features cli --test cli_test`
Expected: PASS (all 5 new tests plus Task 1's 2 tests).

- [ ] **Step 5: Determine and document actual multi-JSON-document stdin behavior**

Run manually: `printf '{"a":1}\n{"a":2}\n' | cargo run --release --features cli --bin jsonata -- a`
Observe the actual output/error (this exercises `serde_json`'s underlying parse-first-value-then-error-on-trailing-data behavior via `JValue::from_json_str`). Add a code comment directly above the `read_input` function in `src/bin/jsonata/main.rs` recording exactly what was observed, e.g.:

```rust
// NOTE: multi-document stdin (e.g. `{"a":1}\n{"a":2}\n`) is NOT supported —
// JValue::from_json_str rejects trailing non-whitespace content after the
// first JSON value with a serde_json "trailing characters" error, surfaced
// here as exit code 3. jq's --slurp/streaming semantics are explicitly out
// of scope for this CLI (see Phase 1 of the design spec).
```//
Add one more test to `tests/cli_test.rs` asserting this exact behavior so it's regression-covered, not just documented:

```rust
#[test]
fn multi_document_stdin_is_rejected_not_silently_truncated() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a")
        .write_stdin("{\"a\":1}\n{\"a\":2}\n")
        .assert()
        .code(3)
        .stderr(contains("invalid JSON input"));
}
```

Run: `cargo test --release --features cli --test cli_test`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/bin/jsonata/main.rs tests/cli_test.rs
git commit -m "feat(cli): evaluate expressions against stdin/file JSON input"
```

---

### Task 3: `-c/--compact` and `-r/--raw-output`

**Files:**
- Modify: `src/bin/jsonata/main.rs`
- Modify: `tests/cli_test.rs`

**Interfaces:**
- Consumes: `cli.compact: bool`, `cli.raw_output: bool` (already parsed by clap since Task 1), `JValue::to_json_string()` (`src/value.rs:564`).
- Produces: `print_result(result: &JValue, compact: bool, raw_output: bool)`, extracted out of `run()`'s inline printing logic so later tasks (and tests) can reason about output formatting independently of input handling.

- [ ] **Step 1: Write the failing tests**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn compact_flag_produces_single_line_output() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-c")
        .arg("{\"x\": a}")
        .write_stdin(r#"{"a": 1}"#)
        .assert()
        .success()
        .stdout("{\"x\":1}\n");
}

#[test]
fn raw_output_flag_strips_quotes_from_string_results() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-r")
        .arg("name")
        .write_stdin(r#"{"name": "Alice"}"#)
        .assert()
        .success()
        .stdout("Alice\n");
}

#[test]
fn raw_output_flag_does_not_affect_non_string_results() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-r")
        .arg("-c")
        .arg("age")
        .write_stdin(r#"{"age": 30}"#)
        .assert()
        .success()
        .stdout("30\n");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --release --features cli --test cli_test`
Expected: FAIL on all three — output is currently always pretty-printed JSON, `-c`/`-r` are parsed but unused.

- [ ] **Step 3: Extract `print_result` and wire up `-c`/`-r`**

In `src/bin/jsonata/main.rs`, replace the result-printing block at the end of `run()` (from `if result.is_undefined() { ... }` through the final `println!`/error arm) with:

```rust
    if result.is_undefined() {
        return ExitCode::SUCCESS;
    }

    print_result(&result, cli.compact, cli.raw_output)
```

Add a new function below `run`:

```rust
fn print_result(result: &JValue, compact: bool, raw_output: bool) -> ExitCode {
    if raw_output {
        if let JValue::String(s) = result {
            println!("{}", s);
            return ExitCode::SUCCESS;
        }
    }

    let text = if compact {
        result.to_json_string()
    } else {
        result.to_json_string_pretty()
    };

    match text {
        Ok(s) => {
            println!("{}", s);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not serialize result: {}", e);
            ExitCode::from(1)
        }
    }
}
```

Remove the now-unused inline `match result.to_json_string_pretty() { ... }` block and the trailing `ExitCode::SUCCESS` that used to follow it in `run()` — `print_result` returns the `ExitCode` directly now, so `run()`'s tail becomes `print_result(&result, cli.compact, cli.raw_output)` with no following statement.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --release --features cli --test cli_test`
Expected: PASS (all tests, including Task 2's).

- [ ] **Step 5: Commit**

```bash
git add src/bin/jsonata/main.rs tests/cli_test.rs
git commit -m "feat(cli): add -c/--compact and -r/--raw-output flags"
```

---

### Task 4: `-n/--null-input` and the shifted-positional resolver (for `-f` in Task 5)

**Files:**
- Create: `src/bin/jsonata/resolve.rs`
- Modify: `src/bin/jsonata/main.rs`
- Modify: `tests/cli_test.rs`

**Interfaces:**
- Produces: `resolve::{ExpressionSource, InputSource, resolve}` — `resolve(cli: &Cli) -> Result<(ExpressionSource, InputSource), String>` is a pure function (no I/O), unit-tested directly in `resolve.rs` and consumed by `run()` in `main.rs`. This is the single place all later tasks (5, 6, 7) look at to understand how positional arguments and `-n`/`-f` interact — do not duplicate this logic elsewhere.
- Consumes: `Cli`'s fields (`from_file`, `positional1`, `positional2`, `null_input`) — all already present since Task 1.

This task also finally makes `-f`'s positional re-slotting well-defined (Task 5 wires the actual file-reading), because `-n` and `-f` both change what a bare positional argument means and need one consistent resolver rather than two independent patches.

- [ ] **Step 1: Write the failing unit tests for the resolver**

Create `src/bin/jsonata/resolve.rs`:

```rust
/// Where the JSONata expression text comes from.
#[derive(Debug, PartialEq)]
pub enum ExpressionSource {
    Inline(String),
    File(String),
}

/// Where the input JSON document comes from.
#[derive(Debug, PartialEq)]
pub enum InputSource {
    Stdin,
    File(String),
    Null,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cli;

    fn cli(
        from_file: Option<&str>,
        positional1: Option<&str>,
        positional2: Option<&str>,
        null_input: bool,
    ) -> Cli {
        Cli {
            compact: false,
            raw_output: false,
            null_input,
            from_file: from_file.map(String::from),
            arg: Vec::new(),
            argjson: Vec::new(),
            positional1: positional1.map(String::from),
            positional2: positional2.map(String::from),
        }
    }

    #[test]
    fn plain_expression_and_stdin() {
        let c = cli(None, Some("name"), None, false);
        assert_eq!(
            resolve(&c),
            Ok((ExpressionSource::Inline("name".into()), InputSource::Stdin))
        );
    }

    #[test]
    fn plain_expression_and_file() {
        let c = cli(None, Some("name"), Some("data.json"), false);
        assert_eq!(
            resolve(&c),
            Ok((
                ExpressionSource::Inline("name".into()),
                InputSource::File("data.json".into())
            ))
        );
    }

    #[test]
    fn missing_expression_is_an_error() {
        let c = cli(None, None, None, false);
        assert!(resolve(&c).is_err());
    }

    #[test]
    fn null_input_with_no_data_file_is_null_source() {
        let c = cli(None, Some("$now()"), None, true);
        assert_eq!(
            resolve(&c),
            Ok((ExpressionSource::Inline("$now()".into()), InputSource::Null))
        );
    }

    #[test]
    fn null_input_with_data_file_is_an_error() {
        let c = cli(None, Some("name"), Some("data.json"), true);
        assert!(resolve(&c).is_err());
    }

    #[test]
    fn from_file_shifts_positional1_to_the_data_file() {
        let c = cli(Some("expr.jsonata"), Some("data.json"), None, false);
        assert_eq!(
            resolve(&c),
            Ok((
                ExpressionSource::File("expr.jsonata".into()),
                InputSource::File("data.json".into())
            ))
        );
    }

    #[test]
    fn from_file_with_no_positionals_reads_stdin() {
        let c = cli(Some("expr.jsonata"), None, None, false);
        assert_eq!(
            resolve(&c),
            Ok((ExpressionSource::File("expr.jsonata".into()), InputSource::Stdin))
        );
    }

    #[test]
    fn from_file_with_two_positionals_is_an_error() {
        let c = cli(Some("expr.jsonata"), Some("extra1"), Some("extra2"), false);
        assert!(resolve(&c).is_err());
    }
}

/// Resolves the expression source and input source from parsed CLI
/// arguments. `--from-file` shifts what a bare positional argument means
/// (it becomes the input data file, not the expression); `--null-input`
/// suppresses reading input entirely and conflicts with supplying a data
/// file. See Task 4 of the Phase 1 plan for the full truth table this
/// implements.
pub fn resolve(cli: &crate::Cli) -> Result<(ExpressionSource, InputSource), String> {
    let (expr_source, data_file) = match (&cli.from_file, &cli.positional1, &cli.positional2) {
        (Some(expr_file), data_file, None) => {
            (ExpressionSource::File(expr_file.clone()), data_file.clone())
        }
        (Some(_), _, Some(_)) => {
            return Err(
                "with --from-file, only one positional argument (the input file) is allowed"
                    .to_string(),
            );
        }
        (None, Some(expr), data_file) => {
            (ExpressionSource::Inline(expr.clone()), data_file.clone())
        }
        (None, None, _) => {
            return Err("missing required argument: EXPRESSION (or use --from-file)".to_string());
        }
    };

    if cli.null_input {
        if data_file.is_some() {
            return Err(
                "--null-input cannot be combined with an input file argument".to_string(),
            );
        }
        Ok((expr_source, InputSource::Null))
    } else {
        match data_file {
            Some(f) => Ok((expr_source, InputSource::File(f))),
            None => Ok((expr_source, InputSource::Stdin)),
        }
    }
}
```

- [ ] **Step 2: Wire the new module into `main.rs` and run tests to verify the resolver tests fail to compile**

Add near the top of `src/bin/jsonata/main.rs` (after the `use clap::Parser;` line):

```rust
mod resolve;
```

Also add `#[derive(Debug)]` to the existing `struct Cli` in `main.rs` if not already present (needed for the resolver's test helper to construct `Cli` by hand — check the derive already includes it from `#[derive(Parser, Debug)]` added in Task 1; it does, no change needed there), and make `Cli`'s fields visible to the `resolve` module. Since `resolve.rs` is a child module of the same binary crate, `main.rs`'s `struct Cli { ... }` fields need at least `pub(crate)` visibility. Update the `Cli` struct definition in `main.rs`:

```rust
#[derive(Parser, Debug)]
#[command(name = "jsonata", version, about = "Evaluate JSONata expressions against JSON data")]
pub(crate) struct Cli {
    /// Compact JSON output (default: pretty-printed)
    #[arg(short = 'c', long)]
    pub(crate) compact: bool,

    /// Print string results without surrounding quotes
    #[arg(short = 'r', long = "raw-output")]
    pub(crate) raw_output: bool,

    /// Don't read input; $ is undefined
    #[arg(short = 'n', long = "null-input")]
    pub(crate) null_input: bool,

    /// Read the expression from a file instead of the first positional argument
    #[arg(short = 'f', long = "from-file", value_name = "FILE")]
    pub(crate) from_file: Option<String>,

    /// Bind $NAME to a string value: --arg NAME=VALUE
    #[arg(long = "arg", value_name = "NAME=VALUE", action = clap::ArgAction::Append)]
    pub(crate) arg: Vec<String>,

    /// Bind $NAME to a parsed JSON value: --argjson NAME=JSON
    #[arg(long = "argjson", value_name = "NAME=JSON", action = clap::ArgAction::Append)]
    pub(crate) argjson: Vec<String>,

    /// The JSONata expression (or, with --from-file, the input data file)
    #[arg(value_name = "EXPRESSION_OR_FILE")]
    pub(crate) positional1: Option<String>,

    /// The input data file (used only when --from-file supplies the expression)
    #[arg(value_name = "FILE")]
    pub(crate) positional2: Option<String>,
}
```

Run: `cargo test --release --features cli --lib`
Expected: this specific command has no effect yet (bin targets' `#[cfg(test)] mod tests` run via `cargo test --release --features cli --bin jsonata`). Run that instead:
Run: `cargo test --release --features cli --bin jsonata`
Expected: PASS — all 8 resolver unit tests pass immediately, since `resolve.rs`'s logic was written directly (this task front-loaded implementation and test together; there is no separate red/green split for this pure-function module beyond compiling). If any test fails, fix `resolve()`'s match arms to match the truth table in the docstring above.

- [ ] **Step 3: Write the failing black-box test for `-n`**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn null_input_flag_evaluates_without_reading_stdin() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-n")
        .arg("1 + 1")
        .assert()
        .success()
        .stdout("2\n");
}

#[test]
fn null_input_with_file_argument_is_a_usage_error() {
    let dir = std::env::temp_dir();
    let path = dir.join("jsonata_cli_test_null_conflict.json");
    std::fs::write(&path, r#"{"a": 1}"#).unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-n")
        .arg("1 + 1")
        .arg(path.to_str().unwrap())
        .assert()
        .code(2);

    std::fs::remove_file(&path).unwrap();
}
```

Run: `cargo test --release --features cli --test cli_test`
Expected: FAIL — `run()` in `main.rs` doesn't call `resolve::resolve` yet, so `-n` is parsed but ignored and stdin is still read (hanging or erroring in the test harness rather than succeeding).

- [ ] **Step 4: Wire `resolve()` into `run()`**

Replace the start of `run()` in `src/bin/jsonata/main.rs` (from `let expression = ...` through the `read_input` call) with:

```rust
    let (expr_source, input_source) = match resolve::resolve(&cli) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let expression = match expr_source {
        resolve::ExpressionSource::Inline(s) => s,
        resolve::ExpressionSource::File(_) => {
            unreachable!("Task 5 implements ExpressionSource::File handling")
        }
    };

    let data = match input_source {
        resolve::InputSource::Null => JValue::Undefined,
        resolve::InputSource::Stdin => match read_input(None) {
            Ok(data) => data,
            Err((msg, code)) => {
                eprintln!("{}", msg);
                return ExitCode::from(code);
            }
        },
        resolve::InputSource::File(path) => match read_input(Some(&path)) {
            Ok(data) => data,
            Err((msg, code)) => {
                eprintln!("{}", msg);
                return ExitCode::from(code);
            }
        },
    };
```

Update `read_input`'s signature usage stays the same (`Option<&str>`); no change needed to the function itself. Remove the old `let data_source = cli.positional2.clone();` line and the old inline `let expression = match &cli.positional1 { ... }` block — both are superseded by the block above.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --release --features cli --test cli_test`
Expected: PASS (all tests so far).

- [ ] **Step 6: Commit**

```bash
git add src/bin/jsonata/main.rs src/bin/jsonata/resolve.rs tests/cli_test.rs
git commit -m "feat(cli): add -n/--null-input and the positional-argument resolver"
```

---

### Task 5: `-f/--from-file`

**Files:**
- Modify: `src/bin/jsonata/main.rs`
- Modify: `tests/cli_test.rs`

**Interfaces:**
- Consumes: `resolve::ExpressionSource::File` (already defined and covered by unit tests in Task 4) — this task only implements the runtime file-read behind the `unreachable!()` placeholder left in Task 4.

- [ ] **Step 1: Write the failing tests**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn from_file_reads_expression_from_a_file() {
    let dir = std::env::temp_dir();
    let expr_path = dir.join("jsonata_cli_test_expr.jsonata");
    std::fs::write(&expr_path, "name").unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-f")
        .arg(expr_path.to_str().unwrap())
        .write_stdin(r#"{"name": "Carol"}"#)
        .assert()
        .success()
        .stdout("\"Carol\"\n");

    std::fs::remove_file(&expr_path).unwrap();
}

#[test]
fn from_file_with_data_file_argument() {
    let dir = std::env::temp_dir();
    let expr_path = dir.join("jsonata_cli_test_expr2.jsonata");
    let data_path = dir.join("jsonata_cli_test_data2.json");
    std::fs::write(&expr_path, "name").unwrap();
    std::fs::write(&data_path, r#"{"name": "Dave"}"#).unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-f")
        .arg(expr_path.to_str().unwrap())
        .arg(data_path.to_str().unwrap())
        .assert()
        .success()
        .stdout("\"Dave\"\n");

    std::fs::remove_file(&expr_path).unwrap();
    std::fs::remove_file(&data_path).unwrap();
}

#[test]
fn from_file_with_nonexistent_expression_file_is_usage_error() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-f")
        .arg("/nonexistent/path/to/expr.jsonata")
        .write_stdin("{}")
        .assert()
        .code(2)
        .stderr(contains("could not read expression file"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --release --features cli --test cli_test`
Expected: `from_file_reads_expression_from_a_file` and `from_file_with_data_file_argument` panic on the `unreachable!()` in `main.rs`; `from_file_with_nonexistent_expression_file_is_usage_error` also hits it (the file-not-found check hasn't been implemented yet).

- [ ] **Step 3: Implement `ExpressionSource::File` handling**

In `src/bin/jsonata/main.rs`, replace:

```rust
    let expression = match expr_source {
        resolve::ExpressionSource::Inline(s) => s,
        resolve::ExpressionSource::File(_) => {
            unreachable!("Task 5 implements ExpressionSource::File handling")
        }
    };
```

with:

```rust
    let expression = match expr_source {
        resolve::ExpressionSource::Inline(s) => s,
        resolve::ExpressionSource::File(path) => match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: could not read expression file {}: {}", path, e);
                return ExitCode::from(2);
            }
        },
    };
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --release --features cli --test cli_test`
Expected: PASS (all tests so far).

- [ ] **Step 5: Commit**

```bash
git add src/bin/jsonata/main.rs tests/cli_test.rs
git commit -m "feat(cli): add -f/--from-file expression source"
```

---

### Task 6: `--arg`/`--argjson` bindings, and a shared error-message accessor

**Files:**
- Create: `src/bin/jsonata/bindings.rs`
- Create: `src/bin/jsonata/error_format.rs`
- Modify: `src/evaluator.rs` (add `impl EvaluatorError { pub fn message(&self) -> &str }`)
- Modify: `src/parser.rs` (add `impl ParserError { pub fn display_message(&self) -> String }`)
- Modify: `src/lib.rs` (retarget `evaluator_error_to_py`/`parser_error_to_py` to the new methods; delete the now-redundant private `format_parser_error_message`; retarget its two existing unit tests)
- Modify: `src/bin/jsonata/main.rs`
- Modify: `tests/cli_test.rs`

**Why this touches library code, not just the CLI:** the pre-flight review of this plan flagged that formatting `EvaluatorError`/`ParserError` for display requires unwrapping thiserror's `Display` wrapper to reach the JSONata-spec-coded inner message — logic that already exists, privately, in `src/lib.rs` (`evaluator_error_to_py`'s match arms; `format_parser_error_message`) gated behind `#[cfg(feature = "python")]` for the first and ungated-but-private for the second. Duplicating that logic again inside `src/bin/jsonata/error_format.rs` would be exactly the kind of verbatim-duplicated-logic-block the task reviewer's rubric treats as an Important finding. The user's explicit decision (see project conversation) was to extract it instead: put the unwrap-and-format logic as `pub` methods directly on `EvaluatorError`/`ParserError` in their owning modules (not in `lib.rs`, which is documented as the "Python API boundary" — the CLI has no reason to depend on PyO3-adjacent code), then have both `src/lib.rs`'s existing Python conversion AND the new CLI call the same method. This is the one place in Phase 1 where "all new code is additive" (Global Constraints) is deliberately not the case — it's a small, behavior-preserving extraction of existing private logic into two public methods, not a change to what either the Python bindings or the CLI actually do.

**Interfaces:**
- Produces: `EvaluatorError::message(&self) -> &str` (`src/evaluator.rs`) — unwraps the variant's inner `String` without the enum's `Display`-derived outer prefix. `ParserError::display_message(&self) -> String` (`src/parser.rs`) — the full CLI/Python-ready string: `Coded` variants pass through as `"CODE: message"`, others get a `"Parse error: "` prefix. `bindings::parse_bindings(arg: &[String], argjson: &[String]) -> Result<std::collections::HashMap<String, JValue>, String>`. `error_format::format_evaluator_error(e: &EvaluatorError) -> String` — CLI-only presentation layer on top of `e.message()` (adds the `"error: "` prefix for non-spec-coded messages; this part is CLI-specific and has no Python-side equivalent, so it stays in the CLI, not the library).
- Consumes: `jsonata_core::evaluator::EvaluatorError`, `jsonata_core::parser::ParserError` (both existing, `src/evaluator.rs:2511-2523`, `src/parser.rs:9-46`), `jsonata_core::value::JValue::string`/`from_json_str`, `jsonata_core::evaluator::Context::bind` (`src/evaluator.rs:2638`).

- [ ] **Step 1: Add the shared `message()`/`display_message()` methods to the library's error types, with unit tests colocated with each type**

In `src/evaluator.rs`, immediately after the existing `EvaluatorError` enum definition and its `impl From<...>` blocks (i.e. right after the closing brace of `impl From<crate::datetime::DateTimeError> for EvaluatorError { ... }`, around line 2532), add:

```rust
impl EvaluatorError {
    /// The underlying message, without the outer "Type error: "/
    /// "Reference error: "/"Evaluation error: " prefix that `Display` (via
    /// thiserror's `#[error("Type error: {0}")]` etc.) would add. This is
    /// what JSONata-spec-coded messages like "T2002: ..." actually look
    /// like — the coded prefix is INSIDE this string, not added by
    /// `Display`. Used by both the Python bindings (`src/lib.rs`) and the
    /// `jsonata` CLI so the two never need to duplicate this unwrap.
    pub fn message(&self) -> &str {
        match self {
            EvaluatorError::TypeError(m) => m,
            EvaluatorError::ReferenceError(m) => m,
            EvaluatorError::EvaluationError(m) => m,
        }
    }
}

#[cfg(test)]
mod evaluator_error_message_tests {
    use super::EvaluatorError;

    #[test]
    fn message_strips_the_display_prefix() {
        let e = EvaluatorError::TypeError(
            "T2002: The left side of the + operator must evaluate to a number".to_string(),
        );
        assert_eq!(
            e.message(),
            "T2002: The left side of the + operator must evaluate to a number"
        );
        // Display, by contrast, adds the "Type error: " wrapper -- this is
        // exactly the distinction `message()` exists to avoid.
        assert_eq!(
            e.to_string(),
            "Type error: T2002: The left side of the + operator must evaluate to a number"
        );
    }

    #[test]
    fn message_works_for_all_variants() {
        assert_eq!(
            EvaluatorError::ReferenceError("$foo is not defined".to_string()).message(),
            "$foo is not defined"
        );
        assert_eq!(
            EvaluatorError::EvaluationError("something went wrong".to_string()).message(),
            "something went wrong"
        );
    }
}
```

In `src/parser.rs`, immediately after the existing `ParserError` enum definition, add:

```rust
impl ParserError {
    /// The full display-ready message: `Coded` variants (e.g. S0214) are
    /// already exactly "code: message" via `Display`, so they pass
    /// through unchanged; every other variant gets a "Parse error: "
    /// prefix added. Used by both the Python bindings (`src/lib.rs`) and
    /// the `jsonata` CLI.
    pub fn display_message(&self) -> String {
        let msg = self.to_string();
        if matches!(self, ParserError::Coded { .. }) {
            msg
        } else {
            format!("Parse error: {}", msg)
        }
    }
}

#[cfg(test)]
mod parser_error_display_message_tests {
    use super::ParserError;

    #[test]
    fn coded_error_passes_through_unchanged() {
        let e = ParserError::Coded {
            code: "S0214",
            message: "The % operator is invalid outside a path".to_string(),
        };
        assert_eq!(
            e.display_message(),
            "S0214: The % operator is invalid outside a path"
        );
    }

    #[test]
    fn uncoded_error_gets_parse_error_prefix() {
        let e = ParserError::UnexpectedToken("foo".to_string());
        assert_eq!(e.display_message(), "Parse error: Unexpected token: foo");
    }
}
```

- [ ] **Step 2: Retarget `src/lib.rs`'s existing Python error conversion onto the new methods, and run the full existing test suite to confirm no behavior change**

In `src/lib.rs`, replace:

```rust
/// Convert an EvaluatorError to a PyErr
#[cfg(feature = "python")]
fn evaluator_error_to_py(e: evaluator::EvaluatorError) -> PyErr {
    match e {
        evaluator::EvaluatorError::TypeError(msg) => PyValueError::new_err(msg),
        evaluator::EvaluatorError::ReferenceError(msg) => PyValueError::new_err(msg),
        evaluator::EvaluatorError::EvaluationError(msg) => PyValueError::new_err(msg),
    }
}

/// Format a ParserError's Python-facing message.
/// Coded errors (e.g., S0214) have their message formatted as "code: message",
/// so they are passed through directly without an additional "Parse error: " prefix.
/// Other errors get the "Parse error: " prefix for clarity.
///
/// Split out from `parser_error_to_py` so this formatting logic can be unit
/// tested without constructing a `PyErr` (which requires an initialized
/// Python interpreter -- fine under `maturin develop`/pytest, but panics
/// under a plain `cargo test --all-features` with no embedded interpreter).
fn format_parser_error_message(e: &parser::ParserError) -> String {
    let msg = e.to_string();
    if matches!(e, parser::ParserError::Coded { .. }) {
        // Coded errors already have the format "code: message"
        msg
    } else {
        // Other errors get the "Parse error: " prefix
        format!("Parse error: {}", msg)
    }
}

/// Convert a ParserError to a PyErr
#[cfg(feature = "python")]
fn parser_error_to_py(e: parser::ParserError) -> PyErr {
    PyValueError::new_err(format_parser_error_message(&e))
}
```

with:

```rust
/// Convert an EvaluatorError to a PyErr
#[cfg(feature = "python")]
fn evaluator_error_to_py(e: evaluator::EvaluatorError) -> PyErr {
    PyValueError::new_err(e.message().to_string())
}

/// Convert a ParserError to a PyErr
#[cfg(feature = "python")]
fn parser_error_to_py(e: parser::ParserError) -> PyErr {
    PyValueError::new_err(e.display_message())
}
```

`src/lib.rs` has two existing unit tests (in its own `#[cfg(test)] mod tests` block, search for `test_parser_error_to_py_coded_error_no_prefix` and `test_parser_error_to_py_non_coded_error_with_prefix`) that call the now-deleted `format_parser_error_message` directly. Update both call sites from `format_parser_error_message(&coded_error)` / `format_parser_error_message(&non_coded_error)` to `coded_error.display_message()` / `non_coded_error.display_message()` respectively — same assertions, just calling the relocated method.

Run: `cargo test --release --all-features`
Expected: PASS in full, including the two retargeted `src/lib.rs` tests and the four new tests added in Step 2. This confirms the extraction changed *where* the logic lives, not *what* it does — the Python-facing error strings are byte-for-byte identical to before.

- [ ] **Step 3: Write the CLI-only presentation layer**

Create `src/bin/jsonata/error_format.rs` — this is intentionally small: the shared unwrap/format logic now lives in the library (Step 2), so this module only adds the CLI's own stderr convention (an `"error: "` prefix for messages that don't already start with a JSONata spec code):

```rust
use jsonata_core::evaluator::EvaluatorError;

/// Formats an `EvaluatorError` for CLI stderr output: `e.message()` (the
/// library's shared unwrap, see `EvaluatorError::message` in
/// `src/evaluator.rs`) already carries a JSONata spec code prefix like
/// "T2002: ..." when applicable; this adds an "error: " prefix only when
/// it doesn't. `ParserError::display_message()` needs no equivalent
/// wrapper here — it's already fully CLI-ready (see `src/parser.rs`), so
/// callers use it directly.
pub fn format_evaluator_error(e: &EvaluatorError) -> String {
    let msg = e.message();
    if is_coded_error(msg) {
        msg.to_string()
    } else {
        format!("error: {}", msg)
    }
}

fn is_coded_error(message: &str) -> bool {
    let bytes = message.as_bytes();
    bytes.len() >= 6
        && matches!(bytes[0], b'T' | b'D' | b'U' | b'S')
        && bytes[1..5].iter().all(u8::is_ascii_digit)
        && bytes[5] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coded_evaluator_error_passes_through_unwrapped() {
        let e = EvaluatorError::TypeError(
            "T2002: The left side of the + operator must evaluate to a number".to_string(),
        );
        assert_eq!(
            format_evaluator_error(&e),
            "T2002: The left side of the + operator must evaluate to a number"
        );
    }

    #[test]
    fn uncoded_evaluator_error_gets_error_prefix() {
        let e = EvaluatorError::ReferenceError("$foo is not defined".to_string());
        assert_eq!(format_evaluator_error(&e), "error: $foo is not defined");
    }
}
```

- [ ] **Step 4: Write the failing unit tests for binding parsing, then implement**

Create `src/bin/jsonata/bindings.rs`:

```rust
use jsonata_core::value::JValue;
use std::collections::HashMap;

/// Parses `--arg NAME=VALUE` (bound as a string) and `--argjson NAME=JSON`
/// (bound as a parsed JSON value) specs into a name -> JValue map.
pub fn parse_bindings(
    arg: &[String],
    argjson: &[String],
) -> Result<HashMap<String, JValue>, String> {
    let mut bindings = HashMap::new();
    for spec in arg {
        let (name, value) = split_name_value(spec, "--arg")?;
        bindings.insert(name, JValue::string(value));
    }
    for spec in argjson {
        let (name, value) = split_name_value(spec, "--argjson")?;
        let parsed = JValue::from_json_str(&value)
            .map_err(|e| format!("--argjson {}: invalid JSON value: {}", name, e))?;
        bindings.insert(name, parsed);
    }
    Ok(bindings)
}

fn split_name_value(spec: &str, flag: &str) -> Result<(String, String), String> {
    match spec.split_once('=') {
        Some((name, value)) if !name.is_empty() => Ok((name.to_string(), value.to_string())),
        _ => Err(format!("{} expects NAME=VALUE, got: {}", flag, spec)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arg_binds_a_string() {
        let b = parse_bindings(&["region=us".to_string()], &[]).unwrap();
        assert_eq!(b.get("region"), Some(&JValue::string("us")));
    }

    #[test]
    fn argjson_binds_a_parsed_value() {
        let b = parse_bindings(&[], &["limit=42".to_string()]).unwrap();
        assert_eq!(b.get("limit"), Some(&JValue::Number(42.0)));
    }

    #[test]
    fn arg_without_equals_is_an_error() {
        assert!(parse_bindings(&["justaname".to_string()], &[]).is_err());
    }

    #[test]
    fn argjson_with_invalid_json_is_an_error() {
        assert!(parse_bindings(&[], &["x=not json".to_string()]).is_err());
    }

    #[test]
    fn arg_value_may_contain_equals_signs() {
        let b = parse_bindings(&["eq=a=b".to_string()], &[]).unwrap();
        assert_eq!(b.get("eq"), Some(&JValue::string("a=b")));
    }
}
```

- [ ] **Step 5: Run all binary-internal unit tests**

Run: `cargo test --release --features cli --bin jsonata`
Expected: PASS — `bindings.rs` and `error_format.rs` are self-contained pure-function code with no dependency on `main.rs`'s wiring yet, so once they compile they should pass immediately (front-loaded implementation-with-tests, same pattern as Task 4's resolver).

If `cargo build` fails because `bindings`/`error_format` aren't declared as modules yet, add to `src/bin/jsonata/main.rs` (near the existing `mod resolve;` line):

```rust
mod bindings;
mod error_format;
```

- [ ] **Step 6: Write the failing black-box tests**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn arg_binds_a_string_variable() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--arg")
        .arg("region=us")
        .arg("$region")
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("\"us\"\n");
}

#[test]
fn argjson_binds_a_json_variable() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--argjson")
        .arg("limit=5")
        .arg("$limit * 2")
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("10\n");
}

#[test]
fn malformed_arg_binding_is_a_usage_error() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--arg")
        .arg("noequalssign")
        .arg("$x")
        .write_stdin("{}")
        .assert()
        .code(2);
}

#[test]
fn evaluation_error_preserves_jsonata_error_code() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("1 + \"a\"")
        .write_stdin("{}")
        .assert()
        .code(1)
        .stderr(contains("T2002"));
}
```

- [ ] **Step 7: Run tests to verify they fail**

Run: `cargo test --release --features cli --test cli_test`
Expected: FAIL — `--arg`/`--argjson` are parsed by clap but never applied to the evaluation context; error messages still use the placeholder `eprintln!("error: {}", e)` from Task 2 (which, for `EvaluatorError`, would print `"error: Type error: T2002: ..."` — code buried behind two prefixes, failing the `contains("T2002")` assertion only incidentally since T2002 IS still a substring; verify this test actually distinguishes the fix by checking the FULL expected message shape, not just substring — if it passes before Step 9, add a stricter assertion `stderr(predicates::str::starts_with("T2002:"))` instead of `contains` to force the distinction).

Given that risk, use this stricter version instead in Step 7 for `evaluation_error_preserves_jsonata_error_code`:

```rust
#[test]
fn evaluation_error_preserves_jsonata_error_code() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("1 + \"a\"")
        .write_stdin("{}")
        .assert()
        .code(1)
        .stderr(predicates::str::starts_with("T2002:"));
}
```

- [ ] **Step 8: Wire bindings and the shared/CLI error formatting into `run()`**

In `src/bin/jsonata/main.rs`, replace:

```rust
    let mut evaluator = Evaluator::with_context(Context::new());
    let result = match evaluator.evaluate(&ast, &data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };
```

with:

```rust
    let var_bindings = match bindings::parse_bindings(&cli.arg, &cli.argjson) {
        Ok(b) => b,
        Err(msg) => {
            eprintln!("error: {}", msg);
            return ExitCode::from(2);
        }
    };

    let mut context = Context::new();
    for (name, value) in var_bindings {
        context.bind(name, value);
    }
    let mut evaluator = Evaluator::with_context(context);
    let result = match evaluator.evaluate(&ast, &data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", error_format::format_evaluator_error(&e));
            return ExitCode::from(1);
        }
    };
```

Also replace the parse-error arm earlier in `run()`:

```rust
    let ast = match parser::parse(&expression) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };
```

with (calling `ParserError::display_message()` from Step 2 directly — it's already fully CLI-ready, no `error_format` wrapper needed for this one):

```rust
    let ast = match parser::parse(&expression) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("{}", e.display_message());
            return ExitCode::from(1);
        }
    };
```

- [ ] **Step 9: Run tests to verify they pass**

Run: `cargo test --release --features cli --test cli_test`
Run: `cargo test --release --features cli --bin jsonata`
Run: `cargo test --release --all-features`
Expected: PASS across all three (binary-internal unit tests, black-box integration tests, and the full existing library suite including the retargeted `src/lib.rs` tests from Step 3).

- [ ] **Step 10: Commit**

```bash
git add src/evaluator.rs src/parser.rs src/lib.rs src/bin/jsonata/main.rs src/bin/jsonata/bindings.rs src/bin/jsonata/error_format.rs tests/cli_test.rs
git commit -m "feat(cli): add --arg/--argjson bindings; extract shared error message accessors"
```

---

### Task 7: Exit-code contract cleanup pass + `cargo fmt`/`clippy`

**Files:**
- Modify: `src/bin/jsonata/main.rs`
- Modify: `tests/cli_test.rs`

**Interfaces:**
- No new interfaces — this task is a verification and edge-case-closure pass over the exit-code contract established in the Global Constraints section, confirming every documented case is actually covered by a test, then satisfying the repo's lint gates.

- [ ] **Step 1: Add the remaining exit-code edge-case tests**

Add to `tests/cli_test.rs`:

```rust
#[test]
fn invalid_json_input_exits_three() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a")
        .write_stdin("not json")
        .assert()
        .code(3)
        .stderr(contains("invalid JSON input"));
}

#[test]
fn parse_error_exits_one() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a[")
        .write_stdin("{}")
        .assert()
        .code(1);
}

#[test]
fn missing_expression_argument_exits_two() {
    Command::cargo_bin("jsonata").unwrap().assert().code(2);
}

#[test]
fn nonexistent_input_file_exits_two() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a")
        .arg("/nonexistent/path/data.json")
        .assert()
        .code(2)
        .stderr(contains("could not read input file"));
}

#[test]
fn unknown_flag_exits_two_via_clap_default() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--not-a-real-flag")
        .arg("a")
        .assert()
        .code(2);
}
```

- [ ] **Step 2: Run all CLI tests and confirm they already pass**

Run: `cargo test --release --features cli --test cli_test`
Expected: PASS. If any fail, they're gaps in Tasks 2-6's implementation, not new behavior — fix the specific `main.rs` arm that doesn't match the Global Constraints table (most likely a wrong exit code literal) before proceeding.

- [ ] **Step 3: Format and lint**

Run: `cargo fmt --all`
Run: `cargo fmt --all -- --check`
Expected: no diff (Step 3's `cargo fmt --all` should have already applied any needed changes; the `--check` run confirms clean).

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings. Fix any that appear (common ones for this kind of code: `clippy::redundant_clone` on the `.clone()` calls in `resolve.rs`'s match arms — review each flagged instance individually rather than blanket-suppressing).

- [ ] **Step 4: Full test suite regression check**

Run: `cargo test --release --all-features`
Expected: PASS — confirms the new `cli` feature and binary haven't broken the existing 137+ Rust unit/integration tests or the reference suite invocation path.

- [ ] **Step 5: Commit**

```bash
git add tests/cli_test.rs
git commit -m "test(cli): close out exit-code contract coverage"
```

(If Step 3 produced formatting fixes, include those files in this commit too: `git add -u` before committing.)

---

### Task 8: Shared fixture file + CLI spec doc for Phase 2 parity

**Files:**
- Create: `study/cli_spec.md`
- Create: `study/cli_fixtures.json`
- Create: `tests/cli_fixtures_test.rs`

**Interfaces:**
- Produces: `study/cli_fixtures.json`, an array of test-case objects consumed by both this task's Rust runner and (in Phase 2's future plan) a Python pytest runner testing `jsonatapy`'s CLI entry point against the identical cases. Schema (documented in `study/cli_spec.md` too): `{"name": string, "args": string[], "stdin": string | null, "expected_exit": number, "expected_stdout": string | null, "expected_stderr_contains": string | null}`. `null` for `expected_stdout`/`expected_stderr_contains` means "don't check this stream."

- [ ] **Step 1: Write `study/cli_fixtures.json`**

Create `study/cli_fixtures.json`:

```json
[
  {
    "name": "simple_field_access",
    "args": ["name"],
    "stdin": "{\"name\": \"Alice\"}",
    "expected_exit": 0,
    "expected_stdout": "\"Alice\"\n",
    "expected_stderr_contains": null
  },
  {
    "name": "compact_output",
    "args": ["-c", "{\"x\": a}"],
    "stdin": "{\"a\": 1}",
    "expected_exit": 0,
    "expected_stdout": "{\"x\":1}\n",
    "expected_stderr_contains": null
  },
  {
    "name": "raw_output_string",
    "args": ["-r", "name"],
    "stdin": "{\"name\": \"Alice\"}",
    "expected_exit": 0,
    "expected_stdout": "Alice\n",
    "expected_stderr_contains": null
  },
  {
    "name": "raw_output_non_string_unaffected",
    "args": ["-r", "-c", "age"],
    "stdin": "{\"age\": 30}",
    "expected_exit": 0,
    "expected_stdout": "30\n",
    "expected_stderr_contains": null
  },
  {
    "name": "null_input_no_stdin_needed",
    "args": ["-n", "1 + 1"],
    "stdin": null,
    "expected_exit": 0,
    "expected_stdout": "2\n",
    "expected_stderr_contains": null
  },
  {
    "name": "undefined_result_prints_nothing",
    "args": ["nonexistent_field"],
    "stdin": "{\"a\": 1}",
    "expected_exit": 0,
    "expected_stdout": "",
    "expected_stderr_contains": null
  },
  {
    "name": "null_result_prints_literal_null",
    "args": ["nullField"],
    "stdin": "{\"nullField\": null}",
    "expected_exit": 0,
    "expected_stdout": "null\n",
    "expected_stderr_contains": null
  },
  {
    "name": "arg_binds_string",
    "args": ["--arg", "region=us", "$region"],
    "stdin": "{}",
    "expected_exit": 0,
    "expected_stdout": "\"us\"\n",
    "expected_stderr_contains": null
  },
  {
    "name": "argjson_binds_json_value",
    "args": ["--argjson", "limit=5", "$limit * 2"],
    "stdin": "{}",
    "expected_exit": 0,
    "expected_stdout": "10\n",
    "expected_stderr_contains": null
  },
  {
    "name": "invalid_json_input_is_exit_3",
    "args": ["a"],
    "stdin": "not json",
    "expected_exit": 3,
    "expected_stdout": null,
    "expected_stderr_contains": "invalid JSON input"
  },
  {
    "name": "parse_error_is_exit_1",
    "args": ["a["],
    "stdin": "{}",
    "expected_exit": 1,
    "expected_stdout": null,
    "expected_stderr_contains": null
  },
  {
    "name": "evaluation_error_preserves_error_code",
    "args": ["1 + \"a\""],
    "stdin": "{}",
    "expected_exit": 1,
    "expected_stdout": null,
    "expected_stderr_contains": "T2002:"
  },
  {
    "name": "malformed_arg_binding_is_exit_2",
    "args": ["--arg", "noequalssign", "$x"],
    "stdin": "{}",
    "expected_exit": 2,
    "expected_stdout": null,
    "expected_stderr_contains": null
  },
  {
    "name": "missing_expression_is_exit_2",
    "args": [],
    "stdin": "{}",
    "expected_exit": 2,
    "expected_stdout": null,
    "expected_stderr_contains": null
  },
  {
    "name": "null_input_with_data_file_conflict_is_exit_2",
    "args": ["-n", "1 + 1"],
    "stdin": null,
    "expected_exit": 0,
    "expected_stdout": "2\n",
    "expected_stderr_contains": null
  }
]
```

- [ ] **Step 2: Write the failing fixture-runner test**

Create `tests/cli_fixtures_test.rs`:

```rust
use assert_cmd::Command;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    args: Vec<String>,
    stdin: Option<String>,
    expected_exit: i32,
    expected_stdout: Option<String>,
    expected_stderr_contains: Option<String>,
}

#[test]
fn all_fixtures_pass() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/study/cli_fixtures.json"
    ))
    .expect("study/cli_fixtures.json must exist");
    let fixtures: Vec<Fixture> =
        serde_json::from_str(&raw).expect("study/cli_fixtures.json must be valid JSON");

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let mut cmd = Command::cargo_bin("jsonata").unwrap();
        cmd.args(&fixture.args);
        if let Some(stdin) = &fixture.stdin {
            cmd.write_stdin(stdin.clone());
        }
        let output = cmd.output().unwrap();

        let actual_exit = output.status.code().unwrap_or(-1);
        if actual_exit != fixture.expected_exit {
            failures.push(format!(
                "{}: expected exit {}, got {}",
                fixture.name, fixture.expected_exit, actual_exit
            ));
            continue;
        }

        if let Some(expected_stdout) = &fixture.expected_stdout {
            let actual_stdout = String::from_utf8_lossy(&output.stdout);
            if &*actual_stdout != expected_stdout {
                failures.push(format!(
                    "{}: expected stdout {:?}, got {:?}",
                    fixture.name, expected_stdout, actual_stdout
                ));
            }
        }

        if let Some(expected_fragment) = &fixture.expected_stderr_contains {
            let actual_stderr = String::from_utf8_lossy(&output.stderr);
            if !actual_stderr.contains(expected_fragment.as_str()) {
                failures.push(format!(
                    "{}: expected stderr to contain {:?}, got {:?}",
                    fixture.name, expected_fragment, actual_stderr
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "fixture failures:\n{}",
        failures.join("\n")
    );
}
```

- [ ] **Step 3: Run the fixture test**

Run: `cargo test --release --features cli --test cli_fixtures_test`
Expected: PASS — every fixture case is a re-expression of behavior already implemented and tested in Tasks 2-7; this test's purpose is to be the shared, language-agnostic source of truth Phase 2 will point its own test runner at, not to introduce new behavior. If it fails, the fixture JSON has a typo relative to the actual (already-correct) binary behavior — fix the JSON, not the binary.

- [ ] **Step 4: Write `study/cli_spec.md`**

Create `study/cli_spec.md`:

```markdown
# `jsonata` CLI flag and exit-code contract

This document is the canonical, cross-language contract for the `jsonata` CLI.
The Rust binary (`src/bin/jsonata/`) implements it; Phase 2's Python
`jsonatapy` entry point mirrors it exactly. `cli_fixtures.json` in this same
directory is the shared, executable test-case list both implementations are
tested against — do not let this document and that file drift from each
other or from either implementation.

## Usage

```
jsonata [OPTIONS] [EXPRESSION] [FILE]
jsonata [OPTIONS] --from-file <EXPR_FILE> [FILE]
```

## Flags

| Flag | Description |
|---|---|
| `-c`, `--compact` | Compact JSON output (default: pretty-printed) |
| `-r`, `--raw-output` | Print string results without surrounding quotes (non-string results are unaffected) |
| `-n`, `--null-input` | Don't read input; `$` is `Undefined`. Cannot be combined with a data-file argument. |
| `-f`, `--from-file <FILE>` | Read the expression from `FILE` instead of the first positional argument. The (now single) remaining positional argument, if given, is the input data file. |
| `--arg NAME=VALUE` | Bind `$NAME` to the string `VALUE`. Repeatable. `VALUE` may itself contain `=` characters (only the first `=` splits name from value). |
| `--argjson NAME=JSON` | Bind `$NAME` to the JSON value parsed from `JSON`. Repeatable. |
| `-V`, `--version` | Print version and exit 0. |
| `-h`, `--help` | Print help and exit 0. |

## Input resolution

- No `-n`, no `-f`: first positional argument is the expression; second
  positional argument (optional) is the input file, else stdin.
- `-f <EXPR_FILE>`, no `-n`: expression comes from `EXPR_FILE`; the (only)
  positional argument, if given, is the input file, else stdin.
- `-n`: input is never read (`$` = `Undefined`), regardless of `-f`. A data
  file positional argument combined with `-n` is a usage error (exit 2).
- Multi-JSON-document input (e.g. NDJSON) is **not** supported — input must
  be exactly one JSON value; trailing content after it is a parse error
  (exit 3). No slurp/streaming mode exists in this version.

## Output

- A JSONata `Undefined` result prints nothing to stdout, exit 0.
- A JSON `null` result prints the literal text `null`, exit 0.
- Otherwise, the result is printed as JSON (pretty by default, single-line
  with `-c`), followed by a trailing newline. With `-r`, string results are
  printed unquoted/unescaped instead; non-string results are unaffected by
  `-r`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (including an `Undefined` result, and `--version`/`--help`) |
| 1 | Expression parse error or evaluation error |
| 2 | Usage/invocation error: bad flags, malformed `--arg`/`--argjson`, an incompatible flag combination (e.g. `-n` + data file), or an expression/input file that could not be read |
| 3 | Input was read successfully but is not valid JSON |

## Error message format

Errors go to stderr. JSONata spec-coded errors (e.g. from evaluation or
parsing — codes match `^[TDUS]\d{4}:`) are printed exactly as `CODE: message`
with no extra prefix, so scripts/agents can pattern-match on the code
directly. All other errors are prefixed with `error: ` (or, for
non-spec-coded parse errors specifically, `Parse error: `).
```

- [ ] **Step 5: Run the full test suite one more time**

Run: `cargo test --release --all-features`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add study/cli_spec.md study/cli_fixtures.json tests/cli_fixtures_test.rs
git commit -m "docs(cli): add shared CLI spec + fixture suite for Phase 2 parity"
```

---

### Task 9: Distribute the binary via GitHub Releases

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- No new Rust interfaces — this task adds a CI job (`build-cli-binaries`) parallel to the existing `build-wheels` job, and extends `create-github-release`'s asset list to include its output.

- [ ] **Step 1: Add the `build-cli-binaries` job**

In `.github/workflows/release.yml`, add a new job after `build-wheels` (before `build-sdist`), using the exact same matrix as `build-wheels`:

```yaml
  build-cli-binaries:
    name: Build CLI binary on ${{ matrix.platform.os }} for ${{ matrix.platform.target }}
    runs-on: ${{ matrix.platform.runner }}
    needs: [validate-version, update-version]
    timeout-minutes: 60

    strategy:
      fail-fast: false
      matrix:
        platform:
          # Linux
          - { os: 'Linux', runner: ubuntu-latest, target: x86_64-unknown-linux-gnu }
          - { os: 'Linux', runner: ubuntu-latest, target: aarch64-unknown-linux-gnu }
          # Windows
          - { os: 'Windows', runner: windows-latest, target: x86_64-pc-windows-msvc }
          # macOS (self-hosted Mac Mini, native ARM64 build)
          - { os: 'macOS', runner: [self-hosted, macOS, ARM64], target: aarch64-apple-darwin }

    steps:
      - name: Checkout
        uses: actions/checkout@v6
        with:
          ref: refs/tags/v${{ needs.validate-version.outputs.version }}
          submodules: true

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.platform.target }}

      - name: Cache Rust dependencies
        uses: Swatinem/rust-cache@v2
        with:
          key: cli-${{ matrix.platform.target }}

      - name: Install cross-compilation tools (Linux aarch64)
        if: matrix.platform.target == 'aarch64-unknown-linux-gnu'
        run: |
          sudo apt-get update
          sudo apt-get install -y gcc-aarch64-linux-gnu

      - name: Build CLI binary
        shell: bash
        run: |
          if [ "${{ matrix.platform.target }}" = "aarch64-unknown-linux-gnu" ]; then
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
          fi
          cargo build --release --features cli --target ${{ matrix.platform.target }}

      - name: Package binary (Unix)
        if: matrix.platform.os != 'Windows'
        shell: bash
        run: |
          mkdir -p dist-cli
          tar czf dist-cli/jsonata-${{ matrix.platform.target }}.tar.gz \
            -C target/${{ matrix.platform.target }}/release jsonata

      - name: Package binary (Windows)
        if: matrix.platform.os == 'Windows'
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Force -Path dist-cli
          Compress-Archive -Path target/${{ matrix.platform.target }}/release/jsonata.exe `
            -DestinationPath dist-cli/jsonata-${{ matrix.platform.target }}.zip

      - name: Upload CLI binary
        uses: actions/upload-artifact@v7
        with:
          name: cli-${{ matrix.platform.os }}-${{ matrix.platform.target }}
          path: dist-cli
          retention-days: 7
```

- [ ] **Step 2: Include the new artifacts in the GitHub Release and update `create-github-release`'s `needs`**

In `.github/workflows/release.yml`, find the `create-github-release` job and:

1. Add `build-cli-binaries` to its `needs` list:

```yaml
    needs: [validate-version, test-wheels, build-cli-binaries, generate-changelog, publish-pypi, publish-crates]
```

2. Update the `if` condition to also require the new job's success:

```yaml
    if: always() && needs.test-wheels.result == 'success' && needs.build-cli-binaries.result == 'success'
```

3. Extend the `files:` block in the `Create GitHub Release` step:

```yaml
          files: |
            artifacts/wheels-*/*
            artifacts/sdist/*
            artifacts/cli-*/*
```

- [ ] **Step 3: Update the release summary text**

In `create-github-release`'s `Add release summary` step, extend the heredoc to mention the CLI binaries:

```yaml
      - name: Add release summary
        run: |
          cat >> $GITHUB_STEP_SUMMARY << EOF
          # Release v${{ needs.validate-version.outputs.version }} Published! 🎉

          - **Version:** ${{ needs.validate-version.outputs.version }}
          - **JSONata Compatibility:** 2.1.0 (100% test suite compatibility)
          - **PyPI:** [jsonatapy ${{ needs.validate-version.outputs.version }}](https://pypi.org/project/jsonatapy/${{ needs.validate-version.outputs.version }}/)
          - **GitHub:** [Release v${{ needs.validate-version.outputs.version }}](https://github.com/${{ github.repository }}/releases/tag/v${{ needs.validate-version.outputs.version }})

          ## Installation

          \`\`\`bash
          pip install jsonatapy==${{ needs.validate-version.outputs.version }}
          \`\`\`

          ## CLI binary

          Prebuilt \`jsonata\` binaries for Linux (x86_64/aarch64), Windows
          (x86_64), and macOS (aarch64) are attached to this release. Or
          build from source:

          \`\`\`bash
          cargo install jsonata-core --features cli
          \`\`\`
          EOF
```

- [ ] **Step 4: Validate the YAML locally**

Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))"` (or any available YAML linter)
Expected: no parse errors.

If a YAML linting tool such as `yamllint` is available, also run it: `yamllint .github/workflows/release.yml` and address any new issues introduced by this change (pre-existing issues elsewhere in the file are out of scope).

- [ ] **Step 5: Verify `cargo install jsonata-core --features cli` works locally as a stand-in for the real release path**

Run (from a scratch directory, not the repo root, to simulate a real end-user install):
`cargo install --path /mnt/c/Users/mboha/source/repos/jsonatapy --features cli --force`
Expected: succeeds, installs a `jsonata` binary onto `PATH` (typically `~/.cargo/bin/jsonata`). Run `jsonata --version` to confirm it works, then `cargo uninstall jsonata-core` to clean up the test install.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci(release): build and publish jsonata CLI binaries"
```

**Note for whoever executes this task:** modifying `.github/workflows/release.yml` is a CI/CD pipeline change — per this project's standing guidance, confirm with the user before pushing/merging this specific commit even though it was pre-approved as part of this plan, since the actual end-to-end validation (a real release run) can't happen until the next real version is cut. The full job only proves itself on the next real `workflow_dispatch` release run — flag this explicitly rather than treating the local `cargo install` check in Step 5 as equivalent coverage.

---

## Definition of Done

- `cargo build --release --features cli` produces a working `target/release/jsonata` binary; `cargo build --release` (no features) does not pull in `clap`.
- All flags in the Global Constraints table are implemented and covered by both `tests/cli_test.rs` (incremental, task-by-task) and `study/cli_fixtures.json` (consolidated, shared with Phase 2).
- The four exit codes (0/1/2/3) match their documented meanings exactly, verified by `tests/cli_test.rs`'s dedicated exit-code tests (Task 7) and cross-checked by the fixture suite (Task 8).
- Evaluator/parser error messages preserve JSONata spec codes at the start of the string when present (verified by `evaluation_error_preserves_jsonata_error_code`), via the shared `EvaluatorError::message()`/`ParserError::display_message()` methods that both `src/lib.rs`'s Python error conversion and the CLI call — not a second, CLI-local reimplementation.
- `study/cli_spec.md` + `study/cli_fixtures.json` exist and are committed — this is the artifact Phase 2's Python CLI plan will be written against next.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings` are clean.
- `cargo test --release --all-features` passes in full (new CLI tests plus the pre-existing suite).
- `.github/workflows/release.yml` has a `build-cli-binaries` job wired into `create-github-release`'s asset list (real end-to-end verification deferred to the next actual release cut, per Task 9's note).
