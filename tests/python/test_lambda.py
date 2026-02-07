#!/usr/bin/env python3
"""Test higher-order functions with proper lambda syntax"""

import json

import jsonatapy

print("=" * 70)
print("Testing Higher-Order Functions with Lambda Syntax")
print("=" * 70)

# Test 1: $map with lambda
print("\n1. $map with lambda function($x) { $x * 2 }")
print("-" * 70)
data1 = {"numbers": [1, 2, 3, 4, 5]}
expr1 = jsonatapy.compile("$map(numbers, function($x) { $x * 2 })")
try:
    result1 = expr1.evaluate(data1)
    print(f"Result: {result1}")
    print("Expected: [2, 4, 6, 8, 10]")
    print("✓ PASS" if result1 == [2, 4, 6, 8, 10] else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 2: $map with field access (no lambda)
print("\n2. $map with simple expression (items.price)")
print("-" * 70)
data2 = {"items": [{"name": "A", "price": 10}, {"name": "B", "price": 20}]}
expr2 = jsonatapy.compile("$map(items, price)")
try:
    result2 = expr2.evaluate(data2)
    print(f"Result: {result2}")
    print("Expected: [10, 20]")
    print("✓ PASS" if result2 == [10, 20] else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 3: $filter with lambda
print("\n3. $filter with lambda function($x) { $x > 5 }")
print("-" * 70)
data3 = {"numbers": [1, 3, 5, 7, 9, 11]}
expr3 = jsonatapy.compile("$filter(numbers, function($x) { $x > 5 })")
try:
    result3 = expr3.evaluate(data3)
    print(f"Result: {result3}")
    print("Expected: [7, 9, 11]")
    print("✓ PASS" if result3 == [7, 9, 11] else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 4: $filter with simple expression
print("\n4. $filter with simple expression (price > 15)")
print("-" * 70)
expr4 = jsonatapy.compile("$filter(items, price > 15)")
try:
    result4 = expr4.evaluate(data2)
    print(f"Result: {json.dumps(result4, indent=2)}")
    print("Expected: 1 item with price 20")
    print("✓ PASS" if len(result4) == 1 and result4[0]["price"] == 20 else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 5: $reduce with lambda
print("\n5. $reduce with lambda function($acc, $val) { $acc + $val }")
print("-" * 70)
data5 = {"numbers": [1, 2, 3, 4, 5]}
expr5 = jsonatapy.compile("$reduce(numbers, function($acc, $val) { $acc + $val }, 0)")
try:
    result5 = expr5.evaluate(data5)
    print(f"Result: {result5}")
    print("Expected: 15")
    print("✓ PASS" if result5 == 15 else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 6: $reduce for product
print("\n6. $reduce with lambda function($acc, $val) { $acc * $val }")
print("-" * 70)
expr6 = jsonatapy.compile("$reduce(numbers, function($acc, $val) { $acc * $val }, 1)")
try:
    result6 = expr6.evaluate(data5)
    print(f"Result: {result6}")
    print("Expected: 120")
    print("✓ PASS" if result6 == 120 else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 7: Chaining $filter and $map with lambdas
print("\n7. Chaining $filter and $map with lambdas")
print("-" * 70)
data7 = {
    "items": [
        {"name": "Item 1", "price": 10},
        {"name": "Item 2", "price": 20},
        {"name": "Item 3", "price": 30},
    ]
}
expr7 = jsonatapy.compile(
    "$map($filter(items, function($x) { $x.price > 15 }), function($x) { $x.name })"
)
try:
    result7 = expr7.evaluate(data7)
    print(f"Result: {result7}")
    print("Expected: ['Item 2', 'Item 3']")
    print("✓ PASS" if result7 == ["Item 2", "Item 3"] else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

# Test 8: Lambda with object access
print("\n8. $map with lambda accessing object fields")
print("-" * 70)
expr8 = jsonatapy.compile("$map(items, function($x) { $x.price * 2 })")
try:
    result8 = expr8.evaluate(data7)
    print(f"Result: {result8}")
    print("Expected: [20, 40, 60]")
    print("✓ PASS" if result8 == [20, 40, 60] else "✗ FAIL")
except Exception as e:
    print(f"✗ ERROR: {e}")

print("\n" + "=" * 70)
print("Testing Complete")
print("=" * 70)
