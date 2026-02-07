# Error Handling

Comprehensive guide to error handling in jsonatapy.

## Table of Contents

- [Error Types](#error-types)
- [Common Errors](#common-errors)
- [Error Handling Patterns](#error-handling-patterns)
- [Debugging Tips](#debugging-tips)
- [Error Message Examples](#error-message-examples)

## Error Types

All jsonatapy errors are raised as Python `ValueError` exceptions with descriptive messages.

### Parse Errors

Raised when JSONata expression syntax is invalid.

```python
import jsonatapy

try:
    expr = jsonatapy.compile("invalid [[ syntax")
except ValueError as e:
    print(f"Parse error: {e}")
    # Parse error: Unexpected token at position 9
```

**Common causes:**
- Unmatched brackets or parentheses
- Invalid operators
- Malformed expressions
- Incorrect syntax

### Evaluation Errors

Raised when expression evaluation fails.

```python
import jsonatapy

try:
    result = jsonatapy.evaluate("$undefined_func()", {})
except ValueError as e:
    print(f"Evaluation error: {e}")
    # Evaluation error: Unknown function: undefined_func
```

**Common causes:**
- Unknown function names
- Invalid function arguments
- Accessing undefined variables
- Runtime type mismatches

### Type Errors

Raised when operations receive incompatible types.

```python
import jsonatapy

try:
    result = jsonatapy.evaluate("$sum('not a number')", {})
except ValueError as e:
    print(f"Type error: {e}")
    # Type error: Argument 1 of function "sum" must be an array
```

**Common causes:**
- Wrong argument types for functions
- Invalid operations on incompatible types
- Type conversion failures

## Common Errors

### Syntax Errors

#### Unmatched Brackets

```python
# ❌ Error: Unmatched bracket
jsonatapy.compile("items[price > 100")
# ValueError: Expected ']' at end of expression

# ✅ Correct
jsonatapy.compile("items[price > 100]")
```

#### Invalid Operators

```python
# ❌ Error: Invalid operator
jsonatapy.compile("items => price")
# ValueError: Unexpected token '=>' at position 6

# ✅ Correct
jsonatapy.compile("items ~> $map(function($v) { $v.price })")
```

#### Malformed Path Expressions

```python
# ❌ Error: Invalid path
jsonatapy.compile("..items")
# ValueError: Unexpected token '..' at position 0

# ✅ Correct
jsonatapy.compile("items")
```

### Function Errors

#### Unknown Function

```python
# ❌ Error: Function doesn't exist
jsonatapy.evaluate("$myFunc()", {})
# ValueError: Unknown function: myFunc

# ✅ Use built-in functions
jsonatapy.evaluate("$uppercase('hello')", {})
```

#### Wrong Argument Count

```python
# ❌ Error: Too few arguments
jsonatapy.evaluate("$substring('hello')", {})
# ValueError: The substring function requires at least 2 arguments

# ✅ Provide required arguments
jsonatapy.evaluate("$substring('hello', 0, 2)", {})
```

#### Wrong Argument Type

```python
# ❌ Error: Wrong type
jsonatapy.evaluate("$sum('not an array')", {})
# ValueError: Argument 1 of function "sum" must be an array

# ✅ Pass correct type
jsonatapy.evaluate("$sum([1, 2, 3])", {})
```

### Path Errors

#### Non-existent Fields

```python
# Returns undefined (None in Python)
result = jsonatapy.evaluate("missing.field", {})
print(result)  # None

# Use default value pattern
result = jsonatapy.evaluate("missing.field ? missing.field : 'default'", {})
print(result)  # "default"
```

#### Type Mismatch in Path

```python
# ❌ Error: Cannot access property of non-object
data = {"value": 123}
jsonatapy.evaluate("value.property", data)
# Returns None (undefined behavior)

# ✅ Check type first
jsonatapy.evaluate("$type(value) = 'number' ? value : value.property", data)
```

### JSON Parsing Errors

When using `evaluate_json()`:

```python
import jsonatapy

expr = jsonatapy.compile("items[price > 100]")

# ❌ Error: Invalid JSON
try:
    result = expr.evaluate_json("not valid json")
except ValueError as e:
    print(f"JSON error: {e}")
    # JSON error: expected value at line 1 column 1

# ✅ Valid JSON
import json
json_str = json.dumps({"items": []})
result = expr.evaluate_json(json_str)
```

## Error Handling Patterns

### Basic Try-Except

```python
import jsonatapy

def safe_evaluate(expression, data):
    """Safely evaluate with error handling."""
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError as e:
        print(f"JSONata error: {e}")
        return None
```

### Separate Parse and Evaluation Errors

```python
import jsonatapy

def compile_and_evaluate(expression, data):
    """Separate compilation and evaluation errors."""
    try:
        expr = jsonatapy.compile(expression)
    except ValueError as e:
        print(f"Syntax error in expression: {e}")
        return None

    try:
        return expr.evaluate(data)
    except ValueError as e:
        print(f"Evaluation error: {e}")
        return None
```

### Validation Before Evaluation

```python
import jsonatapy

def validate_expression(expression):
    """Check if expression is syntactically valid."""
    try:
        jsonatapy.compile(expression)
        return True, None
    except ValueError as e:
        return False, str(e)

# Usage
is_valid, error = validate_expression("items[price > 100]")
if is_valid:
    result = jsonatapy.evaluate("items[price > 100]", data)
else:
    print(f"Invalid expression: {error}")
```

### Default Value on Error

```python
import jsonatapy

def evaluate_with_default(expression, data, default=None):
    """Evaluate with default value on error."""
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError:
        return default

# Usage
result = evaluate_with_default("missing.field", {}, default="N/A")
print(result)  # "N/A"
```

### Logging Errors

```python
import jsonatapy
import logging

logger = logging.getLogger(__name__)

def evaluate_with_logging(expression, data):
    """Evaluate with error logging."""
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError as e:
        logger.error(f"JSONata error: {e}", extra={
            "expression": expression,
            "data": data
        })
        raise
```

### Context Manager Pattern

```python
import jsonatapy
from contextlib import contextmanager

@contextmanager
def jsonata_context(expression):
    """Context manager for JSONata expression."""
    try:
        expr = jsonatapy.compile(expression)
        yield expr
    except ValueError as e:
        print(f"Error: {e}")
        yield None

# Usage
with jsonata_context("items[price > 100]") as expr:
    if expr:
        result = expr.evaluate(data)
```

## Debugging Tips

### 1. Test Expression Syntax First

```python
import jsonatapy

# Validate syntax before using
expression = "items[price > 100].name"
try:
    expr = jsonatapy.compile(expression)
    print("Expression is valid")
except ValueError as e:
    print(f"Syntax error: {e}")
```

### 2. Use Simple Test Data

```python
import jsonatapy

# Test with minimal data
test_data = {"items": [{"price": 150, "name": "Widget"}]}
result = jsonatapy.evaluate("items[price > 100].name", test_data)
print(result)  # ["Widget"]
```

### 3. Break Down Complex Expressions

```python
import jsonatapy

data = {"orders": [{"total": 100}, {"total": 200}]}

# Test each part separately
step1 = jsonatapy.evaluate("orders", data)
print("Step 1:", step1)

step2 = jsonatapy.evaluate("orders.total", data)
print("Step 2:", step2)

step3 = jsonatapy.evaluate("$sum(orders.total)", data)
print("Step 3:", step3)
```

### 4. Check Data Types

```python
import jsonatapy

# Verify data structure matches expression
data = {"value": "123"}  # String, not number

# This will return None (undefined)
result = jsonatapy.evaluate("value > 100", data)
print(result)  # None

# Fix: Convert to number
result = jsonatapy.evaluate("$number(value) > 100", data)
print(result)  # True
```

### 5. Use JSONata Playground

Test expressions online first:
- Visit [try.jsonata.org](https://try.jsonata.org/)
- Test your expression with sample data
- Debug syntax issues before using in Python

### 6. Add Verbose Error Handling

```python
import jsonatapy
import traceback

def debug_evaluate(expression, data):
    """Evaluate with detailed error information."""
    print(f"Expression: {expression}")
    print(f"Data: {data}")

    try:
        result = jsonatapy.evaluate(expression, data)
        print(f"Result: {result}")
        return result
    except ValueError as e:
        print(f"Error: {e}")
        traceback.print_exc()
        return None
```

## Error Message Examples

### Parse Error Messages

```
Unexpected token '[' at position 5
Expected ']' at end of expression
Invalid number format at position 12
Unterminated string literal at position 8
```

### Evaluation Error Messages

```
Unknown function: myFunc
Argument 1 of function "sum" must be an array
Division by zero
Cannot access property "field" of undefined
Stack overflow (recursion depth exceeded)
```

### Type Error Messages

```
Cannot convert "text" to number
Expected array, got string
Cannot apply operator '+' to types object and number
```

## Best Practices

### 1. Fail Fast During Initialization

```python
import jsonatapy

class DataTransformer:
    def __init__(self, expression):
        # Compile at init time to catch syntax errors early
        self.expr = jsonatapy.compile(expression)

    def transform(self, data):
        # Only evaluation errors possible here
        return self.expr.evaluate(data)
```

### 2. Provide User-Friendly Error Messages

```python
import jsonatapy

def user_friendly_evaluate(expression, data):
    """Evaluate with user-friendly error messages."""
    try:
        return jsonatapy.evaluate(expression, data)
    except ValueError as e:
        error_msg = str(e)
        if "Unknown function" in error_msg:
            return {"error": "The function you used doesn't exist. Check the function name."}
        elif "Unexpected token" in error_msg:
            return {"error": "Your expression has a syntax error. Please check the syntax."}
        else:
            return {"error": f"An error occurred: {error_msg}"}
```

### 3. Handle Errors at the Right Level

```python
import jsonatapy

def process_records(records, expression):
    """Process multiple records with per-record error handling."""
    expr = jsonatapy.compile(expression)  # Fail fast if syntax error

    results = []
    for i, record in enumerate(records):
        try:
            result = expr.evaluate(record)
            results.append({"success": True, "result": result})
        except ValueError as e:
            results.append({"success": False, "error": str(e), "record_index": i})

    return results
```

### 4. Document Expected Errors

```python
import jsonatapy

def transform_data(expression: str, data: dict) -> dict:
    """
    Transform data using JSONata expression.

    Args:
        expression: JSONata expression string
        data: Input data dictionary

    Returns:
        Transformed data

    Raises:
        ValueError: If expression syntax is invalid or evaluation fails

    Example:
        >>> transform_data("items[price > 100]", {"items": [...]})
    """
    return jsonatapy.evaluate(expression, data)
```

## Next Steps

- [Learn optimization tips](optimization-tips.md)
- [Review API reference](api.md)
- [Explore usage patterns](usage.md)
- [Check compatibility notes](compatibility.md)
