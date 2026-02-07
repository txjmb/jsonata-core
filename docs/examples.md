# Examples

This page provides practical examples of using jsonatapy for common tasks.

## Basic Queries

### Simple Path Navigation

```python
import jsonatapy

data = {
    "user": {
        "name": "Alice",
        "email": "alice@example.com"
    }
}

# Simple path
result = jsonatapy.evaluate("user.name", data)
print(result)  # "Alice"

# Nested path
result = jsonatapy.evaluate("user.email", data)
print(result)  # "alice@example.com"
```

### Array Operations

```python
import jsonatapy

data = {
    "products": [
        {"name": "Widget", "price": 10.99, "inStock": True},
        {"name": "Gadget", "price": 24.99, "inStock": False},
        {"name": "Doohickey", "price": 5.99, "inStock": True}
    ]
}

# Filter array
result = jsonatapy.evaluate("products[inStock]", data)
# Returns: [{"name": "Widget", ...}, {"name": "Doohickey", ...}]

# Map array
result = jsonatapy.evaluate("products.name", data)
# Returns: ["Widget", "Gadget", "Doohickey"]

# Filter and map
result = jsonatapy.evaluate("products[price > 10].name", data)
# Returns: ["Gadget"]
```

## Data Transformation

### Object Construction

```python
import jsonatapy

data = {
    "firstName": "John",
    "lastName": "Doe",
    "age": 30
}

# Create new object structure
expression = '''
{
    "fullName": firstName & " " & lastName,
    "isAdult": age >= 18
}
'''

result = jsonatapy.evaluate(expression, data)
# Returns: {"fullName": "John Doe", "isAdult": true}
```

### Array Transformation

```python
import jsonatapy

data = {
    "orders": [
        {"id": 1, "total": 100, "items": 3},
        {"id": 2, "total": 250, "items": 5},
        {"id": 3, "total": 75, "items": 2}
    ]
}

# Transform array
expression = '''
orders{
    "orderId": id,
    "averagePrice": total / items
}
'''

result = jsonatapy.evaluate(expression, data)
```

## Aggregation

### Built-in Aggregation Functions

```python
import jsonatapy

data = {
    "sales": [
        {"amount": 100, "region": "North"},
        {"amount": 200, "region": "South"},
        {"amount": 150, "region": "North"}
    ]
}

# Sum
total = jsonatapy.evaluate("$sum(sales.amount)", data)
# Returns: 450

# Average
avg = jsonatapy.evaluate("$average(sales.amount)", data)
# Returns: 150

# Max
max_sale = jsonatapy.evaluate("$max(sales.amount)", data)
# Returns: 200

# Count
count = jsonatapy.evaluate("$count(sales)", data)
# Returns: 3
```

### Grouping and Aggregation

```python
import jsonatapy

data = {
    "sales": [
        {"amount": 100, "region": "North"},
        {"amount": 200, "region": "South"},
        {"amount": 150, "region": "North"},
        {"amount": 180, "region": "South"}
    ]
}

# Group by region and sum
expression = '''
sales{
    region: $sum(amount)
}
'''

result = jsonatapy.evaluate(expression, data)
# Returns: {"North": 250, "South": 380}
```

## String Operations

### String Functions

```python
import jsonatapy

data = {
    "text": "Hello, World!"
}

# Uppercase
result = jsonatapy.evaluate("$uppercase(text)", data)
# Returns: "HELLO, WORLD!"

# Lowercase
result = jsonatapy.evaluate("$lowercase(text)", data)
# Returns: "hello, world!"

# Substring
result = jsonatapy.evaluate("$substring(text, 0, 5)", data)
# Returns: "Hello"

# Contains
result = jsonatapy.evaluate("$contains(text, 'World')", data)
# Returns: true

# String concatenation
result = jsonatapy.evaluate("text & ' How are you?'", data)
# Returns: "Hello, World! How are you?"
```

## Advanced Features

### Higher-Order Functions

```python
import jsonatapy

data = {
    "numbers": [1, 2, 3, 4, 5]
}

# Map with lambda
result = jsonatapy.evaluate(
    "$map(numbers, function($v) { $v * 2 })",
    data
)
# Returns: [2, 4, 6, 8, 10]

# Filter with lambda
result = jsonatapy.evaluate(
    "$filter(numbers, function($v) { $v > 2 })",
    data
)
# Returns: [3, 4, 5]

# Reduce with lambda
result = jsonatapy.evaluate(
    "$reduce(numbers, function($acc, $v) { $acc + $v }, 0)",
    data
)
# Returns: 15
```

### Conditional Expressions

```python
import jsonatapy

data = {
    "temperature": 25,
    "unit": "C"
}

# Ternary operator
expression = 'temperature > 30 ? "Hot" : "Comfortable"'
result = jsonatapy.evaluate(expression, data)
# Returns: "Comfortable"

# Nested conditionals
expression = '''
temperature > 30 ? "Hot" :
temperature > 20 ? "Warm" :
temperature > 10 ? "Cool" : "Cold"
'''
result = jsonatapy.evaluate(expression, data)
# Returns: "Warm"
```

## Performance Optimization

### Using JsonataData Handles

For repeated queries on the same data, use `JsonataData` handles to avoid re-parsing the data:

```python
import jsonatapy

# Parse data once
data_handle = jsonatapy.JsonataData(large_dataset)

# Reuse the parsed data for multiple queries
expr1 = jsonatapy.JsonataExpression("products[category='Electronics']")
result1 = expr1.evaluate_with_data(data_handle)

expr2 = jsonatapy.JsonataExpression("$sum(products.price)")
result2 = expr2.evaluate_with_data(data_handle)

# Much faster than calling evaluate() multiple times with the same dict
```

### Pre-compiling Expressions

For repeated evaluations with different data, pre-compile the expression:

```python
import jsonatapy

# Compile once
expr = jsonatapy.JsonataExpression("products[price > threshold].name")

# Evaluate multiple times with different data
for dataset in datasets:
    result = expr.evaluate(dataset)
    print(result)
```

## Real-World Example

### E-Commerce Order Processing

```python
import jsonatapy

orders_data = {
    "orders": [
        {
            "id": "ORD-001",
            "customer": "Alice",
            "items": [
                {"product": "Widget", "price": 10.99, "qty": 2},
                {"product": "Gadget", "price": 24.99, "qty": 1}
            ],
            "status": "pending"
        },
        {
            "id": "ORD-002",
            "customer": "Bob",
            "items": [
                {"product": "Doohickey", "price": 5.99, "qty": 5}
            ],
            "status": "shipped"
        }
    ]
}

# Calculate total value of all orders
expression = '''
{
    "totalOrders": $count(orders),
    "totalRevenue": $sum(orders.items.(price * qty)),
    "pendingOrders": $count(orders[status='pending']),
    "averageOrderValue": $sum(orders.items.(price * qty)) / $count(orders)
}
'''

result = jsonatapy.evaluate(expression, orders_data)
print(result)
# {
#     "totalOrders": 2,
#     "totalRevenue": 76.92,
#     "pendingOrders": 1,
#     "averageOrderValue": 38.46
# }
```

## See Also

- [API Reference](api.md) - Complete API documentation
- [JSONata Language](jsonata-language.md) - Language specification
- [Performance](performance.md) - Performance optimization guide
