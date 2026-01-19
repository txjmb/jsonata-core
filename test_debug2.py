import jsonatapy

data = {'items': [{'name': 'A', 'price': 10}]}

print('Test 1: items[0].price')
expr1 = jsonatapy.compile('items[0].price')
result1 = expr1.evaluate(data)
print('Result:', result1)
print()

print('Test 2: map with simple expression')
expr2 = jsonatapy.compile('$'+'map(items, price)')
result2 = expr2.evaluate(data)
print('Result:', result2)
print()

print('Test 3: map with lambda returning x')
expr3 = jsonatapy.compile('$'+'map(items, function($'+'x) { $'+'x })')
result3 = expr3.evaluate(data)
print('Result:', result3)
print()

print('Test 4: map with lambda accessing x.price')
expr4 = jsonatapy.compile('$'+'map(items, function($'+'x) { $'+'x.price })')
result4 = expr4.evaluate(data)
print('Result:', result4)
print('Expected: [10]')
