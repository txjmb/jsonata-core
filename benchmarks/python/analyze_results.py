#!/usr/bin/env python3
"""
Analyze and visualize benchmark results.

Usage:
    python analyze_results.py [results_file.json]

If no file is provided, analyzes the most recent results file.
"""

import glob
import json
import sys
from pathlib import Path


def load_results(filepath: str | None = None) -> dict:
    """Load benchmark results from JSON file."""
    results_dir = Path(__file__).parent.parent / "results"

    if filepath:
        with open(filepath) as f:
            return json.load(f)

    # Find most recent results file
    files = sorted(glob.glob(str(results_dir / "*.json")))
    if not files:
        print("No results files found in benchmarks/results/")
        sys.exit(1)

    latest = files[-1]
    print(f"Loading: {latest}\n")

    with open(latest) as f:
        return json.load(f)


def print_summary(data: dict):
    """Print summary statistics."""
    results = data["results"]
    implementations = data["implementations"]

    print("=" * 80)
    print("BENCHMARK RESULTS SUMMARY")
    print("=" * 80)
    print(f"Timestamp: {data['timestamp']}")
    print("\nImplementations tested:")
    for impl, available in implementations.items():
        status = "✓" if available else "✗"
        print(f"  {status} {impl}")

    print(f"\nTotal tests: {len(results)}")

    # Calculate statistics
    speedups = [r["jsonatapy_speedup"] for r in results if r["jsonatapy_speedup"]]

    if speedups:
        avg = sum(speedups) / len(speedups)
        min_speedup = min(speedups)
        max_speedup = max(speedups)
        faster_count = sum(1 for s in speedups if s > 1)

        print(f"\n{'=' * 80}")
        print("jsonatapy vs JavaScript")
        print("=" * 80)
        print(f"Average speedup:  {avg:6.2f}x")
        print(f"Min speedup:      {min_speedup:6.2f}x")
        print(f"Max speedup:      {max_speedup:6.2f}x")
        print(
            f"Tests faster:     {faster_count}/{len(speedups)} ({100 * faster_count / len(speedups):.1f}%)"
        )

    # Category breakdown
    print(f"\n{'=' * 80}")
    print("CATEGORY BREAKDOWN")
    print("=" * 80)

    categories = {}
    for r in results:
        cat = r["category"]
        if cat not in categories:
            categories[cat] = []
        if r["jsonatapy_speedup"]:
            categories[cat].append(r["jsonatapy_speedup"])

    for cat, speedups in sorted(categories.items()):
        if speedups:
            avg = sum(speedups) / len(speedups)
            min_s = min(speedups)
            max_s = max(speedups)
            print(f"{cat:30} avg: {avg:6.2f}x  [min: {min_s:6.2f}x, max: {max_s:6.2f}x]")


def print_top_performers(data: dict, top_n: int = 5):
    """Print top and bottom performers."""
    results = data["results"]

    # Filter results with speedup data
    valid_results = [r for r in results if r["jsonatapy_speedup"]]

    if not valid_results:
        return

    # Sort by speedup
    by_speedup = sorted(valid_results, key=lambda r: r["jsonatapy_speedup"], reverse=True)

    print(f"\n{'=' * 80}")
    print(f"TOP {top_n} FASTEST (jsonatapy vs JS)")
    print("=" * 80)
    for i, r in enumerate(by_speedup[:top_n], 1):
        print(f"{i}. {r['name']:40} {r['jsonatapy_speedup']:6.2f}x")
        print(f"   Expression: {r['expression'][:60]}{'...' if len(r['expression']) > 60 else ''}")

    print(f"\n{'=' * 80}")
    print(f"TOP {top_n} NEEDING OPTIMIZATION (slowest)")
    print("=" * 80)
    for i, r in enumerate(by_speedup[-top_n:][::-1], 1):
        print(f"{i}. {r['name']:40} {r['jsonatapy_speedup']:6.2f}x")
        print(f"   Expression: {r['expression'][:60]}{'...' if len(r['expression']) > 60 else ''}")


def compare_results(file1: str, file2: str):
    """Compare two benchmark results files."""
    with open(file1) as f:
        data1 = json.load(f)

    with open(file2) as f:
        data2 = json.load(f)

    print("=" * 80)
    print("COMPARING BENCHMARK RESULTS")
    print("=" * 80)
    print(f"File 1: {file1}")
    print(f"  Timestamp: {data1['timestamp']}")
    print(f"\nFile 2: {file2}")
    print(f"  Timestamp: {data2['timestamp']}")

    # Build lookup tables
    results1 = {r["name"]: r for r in data1["results"]}
    results2 = {r["name"]: r for r in data2["results"]}

    # Find common tests
    common = set(results1.keys()) & set(results2.keys())

    if not common:
        print("\nNo common tests found!")
        return

    print(f"\nCommon tests: {len(common)}")
    print("\n" + "=" * 80)
    print("PERFORMANCE CHANGES")
    print("=" * 80)

    improvements = []
    regressions = []

    for name in sorted(common):
        r1 = results1[name]
        r2 = results2[name]

        if r1["jsonatapy_speedup"] and r2["jsonatapy_speedup"]:
            speedup1 = r1["jsonatapy_speedup"]
            speedup2 = r2["jsonatapy_speedup"]
            change = speedup2 / speedup1

            if change > 1.05:  # 5% improvement
                improvements.append((name, speedup1, speedup2, change))
            elif change < 0.95:  # 5% regression
                regressions.append((name, speedup1, speedup2, change))

    if improvements:
        print("\nIMPROVEMENTS:")
        for name, old, new, change in sorted(improvements, key=lambda x: x[3], reverse=True):
            print(f"  {name:40} {old:6.2f}x → {new:6.2f}x ({change:5.2f}x)")

    if regressions:
        print("\nREGRESSIONS:")
        for name, old, new, change in sorted(regressions, key=lambda x: x[3]):
            print(f"  {name:40} {old:6.2f}x → {new:6.2f}x ({change:5.2f}x)")

    if not improvements and not regressions:
        print("\nNo significant changes (>5% difference)")


def main():
    """Main entry point."""
    if len(sys.argv) == 3:
        # Compare mode
        compare_results(sys.argv[1], sys.argv[2])
    elif len(sys.argv) == 2:
        # Analyze specific file
        data = load_results(sys.argv[1])
        print_summary(data)
        print_top_performers(data)
    else:
        # Analyze most recent
        data = load_results()
        print_summary(data)
        print_top_performers(data)


if __name__ == "__main__":
    main()
