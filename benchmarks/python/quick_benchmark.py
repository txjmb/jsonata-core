#!/usr/bin/env python3
"""
Quick benchmark tool for testing JSONata expressions interactively.

Usage:
    python quick_benchmark.py "expression" '{"data": "here"}' [iterations]

Example:
    python quick_benchmark.py "user.name" '{"user": {"name": "Alice"}}' 1000
"""

import json
import sys
import time
import subprocess
from pathlib import Path

try:
    import jsonatapy
    JSONATAPY_AVAILABLE = True
except ImportError:
    print("⚠ jsonatapy not available. Install with: maturin develop --release")
    JSONATAPY_AVAILABLE = False


def run_jsonatapy(expression: str, data: dict, iterations: int) -> float:
    """Benchmark jsonatapy implementation."""
    if not JSONATAPY_AVAILABLE:
        return -1.0

    try:
        compiled = jsonatapy.compile(expression)
        result = compiled.evaluate(data)

        # Warmup
        for _ in range(min(10, iterations // 10)):
            compiled.evaluate(data)

        # Measure
        start = time.perf_counter()
        for _ in range(iterations):
            compiled.evaluate(data)
        elapsed = time.perf_counter() - start

        return elapsed * 1000, result
    except Exception as e:
        print(f"❌ jsonatapy error: {e}")
        return -1.0, None


def run_javascript(expression: str, data: dict, iterations: int) -> float:
    """Benchmark JavaScript implementation."""
    bench_dir = Path(__file__).parent
    js_script = bench_dir / "benchmark.js"

    if not js_script.exists():
        return -1.0, None

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
            print(f"❌ JavaScript error: {result.stderr}")
            return -1.0, None

        return float(result.stdout.strip()), None
    except Exception as e:
        print(f"❌ JavaScript error: {e}")
        return -1.0, None


def main():
    """Main entry point."""
    if len(sys.argv) < 3:
        print("Usage: python quick_benchmark.py <expression> <data_json> [iterations]")
        print("\nExample:")
        print('  python quick_benchmark.py "user.name" \'{"user": {"name": "Alice"}}\' 1000')
        sys.exit(1)

    expression = sys.argv[1]
    data_str = sys.argv[2]
    iterations = int(sys.argv[3]) if len(sys.argv) > 3 else 1000

    try:
        data = json.loads(data_str)
    except json.JSONDecodeError as e:
        print(f"❌ Invalid JSON data: {e}")
        sys.exit(1)

    print("="*70)
    print("Quick Benchmark")
    print("="*70)
    print(f"Expression: {expression}")
    print(f"Data: {json.dumps(data, indent=2)}")
    print(f"Iterations: {iterations:,}")
    print("="*70)

    # Run jsonatapy
    if JSONATAPY_AVAILABLE:
        jsonatapy_time, result = run_jsonatapy(expression, data, iterations)

        if jsonatapy_time > 0:
            print(f"\n✓ jsonatapy")
            print(f"  Total time:     {jsonatapy_time:8.2f} ms")
            print(f"  Per iteration:  {jsonatapy_time/iterations:8.6f} ms")
            print(f"  Result:         {json.dumps(result)}")
        else:
            print(f"\n✗ jsonatapy FAILED")
    else:
        print(f"\n✗ jsonatapy NOT AVAILABLE")

    # Run JavaScript
    js_time, _ = run_javascript(expression, data, iterations)

    if js_time > 0:
        print(f"\n✓ JavaScript")
        print(f"  Total time:     {js_time:8.2f} ms")
        print(f"  Per iteration:  {js_time/iterations:8.6f} ms")
    else:
        print(f"\n✗ JavaScript FAILED or NOT AVAILABLE")

    # Compare
    if jsonatapy_time > 0 and js_time > 0:
        speedup = js_time / jsonatapy_time
        print(f"\n{'='*70}")
        print("Comparison")
        print("="*70)

        if speedup > 1:
            print(f"jsonatapy is {speedup:.2f}x FASTER than JavaScript")
        else:
            print(f"jsonatapy is {1/speedup:.2f}x SLOWER than JavaScript")

        print(f"\nSpeedup factor: {speedup:.4f}x")


if __name__ == "__main__":
    main()
