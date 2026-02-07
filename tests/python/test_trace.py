import jsonatapy
import json

# Simplest possible case to isolate the issue
data = {"items": [{"name": "Item 1", "price": 100}]}

# This should work - simple field access
expr1 = jsonatapy.compile('items.{"n": name}')
try:
    result1 = expr1.evaluate(data)
    print("Simple field in object:")
    print(json.dumps(result1, indent=2))
except Exception as e:
    print(f"ERROR: {e}")

# Does the field need to be a Path?
expr2 = jsonatapy.compile("items.name")
try:
    result2 = expr2.evaluate(data)
    print("\nJust field access (no object):")
    print(json.dumps(result2, indent=2))
except Exception as e:
    print(f"ERROR: {e}")

# Let's try with a filter
expr3 = jsonatapy.compile('items[price > 50].{"n": name}')
try:
    result3 = expr3.evaluate(data)
    print("\nWith filter:")
    print(json.dumps(result3, indent=2))
except Exception as e:
    print(f"ERROR: {e}")

# And the simplest object construction case
expr4 = jsonatapy.compile('{"test": items[0].name}')
try:
    result4 = expr4.evaluate(data)
    print("\nObject at root with path value:")
    print(json.dumps(result4, indent=2))
except Exception as e:
    print(f"ERROR: {e}")
