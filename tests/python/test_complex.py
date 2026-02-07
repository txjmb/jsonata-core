import jsonatapy
import json

# Test the complex expression from the benchmark
data = {
    "items": [
        {"name": "Item 1", "price": 60, "stock": 100},
        {"name": "Item 2", "price": 40, "stock": 200},
        {"name": "Item 3", "price": 70, "stock": 120},
        {"name": "Item 4", "price": 55, "stock": 80},
    ]
}

try:
    expr = jsonatapy.compile(
        'items[price > 50 and stock < 150].{"name": name, "value": price * stock}'
    )
    result = expr.evaluate(data)
    print("Complex expression result:")
    print(json.dumps(result, indent=2))
except Exception as e:
    print(f"Error: {e}")
