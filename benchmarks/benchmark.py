#!/usr/bin/env python3
"""
Benchmark JSONataPy (Rust) vs JSONata (JavaScript reference implementation)

Compares performance across various query types and data sizes.
"""

import json
import time
import subprocess
import sys
from pathlib import Path
from typing import Dict, List, Tuple, Any

try:
    import jsonatapy
except ImportError:
    print("Error: jsonatapy not installed. Run 'maturin develop' first.")
    sys.exit(1)


class BenchmarkSuite:
    """Run performance benchmarks comparing Rust and JavaScript implementations."""

    def __init__(self):
        self.results = []
        self.node_available = self._check_node()

    def _check_node(self) -> bool:
        """Check if Node.js is available."""
        try:
            result = subprocess.run(
                ["node", "--version"],
                capture_output=True,
                text=True,
                timeout=5
            )
            if result.returncode == 0:
                print(f"✓ Node.js detected: {result.stdout.strip()}")
                return True
        except (subprocess.SubprocessError, FileNotFoundError):
            pass

        print("⚠ Node.js not found - will only benchmark Rust implementation")
        return False

    def _run_js_benchmark(self, expression: str, data: Any, iterations: int) -> float:
        """Run benchmark using JavaScript reference implementation."""
        if not self.node_available:
            return -1.0

        # Create temporary files for data exchange
        bench_dir = Path(__file__).parent
        js_script = bench_dir / "benchmark.js"

        if not js_script.exists():
            print(f"⚠ JavaScript benchmark script not found at {js_script}")
            return -1.0

        # Prepare benchmark data
        benchmark_data = {
            "expression": expression,
            "data": data,
            "iterations": iterations
        }

        try:
            result = subprocess.run(
                ["node", str(js_script)],
                input=json.dumps(benchmark_data),
                capture_output=True,
                text=True,
                timeout=30
            )

            if result.returncode != 0:
                print(f"⚠ JavaScript benchmark failed: {result.stderr}")
                return -1.0

            return float(result.stdout.strip())
        except Exception as e:
            print(f"⚠ Error running JavaScript benchmark: {e}")
            return -1.0

    def _run_rust_benchmark(self, expression: str, data: Any, iterations: int) -> float:
        """Run benchmark using Rust implementation."""
        # Compile once
        try:
            compiled = jsonatapy.compile(expression)
        except Exception as e:
            print(f"⚠ Compilation failed: {e}")
            return -1.0

        # Warm up
        for _ in range(min(10, iterations // 10)):
            compiled.evaluate(data)

        # Measure
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate(data)
        elapsed = time.perf_counter() - start

        return elapsed * 1000  # Convert to milliseconds

    def benchmark(
        self,
        name: str,
        expression: str,
        data: Any,
        iterations: int = 1000
    ):
        """Run a single benchmark test."""
        print(f"\n{'='*60}")
        print(f"Benchmark: {name}")
        print(f"Expression: {expression}")
        print(f"Iterations: {iterations:,}")
        print(f"{'='*60}")

        # Run Rust benchmark
        rust_time = self._run_rust_benchmark(expression, data, iterations)

        if rust_time > 0:
            print(f"Rust:       {rust_time:8.2f} ms ({rust_time/iterations:8.4f} ms/iter)")
        else:
            print(f"Rust:       FAILED")
            return

        # Run JavaScript benchmark
        js_time = self._run_js_benchmark(expression, data, iterations)

        if js_time > 0:
            print(f"JavaScript: {js_time:8.2f} ms ({js_time/iterations:8.4f} ms/iter)")
            speedup = js_time / rust_time
            print(f"\n{'Speedup:':12} {speedup:6.2f}x faster" if speedup > 1 else f"{'Slowdown:':12} {1/speedup:6.2f}x slower")
        else:
            print(f"JavaScript: SKIPPED")
            speedup = None

        # Store results
        self.results.append({
            "name": name,
            "expression": expression,
            "iterations": iterations,
            "rust_ms": rust_time,
            "js_ms": js_time,
            "speedup": speedup
        })

    def print_summary(self):
        """Print summary of all benchmark results."""
        print("\n" + "="*80)
        print("BENCHMARK SUMMARY")
        print("="*80)
        print(f"{'Test Name':<30} {'Rust (ms)':<12} {'JS (ms)':<12} {'Speedup':<10}")
        print("-"*80)

        total_rust = 0
        total_js = 0
        speedups = []

        for result in self.results:
            rust_ms = result["rust_ms"]
            js_ms = result["js_ms"]
            speedup = result["speedup"]

            total_rust += rust_ms if rust_ms > 0 else 0
            total_js += js_ms if js_ms > 0 else 0

            if speedup:
                speedups.append(speedup)
                speedup_str = f"{speedup:.2f}x"
            else:
                speedup_str = "N/A"

            print(f"{result['name']:<30} {rust_ms:>10.2f}   {js_ms:>10.2f}   {speedup_str:>8}")

        print("-"*80)

        if speedups:
            avg_speedup = sum(speedups) / len(speedups)
            print(f"{'AVERAGE':<30} {total_rust:>10.2f}   {total_js:>10.2f}   {avg_speedup:>7.2f}x")
            print(f"\nOverall: Rust is {avg_speedup:.1f}x faster on average")
        else:
            print(f"{'TOTAL':<30} {total_rust:>10.2f}   {'N/A':>10}   {'N/A':>8}")


def main():
    """Run comprehensive benchmark suite."""
    suite = BenchmarkSuite()

    # Small data benchmarks
    print("\n" + "█"*60)
    print("PART 1: SIMPLE QUERIES")
    print("█"*60)

    suite.benchmark(
        "Simple Path",
        "user.name",
        {"user": {"name": "Alice", "age": 30}},
        iterations=10000
    )

    suite.benchmark(
        "Deep Path",
        "a.b.c.d.e.f",
        {"a": {"b": {"c": {"d": {"e": {"f": 42}}}}}},
        iterations=10000
    )

    suite.benchmark(
        "Arithmetic",
        "price * quantity",
        {"price": 10.5, "quantity": 3},
        iterations=10000
    )

    suite.benchmark(
        "String Function",
        '$uppercase(name)',
        {"name": "hello world"},
        iterations=10000
    )

    # Medium data benchmarks
    print("\n" + "█"*60)
    print("PART 2: ARRAY OPERATIONS")
    print("█"*60)

    medium_data_simple = {"values": list(range(100))}

    suite.benchmark(
        "Array Sum (100 elements)",
        '$sum(values)',
        medium_data_simple,
        iterations=1000
    )

    suite.benchmark(
        "Array Index Access",
        'values[50]',
        medium_data_simple,
        iterations=5000
    )

    suite.benchmark(
        "Array Max (100 elements)",
        '$max(values)',
        medium_data_simple,
        iterations=1000
    )

    suite.benchmark(
        "Array Count",
        '$count(values)',
        medium_data_simple,
        iterations=2000
    )

    # Array mapping benchmarks
    products_data = {
        "products": [
            {"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5}
            for i in range(100)
        ]
    }

    suite.benchmark(
        "Array Mapping (extract field)",
        'products.price',
        products_data,
        iterations=1000
    )

    suite.benchmark(
        "Array Mapping + Sum",
        '$sum(products.price)',
        products_data,
        iterations=1000
    )

    # Large data benchmarks
    print("\n" + "█"*60)
    print("PART 3: LARGE DATA")
    print("█"*60)

    large_data_prices = {"prices": [10.0 + i * 0.5 for i in range(1000)]}

    suite.benchmark(
        "Large Array Sum (1000 elements)",
        '$sum(prices)',
        large_data_prices,
        iterations=100
    )

    suite.benchmark(
        "Large Array Max (1000 elements)",
        '$max(prices)',
        large_data_prices,
        iterations=100
    )

    suite.benchmark(
        "Large Array Index Access",
        'prices[500]',
        large_data_prices,
        iterations=5000
    )

    # Complex queries
    print("\n" + "█"*60)
    print("PART 4: COMPLEX QUERIES")
    print("█"*60)

    suite.benchmark(
        "Multiple Functions",
        '$length($uppercase(name))',
        {"name": "JSONata Performance Test"},
        iterations=5000
    )

    suite.benchmark(
        "Conditional",
        'age >= 18 ? "adult" : "minor"',
        {"age": 25},
        iterations=5000
    )

    suite.benchmark(
        "String Operations",
        '$join([$uppercase(first), $uppercase(last)], " ")',
        {"first": "john", "last": "doe"},
        iterations=5000
    )

    # Print summary
    suite.print_summary()


if __name__ == "__main__":
    main()
