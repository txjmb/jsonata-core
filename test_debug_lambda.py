import jsonatapy
import json

# Test: What does the lambda body evaluate to?
data = {'items': [{'name': 'A', 'price': 10}]}

# First, verify that simple field access works on the data
print('1. Direct field access on array item:')
expr1 = jsonatapy.compile('items[0].price')
result1 = expr1.evaluate(data)
print(f'   items[0].price = {result1}')
print()

# Now test with  using a simple expression (no lambda)
print('2.  with simple expression:')
expr2 = jsonatapy.compile('$'+'map(items, price)')
result2 = expr2.evaluate(data)
print(f'   $'+'map(items, price) = {result2}')
print()

# Now test  with lambda returning just 
print('3.  with lambda returning :')
expr3 = jsonatapy.compile('$'+'map(items, function($'+'x) { $'+'x })')
result3 = expr3.evaluate(data)
print(f'   $'+'map(items, function($'+'x) {{ $'+'x }}) = {json.dumps(result3)}')
print()

# Now test  with lambda accessing .price
print('4.  with lambda accessing .price:')
expr4 = jsonatapy.compile('$'+'map(items, function($'+'x) { $'+'x.price })')
result4 = expr4.evaluate(data)
print(f'   $'+'map(items, function($'+'x) {{ $'+'x.price }}) = {result4}')
print(f'   Expected: [10]')
