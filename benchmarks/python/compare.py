#!/usr/bin/env python3
"""Compare current benchmark results against a baseline, detecting regressions.

A test is a regression if jsonatapy_ms is more than 10% slower than the
baseline; an improvement if more than 10% faster. Writes comparison.json
(in the current working directory) with both lists, and — if the
GITHUB_OUTPUT environment variable is set — appends
"regression_detected=true" to it when any regression is found.

Usage: compare.py <baseline.json> <results.json>
"""

import json
import os
import sys

REGRESSION_THRESHOLD_PCT = 10
IMPROVEMENT_THRESHOLD_PCT = -10


def main() -> int:
    # Force UTF-8 stdout/stderr regardless of platform/console encoding: the
    # output below includes non-ASCII characters (⚠️, ✅, →), and on Windows,
    # when stdout isn't a real console (e.g. under subprocess.run() in tests
    # or CI), Python falls back to the legacy code page (cp1252), which can't
    # encode them and crashes with UnicodeEncodeError before finishing.
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")

    if len(sys.argv) != 3:
        print("Usage: compare.py <baseline.json> <results.json>", file=sys.stderr)
        return 2

    baseline_path, results_path = sys.argv[1], sys.argv[2]

    with open(baseline_path) as f:
        baseline = json.load(f)
    with open(results_path) as f:
        current = json.load(f)

    regressions = []
    improvements = []

    for curr_result in current["results"]:
        name = curr_result["name"]
        baseline_result = next((r for r in baseline["results"] if r["name"] == name), None)

        if not baseline_result:
            continue
        if not curr_result.get("jsonatapy_ms") or not baseline_result.get("jsonatapy_ms"):
            continue

        curr_time = curr_result["jsonatapy_ms"]
        base_time = baseline_result["jsonatapy_ms"]
        change_pct = ((curr_time - base_time) / base_time) * 100

        entry = {
            "name": name,
            "baseline_ms": base_time,
            "current_ms": curr_time,
            "change_pct": change_pct,
        }

        if change_pct > REGRESSION_THRESHOLD_PCT:
            regressions.append(entry)
        elif change_pct < IMPROVEMENT_THRESHOLD_PCT:
            improvements.append(entry)

    print(f"Found {len(regressions)} regressions and {len(improvements)} improvements")

    if regressions:
        print("\n⚠️ Performance Regressions Detected:")
        for r in regressions:
            print(
                f"  - {r['name']}: {r['baseline_ms']:.2f}ms → {r['current_ms']:.2f}ms ({r['change_pct']:+.1f}%)"
            )

    if improvements:
        print("\n✅ Performance Improvements:")
        for i in improvements:
            print(
                f"  - {i['name']}: {i['baseline_ms']:.2f}ms → {i['current_ms']:.2f}ms ({i['change_pct']:+.1f}%)"
            )

    with open("comparison.json", "w") as f:
        json.dump({"regressions": regressions, "improvements": improvements}, f, indent=2)

    if regressions:
        github_output = os.environ.get("GITHUB_OUTPUT")
        if github_output:
            with open(github_output, "a") as f:
                f.write("regression_detected=true\n")

    return 0


if __name__ == "__main__":
    sys.exit(main())
