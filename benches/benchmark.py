#!/usr/bin/env python3
"""
Benchmark script comparing jsonatapy (Python/Rust) with jsonata (JavaScript).
"""

import json
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Dict, List, Tuple

import jsonatapy


# Sample data for benchmarks
SAMPLE_DATA = {
    "items": [
        {"name": "Item 1", "price": 10.99, "category": "A", "stock": 100},
        {"name": "Item 2", "price": 25.50, "category": "B", "stock": 50},
        {"name": "Item 3", "price": 5.99, "category": "A", "stock": 200},
        {"name": "Item 4", "price": 15.00, "category": "C", "stock": 75},
        {"name": "Item 5", "price": 30.00, "category": "B", "stock": 25},
    ],
    "user": {
        "name": "John Doe",
        "email": "john@example.com",
        "preferences": {
            "theme": "dark",
            "language": "en"
        }
    },
    "stats": {
        "totalOrders": 150,
        "revenue": 5000.50
    }
}

# Generate larger dataset
LARGE_DATA = {
    "items": [
        {
            "name": f"Item {i}",
            "price": 10.0 + (i % 100),
            "category": chr(65 + (i % 26)),  # A-Z
            "stock": 50 + (i % 200),
            "id": i
        }
        for i in range(1000)
    ]
}


# Benchmark queries
BENCHMARKS = [
    ("Simple path", "user.name", SAMPLE_DATA),
    ("Nested path", "user.preferences.theme", SAMPLE_DATA),
    ("Array access", "items[0].name", SAMPLE_DATA),
    ("Array mapping", "items.name", SAMPLE_DATA),
    ("Array filter", "items[price > 15]", SAMPLE_DATA),
    ("Array filter complex", "items[price > 10 and category = 'A']", SAMPLE_DATA),
    ("Arithmetic", "stats.revenue * 1.1", SAMPLE_DATA),
    ("String concat", "user.name & ' (' & user.email & ')'", SAMPLE_DATA),
    ("$sum function", "$sum(items.price)", SAMPLE_DATA),
    ("$count function", "$count(items)", SAMPLE_DATA),
    ("$map function", "$map(items, function($v) { $v.price * 2 })", SAMPLE_DATA),
    ("Large array map", "items.name", LARGE_DATA),
    ("Large array filter", "items[price > 50]", LARGE_DATA),
    ("Large array complex", "items[price > 50 and stock < 150].{\"name\": name, \"value\": price * stock}", LARGE_DATA),
]


def benchmark_python(expression: str, data: Dict[str, Any], iterations: int = 1000) -> float:
    """Benchmark jsonatapy Python implementation."""
    expr = jsonatapy.compile(expression)

    # Warmup
    for _ in range(10):
        expr.evaluate(data)

    # Actual benchmark
    start = time.perf_counter()
    for _ in range(iterations):
        result = expr.evaluate(data)
    end = time.perf_counter()

    return (end - start) / iterations


def benchmark_javascript(expression: str, data: Dict[str, Any], iterations: int = 1000) -> float:
    """Benchmark jsonata JavaScript implementation."""
    # Create a Node.js script
    script = f"""
const jsonata = require('jsonata');

const data = {json.dumps(data)};
const expression = {json.dumps(expression)};
const iterations = {iterations};

const expr = jsonata(expression);

// Warmup
for (let i = 0; i < 10; i++) {{
    expr.evaluate(data);
}}

// Actual benchmark
const start = process.hrtime.bigint();
for (let i = 0; i < iterations; i++) {{
    expr.evaluate(data);
}}
const end = process.hrtime.bigint();

// Output time in seconds
const timeNs = Number(end - start);
const avgTimeSeconds = (timeNs / 1e9) / iterations;
console.log(avgTimeSeconds);
"""

    # Write script to benches directory
    script_path = Path(__file__).parent / f"_benchmark_temp_{id(data)}.js"
    script_path.write_text(script)

    try:
        # Run Node.js script via WSL
        result = subprocess.run(
            ['wsl.exe', '-d', 'Ubuntu', 'bash', '-c',
             f'cd /mnt/c/Users/mboha/source/repos/jsonatapy && node benches/{script_path.name}'],
            capture_output=True,
            text=True,
            timeout=30
        )

        if result.returncode != 0:
            print(f"JavaScript error: {result.stderr}")
            return float('inf')

        return float(result.stdout.strip())
    except Exception as e:
        print(f"Error running JavaScript benchmark: {e}")
        return float('inf')
    finally:
        script_path.unlink(missing_ok=True)


def format_time(seconds: float) -> str:
    """Format time in appropriate units."""
    if seconds < 1e-6:
        return f"{seconds * 1e9:.2f} ns"
    elif seconds < 1e-3:
        return f"{seconds * 1e6:.2f} µs"
    elif seconds < 1:
        return f"{seconds * 1e3:.2f} ms"
    else:
        return f"{seconds:.2f} s"


def main():
    """Run all benchmarks and display results."""
    print("JSONata Performance Comparison")
    print("=" * 100)
    print(f"{'Benchmark':<30} {'Python Time':<15} {'JS Time':<15} {'Speedup':<15} {'Winner':<10}")
    print("-" * 100)

    results = []

    for name, expression, data in BENCHMARKS:
        # Adjust iterations based on data size
        iterations = 100 if "Large" in name else 1000

        try:
            py_time = benchmark_python(expression, data, iterations)
            py_time_str = format_time(py_time)
        except Exception as e:
            py_time = float('inf')
            py_time_str = f"ERROR: {e}"

        try:
            js_time = benchmark_javascript(expression, data, iterations)
            js_time_str = format_time(js_time)
        except Exception as e:
            js_time = float('inf')
            js_time_str = f"ERROR: {e}"

        # Calculate speedup
        if py_time != float('inf') and js_time != float('inf') and js_time > 0:
            speedup = js_time / py_time
            if speedup > 1:
                speedup_str = f"{speedup:.2f}x faster"
                winner = "Python"
            else:
                speedup_str = f"{1/speedup:.2f}x slower"
                winner = "JS"
        else:
            speedup_str = "N/A"
            winner = "N/A"

        print(f"{name:<30} {py_time_str:<15} {js_time_str:<15} {speedup_str:<15} {winner:<10}")

        results.append({
            "name": name,
            "python_time": py_time,
            "js_time": js_time,
            "speedup": speedup if speedup_str != "N/A" else None
        })

    print("=" * 100)

    # Summary statistics
    valid_results = [r for r in results if r["speedup"] is not None]
    if valid_results:
        avg_speedup = sum(r["speedup"] for r in valid_results) / len(valid_results)
        wins = sum(1 for r in valid_results if r["speedup"] > 1)
        losses = sum(1 for r in valid_results if r["speedup"] < 1)

        print(f"\nSummary:")
        print(f"  Average speedup: {avg_speedup:.2f}x")
        print(f"  Python wins: {wins}/{len(valid_results)}")
        print(f"  JavaScript wins: {losses}/{len(valid_results)}")

        if avg_speedup > 1:
            print(f"  Overall: Python implementation is {avg_speedup:.2f}x faster on average")
        else:
            print(f"  Overall: JavaScript implementation is {1/avg_speedup:.2f}x faster on average")


if __name__ == "__main__":
    main()
