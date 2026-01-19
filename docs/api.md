# Python API Reference

Complete reference for the jsonatapy Python API.

## Module Overview

```python
import jsonatapy

# Quick evaluation
result = jsonatapy.evaluate(expression, data, bindings=None)

# Compile and reuse
expr = jsonatapy.compile(expression)
result = expr.evaluate(data, bindings=None)

# High-performance JSON string API
result_json = expr.evaluate_json(json_str, bindings=None)
```

## Functions

### `evaluate(expression, data, bindings=None)`

Compile and evaluate a JSONata expression in one step.

**Parameters:**
- `expression` (str): A JSONata query/transformation expression
- `data` (Any): The data to query/transform (typically dict or list)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings

**Returns:**
- `Any`: The result of evaluating the expression

**Raises:**
- `ValueError`: If parsing or evaluation fails

**Example:**
```python
import jsonatapy

# Simple query
data = {"name": "Alice", "age": 30}
result = jsonatapy.evaluate("name", data)
print(result)  # "Alice"

# With transformation
data = {"items": [{"name": "A", "price": 10}, {"name": "B", "price": 20}]}
result = jsonatapy.evaluate("items[price > 15].name", data)
print(result)  # ["B"]

# With bindings
result = jsonatapy.evaluate("name & suffix",
                            {"name": "Hello"},
                            {"suffix": "!"})
print(result)  # "Hello!"
```

**Performance Note:**
For repeated evaluations with the same expression, use `compile()` instead for better performance.

---

### `compile(expression)`

Compile a JSONata expression into an executable form for repeated evaluation.

**Parameters:**
- `expression` (str): A JSONata query/transformation expression string

**Returns:**
- `JsonataExpression`: A compiled expression object

**Raises:**
- `ValueError`: If the expression cannot be parsed

**Example:**
```python
import jsonatapy

# Compile once
expr = jsonatapy.compile("orders[price > 100].product")

# Evaluate many times
data1 = {"orders": [{"product": "A", "price": 150}]}
result1 = expr.evaluate(data1)

data2 = {"orders": [{"product": "B", "price": 50}]}
result2 = expr.evaluate(data2)

print(result1)  # ["A"]
print(result2)  # []
```

**When to Use:**
- ✅ Same expression evaluated multiple times
- ✅ Performance-critical code
- ✅ Expression is known at module load time
- ❌ One-off queries (use `evaluate()` instead)

---

## JsonataExpression Class

A compiled JSONata expression that can be evaluated multiple times.

### Constructor

```python
# Don't instantiate directly - use compile() instead
expr = jsonatapy.compile("expression")
```

**Note:** Use the `compile()` function or class method rather than the constructor directly.

---

### `evaluate(data, bindings=None)`

Evaluate the compiled expression against data.

**Parameters:**
- `data` (Any): The data to query/transform (typically dict or list)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings

**Returns:**
- `Any`: The result of evaluating the expression

**Raises:**
- `ValueError`: If evaluation fails

**Example:**
```python
import jsonatapy

# Compile expression
expr = jsonatapy.compile("$uppercase(name)")

# Evaluate with different data
print(expr.evaluate({"name": "alice"}))  # "ALICE"
print(expr.evaluate({"name": "bob"}))    # "BOB"

# With bindings
expr2 = jsonatapy.compile("$greeting & name")
result = expr2.evaluate(
    {"name": "World"},
    {"greeting": "Hello, "}
)
print(result)  # "Hello, World"
```

**Type Conversions:**

Python to JSONata:
- `None` → `null`
- `bool` → boolean
- `int`, `float` → number
- `str` → string
- `list` → array
- `dict` → object

JSONata to Python:
- `null` → `None`
- boolean → `bool`
- number → `int` or `float`
- string → `str`
- array → `list`
- object → `dict`

---

### `evaluate_json(json_str, bindings=None)`

Evaluate with JSON string input/output for maximum performance.

**Parameters:**
- `json_str` (str): Input data as a JSON string
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings

**Returns:**
- `str`: The result as a JSON string

**Raises:**
- `ValueError`: If JSON parsing or evaluation fails

**Example:**
```python
import json
import jsonatapy

# Compile expression
expr = jsonatapy.compile("items[price > 100]")

# Prepare JSON string input
data = {"items": [{"name": "A", "price": 150}, {"name": "B", "price": 50}]}
json_str = json.dumps(data)

# Evaluate (faster for large data)
result_str = expr.evaluate_json(json_str)

# Parse result
result = json.loads(result_str)
print(result)  # [{"name": "A", "price": 150}]
```

**Performance Benefits:**

For large datasets (1000+ items):
- **10-50x faster** than `evaluate()`
- Avoids Python↔Rust object conversion overhead
- Direct JSON-to-JSON processing

**When to Use:**
- ✅ Large datasets (1000+ items)
- ✅ High-frequency evaluation (millions of calls)
- ✅ Data already in JSON format (from API/file)
- ✅ Performance-critical hot paths
- ❌ Small data (overhead not worth it)
- ❌ When you need Python objects (use `evaluate()`)

**Benchmark Example:**
```python
import json
import time
import jsonatapy

# Large dataset
data = {
    "items": [
        {"name": f"Item {i}", "price": i, "stock": i * 10}
        for i in range(1000)
    ]
}

expr = jsonatapy.compile('items[price > 500].{"name": name, "value": price * stock}')

# Method 1: evaluate() with Python objects
start = time.time()
for _ in range(100):
    result = expr.evaluate(data)
time_py = (time.time() - start) / 100
print(f"evaluate(): {time_py*1000:.2f}ms")

# Method 2: evaluate_json() with JSON strings
json_str = json.dumps(data)
start = time.time()
for _ in range(100):
    result_str = expr.evaluate_json(json_str)
    result = json.loads(result_str)  # Include parsing time
time_json = (time.time() - start) / 100
print(f"evaluate_json(): {time_json*1000:.2f}ms")

print(f"Speedup: {time_py/time_json:.1f}x")
```

---

### Class Method: `JsonataExpression.compile(expression)`

Alternative way to compile expressions using class method.

**Parameters:**
- `expression` (str): A JSONata expression string

**Returns:**
- `JsonataExpression`: Compiled expression object

**Example:**
```python
from jsonatapy import JsonataExpression

# Using class method
expr = JsonataExpression.compile("$.name")

# Equivalent to module function
import jsonatapy
expr = jsonatapy.compile("$.name")
```

**Note:** Both approaches are identical - use whichever style you prefer.

---

## Module Attributes

### `__version__`

The version of the jsonatapy package.

```python
import jsonatapy
print(jsonatapy.__version__)  # "0.1.0"
```

---

### `__jsonata_version__`

The JSONata specification version supported.

```python
import jsonatapy
print(jsonatapy.__jsonata_version__)  # "2.1.0"
```

---

## Error Handling

All functions and methods raise `ValueError` with descriptive messages on errors.

### Common Error Scenarios

**Parse Error:**
```python
import jsonatapy

try:
    expr = jsonatapy.compile("invalid [[ syntax")
except ValueError as e:
    print(f"Parse error: {e}")
    # Parse error: Unexpected token at position 9
```

**Evaluation Error:**
```python
import jsonatapy

try:
    result = jsonatapy.evaluate("$undefined_func()", {})
except ValueError as e:
    print(f"Evaluation error: {e}")
    # Evaluation error: Unknown function: undefined_func
```

**Type Error:**
```python
import jsonatapy

try:
    result = jsonatapy.evaluate("$sum('not a number')", {})
except ValueError as e:
    print(f"Type error: {e}")
    # Type error: sum() requires array of numbers
```

### Best Practices for Error Handling

```python
import jsonatapy

def safe_evaluate(expression, data):
    """Safely evaluate JSONata with error handling."""
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError as e:
        print(f"JSONata error: {e}")
        return None

# Compile-time validation
def validate_expression(expression):
    """Check if expression is valid."""
    try:
        jsonatapy.compile(expression)
        return True
    except ValueError:
        return False

# With custom error handling
def evaluate_with_default(expression, data, default=None):
    """Evaluate with default value on error."""
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError:
        return default
```

---

## Type Hints

jsonatapy is fully typed with comprehensive type hints:

```python
from typing import Any, Dict, Optional
import jsonatapy

def process_data(
    expression: str,
    data: Any,
    bindings: Optional[Dict[str, Any]] = None
) -> Any:
    """Type-hinted function using jsonatapy."""
    return jsonatapy.evaluate(expression, data, bindings)

# Type checking with mypy
expr: jsonatapy.JsonataExpression = jsonatapy.compile("name")
result: Any = expr.evaluate({"name": "Alice"})
```

---

## Thread Safety

jsonatapy is thread-safe:

- ✅ Multiple threads can call functions concurrently
- ✅ `JsonataExpression` objects can be shared across threads
- ✅ Evaluation is stateless (except for bindings)
- ✅ No global state modifications during evaluation

**Example:**
```python
import jsonatapy
from concurrent.futures import ThreadPoolExecutor

# Compile once
expr = jsonatapy.compile("items[price > 100].name")

# Share across threads
def process(data):
    return expr.evaluate(data)

with ThreadPoolExecutor(max_workers=10) as executor:
    results = executor.map(process, data_list)
```

---

## Performance Tips

### 1. Compile Once, Evaluate Many Times

```python
# ❌ Slow - compiles on every call
for data in dataset:
    result = jsonatapy.evaluate("items[price > 100]", data)

# ✅ Fast - compile once
expr = jsonatapy.compile("items[price > 100]")
for data in dataset:
    result = expr.evaluate(data)
```

### 2. Use JSON String API for Large Data

```python
import json

# ✅ Fast for large datasets
expr = jsonatapy.compile("items[price > 100]")
json_str = json.dumps(large_data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

### 3. Simplify Expressions

```python
# ❌ Slower - multiple passes
result = jsonatapy.evaluate("$filter($map(items, ...), ...)", data)

# ✅ Faster - single pass
result = jsonatapy.evaluate("items[...].{...}", data)
```

### 4. Avoid Repeated Conversions

```python
# ❌ Converts data on every call
for expr_str in expressions:
    result = jsonatapy.evaluate(expr_str, data)

# ✅ Compile all expressions first
exprs = [jsonatapy.compile(e) for e in expressions]
for expr in exprs:
    result = expr.evaluate(data)
```

---

## Complete Example

```python
import jsonatapy
import json

# Sample data
data = {
    "invoice": {
        "number": "INV-001",
        "date": "2024-01-15",
        "items": [
            {"product": "Widget", "quantity": 5, "price": 12.50},
            {"product": "Gadget", "quantity": 2, "price": 45.00},
            {"product": "Doohickey", "quantity": 10, "price": 3.25}
        ]
    }
}

# 1. Simple query
invoice_num = jsonatapy.evaluate("invoice.number", data)
print(f"Invoice: {invoice_num}")

# 2. Filtering and mapping
expr = jsonatapy.compile('''
    invoice.items[price > 10].{
        "product": product,
        "total": quantity * price
    }
''')
expensive_items = expr.evaluate(data)
print(f"Expensive items: {expensive_items}")

# 3. Aggregation
total = jsonatapy.evaluate("$sum(invoice.items.(quantity * price))", data)
print(f"Total: ${total:.2f}")

# 4. With bindings
expr = jsonatapy.compile("invoice.items[price > $threshold].product")
result = expr.evaluate(data, {"threshold": 20})
print(f"Products over $20: {result}")

# 5. High-performance with JSON strings
json_str = json.dumps(data)
expr = jsonatapy.compile("invoice.items[quantity > 5]")
result_str = expr.evaluate_json(json_str)
print(f"High quantity items: {json.loads(result_str)}")
```

---

## Next Steps

- [Learn common usage patterns](usage.md)
- [Understand performance optimization](performance.md)
- [Read JSONata language reference](https://docs.jsonata.org/)
- [Build from source](building.md)
