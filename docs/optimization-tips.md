# Optimization Tips

Best practices for maximizing jsonatapy performance.

## Table of Contents

- [Quick Wins](#quick-wins)
- [Pre-compilation Strategies](#pre-compilation-strategies)
- [Data Format Optimization](#data-format-optimization)
- [Expression Patterns](#expression-patterns)
- [Memory Management](#memory-management)
- [Advanced Techniques](#advanced-techniques)

## Quick Wins

### 1. Compile Once, Evaluate Many Times

**Impact:** 10-1000x faster for repeated evaluations

```python
import jsonatapy

# ❌ Slow - compiles every time
for record in records:
    result = jsonatapy.evaluate("items[price > 100]", record)

# ✅ Fast - compile once
expr = jsonatapy.compile("items[price > 100]")
for record in records:
    result = expr.evaluate(record)
```

**When to use:**
- Same expression used multiple times
- Processing streams of data
- API endpoints with fixed transformations
- ETL pipelines

### 2. Use JsonataData for Repeated Queries

**Impact:** Eliminates Python-to-Rust conversion overhead

```python
import jsonatapy

# Convert data once
data = jsonatapy.JsonataData(large_dataset)

# Reuse data handle for multiple expressions
expr1 = jsonatapy.compile("orders[total > 100]")
expr2 = jsonatapy.compile("$sum(orders.total)")

result1 = expr1.evaluate_with_data(data)
result2 = expr2.evaluate_with_data(data)
```

**When to use:**
- Multiple expressions on same data
- Dashboard queries with shared data
- Interactive data exploration

### 3. Use JSON String API for Large Data

**Impact:** 10-50x faster for datasets with 1000+ items

```python
import json
import jsonatapy

expr = jsonatapy.compile("items[price > 100]")

# ✅ Fast path - JSON string in/out
json_str = json.dumps(large_data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

**When to use:**
- Large datasets (1000+ items)
- High-frequency evaluation (millions of calls)
- Data already in JSON format (API responses, files)

### 4. Fastest Path: Pre-converted Data + JSON Output

**Impact:** Maximum performance, zero conversion overhead

```python
import json
import jsonatapy

# Convert data once from JSON
data = jsonatapy.JsonataData.from_json(json_str)

# Compile expression once
expr = jsonatapy.compile("items[price > 100]")

# Evaluate with zero overhead
result_str = expr.evaluate_data_to_json(data)
result = json.loads(result_str)
```

**When to use:**
- Performance-critical hot paths
- Maximum throughput scenarios
- Real-time data processing

## Pre-compilation Strategies

### Module-Level Compilation

```python
import jsonatapy

# Compile at module load time
FILTER_EXPENSIVE = jsonatapy.compile("items[price > 100]")
CALCULATE_TOTAL = jsonatapy.compile("$sum(items.(quantity * price))")
EXTRACT_NAMES = jsonatapy.compile("items.name")

def process_order(order_data):
    """Process order with pre-compiled expressions."""
    expensive = FILTER_EXPENSIVE.evaluate(order_data)
    total = CALCULATE_TOTAL.evaluate(order_data)
    names = EXTRACT_NAMES.evaluate(order_data)
    return {"expensive": expensive, "total": total, "names": names}
```

### Class-Based Expression Management

```python
import jsonatapy

class OrderProcessor:
    """Encapsulate expressions for order processing."""

    def __init__(self):
        # Compile all expressions at initialization
        self.filter_expr = jsonatapy.compile("orders[total > $threshold]")
        self.sum_expr = jsonatapy.compile("$sum(orders.total)")
        self.group_expr = jsonatapy.compile("orders^(region)")

    def filter_orders(self, data, threshold):
        return self.filter_expr.evaluate(data, {"threshold": threshold})

    def calculate_total(self, data):
        return self.sum_expr.evaluate(data)

    def group_by_region(self, data):
        return self.group_expr.evaluate(data)

# Initialize once
processor = OrderProcessor()

# Use many times
result1 = processor.filter_orders(data1, 1000)
result2 = processor.filter_orders(data2, 2000)
```

### Expression Registry Pattern

```python
import jsonatapy

class ExpressionRegistry:
    """Registry for pre-compiled expressions."""

    def __init__(self):
        self._expressions = {}

    def register(self, name, expression_str):
        """Register and compile expression."""
        self._expressions[name] = jsonatapy.compile(expression_str)

    def evaluate(self, name, data, bindings=None):
        """Evaluate registered expression."""
        return self._expressions[name].evaluate(data, bindings)

# Setup
registry = ExpressionRegistry()
registry.register("filter", "items[price > 100]")
registry.register("sum", "$sum(items.price)")

# Use
result = registry.evaluate("filter", data)
```

## Data Format Optimization

### Choose the Right Input Format

```python
import json
import jsonatapy

expr = jsonatapy.compile("large_array[field > 100]")

# Benchmark results (1000 items):
# evaluate():           ~50ms  (Python object conversion)
# evaluate_json():      ~5ms   (JSON string, 10x faster)
# evaluate_with_data(): ~45ms  (pre-converted, amortized)

# For one-time use with large data
json_str = json.dumps(data)
result = expr.evaluate_json(json_str)  # Fastest

# For repeated queries on same data
data_handle = jsonatapy.JsonataData(data)
result1 = expr.evaluate_with_data(data_handle)  # Fast
result2 = expr.evaluate_with_data(data_handle)  # No re-conversion
```

### Benchmark Your Use Case

```python
import time
import json
import jsonatapy

def benchmark(name, func, iterations=100):
    start = time.time()
    for _ in range(iterations):
        func()
    elapsed = (time.time() - start) / iterations
    print(f"{name}: {elapsed*1000:.2f}ms")

data = {"items": [{"price": i} for i in range(1000)]}
json_str = json.dumps(data)
data_handle = jsonatapy.JsonataData(data)

expr = jsonatapy.compile("items[price > 500]")

benchmark("evaluate()", lambda: expr.evaluate(data))
benchmark("evaluate_json()", lambda: expr.evaluate_json(json_str))
benchmark("evaluate_with_data()", lambda: expr.evaluate_with_data(data_handle))
```

## Expression Patterns

### Use Path Expressions Instead of Higher-Order Functions

```python
import jsonatapy

# ❌ Slower - higher-order function
expr = jsonatapy.compile("$map(items, function($i) { $i.name })")

# ✅ Faster - path expression
expr = jsonatapy.compile("items.name")
```

### Combine Operations in Single Expression

```python
import jsonatapy

# ❌ Slower - multiple evaluations
items = jsonatapy.evaluate("orders.items", data)
filtered = jsonatapy.evaluate("items[price > 100]", {"items": items})
names = jsonatapy.evaluate("items.name", {"items": filtered})

# ✅ Faster - single expression
names = jsonatapy.evaluate("orders.items[price > 100].name", data)
```

### Use Specialized Predicates

**Note:** Simple field comparisons are optimized internally.

```python
import jsonatapy

# ✅ Optimized - simple comparison
expr = jsonatapy.compile("items[price > 100]")

# ✅ Optimized - field equality
expr = jsonatapy.compile("items[category = 'electronics']")

# ⚠️ Not optimized - complex predicate
expr = jsonatapy.compile("items[$contains(name, 'widget')]")
```

**Predicate optimization applies to:**
- Simple field comparisons: `field > value`, `field = value`, etc.
- Direct field access in predicates
- Numeric and string comparisons

**Not optimized:**
- Function calls in predicates
- Complex boolean logic
- Nested predicates

### Avoid Deep Nesting

```python
import jsonatapy

# ❌ Slower - deeply nested
expr = jsonatapy.compile("$map($map($map(items, f1), f2), f3)")

# ✅ Faster - flat structure
expr = jsonatapy.compile("items.{ ... }")
```

### Pre-filter Before Expensive Operations

```python
import jsonatapy

# ❌ Slower - sorts all items first
expr = jsonatapy.compile("$sort(items, function($a, $b) { $a.price - $b.price })[0:10]")

# ✅ Faster - filter then sort
expr = jsonatapy.compile("$sort(items[price > 100], function($a, $b) { $a.price - $b.price })[0:10]")
```

## Memory Management

### Avoid Creating Large Intermediate Results

```python
import jsonatapy

# ❌ Creates large intermediate array
expr = jsonatapy.compile("$map(items, function($i) { $i.details }).$join(', ')")

# ✅ More memory efficient
expr = jsonatapy.compile("$join(items.details, ', ')")
```

### Use Streaming Patterns for Large Datasets

```python
import jsonatapy
import json

def process_large_file(filename, expression_str):
    """Process large JSON file in chunks."""
    expr = jsonatapy.compile(expression_str)

    with open(filename, 'r') as f:
        # Read line by line if JSONL format
        for line in f:
            data = json.loads(line)
            result = expr.evaluate(data)
            yield result
```

### Clear References to Large Objects

```python
import jsonatapy

def process_batch(data_list):
    """Process batch and release memory."""
    expr = jsonatapy.compile("items[price > 100]")
    results = []

    for data in data_list:
        result = expr.evaluate(data)
        results.append(result)
        # Data reference released here

    return results
```

## Advanced Techniques

### Parallel Processing with Thread Pool

```python
import jsonatapy
from concurrent.futures import ThreadPoolExecutor

# Compile once (thread-safe)
expr = jsonatapy.compile("items[price > 100].name")

def process_record(data):
    """Process single record."""
    return expr.evaluate(data)

# Process in parallel
with ThreadPoolExecutor(max_workers=4) as executor:
    results = list(executor.map(process_record, data_list))
```

### Caching Results

```python
import jsonatapy
from functools import lru_cache

class CachedEvaluator:
    """Evaluator with result caching."""

    def __init__(self, expression_str):
        self.expr = jsonatapy.compile(expression_str)

    @lru_cache(maxsize=1000)
    def evaluate_cached(self, data_json_str):
        """Evaluate with caching (requires hashable input)."""
        return self.expr.evaluate_json(data_json_str)

# Usage
evaluator = CachedEvaluator("items[price > 100]")

# First call - computes result
result1 = evaluator.evaluate_cached(json_str1)

# Second call with same input - returns cached result
result2 = evaluator.evaluate_cached(json_str1)
```

### Batch Processing with JsonataData

```python
import jsonatapy

def process_batch_efficiently(data_list, expressions):
    """Efficiently process multiple expressions on batch of data."""
    # Pre-compile all expressions
    compiled = [jsonatapy.compile(e) for e in expressions]

    results = []
    for data in data_list:
        # Convert data once
        data_handle = jsonatapy.JsonataData(data)

        # Evaluate all expressions on same data
        record_results = [
            expr.evaluate_with_data(data_handle)
            for expr in compiled
        ]
        results.append(record_results)

    return results
```

### Expression Optimization Checklist

Before deploying to production:

- [ ] Expressions compiled at initialization time
- [ ] Using appropriate data format (evaluate, evaluate_json, or evaluate_with_data)
- [ ] Simple path expressions instead of HOFs where possible
- [ ] Combined operations in single expression
- [ ] Pre-filtering before expensive operations
- [ ] No unnecessary intermediate results
- [ ] Profiled performance on representative data

### Profiling Example

```python
import time
import json
import jsonatapy

def profile_expression(expression_str, data, iterations=1000):
    """Profile expression performance."""
    # Compilation time
    start = time.time()
    expr = jsonatapy.compile(expression_str)
    compile_time = time.time() - start

    # Evaluation time
    start = time.time()
    for _ in range(iterations):
        result = expr.evaluate(data)
    eval_time = (time.time() - start) / iterations

    # JSON string evaluation time
    json_str = json.dumps(data)
    start = time.time()
    for _ in range(iterations):
        result = expr.evaluate_json(json_str)
    json_time = (time.time() - start) / iterations

    print(f"Expression: {expression_str}")
    print(f"  Compile time: {compile_time*1000:.2f}ms")
    print(f"  Evaluate time: {eval_time*1000:.3f}ms")
    print(f"  JSON evaluate time: {json_time*1000:.3f}ms")
    print(f"  Speedup: {eval_time/json_time:.1f}x")

# Example usage
data = {"items": [{"price": i, "name": f"Item {i}"} for i in range(100)]}
profile_expression("items[price > 50].name", data)
```

## Performance Comparison Summary

| Scenario | Method | Relative Speed | Best For |
|----------|--------|----------------|----------|
| Small data, one-time | `evaluate()` | 1x | Quick queries |
| Large data, one-time | `evaluate_json()` | 10-50x | API responses |
| Small data, repeated | `compile()` + `evaluate()` | 10-100x | Multiple queries |
| Large data, repeated | `compile()` + `evaluate_json()` | 100-1000x | High throughput |
| Multiple exprs, same data | `JsonataData` + `evaluate_with_data()` | 50-200x | Dashboards |
| Maximum performance | `JsonataData.from_json()` + `evaluate_data_to_json()` | 100-1000x | Critical paths |

## Next Steps

- [Review error handling patterns](error-handling.md)
- [Learn migration strategies](migration-from-js.md)
- [Explore API reference](api.md)
- [Check performance benchmarks](benchmarks.md)
