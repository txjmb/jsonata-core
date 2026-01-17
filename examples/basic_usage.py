"""
Basic usage examples for jsonatapy

Run this after building the extension with: maturin develop
"""

import jsonatapy

print("=" * 60)
print("JSONataPy - Basic Usage Examples")
print("=" * 60)

# Example 1: Simple literals
print("\n1. Simple Literals")
print("-" * 40)
result = jsonatapy.evaluate("42", {})
print(f'jsonatapy.evaluate("42", {{}}) = {result}')

result = jsonatapy.evaluate('"Hello, World!"', {})
print(f'jsonatapy.evaluate(\'"Hello, World!"\', {{}}) = {result}')

# Example 2: Arithmetic
print("\n2. Arithmetic Operations")
print("-" * 40)
result = jsonatapy.evaluate("1 + 2 * 3", {})
print(f'jsonatapy.evaluate("1 + 2 * 3", {{}}) = {result}')

result = jsonatapy.evaluate("(10 - 5) / 2", {})
print(f'jsonatapy.evaluate("(10 - 5) / 2", {{}}) = {result}')

# Example 3: Path navigation
print("\n3. Path Navigation")
print("-" * 40)
data = {
    "user": {
        "name": "Alice",
        "age": 30,
        "address": {
            "city": "New York",
            "zip": "10001"
        }
    }
}

result = jsonatapy.evaluate("user.name", data)
print(f'user.name = {result}')

result = jsonatapy.evaluate("user.address.city", data)
print(f'user.address.city = {result}')

# Example 4: String functions
print("\n4. String Functions")
print("-" * 40)
result = jsonatapy.evaluate('$uppercase("hello world")', {})
print(f'$uppercase("hello world") = {result}')

result = jsonatapy.evaluate('$length("JSONata")', {})
print(f'$length("JSONata") = {result}')

result = jsonatapy.evaluate('$substring("JSONata", 0, 4)', {})
print(f'$substring("JSONata", 0, 4) = {result}')

# Example 5: Numeric functions
print("\n5. Numeric Functions")
print("-" * 40)
data = {"prices": [10.50, 25.99, 15.00, 30.25]}

result = jsonatapy.evaluate("$sum(prices)", data)
print(f'$sum(prices) = ${result:.2f}')

result = jsonatapy.evaluate("$max(prices)", data)
print(f'$max(prices) = ${result:.2f}')

result = jsonatapy.evaluate("$average(prices)", data)
print(f'$average(prices) = ${result:.2f}')

# Example 6: Array functions
print("\n6. Array Functions")
print("-" * 40)
data = {"items": [1, 2, 2, 3, 3, 3, 4]}

result = jsonatapy.evaluate("$count(items)", data)
print(f'$count(items) = {result}')

result = jsonatapy.evaluate("$distinct(items)", data)
print(f'$distinct(items) = {result}')

result = jsonatapy.evaluate("$sort(items)", data)
print(f'$sort(items) = {result}')

# Example 7: Object functions
print("\n7. Object Functions")
print("-" * 40)
data = {"a": 1, "b": 2, "c": 3}

result = jsonatapy.evaluate("$keys($)", data)
print(f'$keys($) = {result}')

# Example 8: Compiled expressions (reusable)
print("\n8. Compiled Expressions (Efficient Reuse)")
print("-" * 40)
expr = jsonatapy.compile("user.name")

user1 = {"user": {"name": "Alice"}}
user2 = {"user": {"name": "Bob"}}
user3 = {"user": {"name": "Charlie"}}

print(f'Compiled: "user.name"')
print(f'  User 1: {expr.evaluate(user1)}')
print(f'  User 2: {expr.evaluate(user2)}')
print(f'  User 3: {expr.evaluate(user3)}')

# Example 9: Comparison and logical operations
print("\n9. Comparison and Logical Operations")
print("-" * 40)
data = {"age": 25, "hasLicense": True}

result = jsonatapy.evaluate("age >= 18", data)
print(f'age >= 18 = {result}')

result = jsonatapy.evaluate("age >= 18 and hasLicense", data)
print(f'age >= 18 and hasLicense = {result}')

# Example 10: Conditional expressions
print("\n10. Conditional Expressions")
print("-" * 40)
data = {"temperature": 25}

result = jsonatapy.evaluate('temperature > 30 ? "hot" : "comfortable"', data)
print(f'temperature > 30 ? "hot" : "comfortable" = {result}')

# Example 11: Variable bindings
print("\n11. Variable Bindings")
print("-" * 40)
data = {"price": 100}
bindings = {"taxRate": 0.1}

result = jsonatapy.evaluate("price * (1 + $taxRate)", data, bindings)
print(f'price * (1 + $taxRate) where taxRate=0.1 = ${result:.2f}')

# Example 12: Complex real-world example
print("\n12. Complex Real-World Example")
print("-" * 40)
data = {
    "orders": [
        {"id": 1, "product": "Laptop", "price": 999.99, "quantity": 1},
        {"id": 2, "product": "Mouse", "price": 29.99, "quantity": 2},
        {"id": 3, "product": "Keyboard", "price": 79.99, "quantity": 1},
        {"id": 4, "product": "Monitor", "price": 299.99, "quantity": 2},
    ]
}

# Calculate total for each order
print("Order totals:")
for order in data["orders"]:
    expr = jsonatapy.compile("price * quantity")
    total = expr.evaluate(order)
    print(f'  {order["product"]}: ${total:.2f}')

# Calculate grand total using $sum
all_totals = [order["price"] * order["quantity"] for order in data["orders"]]
grand_total_data = {"totals": all_totals}
grand_total = jsonatapy.evaluate("$sum(totals)", grand_total_data)
print(f'\nGrand Total: ${grand_total:.2f}')

print("\n" + "=" * 60)
print("Examples complete!")
print("=" * 60)
