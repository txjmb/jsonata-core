# Usage Guide

Common patterns and examples for jsonatapy.

## Basic Queries

### Simple Path Access

```python
import jsonatapy

data = {
    "user": {
        "name": "Alice",
        "email": "alice@example.com"
    }
}

# Single field
result = jsonatapy.evaluate("user.name", data)
# "Alice"

# Nested field
result = jsonatapy.evaluate("user.email", data)
# "alice@example.com"
```

### Array Access

```python
data = {
    "items": ["apple", "banana", "orange"]
}

# Single element
result = jsonatapy.evaluate("items[0]", data)
# "apple"

# Array slicing
result = jsonatapy.evaluate("items[1..2]", data)
# ["banana", "orange"]
```

## Filtering and Mapping

### Array Filtering

```python
data = {
    "products": [
        {"name": "Laptop", "price": 1200},
        {"name": "Mouse", "price": 25},
        {"name": "Keyboard", "price": 75}
    ]
}

# Filter by condition
result = jsonatapy.evaluate("products[price > 50]", data)
# [{"name": "Laptop", "price": 1200}, {"name": "Keyboard", "price": 75}]

# Extract specific field
result = jsonatapy.evaluate("products[price > 50].name", data)
# ["Laptop", "Keyboard"]
```

### Array Mapping

```python
# Transform array elements
result = jsonatapy.evaluate(
    'products.{"item": name, "cost": price}',
    data
)
# [{"item": "Laptop", "cost": 1200}, ...]
```

## Aggregation

```python
data = {
    "orders": [
        {"quantity": 2, "price": 10},
        {"quantity": 3, "price": 15},
        {"quantity": 1, "price": 20}
    ]
}

# Sum
total = jsonatapy.evaluate("$sum(orders.(quantity * price))", data)
# 85

# Count
count = jsonatapy.evaluate("$count(orders)", data)
# 3

# Average
avg = jsonatapy.evaluate("$average(orders.price)", data)
# 15

# Min/Max
min_price = jsonatapy.evaluate("$min(orders.price)", data)
max_price = jsonatapy.evaluate("$max(orders.price)", data)
```

## String Operations

```python
data = {"name": "alice"}

# Uppercase
result = jsonatapy.evaluate("$uppercase(name)", data)
# "ALICE"

# Lowercase
result = jsonatapy.evaluate("$lowercase(name)", data)
# "alice"

# Concatenation
result = jsonatapy.evaluate('"Hello, " & name', data)
# "Hello, alice"

# Substring
result = jsonatapy.evaluate('$substring("hello", 1, 4)', {})
# "ell"

# Contains
result = jsonatapy.evaluate('$contains("hello", "ell")', {})
# true
```

## Conditional Expressions

```python
data = {"price": 150}

# Ternary operator
result = jsonatapy.evaluate(
    'price > 100 ? "expensive" : "affordable"',
    data
)
# "expensive"

# Conditional field
result = jsonatapy.evaluate(
    '{"price": price, "category": price > 100 ? "premium" : "standard"}',
    data
)
# {"price": 150, "category": "premium"}
```

## Object Construction

```python
data = {
    "firstName": "Alice",
    "lastName": "Smith",
    "age": 30
}

# Build new object
result = jsonatapy.evaluate(
    '{"fullName": firstName & " " & lastName, "age": age}',
    data
)
# {"fullName": "Alice Smith", "age": 30}
```

## Using Bindings

```python
# Define variables
expr = jsonatapy.compile("items[price > $threshold].name")

result = expr.evaluate(
    {"items": [{"name": "A", "price": 100}, {"name": "B", "price": 50}]},
    {"threshold": 75}
)
# ["A"]
```

## Compiled Expressions

For repeated evaluations, compile once:

```python
expr = jsonatapy.compile("products[category=$cat].name")

# Evaluate with different data
data1 = {
    "products": [
        {"name": "Item1", "category": "electronics"},
        {"name": "Item2", "category": "books"}
    ]
}

result1 = expr.evaluate(data1, {"cat": "electronics"})
# ["Item1"]

result2 = expr.evaluate(data1, {"cat": "books"})
# ["Item2"]
```

## Higher-Order Functions

### Map

```python
data = {"numbers": [1, 2, 3, 4, 5]}

result = jsonatapy.evaluate(
    "$map(numbers, function($n) { $n * 2 })",
    data
)
# [2, 4, 6, 8, 10]
```

### Filter

```python
result = jsonatapy.evaluate(
    "$filter(numbers, function($n) { $n > 3 })",
    data
)
# [4, 5]
```

### Reduce

```python
result = jsonatapy.evaluate(
    "$reduce(numbers, function($acc, $n) { $acc + $n }, 0)",
    data
)
# 15
```

## Error Handling

```python
def safe_evaluate(expression, data):
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError as e:
        print(f"Error: {e}")
        return None

result = safe_evaluate("invalid[[syntax", {})
# Prints: Error: Parse error...
# Returns: None
```

## Performance Optimization

### Compile Once

```python
# Slow - compiles every time
for data in dataset:
    result = jsonatapy.evaluate("items[price > 100]", data)

# Fast - compile once
expr = jsonatapy.compile("items[price > 100]")
for data in dataset:
    result = expr.evaluate(data)
```

### JSON String API

For large datasets:

```python
import json

expr = jsonatapy.compile("items[price > 100]")

# Large data
data = {"items": [...]}  # 1000+ items

# Fast path
json_str = json.dumps(data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

## Real-World Examples

### API Response Transformation

```python
api_response = {
    "data": {
        "user": {
            "id": 123,
            "firstName": "Alice",
            "lastName": "Smith",
            "orders": [
                {"id": 1, "total": 100},
                {"id": 2, "total": 200}
            ]
        }
    }
}

expr = jsonatapy.compile('''
{
    "userId": data.user.id,
    "fullName": data.user.firstName & " " & data.user.lastName,
    "totalSpent": $sum(data.user.orders.total)
}
''')

result = expr.evaluate(api_response)
# {"userId": 123, "fullName": "Alice Smith", "totalSpent": 300}
```

### Data Filtering and Grouping

```python
transactions = {
    "transactions": [
        {"region": "North", "amount": 100},
        {"region": "South", "amount": 150},
        {"region": "North", "amount": 200}
    ]
}

# Sum by region
result = jsonatapy.evaluate(
    "$sum(transactions[region='North'].amount)",
    transactions
)
# 300
```

### ETL Pipeline

```python
raw_data = {
    "records": [
        {"name": "alice", "status": "active", "amount": 100},
        {"name": "bob", "status": "inactive", "amount": 200},
        {"name": "charlie", "status": "active", "amount": 150}
    ]
}

transform = jsonatapy.compile('''
records[status="active"].{
    "name": $uppercase(name),
    "value": amount * 1.1
}
''')

result = transform.evaluate(raw_data)
# [{"name": "ALICE", "value": 110}, {"name": "CHARLIE", "value": 165}]
```
