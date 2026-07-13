"""Quick gate: lazy-views performance targets from the 2026-07-12 spec.

Run manually: uv run python benchmarks/python/lazy_check.py

Targets revised 2026-07-13 (Task 9b, selective field caching in
LazyPyDict::get_field): pre-implementation estimates replaced with
measured-post-implementation values x ~1.15 (regression tripwire, not an
aspirational target). products.price's target (13.0us) still sits below
jsonata-js's 14.6us on the dev machine (JS-parity bar); the other three
rows beat jsonata-js by a wide margin already.
"""
import time
import jsonatapy

def best_us(fn, n, trials=5):
    fn()
    best = float("inf")
    for _ in range(trials):
        t0 = time.perf_counter()
        for _ in range(n):
            fn()
        best = min(best, (time.perf_counter() - t0) / n * 1e6)
    return best

products = {"products": [{"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5,
                          "inStock": i % 2 == 0} for i in range(100)]}
ecommerce = {"products": [{"id": i, "name": f"Product {i}",
                           "category": ["Electronics", "Clothing", "Books", "Home"][i % 4],
                           "price": 10.0 + i * 5.5, "inStock": i % 3 != 0,
                           "rating": 3.0 + (i % 3) * 0.5, "reviews": i * 2,
                           "tags": [f"tag{j}" for j in range(i % 5)],
                           "vendor": {"name": f"Vendor {i % 10}", "rating": 4.0 + (i % 5) * 0.2}}
                          for i in range(100)]}

CASES = [
    ("products.price", products, 2000, 13.0),     # 11.3µs measured × 1.15
    ('products[category = "Electronics"]', ecommerce, 1000, 27.6),  # 24.0µs measured × 1.15
    ("$sum(products[inStock].price)", ecommerce, 1000, 20.2),       # 17.6µs measured × 1.15
    ('products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}',
     ecommerce, 500, 93.0),   # 80.9µs measured × 1.15 regression gate
]

failed = False
for src, data, iters, target in CASES:
    e = jsonatapy.compile(src)
    us = best_us(lambda: e.evaluate(data), iters)
    ok = us <= target
    failed |= not ok
    print(f"{'PASS' if ok else 'FAIL'}  {us:8.1f}µs (target ≤{target}µs)  {src[:60]}")

raise SystemExit(1 if failed else 0)
