import jsonatapy
import json
import time

# Test: Does keeping the same data object help?
# If conversion is the issue, reusing data should not help since PyO3 converts on each call

data = {"items": [{"name": f"Item {i}", "price": i} for i in range(1000)]}

# Compile once
expr = jsonatapy.compile("items.price")

# Test 1: Call evaluate many times with same data object
iterations = 100
start = time.perf_counter()
for _ in range(iterations):
    result = expr.evaluate(data)
elapsed1 = (time.perf_counter() - start) / iterations * 1000

print(f"Same data object: {elapsed1:.3f} ms per evaluation")

# Test 2: Recreate data each time
start = time.perf_counter()
for _ in range(iterations):
    fresh_data = {"items": [{"name": f"Item {i}", "price": i} for i in range(1000)]}
    result = expr.evaluate(fresh_data)
elapsed2 = (time.perf_counter() - start) / iterations * 1000

print(f"Fresh data each time: {elapsed2:.3f} ms per evaluation")

# Test 3: Smaller data (100 items) for comparison
small_data = {"items": [{"name": f"Item {i}", "price": i} for i in range(100)]}
start = time.perf_counter()
for _ in range(iterations):
    result = expr.evaluate(small_data)
elapsed3 = (time.perf_counter() - start) / iterations * 1000

print(f"Small data (100 items): {elapsed3:.3f} ms per evaluation")
print(f"Per-item overhead: {(elapsed1 - elapsed3) / 900:.3f} ms per 1000 items")
