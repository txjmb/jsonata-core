# Usage Guide

Common patterns and examples for using jsonatapy effectively.

## Table of Contents

- [Basic Queries](#basic-queries)
- [Filtering and Selection](#filtering-and-selection)
- [Transformations](#transformations)
- [Aggregations](#aggregations)
- [Lambda Functions](#lambda-functions)
- [Higher-Order Functions](#higher-order-functions)
- [Complex Patterns](#complex-patterns)
- [Real-World Examples](#real-world-examples)

## Basic Queries

### Path Navigation

```python
import jsonatapy

data = {
    "person": {
        "name": "Alice",
        "age": 30,
        "address": {
            "city": "San Francisco",
            "state": "CA"
        }
    }
}

# Simple field access
name = jsonatapy.evaluate("person.name", data)
print(name)  # "Alice"

# Nested field access
city = jsonatapy.evaluate("person.address.city", data)
print(city)  # "San Francisco"

# Multiple fields
result = jsonatapy.evaluate("person.{name, age}", data)
print(result)  # {"name": "Alice", "age": 30}
```

### Array Access

```python
data = {
    "items": ["apple", "banana", "cherry"]
}

# Access by index
first = jsonatapy.evaluate("items[0]", data)
print(first)  # "apple"

# Last item
last = jsonatapy.evaluate("items[-1]", data)
print(last)  # "cherry"

# All items
all_items = jsonatapy.evaluate("items", data)
print(all_items)  # ["apple", "banana", "cherry"]

# Array of objects
data = {
    "products": [
        {"name": "Widget", "price": 10},
        {"name": "Gadget", "price": 20}
    ]
}

# Extract field from all items
names = jsonatapy.evaluate("products.name", data)
print(names)  # ["Widget", "Gadget"]
```

## Filtering and Selection

### Predicates

```python
data = {
    "products": [
        {"name": "Widget", "price": 10, "stock": 5},
        {"name": "Gadget", "price": 20, "stock": 0},
        {"name": "Doohickey", "price": 15, "stock": 10}
    ]
}

# Simple filter
in_stock = jsonatapy.evaluate("products[stock > 0]", data)
print(in_stock)
# [{"name": "Widget", ...}, {"name": "Doohickey", ...}]

# Multiple conditions
expensive = jsonatapy.evaluate("products[price > 15 and stock > 0]", data)
print(expensive)
# [{"name": "Doohickey", ...}]

# Extract after filtering
names = jsonatapy.evaluate("products[price > 10].name", data)
print(names)  # ["Gadget", "Doohickey"]
```

### Comparison Operators

```python
data = {"items": [1, 2, 3, 4, 5]}

# Greater than
result = jsonatapy.evaluate("items[$ > 3]", data)
print(result)  # [4, 5]

# Less than or equal
result = jsonatapy.evaluate("items[$ <= 2]", data)
print(result)  # [1, 2]

# Equality
data = {"users": [{"name": "Alice", "role": "admin"},
                   {"name": "Bob", "role": "user"}]}
admins = jsonatapy.evaluate("users[role = 'admin']", data)
print(admins)  # [{"name": "Alice", "role": "admin"}]

# Not equal
result = jsonatapy.evaluate("users[role != 'admin'].name", data)
print(result)  # ["Bob"]
```

### Logical Operators

```python
data = {
    "products": [
        {"name": "A", "price": 10, "featured": True},
        {"name": "B", "price": 50, "featured": False},
        {"name": "C", "price": 30, "featured": True}
    ]
}

# AND
result = jsonatapy.evaluate(
    "products[price > 20 and featured].name",
    data
)
print(result)  # ["C"]

# OR
result = jsonatapy.evaluate(
    "products[price < 15 or featured].name",
    data
)
print(result)  # ["A", "C"]

# NOT
result = jsonatapy.evaluate(
    "products[not featured].name",
    data
)
print(result)  # ["B"]
```

## Transformations

### Object Construction

```python
data = {
    "user": {
        "firstName": "Alice",
        "lastName": "Smith",
        "email": "alice@example.com"
    }
}

# Create new object structure
result = jsonatapy.evaluate('''
    {
        "fullName": user.firstName & " " & user.lastName,
        "contact": user.email
    }
''', data)
print(result)
# {"fullName": "Alice Smith", "contact": "alice@example.com"}

# Transform array of objects
data = {
    "products": [
        {"name": "Widget", "price": 10, "quantity": 5},
        {"name": "Gadget", "price": 20, "quantity": 3}
    ]
}

result = jsonatapy.evaluate('''
    products.{
        "item": name,
        "total": price * quantity,
        "inStock": quantity > 0
    }
''', data)
print(result)
# [
#   {"item": "Widget", "total": 50, "inStock": true},
#   {"item": "Gadget", "total": 60, "inStock": true}
# ]
```

### String Operations

```python
data = {"name": "alice", "message": "Hello World"}

# Uppercase
result = jsonatapy.evaluate("$uppercase(name)", data)
print(result)  # "ALICE"

# Lowercase
result = jsonatapy.evaluate("$lowercase(message)", data)
print(result)  # "hello world"

# Substring
result = jsonatapy.evaluate("$substring(message, 0, 5)", data)
print(result)  # "Hello"

# Concatenation
result = jsonatapy.evaluate('"Hello, " & name & "!"', data)
print(result)  # "Hello, alice!"

# String length
result = jsonatapy.evaluate("$length(message)", data)
print(result)  # 11
```

### Numeric Operations

```python
data = {"a": 10, "b": 3, "prices": [10, 20, 30]}

# Arithmetic
result = jsonatapy.evaluate("a + b", data)
print(result)  # 13

result = jsonatapy.evaluate("a * b", data)
print(result)  # 30

result = jsonatapy.evaluate("a / b", data)
print(result)  # 3.333...

# Rounding
result = jsonatapy.evaluate("$round(a / b)", data)
print(result)  # 3

result = jsonatapy.evaluate("$round(a / b, 2)", data)
print(result)  # 3.33

# Math functions
result = jsonatapy.evaluate("$sqrt(16)", data)
print(result)  # 4.0

result = jsonatapy.evaluate("$power(2, 3)", data)
print(result)  # 8

result = jsonatapy.evaluate("$abs(-5)", data)
print(result)  # 5
```

## Aggregations

### Array Functions

```python
data = {
    "numbers": [1, 2, 3, 4, 5],
    "items": [
        {"name": "A", "price": 10},
        {"name": "B", "price": 20},
        {"name": "C", "price": 30}
    ]
}

# Sum
total = jsonatapy.evaluate("$sum(numbers)", data)
print(total)  # 15

# Sum with mapping
total = jsonatapy.evaluate("$sum(items.price)", data)
print(total)  # 60

# Average
avg = jsonatapy.evaluate("$average(numbers)", data)
print(avg)  # 3.0

# Min/Max
min_val = jsonatapy.evaluate("$min(numbers)", data)
print(min_val)  # 1

max_val = jsonatapy.evaluate("$max(items.price)", data)
print(max_val)  # 30

# Count
count = jsonatapy.evaluate("$count(items)", data)
print(count)  # 3
```

### Grouping

```python
data = {
    "sales": [
        {"region": "North", "amount": 100},
        {"region": "South", "amount": 150},
        {"region": "North", "amount": 200},
        {"region": "South", "amount": 120}
    ]
}

# Group by region and sum
result = jsonatapy.evaluate('''
    {
        "North": $sum(sales[region="North"].amount),
        "South": $sum(sales[region="South"].amount)
    }
''', data)
print(result)
# {"North": 300, "South": 270}
```

## Lambda Functions

### Basic Lambda

```python
data = {"numbers": [1, 2, 3, 4, 5]}

# Lambda with single parameter
expr = jsonatapy.compile("$map(numbers, function($x) { $x * 2 })")
result = expr.evaluate(data)
print(result)  # [2, 4, 6, 8, 10]

# Lambda with condition
expr = jsonatapy.compile("$filter(numbers, function($x) { $x > 3 })")
result = expr.evaluate(data)
print(result)  # [4, 5]
```

### Lambda with Object Fields

```python
data = {
    "products": [
        {"name": "Widget", "price": 10, "quantity": 5},
        {"name": "Gadget", "price": 20, "quantity": 3}
    ]
}

# Access object fields in lambda
expr = jsonatapy.compile('''
    $map(products, function($p) {
        {
            "item": $p.name,
            "total": $p.price * $p.quantity
        }
    })
''')
result = expr.evaluate(data)
print(result)
# [
#   {"item": "Widget", "total": 50},
#   {"item": "Gadget", "total": 60}
# ]
```

### Multiple Parameters

```python
data = {"numbers": [1, 2, 3]}

# Lambda with two parameters (value and index)
expr = jsonatapy.compile('''
    $map(numbers, function($v, $i) {
        {"index": $i, "value": $v, "double": $v * 2}
    })
''')
result = expr.evaluate(data)
print(result)
# [
#   {"index": 0, "value": 1, "double": 2},
#   {"index": 1, "value": 2, "double": 4},
#   {"index": 2, "value": 3, "double": 6}
# ]
```

## Higher-Order Functions

### $map

Transform each element of an array.

```python
data = {"items": [1, 2, 3, 4]}

# With lambda
result = jsonatapy.evaluate(
    "$map(items, function($x) { $x * $x })",
    data
)
print(result)  # [1, 4, 9, 16]

# Shorthand syntax
result = jsonatapy.evaluate("items.${ $ * $ }", data)
print(result)  # [1, 4, 9, 16]
```

### $filter

Select elements matching a condition.

```python
data = {"items": [1, 2, 3, 4, 5]}

# With lambda
result = jsonatapy.evaluate(
    "$filter(items, function($x) { $x > 2 })",
    data
)
print(result)  # [3, 4, 5]

# Shorthand
result = jsonatapy.evaluate("items[$ > 2]", data)
print(result)  # [3, 4, 5]
```

### $reduce

Reduce array to single value.

```python
data = {"numbers": [1, 2, 3, 4, 5]}

# Sum with reduce
result = jsonatapy.evaluate(
    "$reduce(numbers, function($acc, $x) { $acc + $x }, 0)",
    data
)
print(result)  # 15

# Product
result = jsonatapy.evaluate(
    "$reduce(numbers, function($acc, $x) { $acc * $x }, 1)",
    data
)
print(result)  # 120
```

### $single

Get exactly one matching element.

```python
data = {
    "users": [
        {"id": 1, "name": "Alice"},
        {"id": 2, "name": "Bob"},
        {"id": 3, "name": "Charlie"}
    ]
}

# Find user by ID (must match exactly one)
result = jsonatapy.evaluate(
    "$single(users, function($u) { $u.id = 2 })",
    data
)
print(result)  # {"id": 2, "name": "Bob"}

# Error if no matches or multiple matches
try:
    result = jsonatapy.evaluate(
        "$single(users, function($u) { $u.id > 1 })",
        data
    )
except ValueError as e:
    print(e)  # "single() predicate matches 2 values"
```

### $sift

Filter object properties.

```python
data = {
    "user": {
        "name": "Alice",
        "age": 30,
        "email": "alice@example.com",
        "phone": None
    }
}

# Remove null/empty values
result = jsonatapy.evaluate(
    "$sift(user, function($v) { $v != null })",
    data
)
print(result)
# {"name": "Alice", "age": 30, "email": "alice@example.com"}

# Keep only specific types
data = {"mixed": {"a": 1, "b": "text", "c": 2, "d": "more"}}
result = jsonatapy.evaluate(
    "$sift(mixed, function($v) { $type($v) = 'number' })",
    data
)
print(result)  # {"a": 1, "c": 2}
```

## Complex Patterns

### Chaining Operations

```python
data = {
    "orders": [
        {"id": 1, "items": [{"price": 10}, {"price": 20}]},
        {"id": 2, "items": [{"price": 15}, {"price": 25}]},
        {"id": 3, "items": [{"price": 5}]}
    ]
}

# Chain filter, map, and aggregation
result = jsonatapy.evaluate('''
    orders
        [id > 1]
        .{
            "order": id,
            "total": $sum(items.price)
        }
        [total > 20]
''', data)
print(result)
# [{"order": 2, "total": 40}]
```

### Conditional Expressions

```python
data = {
    "products": [
        {"name": "Widget", "price": 10},
        {"name": "Gadget", "price": 150},
        {"name": "Doohickey", "price": 30}
    ]
}

# Ternary operator
result = jsonatapy.evaluate('''
    products.{
        "name": name,
        "category": price > 100 ? "premium" : "standard"
    }
''', data)
print(result)
# [
#   {"name": "Widget", "category": "standard"},
#   {"name": "Gadget", "category": "premium"},
#   {"name": "Doohickey", "category": "standard"}
# ]
```

### Nested Transformations

```python
data = {
    "departments": [
        {
            "name": "Engineering",
            "employees": [
                {"name": "Alice", "salary": 100000},
                {"name": "Bob", "salary": 90000}
            ]
        },
        {
            "name": "Sales",
            "employees": [
                {"name": "Charlie", "salary": 80000}
            ]
        }
    ]
}

# Transform nested structure
result = jsonatapy.evaluate('''
    departments.{
        "department": name,
        "headcount": $count(employees),
        "payroll": $sum(employees.salary),
        "avgSalary": $average(employees.salary)
    }
''', data)
print(result)
# [
#   {"department": "Engineering", "headcount": 2, "payroll": 190000, "avgSalary": 95000},
#   {"department": "Sales", "headcount": 1, "payroll": 80000, "avgSalary": 80000}
# ]
```

## Real-World Examples

### API Response Transformation

```python
import jsonatapy

# Raw API response
api_response = {
    "data": {
        "user": {
            "id": 123,
            "attributes": {
                "firstName": "Alice",
                "lastName": "Smith",
                "emailAddress": "alice@example.com"
            },
            "relationships": {
                "orders": [
                    {"id": 1, "total": 100.50},
                    {"id": 2, "total": 75.25}
                ]
            }
        }
    }
}

# Transform to simpler structure
expr = jsonatapy.compile('''
    {
        "userId": data.user.id,
        "name": data.user.attributes.firstName & " " & data.user.attributes.lastName,
        "email": data.user.attributes.emailAddress,
        "orderCount": $count(data.user.relationships.orders),
        "totalSpent": $sum(data.user.relationships.orders.total)
    }
''')

result = expr.evaluate(api_response)
print(result)
# {
#   "userId": 123,
#   "name": "Alice Smith",
#   "email": "alice@example.com",
#   "orderCount": 2,
#   "totalSpent": 175.75
# }
```

### Configuration Processing

```python
import jsonatapy

# Application config with overrides
config = {
    "defaults": {
        "timeout": 30,
        "retries": 3,
        "debug": False
    },
    "environments": {
        "development": {
            "debug": True,
            "timeout": 60
        },
        "production": {
            "retries": 5
        }
    }
}

# Merge environment config with defaults
expr = jsonatapy.compile('''
    $merge([
        defaults,
        environments.$env
    ])
''')

dev_config = expr.evaluate(config, {"env": "development"})
print(dev_config)
# {"timeout": 60, "retries": 3, "debug": true}

prod_config = expr.evaluate(config, {"env": "production"})
print(prod_config)
# {"timeout": 30, "retries": 5, "debug": false}
```

### Data Validation

```python
import jsonatapy

data = {
    "users": [
        {"name": "Alice", "email": "alice@example.com", "age": 30},
        {"name": "Bob", "email": "invalid-email", "age": -5},
        {"name": "", "email": "charlie@example.com", "age": 25}
    ]
}

# Validate and collect errors
expr = jsonatapy.compile('''
    users{
        "name": name,
        "errors": [
            $length(name) = 0 ? "Name is required",
            $not($contains(email, "@")) ? "Invalid email",
            age < 0 ? "Age must be positive"
        ][$ != null]
    }[errors != []]
''')

validation_errors = expr.evaluate(data)
print(validation_errors)
# [
#   {"name": "Bob", "errors": ["Invalid email", "Age must be positive"]},
#   {"name": "", "errors": ["Name is required"]}
# ]
```

### Report Generation

```python
import jsonatapy
from datetime import datetime

# Sales data
sales_data = {
    "period": "2024-Q1",
    "transactions": [
        {"date": "2024-01-15", "product": "Widget", "quantity": 5, "price": 10},
        {"date": "2024-01-20", "product": "Gadget", "quantity": 2, "price": 50},
        {"date": "2024-02-10", "product": "Widget", "quantity": 3, "price": 10},
        {"date": "2024-02-15", "product": "Gadget", "quantity": 4, "price": 50},
        {"date": "2024-03-05", "product": "Widget", "quantity": 2, "price": 10}
    ]
}

# Generate sales report
expr = jsonatapy.compile('''
    {
        "period": period,
        "summary": {
            "totalRevenue": $sum(transactions.(quantity * price)),
            "totalTransactions": $count(transactions),
            "averageOrderValue": $round($sum(transactions.(quantity * price)) / $count(transactions), 2)
        },
        "byProduct": [
            {
                "product": "Widget",
                "quantity": $sum(transactions[product="Widget"].quantity),
                "revenue": $sum(transactions[product="Widget"].(quantity * price))
            },
            {
                "product": "Gadget",
                "quantity": $sum(transactions[product="Gadget"].quantity),
                "revenue": $sum(transactions[product="Gadget"].(quantity * price))
            }
        ]
    }
''')

report = expr.evaluate(sales_data)
print(report)
# {
#   "period": "2024-Q1",
#   "summary": {
#     "totalRevenue": 420,
#     "totalTransactions": 5,
#     "averageOrderValue": 84.0
#   },
#   "byProduct": [
#     {"product": "Widget", "quantity": 10, "revenue": 100},
#     {"product": "Gadget", "quantity": 6, "revenue": 300}
#   ]
# }
```

### ETL Pipeline

```python
import jsonatapy
import json

# Extract, Transform, Load pattern
def etl_pipeline(source_data):
    """Transform data using jsonatapy in ETL pipeline."""

    # Compile transformation once
    transform = jsonatapy.compile('''
        {
            "metadata": {
                "recordCount": $count(records),
                "processedAt": $now()
            },
            "data": records[status = "active"].{
                "id": id,
                "displayName": $uppercase(name),
                "category": type,
                "value": amount * 1.1,
                "tags": tags[$ != "deprecated"]
            }
        }
    ''')

    # For large datasets, use JSON string API
    if len(source_data.get("records", [])) > 1000:
        json_str = json.dumps(source_data)
        result_str = transform.evaluate_json(json_str)
        return json.loads(result_str)
    else:
        return transform.evaluate(source_data)

# Example usage
source = {
    "records": [
        {"id": 1, "name": "item1", "status": "active", "type": "A", "amount": 100, "tags": ["new"]},
        {"id": 2, "name": "item2", "status": "inactive", "type": "B", "amount": 200, "tags": ["old"]},
        {"id": 3, "name": "item3", "status": "active", "type": "A", "amount": 150, "tags": ["new", "deprecated"]}
    ]
}

result = etl_pipeline(source)
print(json.dumps(result, indent=2))
```

## Next Steps

- [Review API reference](api.md)
- [Optimize performance](performance.md)
- [Learn JSONata syntax](https://docs.jsonata.org/)
- [Build from source](building.md)
