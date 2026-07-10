# Python CLI Entry Point + FastMCP Server — Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a `jsonatapy` console-script CLI that mirrors the Rust `jsonata` CLI's exact flag surface, exit-code contract, and error formatting (Phase 1, `study/cli_spec.md`), plus a `jsonatapy mcp` subcommand serving four FastMCP tools (`evaluate`, `validate`, `explain`, `evaluate_batch`) for agentic use.

**Architecture:** A new `python/jsonatapy/_cli/` private subpackage holds the CLI's internals as small, focused modules (`resolve.py`, `bindings.py`, `error_format.py`, `run.py`, `mcp_server.py`), each mirroring its Rust counterpart from `src/bin/jsonata/`. `python/jsonatapy/__main__.py` is a thin dispatcher: `jsonatapy mcp [...]` routes to the MCP server, anything else routes to evaluate-mode. Evaluation goes through the existing public `jsonatapy.compile()`/`JsonataExpression` API, plus one small, purely additive method added to that API in Task 3 (see below) — everything else in this plan is CLI-only and does not touch `src/`.

**Tech Stack:** Python 3.10+ stdlib `argparse` (zero new runtime dependency for the base CLI — matches the project's zero-runtime-dependency stance), `fastmcp>=3.0` as an optional `[mcp]` extra, `pytest-asyncio` for testing the MCP server in-process.

## Global Constraints

- Flag surface, input resolution, output rules, and exit codes must match `study/cli_spec.md` **exactly** — that file is the canonical, already-committed contract from Phase 1. Do not reinterpret it; if this plan's text and `study/cli_spec.md` ever seem to disagree, `study/cli_spec.md` wins and the plan has a bug.
- Exit codes: `0` success (including `Undefined` result, `--version`/`--help`); `1` parse/evaluation error; `2` usage/invocation error; `3` valid-read-but-invalid-JSON input. Usage errors (`2`) are validated before input is read or the expression is parsed — same precedence as the Rust CLI (see `study/cli_spec.md`'s "Precedence" note).
- **Error message convention, grounded against live behavior (verified during planning, not assumed):**
  - `jsonatapy.compile(expr)` raising `ValueError` — the message is **already fully formatted**, exactly matching Rust's `ParserError::display_message()`: either `"CODE: message"` for spec-coded errors or `"Parse error: message"` for everything else. Confirmed live: `jsonatapy.compile("a[")` raises `ValueError: Parse error: Unexpected token: Eof`. Pass `str(exc)` straight to stderr — no further processing.
  - `JsonataExpression.evaluate(...)`/`.evaluate_json(...)` raising `ValueError` — the message is the **raw unwrapped** text, exactly matching Rust's `EvaluatorError::message()`: sometimes spec-coded (`"T2002: The left side of the + operator must evaluate to a number"`, confirmed live via `null + 1`), sometimes not (`"Unknown function: undefinedvar"`, confirmed live). The CLI must add an `"error: "` prefix only when the message doesn't already start with a `[TDUS]\d{4}:` code — same logic as `src/bin/jsonata/error_format.rs`'s `is_coded_error`/`format_evaluator_error`, reimplemented in Python (there is no shared-library trick available here since this is a different language; a small, intentional parallel implementation is correct, not duplication in the Rust-internal sense Phase 1 avoided).
- **Undefined-vs-null result distinction — required a small additive library change (Task 3), decided during planning.** Both `JsonataExpression.evaluate()` and `.evaluate_json()` collapse a JSONata `Undefined` result and an explicit JSON `null` result to the same Python `None` / JSON text `"null"` (confirmed live: `.evaluate_json()` returns the string `"null"` for both a nonexistent-field access and an explicit `null` field — verified by testing both cases directly). Root cause, traced during planning: `src/value.rs`'s `Serialize` impl has `JValue::Undefined => serializer.serialize_none()`, which JSON (having no native "undefined") necessarily renders as the text `"null"` — this is correct behavior for JSON serialization, not a bug, but it means no existing method exposes the pre-serialization `JValue::is_undefined()` check the Rust CLI relies on. In `jsonata-js`, this isn't a problem at all: JS has `undefined` as a first-class value distinct from `null`, so callers just check `result === undefined` directly — there is no reference-implementation "handling" to port, because JS's type system doesn't lose the distinction the way Python's PyO3 bridge does. The fix (approved during planning): add one small, purely additive method, `JsonataExpression.evaluate_json_or_none()`, that checks `is_undefined()` before serializing (Task 3). This is required for `study/cli_fixtures.json`'s `undefined_result_prints_nothing` case to pass for the Python CLI at all — without it, Python cannot faithfully implement `study/cli_spec.md`'s Output section.
- **Known, disclosed divergence that remains out of scope — do not try to fix this in this plan (distinct from the above):** the Python `jsonatapy` public API has no way to construct a JSONata `Undefined` top-level *context* (input), only results. `python_to_json` in `src/lib.rs` maps Python `None` to `JValue::Null`, never `JValue::Undefined` (confirmed by reading `src/lib.rs:503` and live-testing: `jsonatapy.evaluate("$string($)", None)` returns `"null"`, which only happens for `Null` context). This means the Python CLI's `-n`/`--null-input` passes a `null` context, while the Rust CLI's `-n` gives true `Undefined` context. Unobservable for expressions that don't reference `$` (the common case: `-n '1 + 1'`, `-n '$now()'`); observable for ones that do (`-n '$exists($)'`). This is a narrower, `-n`-only gap (unlike the result-side issue above, which affects any expression) — document it in `study/cli_spec.md`'s Python-specific notes and pin it with a regression test (Task 7) so it's tracked, not silently wrong. Do not conflate this with Task 3's fix; they are different gaps on different sides of evaluation (input context vs. result).
- `python/jsonatapy/_cli/*.py` and `python/jsonatapy/__main__.py` must satisfy this project's existing `mypy --strict` config (`disallow_untyped_defs = true` in `pyproject.toml`) — full type hints on every function, no bare `Any` returns without justification.
- `ruff check`/`ruff format --check` (existing `[tool.ruff]` config in `pyproject.toml`) must stay clean.
- `fastmcp` is an **optional** dependency (`[project.optional-dependencies] mcp = [...]`), not in `dependencies = []` — importing `jsonatapy` (the library) must never require `fastmcp`. Only `jsonatapy mcp` touches it, and does so lazily with a friendly install-hint on `ImportError`, never a raw traceback.
- Task 3 is the one place in this plan that touches `src/` and the existing, stable public Python API surface — it must be strictly additive (new method, zero change to any existing method's signature or behavior) and verified against the full pre-existing test suite (1682-case reference suite plus all other `tests/python/*.py` files) to prove zero regression, the same discipline Phase 1's equivalent library-touching task (its Task 6) used.
- Design source of truth: `docs/superpowers/specs/2026-07-09-multi-language-and-agentic-study-design.md` (Phase 2 section + cross-phase Decisions), and `study/cli_spec.md` / `study/cli_fixtures.json` (both already committed from Phase 1).
- Work on branch `worktree-python-cli-phase2` (already created, stacked on the Phase 1 branch's tip since Phase 1's PR is still open — this branch has everything Phase 1 built, including `study/cli_spec.md`/`study/cli_fixtures.json`).

---

### Task 1: Package scaffolding — `[project.scripts]`, `_cli` subpackage, `-V`/`-h`

**Files:**
- Modify: `pyproject.toml`
- Create: `python/jsonatapy/__main__.py`
- Create: `python/jsonatapy/_cli/__init__.py`
- Create: `python/jsonatapy/_cli/run.py`
- Create: `tests/python/test_cli.py`

**Interfaces:**
- Produces: `jsonatapy._cli.run.build_parser() -> argparse.ArgumentParser`, `jsonatapy._cli.run.run(argv: list[str]) -> int` (the function every later task extends), `jsonatapy.__main__.main() -> int` (the console-script entry point). `jsonatapy` command is installed and runnable after `maturin develop`/`pip install -e .` since `python/jsonatapy/_cli/*.py` and `__main__.py` are pure Python — no rebuild needed for changes to them under an editable install.
- Consumes: `jsonatapy.__version__` (already exists in `python/jsonatapy/__init__.py`, confirmed live: `"2.2.2"`).

- [ ] **Step 1: Add the console-script entry point and dev/test dependencies to `pyproject.toml`**

In `pyproject.toml`, add under `[project]` (after the existing `dependencies = []` line):

```toml
[project.scripts]
jsonatapy = "jsonatapy.__main__:main"
```

Add `pytest-asyncio` to the existing `dev` extra and `[dependency-groups] dev` list (needed starting Task 9 for FastMCP in-memory `Client` tests — adding it now keeps dependency bookkeeping in one place):

```toml
[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-cov>=4.0",
    "pytest-xdist>=3.0",  # Parallel test execution
    "pytest-asyncio>=0.24",  # Async tests for the FastMCP server (Task 9)
    "ruff>=0.3.0",        # Linting and formatting
    "mypy>=1.0",
    "maturin>=1.0",
    "fastmcp>=3.0",       # So `dev` installs can run the full test suite (Task 9)
]
```

Add the new `mcp` extra (the end-user-facing minimal install path — `pip install jsonatapy[mcp]`):

```toml
mcp = [
    "fastmcp>=3.0",
]
```

Update `[dependency-groups] dev` the same way (add `"pytest-asyncio>=0.24"` and `"fastmcp>=3.0"`):

```toml
[dependency-groups]
dev = [
    "pytest>=7.0",
    "pytest-cov>=4.0",
    "pytest-xdist>=3.0",
    "pytest-asyncio>=0.24",
    "ruff>=0.3.0",
    "mypy>=1.0",
    "maturin>=1.0",
    "fastmcp>=3.0",
]
```

Add `asyncio_mode = "auto"` to `[tool.pytest.ini_options]` (required for `pytest-asyncio` to run `async def test_...` functions automatically, needed starting Task 9):

```toml
[tool.pytest.ini_options]
testpaths = ["tests/python"]
python_files = "test_*.py"
python_classes = "Test*"
python_functions = "test_*"
addopts = "-v --strict-markers"
asyncio_mode = "auto"
markers = [
    "slow: marks tests as slow (deselect with '-m \"not slow\"')",
    "compatibility: marks tests that verify JS compatibility",
    "reference: marks tests from the reference JSONata suite",
    "group: marks tests from a specific test group (use with group name)",
]
```

- [ ] **Step 2: Create the `_cli` subpackage skeleton and `run.py`'s argument parser**

Create `python/jsonatapy/_cli/__init__.py` (empty, just marks the package):

```python
"""Internal CLI implementation for the `jsonatapy` console script. Not public API."""
```

Create `python/jsonatapy/_cli/run.py`:

```python
"""Core evaluate-mode CLI logic: argument parsing, evaluation, output.

Mirrors src/bin/jsonata/main.rs in the Rust CLI. See study/cli_spec.md for
the full flag/exit-code contract both implementations must satisfy.
"""

from __future__ import annotations

import argparse

import jsonatapy


def build_parser() -> argparse.ArgumentParser:
    """Builds the argparse parser for evaluate-mode (`jsonatapy [OPTIONS] [EXPR] [FILE]`).

    argparse's default error handling (unknown/malformed flags) already
    prints a usage message to stderr and calls sys.exit(2) -- matching this
    CLI's exit-code-2 usage-error convention with no extra code needed.
    """
    parser = argparse.ArgumentParser(
        prog="jsonatapy",
        description="Evaluate JSONata expressions against JSON data",
    )
    parser.add_argument("-c", "--compact", action="store_true", help="Compact JSON output")
    parser.add_argument(
        "-r", "--raw-output", action="store_true", help="Print string results without quotes"
    )
    parser.add_argument(
        "-n", "--null-input", action="store_true", help="Don't read input; $ is null"
    )
    parser.add_argument(
        "-f",
        "--from-file",
        metavar="FILE",
        default=None,
        help="Read the expression from FILE instead of the first positional argument",
    )
    parser.add_argument(
        "--arg",
        action="append",
        default=[],
        metavar="NAME=VALUE",
        help="Bind $NAME to a string value (repeatable)",
    )
    parser.add_argument(
        "--argjson",
        action="append",
        default=[],
        metavar="NAME=JSON",
        help="Bind $NAME to a parsed JSON value (repeatable)",
    )
    parser.add_argument(
        "-V",
        "--version",
        action="version",
        version=f"jsonatapy {jsonatapy.__version__}",
    )
    parser.add_argument(
        "positional1",
        nargs="?",
        default=None,
        metavar="EXPRESSION_OR_FILE",
        help="The JSONata expression (or, with --from-file, the input data file)",
    )
    parser.add_argument(
        "positional2",
        nargs="?",
        default=None,
        metavar="FILE",
        help="The input data file (used only when --from-file supplies the expression)",
    )
    return parser


def run(argv: list[str]) -> int:
    """Parses argv and returns the process exit code. Full logic added starting Task 4."""
    build_parser().parse_args(argv)
    return 0
```

Create `python/jsonatapy/__main__.py`:

```python
"""Console-script entry point for the `jsonatapy` CLI.

Dispatches to the MCP server subcommand (`jsonatapy mcp ...`) or evaluate
mode (everything else). See study/cli_spec.md for the full contract.
"""

from __future__ import annotations

import sys

from ._cli.run import run


def main() -> int:
    return run(sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 3: Write the failing smoke tests**

Create `tests/python/test_cli.py`:

```python
"""Black-box tests for the `jsonatapy` CLI, run as a subprocess exactly like
a real user would invoke it. Mirrors tests/cli_test.rs in the Rust CLI.
"""

from __future__ import annotations

import shutil
import subprocess
import sys


def run_cli(
    args: list[str], stdin: str | None = None
) -> subprocess.CompletedProcess[str]:
    """Runs the jsonatapy CLI via `python -m jsonatapy` (PATH-independent,
    reflects live source under an editable install)."""
    return subprocess.run(
        [sys.executable, "-m", "jsonatapy", *args],
        input=stdin,
        capture_output=True,
        text=True,
    )


def test_version_flag_prints_version_and_exits_zero() -> None:
    result = run_cli(["--version"])
    assert result.returncode == 0
    assert "jsonatapy" in result.stdout


def test_help_flag_lists_known_options() -> None:
    result = run_cli(["--help"])
    assert result.returncode == 0
    assert "--compact" in result.stdout
    assert "--raw-output" in result.stdout
    assert "--null-input" in result.stdout
    assert "--from-file" in result.stdout


def test_installed_console_script_entry_point_resolves() -> None:
    """Proves the [project.scripts] entry point itself is wired correctly,
    not just the `-m jsonatapy` invocation used by the rest of this file."""
    jsonatapy_bin = shutil.which("jsonatapy")
    assert jsonatapy_bin is not None, "jsonatapy console script not found on PATH"
    result = subprocess.run(
        [jsonatapy_bin, "--version"], capture_output=True, text=True
    )
    assert result.returncode == 0
    assert "jsonatapy" in result.stdout
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `uv run maturin develop --release` (ensures the editable install picks up the new `[project.scripts]` entry point — required once after adding it, since entry-point registration itself needs a reinstall even though the Python source files don't).
Run: `uv run pytest tests/python/test_cli.py -v`
Expected: PASS — Step 2's `run()` already handles `--version`/`--help` correctly since `argparse`'s `action="version"` and default `add_help=True` both work immediately once the parser is built.

- [ ] **Step 5: Confirm `mypy --strict` and `ruff` are clean on the new files**

Run: `uv run mypy python/jsonatapy/_cli/ python/jsonatapy/__main__.py`
Expected: no errors (`Success: no issues found`).

Run: `uv run ruff check python/jsonatapy/_cli/ python/jsonatapy/__main__.py tests/python/test_cli.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/ python/jsonatapy/__main__.py tests/python/test_cli.py`
Expected: both clean. Fix any issues (most likely import ordering) before proceeding.

- [ ] **Step 6: Commit**

```bash
git add pyproject.toml python/jsonatapy/__main__.py python/jsonatapy/_cli/__init__.py python/jsonatapy/_cli/run.py tests/python/test_cli.py
git commit -m "feat(pycli): scaffold jsonatapy console script with --version/--help"
```

---

### Task 2: `resolve.py` — expression/input source resolution

**Files:**
- Create: `python/jsonatapy/_cli/resolve.py`
- Test: `tests/python/test_cli_resolve.py`

**Interfaces:**
- Produces: `resolve.ExpressionInline(text: str)`, `resolve.ExpressionFile(path: str)`, `resolve.InputStdin()`, `resolve.InputFile(path: str)`, `resolve.InputNull()` (frozen dataclasses), `resolve.ResolveError(Exception)`, `resolve.resolve(from_file: str | None, positional1: str | None, positional2: str | None, null_input: bool) -> tuple[ExpressionSource, InputSource]` — the single source of truth Task 4 (and no other module) consumes for how positionals/`-n`/`-f` interact. Mirrors `src/bin/jsonata/resolve.rs`'s `resolve()` function and its exact truth table.
- Consumes: nothing from other Phase 2 tasks.

- [ ] **Step 1: Write the failing unit tests**

Create `tests/python/test_cli_resolve.py`:

```python
"""Unit tests for jsonatapy._cli.resolve -- the single source of truth for
how CLI positional arguments, --from-file, and --null-input interact.
Mirrors resolve.rs's own test suite in the Rust CLI exactly, case for case.
"""

from __future__ import annotations

import pytest

from jsonatapy._cli.resolve import (
    ExpressionFile,
    ExpressionInline,
    InputFile,
    InputNull,
    InputStdin,
    ResolveError,
    resolve,
)


def test_plain_expression_and_stdin() -> None:
    expr, inp = resolve(from_file=None, positional1="name", positional2=None, null_input=False)
    assert expr == ExpressionInline("name")
    assert inp == InputStdin()


def test_plain_expression_and_file() -> None:
    expr, inp = resolve(
        from_file=None, positional1="name", positional2="data.json", null_input=False
    )
    assert expr == ExpressionInline("name")
    assert inp == InputFile("data.json")


def test_missing_expression_is_an_error() -> None:
    with pytest.raises(ResolveError):
        resolve(from_file=None, positional1=None, positional2=None, null_input=False)


def test_null_input_with_no_data_file_is_null_source() -> None:
    expr, inp = resolve(
        from_file=None, positional1="$now()", positional2=None, null_input=True
    )
    assert expr == ExpressionInline("$now()")
    assert inp == InputNull()


def test_null_input_with_data_file_is_an_error() -> None:
    with pytest.raises(ResolveError):
        resolve(
            from_file=None, positional1="name", positional2="data.json", null_input=True
        )


def test_from_file_shifts_positional1_to_the_data_file() -> None:
    expr, inp = resolve(
        from_file="expr.jsonata", positional1="data.json", positional2=None, null_input=False
    )
    assert expr == ExpressionFile("expr.jsonata")
    assert inp == InputFile("data.json")


def test_from_file_with_no_positionals_reads_stdin() -> None:
    expr, inp = resolve(
        from_file="expr.jsonata", positional1=None, positional2=None, null_input=False
    )
    assert expr == ExpressionFile("expr.jsonata")
    assert inp == InputStdin()


def test_from_file_with_two_positionals_is_an_error() -> None:
    with pytest.raises(ResolveError):
        resolve(
            from_file="expr.jsonata",
            positional1="extra1",
            positional2="extra2",
            null_input=False,
        )
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_cli_resolve.py -v`
Expected: FAIL — `jsonatapy._cli.resolve` doesn't exist yet (`ModuleNotFoundError`).

- [ ] **Step 3: Implement `resolve.py`**

Create `python/jsonatapy/_cli/resolve.py`:

```python
"""Determines the expression source and input source from parsed CLI
arguments. Single source of truth for how positional arguments,
--from-file, and --null-input interact -- mirrors
src/bin/jsonata/resolve.rs::resolve() in the Rust CLI exactly, so both
implementations agree on every case in study/cli_fixtures.json.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ExpressionInline:
    """The expression text came directly from a positional argument."""

    text: str


@dataclass(frozen=True)
class ExpressionFile:
    """The expression text should be read from this file path (--from-file)."""

    path: str


ExpressionSource = ExpressionInline | ExpressionFile


@dataclass(frozen=True)
class InputStdin:
    """Input JSON should be read from stdin."""


@dataclass(frozen=True)
class InputFile:
    """Input JSON should be read from this file path."""

    path: str


@dataclass(frozen=True)
class InputNull:
    """No input is read; the evaluation context is null (see this plan's
    Global Constraints for why this is null, not JSONata Undefined)."""


InputSource = InputStdin | InputFile | InputNull


class ResolveError(Exception):
    """Raised when CLI arguments cannot be resolved into an expression/input source."""


def resolve(
    from_file: str | None,
    positional1: str | None,
    positional2: str | None,
    null_input: bool,
) -> tuple[ExpressionSource, InputSource]:
    """Resolves the expression source and input source from parsed CLI args."""
    data_file: str | None
    expr_source: ExpressionSource

    if from_file is not None:
        if positional2 is not None:
            raise ResolveError(
                "with --from-file, only one positional argument (the input file) is allowed"
            )
        expr_source = ExpressionFile(from_file)
        data_file = positional1
    elif positional1 is not None:
        expr_source = ExpressionInline(positional1)
        data_file = positional2
    else:
        raise ResolveError("missing required argument: EXPRESSION (or use --from-file)")

    if null_input:
        if data_file is not None:
            raise ResolveError(
                "--null-input cannot be combined with an input file argument"
            )
        return expr_source, InputNull()

    if data_file is not None:
        return expr_source, InputFile(data_file)
    return expr_source, InputStdin()
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_cli_resolve.py -v`
Expected: PASS (8/8).

- [ ] **Step 5: `mypy --strict` and `ruff` clean**

Run: `uv run mypy python/jsonatapy/_cli/resolve.py`
Run: `uv run ruff check python/jsonatapy/_cli/resolve.py tests/python/test_cli_resolve.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/resolve.py tests/python/test_cli_resolve.py`
Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add python/jsonatapy/_cli/resolve.py tests/python/test_cli_resolve.py
git commit -m "feat(pycli): add resolve.py, the single source of truth for input/expression resolution"
```

---

### Task 3: Add `evaluate_json_or_none` to the Rust/PyO3 library

**Files:**
- Modify: `src/lib.rs` (add new PyO3-exposed method on `JsonataExpression`, inside the `#[pymethods]` block starting at line 200, placed directly after the existing `evaluate_json` method at lines 285-321)
- Modify: `python/jsonatapy/__init__.py` (add a corresponding thin wrapper method on the public `JsonataExpression` class, mirroring the existing `evaluate_json` wrapper's structure exactly)
- Test: `tests/python/test_evaluate_json_or_none.py`

**Why this touches library code, not just the CLI:** confirmed during planning that neither `JsonataExpression.evaluate()` nor `.evaluate_json()` can distinguish a JSONata `Undefined` result from an explicit JSON `null` result — both collapse to Python `None` / the JSON text `"null"`. Root cause: `src/value.rs`'s `Serialize` impl for `JValue` has `JValue::Undefined => serializer.serialize_none()`, and JSON has no way to represent "undefined" distinct from "null", so any JSON-serialization-based method necessarily renders both as the text `"null"`. This is correct JSON-serialization behavior, not a bug — but it means the Python API has no equivalent of the Rust CLI's `result.is_undefined()` check (`src/evaluator.rs`, used in `src/bin/jsonata/main.rs` before ever serializing). `study/cli_fixtures.json`'s `undefined_result_prints_nothing` case requires this distinction to build a faithful Python CLI; the user explicitly decided (during planning) that the right fix is a small, purely additive Rust method, not a disclosed Python-CLI-only limitation, since this is a genuine capability gap in the public API (JS callers get this distinction for free via native `undefined !== null`; Python currently cannot).

**Interfaces:**
- Produces: `JsonataExpression.evaluate_json_or_none(data: str, bindings: dict[str, Any] | None = None, *, timeout: int | None = None, max_stack_depth: int | None = None, max_sequence_length: int | None = None) -> str | None` — returns `None` specifically when the JSONata result is `Undefined`, or the JSON-serialized result text otherwise (e.g. the literal string `"null"` for an explicit null result). Consumed by Task 4 (Core evaluation) and Task 6 (`--arg`/`--argjson` wiring, since `evaluate_json_or_none` also takes `bindings`).
- Consumes: `run_eval` (existing private helper already used by `evaluate_json`, confirmed at `src/lib.rs:302-321`), `JValue::is_undefined()` (existing, `src/value.rs:52`), `JValue::to_json_string()` (existing, used by `evaluate_json` the same way).

- [ ] **Step 1: Write the failing Python-level test**

Create `tests/python/test_evaluate_json_or_none.py`:

```python
"""Tests for JsonataExpression.evaluate_json_or_none() -- the one new
public API method this project adds in Phase 2, so the Python bindings can
distinguish a JSONata Undefined result from an explicit JSON null result
(both currently collapse to the same value via evaluate()/evaluate_json()).
"""

from __future__ import annotations

import jsonatapy


def test_undefined_result_returns_none() -> None:
    expr = jsonatapy.compile("nonexistent_field")
    assert expr.evaluate_json_or_none('{"a": 1}') is None


def test_explicit_null_result_returns_the_string_null() -> None:
    expr = jsonatapy.compile("nullField")
    assert expr.evaluate_json_or_none('{"nullField": null}') == "null"


def test_normal_result_returns_serialized_json() -> None:
    expr = jsonatapy.compile("a + b")
    assert expr.evaluate_json_or_none('{"a": 1, "b": 2}') == "3"


def test_bindings_are_applied() -> None:
    expr = jsonatapy.compile("$x * 2")
    assert expr.evaluate_json_or_none("{}", {"x": 5}) == "10"


def test_invalid_json_input_raises_value_error() -> None:
    expr = jsonatapy.compile("a")
    try:
        expr.evaluate_json_or_none("not json")
    except ValueError:
        pass
    else:
        raise AssertionError("expected ValueError for invalid JSON input")


def test_evaluation_error_raises_value_error_with_coded_message() -> None:
    expr = jsonatapy.compile("null + 1")
    try:
        expr.evaluate_json_or_none("{}")
    except ValueError as e:
        assert str(e).startswith("T2002:")
    else:
        raise AssertionError("expected ValueError for null + 1")
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_evaluate_json_or_none.py -v`
Expected: FAIL — `AttributeError: 'JsonataExpression' object has no attribute 'evaluate_json_or_none'` (both the Rust-level `_JsonataExpression` and the Python wrapper class are missing the method).

- [ ] **Step 3: Add the Rust method**

In `src/lib.rs`, immediately after the existing `evaluate_json` method (ends at line 321, right before the closing `}` of the `#[pymethods] impl JsonataExpression` block), add:

```rust

    /// Evaluate with JSON string input, distinguishing an Undefined result
    /// (returns Python None) from an explicit JSON null result (returns
    /// the string "null"). evaluate_json() cannot make this distinction --
    /// JSON serialization has no way to represent "undefined" separately
    /// from "null" -- so this method checks the raw evaluated JValue's
    /// is_undefined() BEFORE serializing, exposing the same signal the
    /// Rust CLI (src/bin/jsonata/main.rs) already uses internally.
    ///
    /// # Errors
    ///
    /// Returns ValueError if JSON parsing or evaluation fails
    #[pyo3(signature = (json_str, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
    #[allow(clippy::too_many_arguments)]
    fn evaluate_json_or_none(
        &self,
        py: Python,
        json_str: &str,
        bindings: Option<Py<PyAny>>,
        timeout: Option<u64>,
        max_stack_depth: Option<usize>,
        max_sequence_length: Option<usize>,
    ) -> PyResult<Option<String>> {
        let json_data = JValue::from_json_str(json_str)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;
        let options = evaluator::EvaluatorOptions {
            timeout_ms: timeout.or(self.default_options.timeout_ms),
            max_stack_depth: max_stack_depth.or(self.default_options.max_stack_depth),
            max_sequence_length: max_sequence_length.or(self.default_options.max_sequence_length),
        };
        let result = self.run_eval(py, &json_data, bindings, options)?;
        if result.is_undefined() {
            return Ok(None);
        }
        result
            .to_json_string()
            .map(Some)
            .map_err(|e| PyValueError::new_err(format!("Failed to serialize result: {}", e)))
    }
```

- [ ] **Step 4: Add the Python wrapper method**

In `python/jsonatapy/__init__.py`, add a new method to the `JsonataExpression` class, immediately after the existing `evaluate_json` method:

```python
    def evaluate_json_or_none(
        self,
        json_str: str,
        bindings: dict[str, Any] | None = None,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> str | None:
        """
        Evaluate with JSON string input, distinguishing Undefined from null.

        Unlike evaluate_json(), which serializes both a JSONata Undefined
        result and an explicit JSON null result to the same text "null",
        this method returns None (the Python value) for Undefined and the
        string "null" for an explicit null result.

        Args:
            json_str: Input data as a JSON string
            bindings: Optional additional variable bindings
            timeout: Maximum evaluation time in milliseconds (raises ValueError
                with a D1012 code on timeout). Overrides any default set via
                `compile(timeout=...)` for this call only.
            max_stack_depth: Maximum recursion stack depth (raises ValueError
                with a D1011 code when exceeded). Overrides any compile-time default.
            max_sequence_length: Maximum length of a query-result sequence
                (map/filter/wildcard/descendants/etc; raises ValueError with a
                D2015 code when exceeded). Overrides any compile-time default.

        Returns:
            None if the result is JSONata Undefined, otherwise the result
            as a JSON string (e.g. "null" for an explicit null result).

        Raises:
            ValueError: If JSON parsing or evaluation fails, or a guardrail is exceeded

        Example:
            >>> expr = compile("nonexistent")
            >>> expr.evaluate_json_or_none('{"a": 1}') is None
            True
            >>> expr2 = compile("a")
            >>> expr2.evaluate_json_or_none('{"a": null}')
            'null'
        """
        return self._expr.evaluate_json_or_none(
            json_str, bindings, timeout, max_stack_depth, max_sequence_length
        )
```

- [ ] **Step 5: Rebuild and run tests to verify they pass**

Run: `uv run maturin develop --release` (rebuilds the Rust extension with the new method).
Run: `uv run pytest tests/python/test_evaluate_json_or_none.py -v`
Expected: PASS (6/6).

- [ ] **Step 6: Full regression check — confirm zero behavior change to existing code**

Run: `uv run pytest tests/python/test_reference_suite.py -q`
Expected: PASS, 1682/1682 — proves the new method didn't alter any existing evaluation path (it's purely additive; `evaluate_json`, `evaluate`, etc. are untouched).

Run: `uv run pytest tests/python/ -v` (excluding nothing — full existing suite)
Expected: all pass, same as before this task started.

- [ ] **Step 7: `cargo fmt`/`clippy` clean on the Rust change, `mypy --strict`/`ruff` clean on the Python change**

Run: `cargo fmt --all -- --check`
Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: both clean (this project's standing Rust quality gate, unchanged by Phase 2 but still binding on any `src/` edit).

Run: `uv run mypy python/jsonatapy/__init__.py`
Run: `uv run ruff check python/jsonatapy/__init__.py tests/python/test_evaluate_json_or_none.py`
Run: `uv run ruff format --check python/jsonatapy/__init__.py tests/python/test_evaluate_json_or_none.py`
Expected: all clean.

- [ ] **Step 8: Commit**

```bash
git add src/lib.rs python/jsonatapy/__init__.py tests/python/test_evaluate_json_or_none.py
git commit -m "feat(py-api): add evaluate_json_or_none to distinguish Undefined from null"
```

---

### Task 4: Core evaluation — expression evaluation, default output, undefined/null semantics

**Files:**
- Modify: `python/jsonatapy/_cli/run.py`
- Modify: `tests/python/test_cli.py`

**Interfaces:**
- Consumes: `jsonatapy._cli.resolve.{resolve, ExpressionInline, ExpressionFile, InputStdin, InputFile, InputNull, ResolveError}` (Task 2). `jsonatapy.compile(expression: str) -> JsonataExpression`, `JsonataExpression.evaluate_json_or_none(json_str: str, bindings: dict | None = None) -> str | None` (Task 3).
- Produces: `run.run(argv: list[str]) -> int` fully implemented for the plain-expression/stdin/file/undefined/null path. `--arg`/`--argjson`/`-c`/`-r` are parsed but not yet wired to behavior — later tasks add that.

- [ ] **Step 1: Write the failing tests**

Add to `tests/python/test_cli.py`:

```python
def test_evaluates_expression_against_stdin_json() -> None:
    result = run_cli(["name"], stdin='{"name": "Alice"}')
    assert result.returncode == 0
    assert result.stdout == '"Alice"\n'


def test_evaluates_expression_against_file_argument(tmp_path: "Path") -> None:
    data_file = tmp_path / "data.json"
    data_file.write_text('{"name": "Bob"}')
    result = run_cli(["name", str(data_file)])
    assert result.returncode == 0
    assert result.stdout == '"Bob"\n'


def test_undefined_result_prints_nothing_and_exits_zero() -> None:
    result = run_cli(["nonexistent_field"], stdin='{"a": 1}')
    assert result.returncode == 0
    assert result.stdout == ""


def test_null_result_prints_literal_null() -> None:
    result = run_cli(["nullField"], stdin='{"nullField": null}')
    assert result.returncode == 0
    assert result.stdout == "null\n"


def test_from_file_reads_expression_from_a_file(tmp_path: "Path") -> None:
    expr_file = tmp_path / "expr.jsonata"
    expr_file.write_text("name")
    result = run_cli(["-f", str(expr_file)], stdin='{"name": "Carol"}')
    assert result.returncode == 0
    assert result.stdout == '"Carol"\n'


def test_invalid_json_input_exits_three() -> None:
    result = run_cli(["a"], stdin="not json")
    assert result.returncode == 3
    assert "invalid JSON input" in result.stderr
```

Add the `Path` import at the top of `tests/python/test_cli.py` (needed for the `tmp_path` fixture's type annotation):

```python
from pathlib import Path
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_cli.py -v`
Expected: FAIL — `run()` currently only parses args and returns 0, doing nothing else.

- [ ] **Step 3: Implement expression evaluation, input resolution, and default output**

Replace `run.py` entirely:

```python
"""Core evaluate-mode CLI logic: argument parsing, evaluation, output.

Mirrors src/bin/jsonata/main.rs in the Rust CLI. See study/cli_spec.md for
the full flag/exit-code contract both implementations must satisfy.
"""

from __future__ import annotations

import argparse
import json
import sys

import jsonatapy

from .resolve import (
    ExpressionFile,
    ExpressionInline,
    InputFile,
    InputNull,
    InputStdin,
    ResolveError,
    resolve,
)


def build_parser() -> argparse.ArgumentParser:
    """Builds the argparse parser for evaluate-mode (`jsonatapy [OPTIONS] [EXPR] [FILE]`).

    argparse's default error handling (unknown/malformed flags) already
    prints a usage message to stderr and calls sys.exit(2) -- matching this
    CLI's exit-code-2 usage-error convention with no extra code needed.
    """
    parser = argparse.ArgumentParser(
        prog="jsonatapy",
        description="Evaluate JSONata expressions against JSON data",
    )
    parser.add_argument("-c", "--compact", action="store_true", help="Compact JSON output")
    parser.add_argument(
        "-r", "--raw-output", action="store_true", help="Print string results without quotes"
    )
    parser.add_argument(
        "-n", "--null-input", action="store_true", help="Don't read input; $ is null"
    )
    parser.add_argument(
        "-f",
        "--from-file",
        metavar="FILE",
        default=None,
        help="Read the expression from FILE instead of the first positional argument",
    )
    parser.add_argument(
        "--arg",
        action="append",
        default=[],
        metavar="NAME=VALUE",
        help="Bind $NAME to a string value (repeatable)",
    )
    parser.add_argument(
        "--argjson",
        action="append",
        default=[],
        metavar="NAME=JSON",
        help="Bind $NAME to a parsed JSON value (repeatable)",
    )
    parser.add_argument(
        "-V",
        "--version",
        action="version",
        version=f"jsonatapy {jsonatapy.__version__}",
    )
    parser.add_argument(
        "positional1",
        nargs="?",
        default=None,
        metavar="EXPRESSION_OR_FILE",
        help="The JSONata expression (or, with --from-file, the input data file)",
    )
    parser.add_argument(
        "positional2",
        nargs="?",
        default=None,
        metavar="FILE",
        help="The input data file (used only when --from-file supplies the expression)",
    )
    return parser


def _read_expression(expr_source: ExpressionInline | ExpressionFile) -> str | int:
    """Returns the expression text, or an int exit code on failure."""
    if isinstance(expr_source, ExpressionInline):
        return expr_source.text
    try:
        with open(expr_source.path, encoding="utf-8") as f:
            return f.read()
    except OSError as e:
        print(
            f"error: could not read expression file {expr_source.path}: {e}",
            file=sys.stderr,
        )
        return 2


def _read_input_json(input_source: InputStdin | InputFile | InputNull) -> str | int:
    """Returns the raw input JSON text (or "null" for InputNull), or an int
    exit code on failure. Does NOT parse the JSON itself -- only validates
    it, since evaluate_json_or_none() takes the raw text directly."""
    if isinstance(input_source, InputNull):
        return "null"
    if isinstance(input_source, InputStdin):
        raw = sys.stdin.read()
    else:  # InputFile
        try:
            with open(input_source.path, encoding="utf-8") as f:
                raw = f.read()
        except OSError as e:
            print(
                f"error: could not read input file {input_source.path}: {e}",
                file=sys.stderr,
            )
            return 2
    try:
        json.loads(raw)  # validate only
    except json.JSONDecodeError as e:
        print(f"error: invalid JSON input: {e}", file=sys.stderr)
        return 3
    return raw


def run(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)

    try:
        expr_source, input_source = resolve(
            args.from_file, args.positional1, args.positional2, args.null_input
        )
    except ResolveError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    expression = _read_expression(expr_source)
    if isinstance(expression, int):
        return expression

    input_json = _read_input_json(input_source)
    if isinstance(input_json, int):
        return input_json

    try:
        expr = jsonatapy.compile(expression)
    except ValueError as e:
        print(str(e), file=sys.stderr)
        return 1

    try:
        result_json = expr.evaluate_json_or_none(input_json)
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    if result_json is None:
        return 0  # Undefined result: print nothing

    print(result_json)
    return 0
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_cli.py -v`
Expected: PASS (all tests, including Task 1's).

- [ ] **Step 5: `mypy --strict` and `ruff` clean**

Run: `uv run mypy python/jsonatapy/_cli/run.py`
Run: `uv run ruff check python/jsonatapy/_cli/run.py tests/python/test_cli.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/run.py tests/python/test_cli.py`
Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add python/jsonatapy/_cli/run.py tests/python/test_cli.py
git commit -m "feat(pycli): evaluate expressions against stdin/file JSON input via evaluate_json_or_none"
```

---

### Task 5: `-c/--compact` and `-r/--raw-output`

**Files:**
- Modify: `python/jsonatapy/_cli/run.py`
- Modify: `tests/python/test_cli.py`

**Interfaces:**
- Consumes: `args.compact: bool`, `args.raw_output: bool` (already parsed since Task 1).
- Produces: `print_result(result_json: str, compact: bool, raw_output: bool) -> None`, extracted from `run()`'s tail so output formatting is independently testable/extensible.

- [ ] **Step 1: Write the failing tests**

Add to `tests/python/test_cli.py`:

```python
def test_compact_flag_produces_single_line_output() -> None:
    result = run_cli(["-c", '{"x": a}'], stdin='{"a": 1}')
    assert result.returncode == 0
    assert result.stdout == '{"x":1}\n'


def test_raw_output_flag_strips_quotes_from_string_results() -> None:
    result = run_cli(["-r", "name"], stdin='{"name": "Alice"}')
    assert result.returncode == 0
    assert result.stdout == "Alice\n"


def test_raw_output_flag_does_not_affect_non_string_results() -> None:
    result = run_cli(["-r", "-c", "items"], stdin='{"items": [1, 2, 3]}')
    assert result.returncode == 0
    assert result.stdout == "[1,2,3]\n"
```

Note: the array-valued case (`[1, 2, 3]`) is used here rather than a bare number, specifically to have stronger discriminating power than a scalar would (a broken `-r` that also strips array formatting would be caught by this test) — this closes a gap flagged during Phase 1's final review, where the Rust CLI's equivalent test only used a number.

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_cli.py -v`
Expected: FAIL — `evaluate_json_or_none()`'s return text is used as-is regardless of `-c`/`-r`, and Task 4's default (non-`-c`) output is not pretty-printed (it's whatever `to_json_string()` produces on the Rust side, which is compact, per the existing `evaluate_json` pattern this method mirrors).

- [ ] **Step 3: Extract `print_result` and wire up `-c`/`-r`**

In `python/jsonatapy/_cli/run.py`, replace the tail of `run()` (from `try: result_json = expr.evaluate_json_or_none(input_json)` through the end of the function) with:

```python
    try:
        result_json = expr.evaluate_json_or_none(input_json)
    except ValueError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1

    if result_json is None:
        return 0  # Undefined result: print nothing

    print_result(result_json, args.compact, args.raw_output)
    return 0
```

Add a new function below `run()`:

```python
def print_result(result_json: str, compact: bool, raw_output: bool) -> None:
    """Prints a non-undefined evaluate_json_or_none() result per -c/-r flags.

    `result_json` is already valid JSON text -- this function only handles
    presentation (raw string unquoting, pretty vs compact), it never
    re-evaluates or re-validates the JSON.
    """
    value = json.loads(result_json)

    if raw_output and isinstance(value, str):
        print(value)
        return

    if compact:
        print(json.dumps(value, separators=(",", ":")))
    else:
        print(json.dumps(value, indent=2))
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_cli.py -v`
Expected: PASS (all tests, including Task 4's).

- [ ] **Step 5: `mypy --strict` and `ruff` clean**

Run: `uv run mypy python/jsonatapy/_cli/run.py`
Run: `uv run ruff check python/jsonatapy/_cli/run.py tests/python/test_cli.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/run.py tests/python/test_cli.py`
Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add python/jsonatapy/_cli/run.py tests/python/test_cli.py
git commit -m "feat(pycli): add -c/--compact and -r/--raw-output flags"
```

---

### Task 6: `--arg`/`--argjson` bindings and CLI-only error formatting

**Files:**
- Create: `python/jsonatapy/_cli/bindings.py`
- Create: `python/jsonatapy/_cli/error_format.py`
- Modify: `python/jsonatapy/_cli/run.py`
- Modify: `tests/python/test_cli.py`

**Interfaces:**
- Produces: `bindings.parse_bindings(arg: list[str], argjson: list[str]) -> dict[str, Any]`, `bindings.BindingError(Exception)`. `error_format.format_evaluation_error(message: str) -> str`.
- Consumes: `jsonatapy._cli.resolve` (Task 2), `jsonatapy._cli.run.run` (Tasks 4-5), `JsonataExpression.evaluate_json_or_none(json_str, bindings)` (Task 3, `bindings` parameter used for the first time).

- [ ] **Step 1: Write the failing unit tests for binding parsing**

Create `tests/python/test_cli_bindings.py`:

```python
"""Unit tests for jsonatapy._cli.bindings."""

from __future__ import annotations

import pytest

from jsonatapy._cli.bindings import BindingError, parse_bindings


def test_arg_binds_a_string() -> None:
    b = parse_bindings(["region=us"], [])
    assert b == {"region": "us"}


def test_argjson_binds_a_parsed_value() -> None:
    b = parse_bindings([], ["limit=42"])
    assert b == {"limit": 42}


def test_arg_without_equals_is_an_error() -> None:
    with pytest.raises(BindingError):
        parse_bindings(["justaname"], [])


def test_argjson_with_invalid_json_is_an_error() -> None:
    with pytest.raises(BindingError):
        parse_bindings([], ["x=not json"])


def test_arg_value_may_contain_equals_signs() -> None:
    b = parse_bindings(["eq=a=b"], [])
    assert b == {"eq": "a=b"}
```

- [ ] **Step 2: Write the failing unit tests for error formatting**

Create `tests/python/test_cli_error_format.py`:

```python
"""Unit tests for jsonatapy._cli.error_format."""

from __future__ import annotations

from jsonatapy._cli.error_format import format_evaluation_error


def test_coded_evaluation_error_passes_through_unwrapped() -> None:
    msg = "T2002: The left side of the + operator must evaluate to a number"
    assert format_evaluation_error(msg) == msg


def test_uncoded_evaluation_error_gets_error_prefix() -> None:
    assert format_evaluation_error("Unknown function: undefinedvar") == (
        "error: Unknown function: undefinedvar"
    )
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_cli_bindings.py tests/python/test_cli_error_format.py -v`
Expected: FAIL — neither module exists yet (`ModuleNotFoundError`).

- [ ] **Step 4: Implement `bindings.py` and `error_format.py`**

Create `python/jsonatapy/_cli/bindings.py`:

```python
"""Parses --arg/--argjson CLI specs into a name -> value map for
evaluate_json_or_none() bindings."""

from __future__ import annotations

import json
from typing import Any


class BindingError(Exception):
    """Raised when an --arg/--argjson spec is malformed."""


def parse_bindings(arg: list[str], argjson: list[str]) -> dict[str, Any]:
    """Parses --arg NAME=VALUE (string) and --argjson NAME=JSON (parsed) specs
    into a single name -> value dict. --argjson wins on name collision
    (applied second, matching src/bin/jsonata/bindings.rs's iteration order)."""
    bindings: dict[str, Any] = {}
    for spec in arg:
        name, value = _split_name_value(spec, "--arg")
        bindings[name] = value
    for spec in argjson:
        name, value = _split_name_value(spec, "--argjson")
        try:
            bindings[name] = json.loads(value)
        except json.JSONDecodeError as e:
            raise BindingError(f"--argjson {name}: invalid JSON value: {e}") from e
    return bindings


def _split_name_value(spec: str, flag: str) -> tuple[str, str]:
    if "=" not in spec:
        raise BindingError(f"{flag} expects NAME=VALUE, got: {spec}")
    name, _, value = spec.partition("=")
    if not name:
        raise BindingError(f"{flag} expects NAME=VALUE, got: {spec}")
    return name, value
```

Create `python/jsonatapy/_cli/error_format.py`:

```python
"""CLI-only error-message presentation for evaluate_json_or_none() failures.

jsonatapy.compile()'s ValueError messages are already fully formatted
(matching Rust's ParserError::display_message()) and need no processing --
callers print str(exc) directly. JsonataExpression.evaluate_json_or_none()'s
ValueError messages are the raw unwrapped text (matching Rust's
EvaluatorError::message()): already spec-coded ("T2002: ...") when
applicable, otherwise plain. This module adds the CLI's "error: " prefix
only when it's not already coded -- mirrors src/bin/jsonata/error_format.rs
exactly.
"""

from __future__ import annotations

import re

_CODE_PREFIX_RE = re.compile(r"^[TDUS]\d{4}:")


def format_evaluation_error(message: str) -> str:
    """Formats an evaluate_json_or_none()-raised ValueError's message for
    CLI stderr output."""
    if _CODE_PREFIX_RE.match(message):
        return message
    return f"error: {message}"
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_cli_bindings.py tests/python/test_cli_error_format.py -v`
Expected: PASS (5/5 + 2/2).

- [ ] **Step 6: Write the failing black-box tests**

Add to `tests/python/test_cli.py`:

```python
def test_arg_binds_a_string_variable() -> None:
    result = run_cli(["--arg", "region=us", "$region"], stdin="{}")
    assert result.returncode == 0
    assert result.stdout == '"us"\n'


def test_argjson_binds_a_json_variable() -> None:
    result = run_cli(["--argjson", "limit=5", "$limit * 2"], stdin="{}")
    assert result.returncode == 0
    assert result.stdout == "10\n"


def test_malformed_arg_binding_is_a_usage_error() -> None:
    result = run_cli(["--arg", "noequalssign", "$x"], stdin="{}")
    assert result.returncode == 2


def test_evaluation_error_preserves_jsonata_error_code() -> None:
    result = run_cli(["null + 1"], stdin="{}")
    assert result.returncode == 1
    assert result.stderr.startswith("T2002:")


def test_malformed_arg_binding_takes_precedence_over_parse_error() -> None:
    """Mirrors the Rust CLI's exit-code-precedence fix from the Phase 1
    final review: a malformed --arg must exit 2 even if the expression
    would also fail to parse."""
    result = run_cli(["--arg", "bad", "a["], stdin="{}")
    assert result.returncode == 2


def test_malformed_arg_binding_takes_precedence_over_invalid_json_input() -> None:
    result = run_cli(["--arg", "bad", "a"], stdin="not json")
    assert result.returncode == 2
```

- [ ] **Step 7: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_cli.py -v`
Expected: FAIL — `--arg`/`--argjson` are parsed by argparse but never applied; evaluation errors currently print with `f"error: {e}"` unconditionally (no code-prefix check). Verify `test_evaluation_error_preserves_jsonata_error_code` genuinely distinguishes the fix (it checks `startswith("T2002:")`, which fails while the current code unconditionally prepends `"error: "`).

- [ ] **Step 8: Wire bindings and error formatting into `run()`**

In `python/jsonatapy/_cli/run.py`, add the import at the top:

```python
from .bindings import BindingError, parse_bindings
from .error_format import format_evaluation_error
```

Replace `run()`'s body from `try: expr_source, input_source = resolve(...)` through the end with:

```python
def run(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)

    try:
        expr_source, input_source = resolve(
            args.from_file, args.positional1, args.positional2, args.null_input
        )
    except ResolveError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    try:
        var_bindings = parse_bindings(args.arg, args.argjson)
    except BindingError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    expression = _read_expression(expr_source)
    if isinstance(expression, int):
        return expression

    input_json = _read_input_json(input_source)
    if isinstance(input_json, int):
        return input_json

    try:
        expr = jsonatapy.compile(expression)
    except ValueError as e:
        print(str(e), file=sys.stderr)
        return 1

    try:
        result_json = expr.evaluate_json_or_none(input_json, var_bindings or None)
    except ValueError as e:
        print(format_evaluation_error(str(e)), file=sys.stderr)
        return 1

    if result_json is None:
        return 0  # Undefined result: print nothing

    print_result(result_json, args.compact, args.raw_output)
    return 0
```

Note the binding-validation (`parse_bindings`) now happens immediately after `resolve()`, before reading the expression file or input — same precedence rule as the Rust CLI's Phase-1-final-review fix, verified by Step 6's two precedence tests.

- [ ] **Step 9: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_cli.py tests/python/test_cli_bindings.py tests/python/test_cli_error_format.py tests/python/test_cli_resolve.py -v`
Expected: PASS (all tests across all four files).

- [ ] **Step 10: `mypy --strict` and `ruff` clean**

Run: `uv run mypy python/jsonatapy/_cli/`
Run: `uv run ruff check python/jsonatapy/_cli/ tests/python/test_cli.py tests/python/test_cli_bindings.py tests/python/test_cli_error_format.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/ tests/python/test_cli.py tests/python/test_cli_bindings.py tests/python/test_cli_error_format.py`
Expected: all clean.

- [ ] **Step 11: Commit**

```bash
git add python/jsonatapy/_cli/run.py python/jsonatapy/_cli/bindings.py python/jsonatapy/_cli/error_format.py tests/python/test_cli.py tests/python/test_cli_bindings.py tests/python/test_cli_error_format.py
git commit -m "feat(pycli): add --arg/--argjson bindings and code-preserving error formatting"
```

---

### Task 7: Exit-code contract coverage + the disclosed null-vs-undefined-context limitation test

**Files:**
- Modify: `tests/python/test_cli.py`
- Modify: `study/cli_spec.md`

**Interfaces:**
- No new interfaces — verification pass over the exit-code contract, plus one test that pins the Global Constraints' disclosed Python-specific `-n` **context** limitation (distinct from Task 3's result-side fix) so it's tracked, not silently wrong.

- [ ] **Step 1: Add the remaining exit-code edge-case tests**

Add to `tests/python/test_cli.py`:

```python
def test_parse_error_exits_one() -> None:
    result = run_cli(["a["], stdin="{}")
    assert result.returncode == 1


def test_missing_expression_argument_exits_two() -> None:
    result = run_cli([])
    assert result.returncode == 2


def test_nonexistent_input_file_exits_two() -> None:
    result = run_cli(["a", "/nonexistent/path/data.json"])
    assert result.returncode == 2
    assert "could not read input file" in result.stderr


def test_unknown_flag_exits_two_via_argparse_default() -> None:
    result = run_cli(["--not-a-real-flag", "a"])
    assert result.returncode == 2
```

- [ ] **Step 2: Add the disclosed null-vs-undefined-context limitation test**

Add to `tests/python/test_cli.py`:

```python
def test_null_input_uses_null_not_undefined_context_known_divergence() -> None:
    """Documents a known, disclosed divergence from the Rust CLI (see this
    plan's Global Constraints): the Python jsonatapy API has no way to
    construct a true JSONata Undefined top-level CONTEXT (input) -- this is
    distinct from Task 3's result-side fix (evaluate_json_or_none), which
    already resolved the RESULT-side undefined/null ambiguity. -n passes a
    null context instead of Undefined. Unobservable for expressions that
    don't reference $ (the common -n use case), but $exists($) distinguishes
    them -- the Rust CLI's -n gives $exists($) == false (context is
    Undefined), while this Python CLI's -n gives $exists($) == true
    (context is null, and $exists(null) is true in JSONata). If this test
    ever starts failing because the divergence was fixed, delete it and
    update study/cli_spec.md's Python-specific notes accordingly."""
    result = run_cli(["-n", "$exists($)"])
    assert result.returncode == 0
    assert result.stdout == "true\n"
```

- [ ] **Step 3: Run all CLI tests and confirm they pass**

Run: `uv run pytest tests/python/test_cli.py -v`
Expected: PASS. If any exit-code test fails, it's a gap in Tasks 4-6's implementation — fix the specific `run.py` branch that doesn't match `study/cli_spec.md`'s exit-code table before proceeding. If `test_null_input_uses_null_not_undefined_context_known_divergence` fails with `$exists($)` returning `false` instead of `true`, that means the divergence doesn't actually exist the way this plan predicted — stop and report this to the controller rather than adjusting the test to match, since it would mean the Global Constraints section's grounding was wrong somewhere.

- [ ] **Step 4: Add the Python-specific note to `study/cli_spec.md`**

Add a new section at the end of `study/cli_spec.md` (append, don't modify existing content):

```markdown

## Python (`jsonatapy`) implementation notes

The Python CLI (`jsonatapy`, this same package's console script) implements
this exact contract, using a new library method, `evaluate_json_or_none()`
(added in Phase 2 specifically for this purpose), to correctly distinguish
an Undefined *result* from an explicit null result -- both `evaluate()` and
`evaluate_json()` collapse that distinction, `evaluate_json_or_none()`
does not.

One disclosed divergence remains, on the **input/context** side rather than
the result side:

- **`-n`/`--null-input` uses a `null` evaluation context, not JSONata
  `Undefined`.** The public `jsonatapy` Python API has no way to construct
  a true `Undefined` top-level context (`None` always maps to `Null` — see
  `python_to_json` in `src/lib.rs`). This is unobservable for expressions
  that don't reference `$` (the common `-n` use case: `-n '1 + 1'`,
  `-n '$now()'`), but is observable for ones that do — e.g. `$exists($)`
  returns `false` under the Rust CLI's `-n` (true `Undefined`) and `true`
  under the Python CLI's `-n` (`null`, and `$exists(null)` is `true`).
  Pinned by `test_null_input_uses_null_not_undefined_context_known_divergence`
  in `tests/python/test_cli.py`.
```

- [ ] **Step 5: `mypy --strict` and `ruff` clean, then commit**

Run: `uv run mypy python/jsonatapy/_cli/`
Run: `uv run ruff check tests/python/test_cli.py`
Run: `uv run ruff format --check tests/python/test_cli.py`
Expected: all clean.

```bash
git add tests/python/test_cli.py study/cli_spec.md
git commit -m "test(pycli): close out exit-code contract coverage; document null-context divergence"
```

---

### Task 8: Cross-language fixture parity — run `study/cli_fixtures.json` against the Python CLI

**Files:**
- Create: `tests/python/test_cli_fixtures.py`

**Interfaces:**
- Produces: nothing new — this is a data-driven consumer of `study/cli_fixtures.json` (already committed from Phase 1), proving the Python CLI agrees with the same shared fixture suite the Rust CLI is tested against.

- [ ] **Step 1: Write the fixture-runner test**

Create `tests/python/test_cli_fixtures.py`:

```python
"""Runs study/cli_fixtures.json against the jsonatapy CLI -- the same
shared, language-agnostic fixture suite tests/cli_fixtures_test.rs runs
against the Rust jsonata CLI. Both must agree on every case.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

FIXTURES_PATH = Path(__file__).parent.parent.parent / "study" / "cli_fixtures.json"


def _load_fixtures() -> list[dict[str, Any]]:
    with open(FIXTURES_PATH, encoding="utf-8") as f:
        data: list[dict[str, Any]] = json.load(f)
    return data


def test_all_fixtures_pass() -> None:
    fixtures = _load_fixtures()
    assert fixtures, "study/cli_fixtures.json must not be empty"

    failures: list[str] = []

    for fixture in fixtures:
        name = fixture["name"]
        args = fixture["args"]
        stdin = fixture["stdin"]
        expected_exit = fixture["expected_exit"]
        expected_stdout = fixture["expected_stdout"]
        expected_stderr_contains = fixture["expected_stderr_contains"]

        result = subprocess.run(
            [sys.executable, "-m", "jsonatapy", *args],
            input=stdin,
            capture_output=True,
            text=True,
        )

        if result.returncode != expected_exit:
            failures.append(
                f"{name}: expected exit {expected_exit}, got {result.returncode} "
                f"(stderr: {result.stderr!r})"
            )
            continue

        if expected_stdout is not None and result.stdout != expected_stdout:
            failures.append(
                f"{name}: expected stdout {expected_stdout!r}, got {result.stdout!r}"
            )

        if (
            expected_stderr_contains is not None
            and expected_stderr_contains not in result.stderr
        ):
            failures.append(
                f"{name}: expected stderr to contain {expected_stderr_contains!r}, "
                f"got {result.stderr!r}"
            )

    assert not failures, "fixture failures:\n" + "\n".join(failures)
```

- [ ] **Step 2: Run the fixture test**

Run: `uv run pytest tests/python/test_cli_fixtures.py -v`
Expected: PASS — every fixture case re-expresses behavior already implemented and tested in Tasks 4-7. If it fails, that's a genuine divergence between the Python and Rust CLIs — fix `python/jsonatapy/_cli/` to match the fixture (the fixture is the shared source of truth both languages must agree with), do not edit `study/cli_fixtures.json` to make Python's actual behavior look correct (that file already passed Rust's identical review in Phase 1; a failure here means Python, not the fixture, has the bug).

If `evaluation_error_preserves_error_code`/`null_input_with_data_file_conflict_is_exit_2` fail (the two cases Phase 1's review found and fixed once already for the Rust side), read their current content in `study/cli_fixtures.json` first — they should already be the corrected versions (expression `null + 1` / a real data-file argument), not the original broken versions, since this branch is stacked on top of Phase 1's fully-reviewed tip.

- [ ] **Step 3: `mypy --strict` and `ruff` clean**

Run: `uv run mypy tests/python/test_cli_fixtures.py`
Run: `uv run ruff check tests/python/test_cli_fixtures.py`
Run: `uv run ruff format --check tests/python/test_cli_fixtures.py`
Expected: all clean.

- [ ] **Step 4: Commit**

```bash
git add tests/python/test_cli_fixtures.py
git commit -m "test(pycli): run the shared study/cli_fixtures.json suite against the Python CLI"
```

---

### Task 9: FastMCP server — `evaluate`, `validate`, `evaluate_batch` tools + `jsonatapy mcp` dispatch

**Files:**
- Create: `python/jsonatapy/_cli/mcp_server.py`
- Modify: `python/jsonatapy/__main__.py`
- Create: `tests/python/test_mcp_server.py`

**Interfaces:**
- Produces: `mcp_server.create_server() -> FastMCP` (builds and returns the configured server with tools registered — `explain` is added in Task 10, this task adds the other three), `mcp_server.serve(http: bool, port: int | None) -> None` (runs the server, stdio or http transport). `__main__.main()` gains `jsonatapy mcp [--http] [--port N]` dispatch.
- Consumes: `jsonatapy.compile`, `JsonataExpression.evaluate_json_or_none` (Task 3). `python/jsonatapy/_cli/error_format.py` (Task 6) for consistent error text in tool responses.

- [ ] **Step 1: Write the failing in-memory server tests**

Create `tests/python/test_mcp_server.py`:

```python
"""In-memory tests for the jsonatapy MCP server -- uses FastMCP's own
in-process Client, no subprocess or network involved. Requires the `mcp`
extra (fastmcp) to be installed; skipped entirely if it isn't.
"""

from __future__ import annotations

import pytest

fastmcp = pytest.importorskip("fastmcp")

from fastmcp import Client  # noqa: E402

from jsonatapy._cli.mcp_server import create_server  # noqa: E402


async def test_evaluate_tool_returns_json_result() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate", {"expression": "a + b", "data": '{"a": 1, "b": 2}'}
        )
        assert result.data == "3"


async def test_evaluate_tool_with_bindings() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate",
            {"expression": "$x * 2", "data": "{}", "bindings": {"x": 5}},
        )
        assert result.data == "10"


async def test_evaluate_tool_undefined_result_returns_empty_string() -> None:
    """MCP tool return types must be JSON-representable, so the
    Undefined-vs-null distinction evaluate_json_or_none() provides in
    Python (None vs "null") is re-flattened to an empty string here (None
    isn't a valid MCP tool string return) -- an empty string is
    distinguishable from the text "null" for a caller checking the result."""
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate", {"expression": "nonexistent", "data": "{}"}
        )
        assert result.data == ""


async def test_evaluate_tool_raises_tool_error_on_evaluation_failure() -> None:
    from fastmcp.exceptions import ToolError

    server = create_server()
    async with Client(server) as client:
        with pytest.raises(ToolError):
            await client.call_tool("evaluate", {"expression": "null + 1", "data": "{}"})


async def test_validate_tool_reports_ok_for_valid_expression() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("validate", {"expression": "a.b.c"})
        assert result.data == {"ok": True}


async def test_validate_tool_reports_error_for_invalid_expression() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("validate", {"expression": "a["})
        assert result.data["ok"] is False
        assert "Parse error" in result.data["error"]


async def test_evaluate_batch_runs_multiple_expressions() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate_batch",
            {"expressions": ["a", "b", "a + b"], "data": '{"a": 1, "b": 2}'},
        )
        assert result.data == ["1", "2", "3"]


async def test_evaluate_batch_reports_per_expression_errors_without_failing_the_batch() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate_batch",
            {"expressions": ["a", "null + 1"], "data": '{"a": 1}'},
        )
        assert result.data[0] == "1"
        assert "T2002:" in result.data[1]
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv sync --extra dev` (installs `fastmcp`/`pytest-asyncio` from Task 1's `pyproject.toml` changes).
Run: `uv run pytest tests/python/test_mcp_server.py -v`
Expected: FAIL — `jsonatapy._cli.mcp_server` doesn't exist yet.

- [ ] **Step 3: Implement `mcp_server.py`**

Create `python/jsonatapy/_cli/mcp_server.py`:

```python
"""FastMCP server exposing JSONata evaluation as MCP tools for agentic use.

Four tools: evaluate, validate, explain (Task 10), evaluate_batch. See the
design spec's Phase 2 section for the tool contract. This module imports
`fastmcp` at module level -- callers (jsonatapy.__main__) must catch
ImportError around importing THIS MODULE, not around individual calls,
since fastmcp is an optional dependency (the `mcp` extra).
"""

from __future__ import annotations

from typing import Any

from fastmcp import FastMCP
from fastmcp.exceptions import ToolError

import jsonatapy

from .error_format import format_evaluation_error


def create_server() -> FastMCP[Any]:
    mcp: FastMCP[Any] = FastMCP(name="jsonatapy")

    @mcp.tool
    def evaluate(
        expression: str, data: str, bindings: dict[str, Any] | None = None
    ) -> str:
        """Evaluate a JSONata expression against a JSON document.

        Args:
            expression: A JSONata expression string.
            data: The input document as a JSON string.
            bindings: Optional variable bindings (name -> JSON-compatible value).

        Returns:
            The result as a JSON string. Empty string means the JSONata
            result was Undefined (no match) -- distinct from the text
            "null", which means an explicit null result.
        """
        try:
            expr = jsonatapy.compile(expression)
        except ValueError as e:
            raise ToolError(str(e)) from e
        try:
            result = expr.evaluate_json_or_none(data, bindings)
        except ValueError as e:
            raise ToolError(format_evaluation_error(str(e))) from e
        return result if result is not None else ""

    @mcp.tool
    def validate(expression: str) -> dict[str, Any]:
        """Check whether a JSONata expression parses without evaluating it.

        Args:
            expression: A JSONata expression string.

        Returns:
            {"ok": True} if the expression parses, or
            {"ok": False, "error": "<message>"} if it doesn't. The error
            message has no structured position field -- see this plan's
            Global Constraints for why.
        """
        try:
            jsonatapy.compile(expression)
        except ValueError as e:
            return {"ok": False, "error": str(e)}
        return {"ok": True}

    @mcp.tool
    def evaluate_batch(expressions: list[str], data: str) -> list[str]:
        """Evaluate multiple JSONata expressions against the same document
        in one call, avoiding N round-trips.

        Args:
            expressions: A list of JSONata expression strings.
            data: The input document as a JSON string, shared by all expressions.

        Returns:
            One result per expression, in order (empty string for an
            Undefined result). A failed expression's entry is its
            formatted error message (same format as `evaluate`'s ToolError
            text) rather than aborting the whole batch -- callers
            distinguish a result from an error by attempting to
            json.loads() it, or by cross-referencing against `validate` first.
        """
        results: list[str] = []
        for expression in expressions:
            try:
                expr = jsonatapy.compile(expression)
                result = expr.evaluate_json_or_none(data)
                results.append(result if result is not None else "")
            except ValueError as e:
                results.append(format_evaluation_error(str(e)))
        return results

    return mcp


def serve(http: bool, port: int | None) -> None:
    """Runs the MCP server. stdio transport by default; HTTP if http=True."""
    server = create_server()
    if http:
        server.run(transport="http", host="127.0.0.1", port=port or 8000)
    else:
        server.run()
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_mcp_server.py -v`
Expected: PASS (8/8).

Note on `evaluate_batch`'s error-reporting design: unlike `evaluate` (which raises `ToolError` on failure, since it's a single call and the caller wants a clean failure signal), `evaluate_batch` returns per-expression error text inline in the results list rather than aborting the whole batch on the first failure — this is a deliberate design choice (partial results are more useful to an agent processing many expressions than an all-or-nothing failure), not an oversight. `test_evaluate_batch_reports_per_expression_errors_without_failing_the_batch` pins this behavior.

- [ ] **Step 5: Wire `jsonatapy mcp` dispatch into `__main__.py`**

Replace `python/jsonatapy/__main__.py` entirely:

```python
"""Console-script entry point for the `jsonatapy` CLI.

Dispatches to the MCP server subcommand (`jsonatapy mcp ...`) or evaluate
mode (everything else). See study/cli_spec.md for the full contract.
"""

from __future__ import annotations

import argparse
import sys

from ._cli.run import run


def _run_mcp(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="jsonatapy mcp", description="Run the JSONata MCP server")
    parser.add_argument("--http", action="store_true", help="Serve over HTTP instead of stdio")
    parser.add_argument("--port", type=int, default=None, metavar="N", help="HTTP port (default 8000)")
    args = parser.parse_args(argv)

    try:
        from ._cli.mcp_server import serve
    except ImportError:
        print(
            "error: the 'mcp' extra is not installed. Run:\n"
            '  uvx --from "jsonatapy[mcp]" jsonatapy mcp\n'
            "or: pip install jsonatapy[mcp]",
            file=sys.stderr,
        )
        return 2

    serve(args.http, args.port)
    return 0


def main() -> int:
    argv = sys.argv[1:]
    if argv[:1] == ["mcp"]:
        return _run_mcp(argv[1:])
    return run(argv)


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 6: Write and run the ImportError-hint test**

Add to `tests/python/test_cli.py`:

```python
def test_mcp_subcommand_dispatches_without_crashing_on_missing_fastmcp(
    monkeypatch: "pytest.MonkeyPatch",
) -> None:
    """Simulates fastmcp not being installed by making the import fail,
    without needing to actually uninstall it from the test environment."""
    import builtins

    real_import = builtins.__import__

    def fake_import(name: str, *args: object, **kwargs: object) -> object:
        if name == "fastmcp" or name.startswith("fastmcp."):
            raise ImportError("No module named 'fastmcp'")
        return real_import(name, *args, **kwargs)  # type: ignore[arg-type]

    monkeypatch.setattr(builtins, "__import__", fake_import)

    from jsonatapy.__main__ import _run_mcp

    exit_code = _run_mcp([])
    assert exit_code == 2
```

Add `import pytest` near the top of `tests/python/test_cli.py` if not already present (needed for the `pytest.MonkeyPatch` type annotation).

Run: `uv run pytest tests/python/test_cli.py::test_mcp_subcommand_dispatches_without_crashing_on_missing_fastmcp -v`
Expected: PASS.

- [ ] **Step 7: `mypy --strict` and `ruff` clean**

Run: `uv run mypy python/jsonatapy/_cli/mcp_server.py python/jsonatapy/__main__.py`
Run: `uv run ruff check python/jsonatapy/_cli/mcp_server.py python/jsonatapy/__main__.py tests/python/test_mcp_server.py tests/python/test_cli.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/mcp_server.py python/jsonatapy/__main__.py tests/python/test_mcp_server.py tests/python/test_cli.py`
Expected: all clean. If mypy flags `FastMCP[Any]`'s generic parameter or the `@mcp.tool`-decorated functions' inferred types, consult `fastmcp`'s own type stubs (it ships typed) rather than adding `# type: ignore` blindly — the decorator is designed to preserve the wrapped function's signature for type checkers.

- [ ] **Step 8: Commit**

```bash
git add python/jsonatapy/_cli/mcp_server.py python/jsonatapy/__main__.py tests/python/test_mcp_server.py tests/python/test_cli.py
git commit -m "feat(pycli): add FastMCP server (evaluate/validate/evaluate_batch) and jsonatapy mcp dispatch"
```

---

### Task 10: `explain` tool — curated JSONata reference content

**Files:**
- Modify: `python/jsonatapy/_cli/mcp_server.py`
- Modify: `tests/python/test_mcp_server.py`

**Interfaces:**
- Produces: `explain` tool added to `create_server()`'s registered tools.
- Consumes: nothing new — this task is primarily content authoring, grounded against this crate's actual implemented function list (verified below), not a generic/hallucinated JSONata reference.

**Content grounding:** the function names below were extracted directly from `src/evaluator.rs`'s function-dispatch match arms during planning (`grep -noP '^\s*"\w+"\s*=>' src/evaluator.rs`) — every name listed is confirmed implemented in this crate, not assumed from general JSONata knowledge. Do not add functions to the content below that aren't in this list without first confirming they're implemented (grep `src/evaluator.rs` and `src/functions.rs` for the name in quotes).

- [ ] **Step 1: Write the failing tests**

Add to `tests/python/test_mcp_server.py`:

```python
async def test_explain_with_no_topic_returns_function_index() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("explain", {"topic": None})
        assert "$sum" in result.data
        assert "$filter" in result.data


async def test_explain_with_specific_topic_returns_that_section() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("explain", {"topic": "string"})
        assert "$uppercase" in result.data
        assert "$substring" in result.data


async def test_explain_with_unknown_topic_lists_available_topics() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("explain", {"topic": "not-a-real-topic"})
        assert "unknown topic" in result.data.lower()
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `uv run pytest tests/python/test_mcp_server.py -k explain -v`
Expected: FAIL — no `explain` tool registered yet.

- [ ] **Step 3: Add the curated reference content and `explain` tool**

Add to `python/jsonatapy/_cli/mcp_server.py`, above `def create_server()`:

```python
# Curated JSONata function reference, grouped by category. Every function
# name here is confirmed implemented in this crate (grep src/evaluator.rs's
# dispatch match arms for the quoted name to re-verify if this list is ever
# extended). Kept concise deliberately -- this content is reused as-is for
# the Phase 3 agentic study's "jsonata+docs" condition cheatsheet, where
# token cost directly affects the study's own measurements.
_REFERENCE: dict[str, str] = {
    "string": (
        "String functions: $string(v) convert to string, $length(s), "
        "$substring(s,start,len?), $substringBefore(s,chars), "
        "$substringAfter(s,chars), $uppercase(s), $lowercase(s), $trim(s), "
        "$pad(s,width,char?), $contains(s,pattern), $split(s,sep,limit?), "
        "$join(arr,sep?), $match(s,pattern,limit?), $replace(s,pattern,repl,limit?)."
    ),
    "numeric": (
        "Numeric functions: $number(v), $abs(n), $floor(n), $ceil(n), "
        "$round(n,precision?), $power(base,exp), $sqrt(n), "
        "$formatNumber(n,picture,options?), $formatBase(n,radix?), "
        "$formatInteger(n,picture), $parseInteger(s,picture)."
    ),
    "aggregation": (
        "Aggregation functions (operate on arrays): $sum(arr), $max(arr), "
        "$min(arr), $average(arr), $count(arr)."
    ),
    "array": (
        "Array functions: $append(arr1,arr2), $count(arr), $distinct(arr), "
        "$reverse(arr), $shuffle(arr), $sort(arr,comparator?), $zip(arr1,arr2,...)."
    ),
    "object": (
        "Object functions: $keys(obj), $lookup(obj,key), $merge(arr_of_objs), "
        "$spread(obj), $sift(obj,predicate), $each(obj,function)."
    ),
    "higher-order": (
        "Higher-order functions: $map(arr,function), $filter(arr,predicate), "
        "$reduce(arr,function,init?), $single(arr,predicate), $sift(obj,predicate), "
        "$each(obj,function)."
    ),
    "boolean": "Boolean functions: $boolean(v), $not(v), $exists(v).",
    "datetime": (
        "Date/time functions: $now(picture?,timezone?), $millis(), "
        "$fromMillis(n,picture?,timezone?), $toMillis(s,picture?)."
    ),
    "encoding": (
        "Encoding functions: $base64encode(s), $base64decode(s), "
        "$encodeUrl(s), $encodeUrlComponent(s), $decodeUrl(s), $decodeUrlComponent(s)."
    ),
    "misc": (
        "Other functions: $type(v) returns the JSONata type name, "
        "$error(msg) raises a custom error, $assert(cond,msg), "
        "$eval(expr_str,context?) evaluates a JSONata expression given as a string."
    ),
}


def _explain(topic: str | None) -> str:
    if topic is None:
        lines = ["JSONata function reference. Call explain(topic=<name>) for details."]
        for category, summary in _REFERENCE.items():
            lines.append(f"- {category}: {summary}")
        return "\n".join(lines)

    normalized = topic.strip().lower()
    if normalized in _REFERENCE:
        return _REFERENCE[normalized]

    available = ", ".join(_REFERENCE.keys())
    return f"unknown topic {topic!r}. Available topics: {available}"
```

Add the tool registration inside `create_server()`, after the `evaluate_batch` tool definition and before `return mcp`:

```python
    @mcp.tool
    def explain(topic: str | None = None) -> str:
        """Get curated JSONata function reference material.

        Args:
            topic: A category name (e.g. "string", "numeric", "array",
                "object", "higher-order", "boolean", "datetime", "encoding",
                "misc"). Omit to get the full category index.

        Returns:
            Reference text for the requested topic, the full index if no
            topic given, or a list of available topics if the given topic
            isn't recognized.
        """
        return _explain(topic)
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `uv run pytest tests/python/test_mcp_server.py -v`
Expected: PASS (all tests, including Task 9's).

- [ ] **Step 5: `mypy --strict` and `ruff` clean**

Run: `uv run mypy python/jsonatapy/_cli/mcp_server.py`
Run: `uv run ruff check python/jsonatapy/_cli/mcp_server.py tests/python/test_mcp_server.py`
Run: `uv run ruff format --check python/jsonatapy/_cli/mcp_server.py tests/python/test_mcp_server.py`
Expected: all clean.

- [ ] **Step 6: Full regression run**

Run: `uv run pytest tests/python/ -v` (excluding nothing — full suite, confirms Phase 2's additions haven't broken the existing 1682-case reference suite, Task 3's `evaluate_json_or_none`, or any other pre-existing Python test file).
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add python/jsonatapy/_cli/mcp_server.py tests/python/test_mcp_server.py
git commit -m "feat(pycli): add explain tool with curated JSONata reference content"
```

---

## Definition of Done

- `uv run maturin develop --release && jsonatapy --version` works with no other setup; `jsonatapy '<expr>' file.json` and `echo '<json>' | jsonatapy '<expr>'` both evaluate correctly.
- `JsonataExpression.evaluate_json_or_none()` exists as a new, purely additive public API method, correctly distinguishing an Undefined result (`None`) from an explicit null result (`"null"`), verified against the full pre-existing 1682-case reference suite with zero regressions.
- `uv run --extra mcp jsonatapy mcp` (or `pip install jsonatapy[mcp] && jsonatapy mcp`) serves the four MCP tools over stdio; `jsonatapy mcp --http --port 8080` serves over HTTP. `jsonatapy mcp` without the `mcp` extra installed prints the install hint and exits 2, never a raw traceback.
- All flags/exit codes/error formats in `study/cli_spec.md` are implemented and pass both `tests/python/test_cli.py` (incremental, hand-written) and `tests/python/test_cli_fixtures.py` (the same shared `study/cli_fixtures.json` suite the Rust CLI is tested against) — proving the two CLIs agree.
- The one remaining disclosed divergence (`-n` giving a `null` **context**, not `Undefined` — distinct from the result-side issue Task 3 fixed) is pinned by a regression test and documented in `study/cli_spec.md`'s Python-specific section, not silently present.
- `uv run mypy python/jsonatapy/_cli/ python/jsonatapy/__main__.py python/jsonatapy/__init__.py` is clean under this project's existing `--strict` config.
- `uv run ruff check` / `uv run ruff format --check` are clean on all new/modified files.
- `cargo fmt --all -- --check` / `cargo clippy --all-targets --all-features -- -D warnings` are clean (Task 3's `src/lib.rs` change).
- `uv run pytest tests/python/` passes in full (new CLI/MCP/library tests plus the pre-existing 1682-case reference suite and all other existing test files).
