import jsonatapy

# Test 1: Simple lambda with field access
data = {'items': [{'name': 'A', 'price': 10}]}
expr = jsonatapy.compile('$'+'map(items, function($'+'x) { $'+'x.price })')
result = expr.evaluate(data)
print(f'Test 1 Result: {result}')
print(f'Expected: [10]')
print()

# Test 2: Check what  evaluates to
data2 = {'obj': {'name': 'Test', 'value': 42}}
expr2 = jsonatapy.compile('function($'+'y) { $'+'y }')
# We can't directly evaluate a lambda, but we can use it in 
expr3 = jsonatapy.compile('$'+'map([obj], function($'+'y) { $'+'y })')
result3 = expr3.evaluate(data2)
print(f'Test 2 Result: {result3}')
print(f'Expected: [{"name": "Test", "value": 42}]')
