import jsonatapy
import json

data = {"items": [{"name": "Item 1", "price": 60, "stock": 100}]}

# Step 1: Get filtered array
expr1 = jsonatapy.compile('items[price > 50]')
filtered = expr1.evaluate(data)
print('Filtered:', json.dumps(filtered))

# Step 2: Apply object mapping - what is the input context?
# Starting from the filtered array, apply object transform
expr2 = jsonatapy.compile('.{"name": name, "value": price * stock}')
result = expr2.evaluate(filtered)
print('Result:', json.dumps(result))
