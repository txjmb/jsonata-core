# Migration from JavaScript jsonata

Guide for migrating from the JavaScript jsonata library to jsonatapy.

## Table of Contents

- [Overview](#overview)
- [API Mapping](#api-mapping)
- [Key Differences](#key-differences)
- [Code Examples](#code-examples)
- [Performance Considerations](#performance-considerations)
- [Common Pitfalls](#common-pitfalls)

## Overview

jsonatapy provides 100% JSONata language compatibility with the JavaScript reference implementation. The core JSONata expressions remain the same, but the Python API differs from the JavaScript API.

### What Stays the Same

- ✅ JSONata expression syntax
- ✅ All built-in functions
- ✅ Lambda functions and closures
- ✅ Higher-order functions
- ✅ Expression semantics and behavior

### What Changes

- ❌ API surface (JavaScript vs Python)
- ❌ Async patterns (JavaScript async vs Python sync)
- ❌ Custom function registration (different syntax)
- ❌ Error handling (JavaScript vs Python exceptions)

## API Mapping

### Basic Evaluation

**JavaScript (jsonata-js):**
```javascript
const jsonata = require('jsonata');

// One-time evaluation
const result = await jsonata('expression').evaluate(data);

// With bindings
const result = await jsonata('expression').evaluate(data, bindings);
```

**Python (jsonatapy):**
```python
import jsonatapy

# One-time evaluation
result = jsonatapy.evaluate('expression', data)

# With bindings
result = jsonatapy.evaluate('expression', data, bindings)
```

### Pre-compilation

**JavaScript:**
```javascript
const jsonata = require('jsonata');

// Compile once
const expr = jsonata('items[price > 100]');

// Evaluate many times
const result1 = await expr.evaluate(data1);
const result2 = await expr.evaluate(data2);
```

**Python:**
```python
import jsonatapy

# Compile once
expr = jsonatapy.compile('items[price > 100]')

# Evaluate many times (synchronous)
result1 = expr.evaluate(data1)
result2 = expr.evaluate(data2)
```

### Custom Function Registration

**JavaScript:**
```javascript
const jsonata = require('jsonata');

const expr = jsonata('$myFunc(value)');

// Register custom function
expr.registerFunction('myFunc', (val) => {
    return val.toUpperCase();
}, '<s:s>');  // Signature: string -> string

const result = await expr.evaluate(data);
```

**Python:**
```python
import jsonatapy

# Custom functions not yet supported
# Use bindings as workaround for constants/data
expr = jsonatapy.compile('$uppercase(value)')
result = expr.evaluate(data)
```

!!! note "Custom Functions"
    Custom function registration is not yet implemented in jsonatapy. For simple cases, use variable bindings. For complex transformations, pre-process data in Python.

### Error Handling

**JavaScript:**
```javascript
const jsonata = require('jsonata');

try {
    const expr = jsonata('invalid [[');
    const result = await expr.evaluate(data);
} catch (err) {
    console.error('Error:', err.message);
    console.error('Position:', err.position);
    console.error('Token:', err.token);
}
```

**Python:**
```python
import jsonatapy

try:
    expr = jsonatapy.compile('invalid [[')
    result = expr.evaluate(data)
except ValueError as e:
    print(f'Error: {e}')
    # Python ValueError with descriptive message
```

## Key Differences

### 1. Synchronous vs Asynchronous

**JavaScript:** Evaluation returns a Promise (async)
```javascript
const result = await expr.evaluate(data);
```

**Python:** Evaluation is synchronous
```python
result = expr.evaluate(data)
```

!!! tip "Performance Impact"
    Python's synchronous API is actually faster for most use cases. Use threading/multiprocessing for concurrency if needed.

### 2. Type Conversions

**JavaScript:**
```javascript
// JavaScript types
null, undefined, boolean, number, string, Array, Object

// JSONata undefined becomes JavaScript undefined
const result = await expr.evaluate(data);  // may return undefined
```

**Python:**
```python
# Python types
None, bool, int, float, str, list, dict

# JSONata undefined becomes Python None
result = expr.evaluate(data)  # may return None
```

**Type mapping:**

| JSONata | JavaScript | Python |
|---------|-----------|--------|
| `null` | `null` | `None` |
| `undefined` | `undefined` | `None` |
| boolean | `Boolean` | `bool` |
| number | `Number` | `int` or `float` |
| string | `String` | `str` |
| array | `Array` | `list` |
| object | `Object` | `dict` |

### 3. Module Import

**JavaScript:**
```javascript
// CommonJS
const jsonata = require('jsonata');

// ES modules
import jsonata from 'jsonata';
```

**Python:**
```python
# Standard import
import jsonatapy

# Alternative
from jsonatapy import compile, evaluate
```

### 4. No Built-in Timeouts

**JavaScript:**
```javascript
// Timeout support
const expr = jsonata('expression');
expr.timeout = 5000;  // 5 seconds

try {
    const result = await expr.evaluate(data);
} catch (err) {
    if (err.message.includes('timeout')) {
        console.error('Expression timed out');
    }
}
```

**Python:**
```python
# No built-in timeout support
# Use Python's signal module or threading for timeouts
import signal
import jsonatapy

def timeout_handler(signum, frame):
    raise TimeoutError('Expression timed out')

signal.signal(signal.SIGALRM, timeout_handler)
signal.alarm(5)  # 5 seconds

try:
    result = expr.evaluate(data)
finally:
    signal.alarm(0)  # Cancel alarm
```

## Code Examples

### Example 1: Simple Query

**JavaScript:**
```javascript
const jsonata = require('jsonata');

const data = {
    "invoice": {
        "total": 150.00,
        "items": [
            {"product": "Widget", "price": 100},
            {"product": "Gadget", "price": 50}
        ]
    }
};

const expr = jsonata('invoice.items[price > 75].product');
const result = await expr.evaluate(data);
console.log(result);  // ["Widget"]
```

**Python:**
```python
import jsonatapy

data = {
    "invoice": {
        "total": 150.00,
        "items": [
            {"product": "Widget", "price": 100},
            {"product": "Gadget", "price": 50}
        ]
    }
}

expr = jsonatapy.compile('invoice.items[price > 75].product')
result = expr.evaluate(data)
print(result)  # ["Widget"]
```

### Example 2: Aggregation

**JavaScript:**
```javascript
const jsonata = require('jsonata');

const data = {
    "orders": [
        {"amount": 100}, {"amount": 200}, {"amount": 150}
    ]
};

const expr = jsonata('$sum(orders.amount)');
const result = await expr.evaluate(data);
console.log(result);  // 450
```

**Python:**
```python
import jsonatapy

data = {
    "orders": [
        {"amount": 100}, {"amount": 200}, {"amount": 150}
    ]
}

expr = jsonatapy.compile('$sum(orders.amount)')
result = expr.evaluate(data)
print(result)  # 450
```

### Example 3: Object Construction

**JavaScript:**
```javascript
const jsonata = require('jsonata');

const expr = jsonata(`
    {
        "total": $sum(items.price),
        "count": $count(items),
        "products": items.name
    }
`);

const result = await expr.evaluate(data);
```

**Python:**
```python
import jsonatapy

expr = jsonatapy.compile('''
    {
        "total": $sum(items.price),
        "count": $count(items),
        "products": items.name
    }
''')

result = expr.evaluate(data)
```

### Example 4: Lambda Functions

**JavaScript:**
```javascript
const jsonata = require('jsonata');

const expr = jsonata(`
    items ~> $map(function($i) {
        {
            "name": $i.name,
            "total": $i.price * $i.quantity
        }
    })
`);

const result = await expr.evaluate(data);
```

**Python:**
```python
import jsonatapy

expr = jsonatapy.compile('''
    items ~> $map(function($i) {
        {
            "name": $i.name,
            "total": $i.price * $i.quantity
        }
    })
''')

result = expr.evaluate(data)
```

### Example 5: With Bindings

**JavaScript:**
```javascript
const jsonata = require('jsonata');

const expr = jsonata('items[price > $threshold]');
const result = await expr.evaluate(data, {threshold: 100});
```

**Python:**
```python
import jsonatapy

expr = jsonatapy.compile('items[price > $threshold]')
result = expr.evaluate(data, {'threshold': 100})
```

## Performance Considerations

### Speed Comparison

jsonatapy is significantly faster than JavaScript jsonata:

| Operation | jsonatapy | JavaScript jsonata | Speedup |
|-----------|-----------|-------------------|---------|
| Simple paths | ~2ms | ~20ms | 10x faster |
| Arithmetic | ~1ms | ~14ms | 14x faster |
| String ops | ~5ms | ~40ms | 8x faster |
| Filtering | ~8ms | ~35ms | 4.4x faster |

### Memory Usage

**JavaScript:**
- V8 heap overhead
- Garbage collection pauses
- Higher base memory usage

**Python:**
- Native Rust implementation
- Minimal overhead
- Efficient memory usage

### Optimization Tips

**1. Pre-compile expressions (both)**
```python
# ✅ Good
expr = jsonatapy.compile('expression')
for data in datasets:
    result = expr.evaluate(data)
```

**2. Use JSON string API for large data (Python only)**
```python
# ✅ Python advantage - 10-50x faster
import json
json_str = json.dumps(large_data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

**3. Batch processing**
```python
# ✅ Efficient in both
expr = jsonatapy.compile('items[price > 100]')
results = [expr.evaluate(d) for d in batch]
```

## Common Pitfalls

### 1. Forgetting to Remove `await`

```javascript
// JavaScript
const result = await expr.evaluate(data);
```

```python
# Python - no await needed
result = expr.evaluate(data)  # Synchronous
```

### 2. Custom Functions

```javascript
// JavaScript - supported
expr.registerFunction('myFunc', fn, signature);
```

```python
# Python - not yet supported
# Workaround: use bindings for constants
result = expr.evaluate(data, {'constant': 42})
```

### 3. Undefined Handling

```javascript
// JavaScript - undefined is distinct from null
if (result === undefined) { ... }
```

```python
# Python - both map to None
if result is None:  # Could be null or undefined
    ...
```

### 4. Error Object Differences

```javascript
// JavaScript - detailed error object
catch (err) {
    console.log(err.position);  // Token position
    console.log(err.token);     // Problematic token
}
```

```python
# Python - string message in ValueError
except ValueError as e:
    print(str(e))  # Error message only
```

### 5. Async Patterns

```javascript
// JavaScript - promise-based
Promise.all([
    expr1.evaluate(data),
    expr2.evaluate(data)
])
```

```python
# Python - use threading for concurrency
from concurrent.futures import ThreadPoolExecutor

with ThreadPoolExecutor() as executor:
    future1 = executor.submit(expr1.evaluate, data)
    future2 = executor.submit(expr2.evaluate, data)
    results = [f.result() for f in [future1, future2]]
```

## Migration Checklist

- [ ] Replace `require('jsonata')` with `import jsonatapy`
- [ ] Remove `await` from evaluate calls
- [ ] Update error handling to use `ValueError`
- [ ] Remove custom function registrations (or use workarounds)
- [ ] Update type checks for `undefined` → `None`
- [ ] Replace Promise patterns with threading if needed
- [ ] Consider using `evaluate_json()` for large data
- [ ] Test expressions with representative data
- [ ] Benchmark performance improvements

## Next Steps

- [Explore optimization tips](optimization-tips.md)
- [Review API reference](api.md)
- [Check performance benchmarks](benchmarks.md)
- [Learn error handling patterns](error-handling.md)
