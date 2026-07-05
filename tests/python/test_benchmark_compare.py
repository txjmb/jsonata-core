"""Tests for benchmarks/python/compare.py's regression-detection logic."""

import json
import os
import subprocess
import sys
from pathlib import Path

COMPARE_SCRIPT = Path(__file__).parent.parent.parent / "benchmarks" / "python" / "compare.py"


def _write_results(path, entries):
    path.write_text(json.dumps({"results": entries}))


def test_detects_regression_over_10_percent(tmp_path):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 12.0}])  # 20% slower

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )

    assert result.returncode == 0, result.stderr
    assert "Found 1 regressions" in result.stdout

    comparison = json.loads((tmp_path / "comparison.json").read_text())
    assert len(comparison["regressions"]) == 1
    assert comparison["regressions"][0]["name"] == "Array Sum"
    assert comparison["improvements"] == []


def test_detects_improvement_over_10_percent(tmp_path):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 8.0}])  # 20% faster

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )

    assert result.returncode == 0, result.stderr
    comparison = json.loads((tmp_path / "comparison.json").read_text())
    assert comparison["regressions"] == []
    assert len(comparison["improvements"]) == 1


def test_within_threshold_is_neither(tmp_path):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 10.5}])  # 5% slower

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )

    assert result.returncode == 0, result.stderr
    comparison = json.loads((tmp_path / "comparison.json").read_text())
    assert comparison["regressions"] == []
    assert comparison["improvements"] == []


def test_writes_regression_detected_to_github_output(tmp_path, monkeypatch):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    output_file = tmp_path / "github_output.txt"
    output_file.write_text("")
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 12.0}])

    env = dict(os.environ)
    env["GITHUB_OUTPUT"] = str(output_file)

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
    )

    assert result.returncode == 0, result.stderr
    assert "regression_detected=true" in output_file.read_text()


def test_missing_args_exits_nonzero():
    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT)],
        capture_output=True,
        text=True,
        encoding="utf-8",
    )
    assert result.returncode != 0
