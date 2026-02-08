import jsonatapy

print("=" * 70)
print("Testing Higher-Order Functions: , , ")
print("=" * 70)

# Test
print("\n1. Testing ")
print("-" * 70)

data1 = {"items": [{"name": "A", "price": 10}, {"name": "B", "price": 20}]}
expr1 = jsonatapy.compile("(items, price)")
result1 = expr1.evaluate(data1)
print(f"Result: {result1}")
print("Expected: [10, 20]")
print(f"Pass: {result1 == [10, 20]}")

# Test
print("\n2. Testing ")
print("-" * 70)

data2 = {"numbers": [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]}
expr2 = jsonatapy.compile("(numbers, $ > 5)")
result2 = expr2.evaluate(data2)
print(f"Result: {result2}")
print("Expected: [6, 7, 8, 9, 10]")
print(f"Pass: {result2 == [6, 7, 8, 9, 10]}")

# Test chaining
print("\n3. Testing Chaining")
print("-" * 70)

data3 = {
    "items": [
        {"name": "Item 1", "price": 15, "inStock": True},
        {"name": "Item 2", "price": 25, "inStock": False},
        {"name": "Item 3", "price": 35, "inStock": True},
    ]
}

expr3 = jsonatapy.compile("((items, inStock), price)")
result3 = expr3.evaluate(data3)
print(f"Result: {result3}")
print("Expected: [15, 35]")
print(f"Pass: {result3 == [15, 35]}")

print("\n" + "=" * 70)
