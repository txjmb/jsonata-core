#!/usr/bin/env python3
"""
Update performance documentation from latest benchmark results.

Usage:
    python benchmarks/update_docs.py
"""

import json
from pathlib import Path
from datetime import datetime
from collections import defaultdict


def find_latest_results():
    """Find the most recent benchmark results JSON file."""
    results_dir = Path(__file__).parent / "results"
    json_files = list(results_dir.glob("benchmark_results_*.json"))

    if not json_files:
        raise FileNotFoundError("No benchmark results found in benchmarks/results/")

    # Sort by modification time, get newest
    latest = max(json_files, key=lambda p: p.stat().st_mtime)
    return latest


def load_results(json_path):
    """Load benchmark results from JSON file."""
    with open(json_path) as f:
        return json.load(f)


def format_time(ms):
    """Format time in milliseconds to 3 decimal places."""
    if ms is None:
        return "N/A"
    return f"{ms:.3f}"


def format_speedup(speedup):
    """Format speedup ratio."""
    if speedup is None:
        return "N/A"
    return f"{speedup:.1f}x"


def group_by_category(results):
    """Group benchmark results by category."""
    grouped = defaultdict(list)
    for result in results:
        category = result.get("category", "Other")
        grouped[category].append(result)
    return dict(grouped)


def calculate_category_average(results, metric="jsonatapy_speedup"):
    """Calculate average speedup for a category."""
    values = [r[metric] for r in results if r.get(metric) is not None]
    if not values:
        return None
    return sum(values) / len(values)


def generate_markdown(data):
    """Generate markdown documentation from benchmark data."""
    timestamp = data["timestamp"]
    date = datetime.fromisoformat(timestamp).strftime("%Y-%m-%d")
    results = data["results"]
    implementations = data["implementations"]

    grouped = group_by_category(results)

    # Build markdown
    md = [
        "# Performance\n",
        "jsonatapy is a high-performance Rust implementation with Python bindings, "
        "designed to be significantly faster than JavaScript-based alternatives for typical use cases.\n",
        "## Benchmark Results\n",
        f"Latest benchmarks run on {date}.",
    ]

    # Add comparison info
    compared = []
    if implementations.get("javascript"):
        compared.append("JavaScript reference implementation")
    if implementations.get("jsonata_python"):
        compared.append("jsonata-python")
    if implementations.get("jsonata_rs"):
        compared.append("jsonata-rs")

    if compared:
        md.append(f" Comparing jsonatapy against: {', '.join(compared)}.\n")
    else:
        md.append("\n")

    # Summary table
    md.append("### Summary\n")
    md.append("| Category | Average Speedup vs JS |")
    md.append("|----------|----------------------|")

    for category, cat_results in grouped.items():
        avg_speedup = calculate_category_average(cat_results)
        if avg_speedup:
            md.append(f"| {category} | {format_speedup(avg_speedup)} |")

    md.append("")
    md.append("### Detailed Results\n")

    # Detailed results by category
    for category, cat_results in grouped.items():
        md.append(f"#### {category}\n")
        md.append("| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |")
        md.append("|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|")

        for result in cat_results:
            name = result["name"]
            data_size = result.get("data_size", "N/A")
            jsonatapy_ms = format_time(result.get("jsonatapy_ms"))
            jsonatapy_json_ms = format_time(result.get("jsonatapy_json_ms"))
            js_ms = format_time(result.get("js_ms"))
            jsonata_rs_ms = format_time(result.get("jsonata_rs_ms"))
            speedup = format_speedup(result.get("jsonatapy_speedup"))

            md.append(f"| {name} | {data_size} | {jsonatapy_ms} | {jsonatapy_json_ms} | {js_ms} | {jsonata_rs_ms} | {speedup} |")

        md.append("")

    # Comparison table
    md.append("## Comparison with Other Implementations\n")
    md.append("| Implementation | Language | Status |")
    md.append("|----------------|----------|--------|")
    md.append("| **jsonatapy** | Rust + Python | Baseline (this implementation) |")

    if implementations.get("javascript"):
        md.append("| jsonata-js | JavaScript | Tested (reference implementation) |")
    else:
        md.append("| jsonata-js | JavaScript | Not tested |")

    if implementations.get("jsonata_python"):
        md.append("| jsonata-python | Python wrapper | Tested |")
    else:
        md.append("| jsonata-python | Python wrapper | Not tested |")

    if implementations.get("jsonata_rs"):
        md.append("| jsonata-rs | Rust | Tested |")
    else:
        md.append("| jsonata-rs | Rust | Not tested |")

    md.append("")

    # Performance characteristics
    md.append("## Performance Characteristics\n")

    # Find best and worst categories
    category_avgs = {
        cat: calculate_category_average(cat_results)
        for cat, cat_results in grouped.items()
    }
    category_avgs = {k: v for k, v in category_avgs.items() if v is not None}

    if category_avgs:
        best_cats = [cat for cat, avg in category_avgs.items() if avg > 2.0]
        comparable_cats = [cat for cat, avg in category_avgs.items() if 0.8 <= avg <= 2.0]

        if best_cats:
            md.append("jsonatapy excels at:")
            for cat in best_cats:
                md.append(f"- {cat}")
            md.append("")

        if comparable_cats:
            md.append("Comparable performance on:")
            for cat in comparable_cats:
                md.append(f"- {cat}")
            md.append("")

    # Notes
    md.append("## Notes\n")
    md.append("- Benchmarks run on Ubuntu Linux with Python 3.12")
    md.append("- JavaScript benchmarks use Node.js v20+")
    md.append("- Times shown are per operation in milliseconds")
    md.append("- 'Speedup' shows how many times faster jsonatapy is compared to JavaScript")
    md.append("- Values less than 1.0 indicate JavaScript is faster for that specific operation")

    return "\n".join(md) + "\n"


def main():
    """Main entry point."""
    print("Finding latest benchmark results...")
    json_path = find_latest_results()
    print(f"Found: {json_path}")

    print("Loading results...")
    data = load_results(json_path)

    print("Generating markdown...")
    markdown = generate_markdown(data)

    # Write to docs
    docs_path = Path(__file__).parent.parent / "docs" / "performance.md"
    print(f"Writing to {docs_path}...")
    docs_path.write_text(markdown)

    print("Done! Performance documentation updated.")


if __name__ == "__main__":
    main()
