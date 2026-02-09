#!/usr/bin/env python3
"""
Update performance documentation from latest benchmark results.

Usage:
    python benchmarks/update_docs.py
"""

import json
import subprocess
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


def get_versions():
    """Detect versions of all benchmarked implementations."""
    versions = {}

    # jsonatapy
    try:
        import jsonatapy
        versions["jsonatapy"] = jsonatapy.__version__
    except Exception:
        versions["jsonatapy"] = "unknown"

    # jsonata-python
    try:
        result = subprocess.run(
            ["uv", "pip", "show", "jsonata-python"],
            capture_output=True, text=True, timeout=10
        )
        for line in result.stdout.splitlines():
            if line.startswith("Version:"):
                versions["jsonata_python"] = line.split(":", 1)[1].strip()
                break
    except Exception:
        pass
    if "jsonata_python" not in versions:
        versions["jsonata_python"] = "unknown"

    # jsonata-js (Node.js)
    js_pkg = Path(__file__).parent / "javascript" / "node_modules" / "jsonata" / "package.json"
    if js_pkg.exists():
        with open(js_pkg) as f:
            versions["javascript"] = json.load(f).get("version", "unknown")
    else:
        versions["javascript"] = "unknown"

    # jsonata-rs
    rs_cargo = Path(__file__).parent / "rust" / "Cargo.toml"
    if rs_cargo.exists():
        for line in rs_cargo.read_text().splitlines():
            if "jsonata-rs" in line and "=" in line:
                ver = line.split("=", 1)[1].strip().strip('"').strip("'")
                versions["jsonata_rs"] = ver
                break
    if "jsonata_rs" not in versions:
        versions["jsonata_rs"] = "unknown"

    # Node.js version
    try:
        result = subprocess.run(
            ["node", "--version"], capture_output=True, text=True, timeout=5
        )
        versions["nodejs"] = result.stdout.strip()
    except Exception:
        versions["nodejs"] = "unknown"

    return versions


def format_time(ms):
    """Format time in milliseconds to 3 decimal places."""
    if ms is None:
        return "N/A"
    return f"{ms:.3f}"


def format_speedup(speedup):
    """Format speedup ratio."""
    if speedup is None:
        return "N/A"
    if speedup >= 1.0:
        return f"**{speedup:.1f}x faster**"
    else:
        return f"{1.0/speedup:.1f}x slower"


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


def generate_markdown(data, versions):
    """Generate markdown documentation from benchmark data."""
    timestamp = data["timestamp"]
    date = datetime.fromisoformat(timestamp).strftime("%Y-%m-%d")
    results = data["results"]
    implementations = data["implementations"]

    grouped = group_by_category(results)

    md = [
        "# Performance Benchmarks\n",
        "jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. "
        "This page presents benchmark comparisons against other JSONata implementations.\n",
    ]

    # Versions table
    md.append("## Implementations Tested\n")
    md.append("| Implementation | Language | Version | Description |")
    md.append("|----------------|----------|---------|-------------|")
    md.append(
        f"| **jsonatapy** | Rust + Python | {versions['jsonatapy']} | "
        "This project (compiled Rust extension via PyO3) |"
    )
    md.append(
        f"| **jsonatapy** (rust-only) | Rust + Python | {versions['jsonatapy']} | "
        "Same library, JSON string I/O path (bypasses Python object conversion) |"
    )
    if implementations.get("javascript"):
        md.append(
            f"| **jsonata-js** | JavaScript | {versions['javascript']} | "
            f"Reference implementation (Node.js {versions.get('nodejs', '')}) |"
        )
    if implementations.get("jsonata_python"):
        md.append(
            f"| **jsonata-python** | Python | {versions['jsonata_python']} | "
            "Pure Python implementation |"
        )
    if implementations.get("jsonata_rs"):
        md.append(
            f"| **jsonata-rs** | Rust | {versions['jsonata_rs']} | "
            "Pure Rust implementation (CLI benchmark, no Python overhead) |"
        )
    md.append("")

    md.append(f"Benchmarks run on {date}.\n")

    # Summary table
    md.append("## Summary by Category\n")
    md.append("| Category | jsonatapy vs JS |")
    md.append("|----------|----------------|")

    for category, cat_results in grouped.items():
        if category == "Path Comparison":
            continue
        avg_speedup = calculate_category_average(cat_results)
        if avg_speedup is not None:
            md.append(f"| {category} | {format_speedup(avg_speedup)} |")

    md.append("")

    # Detailed results by category
    md.append("## Detailed Results\n")

    for category, cat_results in grouped.items():
        md.append(f"### {category}\n")

        if category == "Path Comparison":
            # Special format for path comparison (no JS/python columns)
            md.append("| Operation | jsonatapy (ms) | Iterations |")
            md.append("|-----------|---------------|------------|")
            for result in cat_results:
                name = result["name"]
                ms = format_time(result.get("jsonatapy_ms"))
                iters = result.get("iterations", "")
                md.append(f"| {name} | {ms} | {iters} |")
            md.append("")
            continue

        md.append(
            "| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | "
            "jsonata-python | jsonata-rs | vs JS |"
        )
        md.append(
            "|-----------|-----------|-----------|------------------|------------|"
            "----------------|------------|-------|"
        )

        for result in cat_results:
            name = result["name"]
            data_size = result.get("data_size", "")
            jsonatapy_ms = format_time(result.get("jsonatapy_ms"))
            jsonatapy_json_ms = format_time(result.get("jsonatapy_json_ms"))
            js_ms = format_time(result.get("js_ms"))
            python_ms = format_time(result.get("jsonata_python_ms"))
            rs_ms = format_time(result.get("jsonata_rs_ms"))
            speedup = result.get("jsonatapy_speedup")
            speedup_str = format_speedup(speedup) if speedup is not None else "N/A"

            md.append(
                f"| {name} | {data_size} | {jsonatapy_ms} | {jsonatapy_json_ms} | "
                f"{js_ms} | {python_ms} | {rs_ms} | {speedup_str} |"
            )

        md.append("")

    # Performance characteristics
    md.append("## Performance Characteristics\n")

    category_avgs = {
        cat: calculate_category_average(cat_results)
        for cat, cat_results in grouped.items()
        if cat != "Path Comparison"
    }
    category_avgs = {k: v for k, v in category_avgs.items() if v is not None}

    if category_avgs:
        best_cats = [cat for cat, avg in category_avgs.items() if avg > 2.0]
        comparable_cats = [cat for cat, avg in category_avgs.items() if 0.8 <= avg <= 2.0]
        slower_cats = [cat for cat, avg in category_avgs.items() if avg < 0.8]

        if best_cats:
            md.append("**Faster than JavaScript:**\n")
            for cat in best_cats:
                avg = category_avgs[cat]
                md.append(f"- {cat} ({avg:.1f}x faster)")
            md.append("")

        if comparable_cats:
            md.append("**Comparable to JavaScript:**\n")
            for cat in comparable_cats:
                md.append(f"- {cat}")
            md.append("")

        if slower_cats:
            md.append("**Slower than JavaScript:**\n")
            for cat in slower_cats:
                avg = category_avgs[cat]
                md.append(f"- {cat} ({1.0/avg:.1f}x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)")
            md.append("")

    # Data handle optimization note
    md.append("### Optimizing Array Workloads\n")
    md.append(
        "For array-heavy workloads, the dominant cost is converting Python dicts to Rust values on every call. "
        "Use `JsonataData` to pre-convert data once and reuse across multiple evaluations:\n"
    )
    md.append("```python")
    md.append("import jsonatapy")
    md.append("")
    md.append("data = {...}  # your data")
    md.append('expr = jsonatapy.compile("products[price > 100]")')
    md.append("")
    md.append("# Pre-convert once")
    md.append("jdata = jsonatapy.JsonataData(data)")
    md.append("")
    md.append("# Reuse many times (6-15x faster than evaluate(dict))")
    md.append("result = expr.evaluate_with_data(jdata)")
    md.append("```\n")

    # Notes
    md.append("## Methodology\n")
    md.append(f"- **Date:** {date}")
    md.append("- **Platform:** Linux (WSL2) on x86_64")
    md.append("- **Python:** 3.13")
    md.append(f"- **Node.js:** {versions.get('nodejs', 'unknown')}")
    md.append("- All times are total wall-clock time for the stated number of iterations")
    md.append("- Each benchmark includes a warmup phase before measurement")
    md.append("- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation")
    md.append("- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster")

    return "\n".join(md) + "\n"


def main():
    """Main entry point."""
    print("Finding latest benchmark results...")
    json_path = find_latest_results()
    print(f"Found: {json_path}")

    print("Loading results...")
    data = load_results(json_path)

    print("Detecting implementation versions...")
    versions = get_versions()
    for name, ver in versions.items():
        print(f"  {name}: {ver}")

    print("Generating markdown...")
    markdown = generate_markdown(data, versions)

    # Write to docs
    docs_path = Path(__file__).parent.parent / "docs" / "performance.md"
    print(f"Writing to {docs_path}...")
    docs_path.write_text(markdown)

    print("Done! Performance documentation updated.")


if __name__ == "__main__":
    main()
