import jsonatapy
import json

# Test evaluating object construction directly on an item
item = {"name": "Item 1", "price": 60, "stock": 100}

try:
    expr = jsonatapy.compile('{"name": name, "value": price * stock}')
    result = expr.evaluate(item)
    print('Direct object construction:')
    print(json.dumps(result, indent=2))
except Exception as e:
    print(f'Error: {e}')

# Test field access
try:
    expr2 = jsonatapy.compile('name')
    result2 = expr2.evaluate(item)
    print('\nField access:', result2)
except Exception as e:
    print(f'Error: {e}')

# Test arithmetic
try:
    expr3 = jsonatapy.compile('price * stock')
    result3 = expr3.evaluate(item)
    print('Arithmetic:', result3)
except Exception as e:
    print(f'Error: {e}')
