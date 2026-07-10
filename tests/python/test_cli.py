"""Black-box tests for the `jsonatapy` CLI, run as a subprocess exactly like
a real user would invoke it. Mirrors tests/cli_test.rs in the Rust CLI.
"""

from __future__ import annotations

import shutil
import subprocess
import sys


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
