"""
Host-callable custom functions (Python binding)

Register Python callables that a JSONata expression can invoke as ``$name(...)``:
  - enrichment/lookup functions (the canonical use case)
  - determinism injection: overriding an impure built-in ($now) with a frozen
    implementation for reproducible output
  - sandboxing: overriding a powerful built-in ($eval) to disable it

Run this after building the extension with: maturin develop
"""

import jsonatapy

print("=" * 60)
print("JSONataPy - Host Functions")
print("=" * 60)

# Example 1: enrichment lookup backed by host-owned data.
# The expression stays a clean artifact; the host owns the data source.
print("\n1. Enrichment lookup")
print("-" * 40)
catalog = {"A-1": "Widget", "B-2": "Gadget"}
expr = jsonatapy.compile("items.{ 'sku': sku, 'name': $productName(sku) }")
expr.register("productName", lambda sku: catalog.get(sku, "Unknown"))
result = expr.evaluate({"items": [{"sku": "A-1"}, {"sku": "B-2"}]})
print(result)

# Example 2: multi-argument enrichment. Arguments arrive already evaluated.
print("\n2. Multi-argument function")
print("-" * 40)
rates = {"EUR": 1.1, "GBP": 1.27}
expr = jsonatapy.compile("$convert(amount, currency)")
expr.register("convert", lambda amount, currency: amount * rates.get(currency, 1.0))
print(expr.evaluate({"amount": 10, "currency": "EUR"}))

# Example 3: determinism injection — freeze $now() for reproducible output.
print("\n3. Override $now (determinism)")
print("-" * 40)
expr = jsonatapy.compile("{ 'generatedAt': $now() }")
expr.register_override("now", lambda: "2020-01-01T00:00:00.000Z")
print(expr.evaluate(None))

# Example 4: sandboxing — disable $eval for semi-trusted expressions.
print("\n4. Sandbox $eval")
print("-" * 40)


def blocked(*_args):
    raise ValueError("$eval is disabled in this environment")


expr = jsonatapy.compile("$eval('1 + 1')")
expr.register_override("eval", blocked)
try:
    expr.evaluate(None)
except ValueError as exc:
    print(f"blocked as expected -> {exc}")

# Example 5: async def is rejected — do async I/O outside jsonata instead.
print("\n5. async def is rejected")
print("-" * 40)


async def fetch(_key):
    return "value"


expr = jsonatapy.compile("$fetch(key)")
expr.register("fetch", fetch)
try:
    expr.evaluate({"key": "k"})
except ValueError as exc:
    print(f"rejected as expected -> {exc}")

print("\n" + "=" * 60)
print("Done")
print("=" * 60)
