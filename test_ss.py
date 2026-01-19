import jsonatapy

# Test 1
data1 = {'items': [{'name': 'Alice', 'age': 25}, {'name': 'Bob', 'age': 30}]}
expr1 = jsonatapy.compile(r'$single(items, function($x) { $x.age = 30 })')
result1 = expr1.evaluate(data1)
print('Test 1 single:', result1['name'])

# Test 2
try:
    expr2 = jsonatapy.compile(r'$single(items, function($x) { $x.age > 50 })')
    result2 = expr2.evaluate(data1)
    print('Test 2: FAIL')
except Exception as e:
    print('Test 2 single no match: OK')

# Test 3
try:
    expr3 = jsonatapy.compile(r'$single(items, function($x) { $x.age > 20 })')
    result3 = expr3.evaluate(data1)
    print('Test 3: FAIL')
except Exception as e:
    print('Test 3 single multiple: OK')

# Test 4
data4 = {'product': {'name': 'Widget', 'price': 100, 'stock': 0}}
expr4 = jsonatapy.compile(r'$sift(product, function($v) { $v })')
result4 = expr4.evaluate(data4)
print('Test 4 sift:', list(result4.keys()))

# Test 5
data5 = {'metrics': {'clicks': 150, 'views': 10, 'errors': 5}}
expr5 = jsonatapy.compile(r'$sift(metrics, function($v) { $v > 10 })')
result5 = expr5.evaluate(data5)
print('Test 5 sift >10:', result5)
