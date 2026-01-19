import jsonatapy
import json

# Test simple object construction
try:
    expr = jsonatapy.compile('{"x": 1, "y": 2}')
    result = expr.evaluate({})
    print('Simple object:', json.dumps(result))
except Exception as e:
    print(f'Error: {e}')

# Test object construction with field references
try:
    data = {"name": "Alice", "age": 30}
    expr2 = jsonatapy.compile('{"person": name, "years": age}')
    result2 = expr2.evaluate(data)
    print('Object with refs:', json.dumps(result2))
except Exception as e:
    print(f'Error: {e}')

# Test array mapping with object construction
try:
    data3 = {"items": [{"name": "a", "price": 20}, {"name": "b", "price": 30}]}
    expr3 = jsonatapy.compile('items.{"item": name, "cost": price}')
    result3 = expr3.evaluate(data3)
    print('Array mapping with objects:', json.dumps(result3))
except Exception as e:
    print(f'Error: {e}')
