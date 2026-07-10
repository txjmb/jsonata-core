"""Black-box tests for the `jsonatapy` CLI, run as a subprocess exactly like
a real user would invoke it. Mirrors tests/cli_test.rs in the Rust CLI.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path


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
