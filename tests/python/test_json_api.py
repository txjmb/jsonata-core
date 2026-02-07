import jsonatapy
import json
import time

print("=" * 70)
print("Benchmark: JSON String API vs Regular API")
print("=" * 70)

# Test data - 1000 item array
data = {"items": [{"name": f"Item {i}", "price": i, "stock": i * 10} for i in range(1000)]}
json_str = json.dumps(data)

# Compile expression
expr = jsonatapy.compile('items[price > 500].{"name": name, "value": price * stock}')

# Test 1: Regular API (with Python↔Rust conversion)
print("\nTest 1: Regular API (evaluate)")
iterations = 100
start = time.perf_counter()
for _ in range(iterations):
    result = expr.evaluate(data)
elapsed_regular = (time.perf_counter() - start) / iterations * 1000
print(f"Time: {elapsed_regular:.3f} ms per evaluation")

# Test 2: JSON String API (no Python↔Rust conversion)
print("\nTest 2: JSON String API (evaluate_json)")
start = time.perf_counter()
for _ in range(iterations):
    result_str = expr.evaluate_json(json_str)
elapsed_json = (time.perf_counter() - start) / iterations * 1000
print(f"Time: {elapsed_json:.3f} ms per evaluation")

# Calculate speedup
speedup = elapsed_regular / elapsed_json
print("\n" + "=" * 70)
print(f"Speedup: {speedup:.2f}x faster with JSON string API")
print(f"Time saved: {elapsed_regular - elapsed_json:.3f} ms per evaluation")
print(f"Percentage reduction: {(1 - elapsed_json / elapsed_regular) * 100:.1f}%")
print("=" * 70)

# Test 3: Smaller data (100 items) for comparison
print("\n\nTest 3: Smaller Data (100 items)")
small_data = {"items": [{"name": f"Item {i}", "price": i, "stock": i * 10} for i in range(100)]}
small_json_str = json.dumps(small_data)

start = time.perf_counter()
for _ in range(iterations):
    result = expr.evaluate(small_data)
elapsed_small_regular = (time.perf_counter() - start) / iterations * 1000

start = time.perf_counter()
for _ in range(iterations):
    result_str = expr.evaluate_json(small_json_str)
elapsed_small_json = (time.perf_counter() - start) / iterations * 1000

print(f"Regular API:    {elapsed_small_regular:.3f} ms")
print(f"JSON String API: {elapsed_small_json:.3f} ms")
print(f"Speedup: {elapsed_small_regular / elapsed_small_json:.2f}x")

# Verify results are the same
print("\n\nVerification:")
result_obj = expr.evaluate(data)
result_json = json.loads(expr.evaluate_json(json_str))
print(f"Results match: {result_obj == result_json}")
print(f"Result count: {len(result_obj)} items")
