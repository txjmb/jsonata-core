import jsonatapy
import json

# Even simpler test
data = {
    "items": [
        {"name": "Item 1", "price": 60},
        {"name": "Item 2", "price": 40},
    ]
}

# Test just filtering (should work)
expr1 = jsonatapy.compile("items[price > 50]")
result1 = expr1.evaluate(data)
print("Just filtering:")
print(json.dumps(result1, indent=2))

# Test just mapping with object (should work based on earlier test)
expr2 = jsonatapy.compile('items.{"name": name, "double": price * 2}')
result2 = expr2.evaluate(data)
print("\nJust mapping:")
print(json.dumps(result2, indent=2))

# Test combined (this is where it fails)
expr3 = jsonatapy.compile('items[price > 50].{"name": name, "double": price * 2}')
result3 = expr3.evaluate(data)
print("\nCombined:")
print(json.dumps(result3, indent=2))
