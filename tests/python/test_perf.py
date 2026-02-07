import time

import jsonatapy

# Create varying sizes to see where the slowdown is
sizes = [10, 100, 500, 1000]

for size in sizes:
    data = {"items": [{"name": f"Item {i}", "price": i * 10} for i in range(size)]}

    expr = jsonatapy.compile("items.name")

    # Warmup
    for _ in range(5):
        expr.evaluate(data)

    # Time it
    iterations = 100
    start = time.perf_counter()
    for _ in range(iterations):
        result = expr.evaluate(data)
    end = time.perf_counter()

    avg_time = (end - start) / iterations
    per_item_us = (avg_time * 1_000_000) / size

    print(f"Size {size:4d}: {avg_time * 1000:.3f} ms total, {per_item_us:.2f} µs per item")
