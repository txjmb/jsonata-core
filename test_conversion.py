import jsonatapy
import time
import json

size = 1000
data = {
    "items": [
        {"name": f"Item {i}", "price": i * 10}
        for i in range(size)
    ]
}

expr = jsonatapy.compile("items.name")

# Test just the evaluate() call overhead
iterations = 100

# Time full evaluate
start = time.perf_counter()
for _ in range(iterations):
    result = expr.evaluate(data)
end = time.perf_counter()
full_time = (end - start) / iterations

print(f"Full evaluate: {full_time * 1000:.3f} ms ({full_time * 1_000_000:.1f} µs)")

# Compare to JSON conversion overhead (rough estimate)
start = time.perf_counter()
for _ in range(iterations):
    # Simulate what happens: dict -> JSON -> dict
    json_str = json.dumps(data)
    parsed = json.loads(json_str)
end = time.perf_counter()
json_time = (end - start) / iterations

print(f"JSON round-trip: {json_time * 1000:.3f} ms ({json_time * 1_000_000:.1f} µs)")
print(f"Conversion overhead: ~{json_time / full_time * 100:.0f}% of total time")
