# API Reference

Complete Python API for jsonatapy.

## Quick Reference

```python
import jsonatapy

# One-off evaluation
result = jsonatapy.evaluate(expression, data, bindings=None)

# Compile and reuse
expr = jsonatapy.compile(expression)
result = expr.evaluate(data, bindings=None)
```

## Functions

### `evaluate(expression, data, bindings=None)`

Compile and evaluate a JSONata expression in one step.

**Parameters:**
- `expression` (str): JSONata query/transformation expression
- `data` (Any): Data to query (typically dict or list)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings

**Returns:** `Any` - Result of evaluating the expression

**Raises:** `ValueError` - If parsing or evaluation fails

**Example:**
```python
data = {"name": "Alice", "age": 30}
result = jsonatapy.evaluate("name", data)
# "Alice"

# With bindings
result = jsonatapy.evaluate(
    "name & suffix",
    {"name": "Hello"},
    {"suffix": "!"}
)
# "Hello!"
```

**Note:** For repeated evaluations with the same expression, use `compile()` for better performance.

### `compile(expression)`

Compile a JSONata expression for repeated evaluation.

**Parameters:**
- `expression` (str): JSONata query/transformation expression

**Returns:** `JsonataExpression` - Compiled expression object

**Raises:** `ValueError` - If expression cannot be parsed

**Example:**
```python
expr = jsonatapy.compile("orders[price > 100].product")

data1 = {"orders": [{"product": "A", "price": 150}]}
result1 = expr.evaluate(data1)  # ["A"]

data2 = {"orders": [{"product": "B", "price": 50}]}
result2 = expr.evaluate(data2)  # []
```

## JsonataExpression Class

Compiled JSONata expression that can be evaluated multiple times.

### `evaluate(data, bindings=None)`

Evaluate the compiled expression against data.

**Parameters:**
- `data` (Any): Data to query (typically dict or list)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings

**Returns:** `Any` - Result of evaluation

**Raises:** `ValueError` - If evaluation fails

**Example:**
```python
expr = jsonatapy.compile("$uppercase(name)")

expr.evaluate({"name": "alice"})  # "ALICE"
expr.evaluate({"name": "bob"})    # "BOB"
```

**Type Conversions:**

Python to JSONata:
- `None` → null
- `bool` → boolean
- `int`, `float` → number
- `str` → string
- `list` → array
- `dict` → object

JSONata to Python:
- null → `None`
- boolean → `bool`
- number → `int` or `float`
- string → `str`
- array → `list`
- object → `dict`

### `evaluate_json(json_str, bindings=None)`

Evaluate with JSON string input/output for maximum performance.

**Parameters:**
- `json_str` (str): Input data as JSON string
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings

**Returns:** `str` - Result as JSON string

**Raises:** `ValueError` - If JSON parsing or evaluation fails

**Example:**
```python
import json

expr = jsonatapy.compile("items[price > 100]")
data = {"items": [{"name": "A", "price": 150}]}

json_str = json.dumps(data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

**Use for:**
- Large datasets (1000+ items)
- High-frequency evaluation
- Data already in JSON format
- Performance-critical code

## Module Attributes

### `__version__`

Package version.

```python
print(jsonatapy.__version__)  # "2.1.0"
```

### `__jsonata_version__`

JSONata specification version supported.

```python
print(jsonatapy.__jsonata_version__)  # "2.1.0"
```

## Error Handling

All functions raise `ValueError` with descriptive messages on errors.

**Parse Error:**
```python
try:
    expr = jsonatapy.compile("invalid [[ syntax")
except ValueError as e:
    print(f"Parse error: {e}")
```

**Evaluation Error:**
```python
try:
    result = jsonatapy.evaluate("$undefined_func()", {})
except ValueError as e:
    print(f"Evaluation error: {e}")
```

**Safe Evaluation:**
```python
def safe_evaluate(expression, data, default=None):
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError as e:
        print(f"Error: {e}")
        return default
```

## Thread Safety

jsonatapy is thread-safe:
- Multiple threads can call functions concurrently
- `JsonataExpression` objects can be shared across threads
- Evaluation is stateless (except for bindings)

**Example:**
```python
from concurrent.futures import ThreadPoolExecutor

expr = jsonatapy.compile("items[price > 100].name")

def process(data):
    return expr.evaluate(data)

with ThreadPoolExecutor(max_workers=10) as executor:
    results = executor.map(process, data_list)
```

## Performance Tips

**Compile once, evaluate many times:**
```python
# Slow
for data in dataset:
    result = jsonatapy.evaluate("items[price > 100]", data)

# Fast
expr = jsonatapy.compile("items[price > 100]")
for data in dataset:
    result = expr.evaluate(data)
```

**Use JSON string API for large data:**
```python
expr = jsonatapy.compile("items[price > 100]")
json_str = json.dumps(large_data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

## Complete Example

```python
import jsonatapy
import json

data = {
    "invoice": {
        "number": "INV-001",
        "items": [
            {"product": "Widget", "quantity": 5, "price": 12.50},
            {"product": "Gadget", "quantity": 2, "price": 45.00},
            {"product": "Doohickey", "quantity": 10, "price": 3.25}
        ]
    }
}

# Simple query
invoice_num = jsonatapy.evaluate("invoice.number", data)

# Filtering and mapping
expr = jsonatapy.compile('''
    invoice.items[price > 10].{
        "product": product,
        "total": quantity * price
    }
''')
expensive_items = expr.evaluate(data)

# Aggregation
total = jsonatapy.evaluate("$sum(invoice.items.(quantity * price))", data)

# With bindings
expr = jsonatapy.compile("invoice.items[price > $threshold].product")
result = expr.evaluate(data, {"threshold": 20})
```
