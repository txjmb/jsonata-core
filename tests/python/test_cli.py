"""Black-box tests for the `jsonatapy` CLI, run as a subprocess exactly like
a real user would invoke it. Mirrors tests/cli_test.rs in the Rust CLI.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

import pytest


def run_cli(args: list[str], stdin: str | None = None) -> subprocess.CompletedProcess[str]:
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
    result = subprocess.run([jsonatapy_bin, "--version"], capture_output=True, text=True)
    assert result.returncode == 0
    assert "jsonatapy" in result.stdout


def test_evaluates_expression_against_stdin_json() -> None:
    result = run_cli(["name"], stdin='{"name": "Alice"}')
    assert result.returncode == 0
    assert result.stdout == '"Alice"\n'


def test_evaluates_expression_against_file_argument(tmp_path: Path) -> None:
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


def test_from_file_reads_expression_from_a_file(tmp_path: Path) -> None:
    expr_file = tmp_path / "expr.jsonata"
    expr_file.write_text("name")
    result = run_cli(["-f", str(expr_file)], stdin='{"name": "Carol"}')
    assert result.returncode == 0
    assert result.stdout == '"Carol"\n'


def test_invalid_json_input_exits_three() -> None:
    result = run_cli(["a"], stdin="not json")
    assert result.returncode == 3
    assert "invalid JSON input" in result.stderr


def test_nan_and_infinity_are_rejected_as_invalid_json() -> None:
    for bad_input in ("NaN", "Infinity", "-Infinity", "1e999"):
        result = run_cli(["a"], stdin=bad_input)
        assert result.returncode == 3, f"expected exit 3 for {bad_input!r}, got {result.returncode}"


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


def test_argjson_nan_is_usage_error_exit_two() -> None:
    result = run_cli(["--argjson", "x=NaN", "$x"], stdin="{}")
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


def test_null_input_uses_null_not_undefined_context_known_divergence() -> None:
    """Documents a known, disclosed divergence from the Rust CLI (see this
    plan's Global Constraints): the Python jsonatapy API has no way to
    construct a true JSONata Undefined top-level CONTEXT (input) -- this is
    distinct from Task 3's result-side fix (evaluate_json_or_none), which
    already resolved the RESULT-side undefined/null ambiguity. -n passes a
    null context instead of Undefined. Unobservable for expressions that
    don't reference $ (the common -n use case). The bare context reference
    $ itself distinguishes them directly -- confirmed live against the
    built Rust binary: `jsonata -n '$'` prints nothing (exit 0, Undefined
    result), while this Python CLI's `-n '$'` prints the text "null" (exit
    0, Null result). (Do NOT use $exists($) for this -- verified live that
    it returns false under -n for BOTH CLIs, because this Rust
    implementation special-cases $exists($) to check named-variable-binding
    presence rather than the actual context value's definedness, so it
    never round-trips through Null-vs-Undefined at all.) If this test ever
    starts failing because the divergence was fixed, delete it and update
    study/cli_spec.md's Python-specific notes accordingly."""
    result = run_cli(["-n", "$"])
    assert result.returncode == 0
    assert result.stdout == "null\n"


def test_mcp_subcommand_dispatches_without_crashing_on_missing_fastmcp(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    """Simulates fastmcp not being installed by making the import fail,
    without needing to actually uninstall it from the test environment.

    Also purges jsonatapy._cli.mcp_server from sys.modules first: when the
    whole test suite is collected together, test_mcp_server.py's module-level
    imports (pytest.importorskip("fastmcp"), etc.) already ran during
    collection and cached both `fastmcp` and `jsonatapy._cli.mcp_server` in
    sys.modules -- without this purge, `from ._cli.mcp_server import serve`
    below would return the cached module without re-running its body (so
    the module-level `from fastmcp import FastMCP` that's supposed to raise
    ImportError never executes), and the failure would surface later from
    fastmcp's own internal lazy submodule import instead, escaping
    _run_mcp's narrow except-ImportError guard around just that one import
    statement."""
    import builtins
    import sys

    monkeypatch.delitem(sys.modules, "jsonatapy._cli.mcp_server", raising=False)

    real_import = builtins.__import__

    def fake_import(name: str, *args: object, **kwargs: object) -> object:
        if name == "fastmcp" or name.startswith("fastmcp."):
            raise ImportError("No module named 'fastmcp'")
        return real_import(name, *args, **kwargs)  # type: ignore[arg-type]

    monkeypatch.setattr(builtins, "__import__", fake_import)

    from jsonatapy.__main__ import _run_mcp

    exit_code = _run_mcp([])
    assert exit_code == 2
