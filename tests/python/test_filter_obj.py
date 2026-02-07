import jsonatapy
import json

data = {
    "items": [
        {"name": "Item 1", "price": 60, "stock": 100},
        {"name": "Item 2", "price": 40, "stock": 200},
    ]
}

# Test filtering alone
try:
    expr1 = jsonatapy.compile("items[price > 50]")
    result1 = expr1.evaluate(data)
    print("Filtering:")
    print(json.dumps(result1, indent=2))
except Exception as e:
    print(f"Error: {e}")

# Test object construction after filtering
try:
    expr2 = jsonatapy.compile('items[price > 50].{"name": name, "value": price * stock}')
    result2 = expr2.evaluate(data)
    print("\nFiltering + Object construction:")
    print(json.dumps(result2, indent=2))
except Exception as e:
    print(f"Error: {e}")
