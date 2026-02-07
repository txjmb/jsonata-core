#!/usr/bin/env python3
"""Quick comparison: JSON API vs JavaScript for array operations"""

import json
import subprocess
import time
from pathlib import Path

import jsonatapy


def run_js_benchmark(expression, data, iterations):
    """Run JavaScript benchmark"""
    bench_script = Path("benchmarks/benchmark.js")
    if not bench_script.exists():
        return -1

    benchmark_data = {"expression": expression, "data": data, "iterations": iterations}

    try:
        result = subprocess.run(
            ["node", str(bench_script)],
            input=json.dumps(benchmark_data),
            capture_output=True,
            text=True,
            timeout=30,
        )
        if result.returncode == 0:
            return float(result.stdout.strip())
    except Exception:
        pass
    return -1


# Test: Array mapping (100 items)
print("=" * 70)
print("Benchmark: Array Mapping (100 items)")
print("=" * 70)

data = {
    "products": [{"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5} for i in range(100)]
}
expression = "products.price"
iterations = 1000

# JavaScript
js_time = run_js_benchmark(expression, data, iterations)
print(f"JavaScript: {js_time:8.2f} ms ({js_time / iterations:8.4f} ms/iter)")

# Python Regular API
expr = jsonatapy.compile(expression)
start = time.perf_counter()
for _ in range(iterations):
    result = expr.evaluate(data)
py_time = (time.perf_counter() - start) * 1000
print(f"Python (regular): {py_time:8.2f} ms ({py_time / iterations:8.4f} ms/iter)")
print(f"  Slowdown: {py_time / js_time:.2f}x")

# Python JSON API
json_str = json.dumps(data)
start = time.perf_counter()
for _ in range(iterations):
    result_str = expr.evaluate_json(json_str)
py_json_time = (time.perf_counter() - start) * 1000
print(f"Python (JSON API): {py_json_time:8.2f} ms ({py_json_time / iterations:8.4f} ms/iter)")
print(f"  Speedup vs Regular: {py_time / py_json_time:.2f}x")
print(
    f"  vs JavaScript: {py_json_time / js_time:.2f}x (slower)"
    if py_json_time > js_time
    else f"  vs JavaScript: {js_time / py_json_time:.2f}x (faster)"
)

# Test 2: Large array (1000 items)
print("\n" + "=" * 70)
print("Benchmark: Array Sum (1000 items)")
print("=" * 70)

large_data = {"prices": [10.0 + i * 0.5 for i in range(1000)]}
expression2 = "(prices)"
iterations2 = 100

js_time2 = run_js_benchmark(expression2, large_data, iterations2)
print(f"JavaScript: {js_time2:8.2f} ms ({js_time2 / iterations2:8.4f} ms/iter)")

expr2 = jsonatapy.compile(expression2)
json_str2 = json.dumps(large_data)
start = time.perf_counter()
for _ in range(iterations2):
    result_str = expr2.evaluate_json(json_str2)
py_json_time2 = (time.perf_counter() - start) * 1000
print(f"Python (JSON API): {py_json_time2:8.2f} ms ({py_json_time2 / iterations2:8.4f} ms/iter)")
print(f"  vs JavaScript: {py_json_time2 / js_time2:.2f}x")
