import jsonatapy
import json
import time

print("="*60)
print("End-to-End Validation Tests")
print("="*60)

# Test 1: Basic object construction
print("\nTest 1: Basic object construction")
data1 = {"name": "Alice", "age": 30}
expr1 = jsonatapy.compile('{"person": name, "years": age}')
result1 = expr1.evaluate(data1)
assert result1 == {"person": "Alice", "years": 30}, f"Failed: {result1}"
print("✓ PASSED")

# Test 2: Array mapping with objects
print("\nTest 2: Array mapping with objects")
data2 = {"items": [{"name": "A", "price": 20}, {"name": "B", "price": 30}]}
expr2 = jsonatapy.compile('items.{"item": name, "cost": price}')
result2 = expr2.evaluate(data2)
assert len(result2) == 2, f"Expected 2 items, got {len(result2)}"
assert result2[0]["item"] == "A", f"Field mapping failed: {result2[0]}"
print("✓ PASSED")

# Test 3: Filter + object construction (CRITICAL TEST)
print("\nTest 3: Filter + object construction (CRITICAL)")
data3 = {"items": [{"name": "Item 1", "price": 60}, {"name": "Item 2", "price": 40}]}
expr3 = jsonatapy.compile('items[price > 50].{"name": name, "double": price * 2}')
result3 = expr3.evaluate(data3)
assert len(result3) == 1, f"Filter failed: expected 1 item, got {len(result3)}"
assert result3[0]["name"] == "Item 1", f"Object construction failed: {result3[0]}"
assert result3[0]["double"] == 120.0, f"Computed field failed: {result3[0]['double']}"
print("✓ PASSED")

# Test 4: Complex boolean expressions with filter
print("\nTest 4: Complex boolean filter + object")
data4 = {
    "items": [
        {"name": "Item 1", "price": 60, "stock": 100},
        {"name": "Item 2", "price": 40, "stock": 200},
        {"name": "Item 3", "price": 70, "stock": 120},
    ]
}
expr4 = jsonatapy.compile('items[price > 50 and stock < 150].{"name": name, "value": price * stock}')
result4 = expr4.evaluate(data4)
assert len(result4) == 2, f"Complex filter failed: expected 2 items, got {len(result4)}"
print("✓ PASSED")

# Test 5: Performance benchmark (1000 items)
print("\nTest 5: Performance benchmark (1000 items)")
large_data = {"items": [{"name": f"Item {i}", "price": i, "stock": i*10} for i in range(1000)]}
expr5 = jsonatapy.compile('items[price > 500].{"name": name, "value": price * stock}')

# Warmup
for _ in range(10):
    _ = expr5.evaluate(large_data)

# Benchmark
start = time.time()
iterations = 100
for _ in range(iterations):
    result5 = expr5.evaluate(large_data)
elapsed = (time.time() - start) / iterations * 1000

assert len(result5) == 499, f"Performance test filter failed: expected 499, got {len(result5)}"
print(f"✓ PASSED - Performance: {elapsed:.2f}ms per evaluation (1000 items, 499 matches)")

# Test 6: Nested object construction
print("\nTest 6: Nested object construction")
data6 = {
    "users": [
        {"name": "Alice", "orders": [{"id": 1, "total": 100}]},
        {"name": "Bob", "orders": [{"id": 2, "total": 200}]}
    ]
}
expr6 = jsonatapy.compile('users.{"user": name, "orderCount": (orders)}')
result6 = expr6.evaluate(data6)
assert len(result6) == 2, f"Nested construction failed: {len(result6)}"
assert result6[0]["user"] == "Alice", f"Nested field failed: {result6[0]}"
print("✓ PASSED")

print("\n" + "="*60)
print("✅ ALL TESTS PASSED!")
print("="*60)
