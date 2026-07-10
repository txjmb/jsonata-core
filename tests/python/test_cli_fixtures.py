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
            failures.append(f"{name}: expected stdout {expected_stdout!r}, got {result.stdout!r}")

        if expected_stderr_contains is not None and expected_stderr_contains not in result.stderr:
            failures.append(
                f"{name}: expected stderr to contain {expected_stderr_contains!r}, "
                f"got {result.stderr!r}"
            )

    assert not failures, "fixture failures:\n" + "\n".join(failures)
