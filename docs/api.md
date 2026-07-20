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

# Pre-convert data for repeated evaluation (fastest path)
data = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data)
```

## Functions

### `evaluate(expression, data, bindings=None, *, timeout=None, max_stack_depth=None, max_sequence_length=None)`

Compile and evaluate a JSONata expression in one step.

**Parameters:**
- `expression` (str): JSONata query/transformation expression
- `data` (Any): Data to query (typically dict or list)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings
- `timeout` (Optional[int]): Maximum evaluation time in milliseconds. Raises `ValueError`
  with a `D1012` code on timeout. Default `None` (unlimited). See [Guardrails](#guardrails).
- `max_stack_depth` (Optional[int]): Maximum recursion stack depth. Raises `ValueError`
  with a `D1011` code when exceeded. Default `None` (unlimited).
- `max_sequence_length` (Optional[int]): Maximum length of a query-result sequence
  (`$map`/`$filter`/wildcards/descendants/etc). Raises `ValueError` with a `D2015` code
  when exceeded. Default `None` (unlimited).

**Returns:** `Any` - Result of evaluating the expression

**Raises:** `ValueError` - If parsing or evaluation fails, or a guardrail is exceeded

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

### `compile(expression, *, timeout=None, max_stack_depth=None, max_sequence_length=None)`

Compile a JSONata expression for repeated evaluation.

**Parameters:**
- `expression` (str): JSONata query/transformation expression
- `timeout` (Optional[int]): Default max evaluation time in milliseconds for all
  `evaluate*()` calls on this expression (can be overridden per-call). See [Guardrails](#guardrails).
- `max_stack_depth` (Optional[int]): Default max recursion stack depth (can be overridden per-call).
- `max_sequence_length` (Optional[int]): Default max query-result sequence length
  (can be overridden per-call).

**Returns:** `JsonataExpression` - Compiled expression object

**Raises:** `ValueError` - If expression cannot be parsed

**Example:**
```python
expr = jsonatapy.compile("orders[price > 100].product")

data1 = {"orders": [{"product": "A", "price": 150}]}
result1 = expr.evaluate(data1)  # ["A"]

data2 = {"orders": [{"product": "B", "price": 50}]}
result2 = expr.evaluate(data2)  # []

# With a compile-time default guardrail
expr = jsonatapy.compile("$sum(items.price)", timeout=1000)
```

## JsonataExpression Class

Compiled JSONata expression that can be evaluated multiple times.

### `evaluate(data, bindings=None, *, timeout=None, max_stack_depth=None, max_sequence_length=None)`

Evaluate the compiled expression against data.

**Parameters:**
- `data` (Any): Data to query (typically dict or list)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings
- `timeout`, `max_stack_depth`, `max_sequence_length` (Optional[int]): Per-call guardrail
  overrides — see `compile()` above and [Guardrails](#guardrails). Each overrides any
  default set at compile time, for this call only.

**Returns:** `Any` - Result of evaluation

**Raises:** `ValueError` - If evaluation fails, or a guardrail is exceeded

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

### `evaluate_json(json_str, bindings=None, *, timeout=None, max_stack_depth=None, max_sequence_length=None)`

Evaluate with JSON string input/output for maximum performance.

**Parameters:**
- `json_str` (str): Input data as JSON string
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings
- `timeout`, `max_stack_depth`, `max_sequence_length` (Optional[int]): Per-call guardrail
  overrides — see [Guardrails](#guardrails).

**Returns:** `str` - Result as JSON string

**Raises:** `ValueError` - If JSON parsing or evaluation fails, or a guardrail is exceeded

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

### `evaluate_with_data(data, bindings=None, *, timeout=None, max_stack_depth=None, max_sequence_length=None)`

Evaluate against a pre-converted `JsonataData` handle — fastest path for repeated
evaluation of the same data, since the Python→Rust conversion happens once (at
`JsonataData` construction) instead of on every call.

**Parameters:**
- `data` (JsonataData): A pre-converted data handle — see [JsonataData Class](#jsonatadata-class)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings
- `timeout`, `max_stack_depth`, `max_sequence_length` (Optional[int]): Per-call guardrail
  overrides — see [Guardrails](#guardrails).

**Returns:** `Any` - Result of evaluation

**Raises:** `ValueError` - If evaluation fails, or a guardrail is exceeded

**Example:**
```python
data = jsonatapy.JsonataData({"orders": [{"price": 150}, {"price": 50}]})
expr = jsonatapy.compile("orders[price > 100]")

result = expr.evaluate_with_data(data)  # 3-15x faster than evaluate(dict) for repeated use
```

### `evaluate_data_to_json(data, bindings=None, *, timeout=None, max_stack_depth=None, max_sequence_length=None)`

Evaluate against a pre-converted `JsonataData` handle and return a JSON string —
combines `evaluate_with_data`'s input-side savings with `evaluate_json`'s
output-side savings, for the fastest path when both the input and the consumer
of the result are JSON.

**Parameters:**
- `data` (JsonataData): A pre-converted data handle — see [JsonataData Class](#jsonatadata-class)
- `bindings` (Optional[Dict[str, Any]]): Optional variable bindings
- `timeout`, `max_stack_depth`, `max_sequence_length` (Optional[int]): Per-call guardrail
  overrides — see [Guardrails](#guardrails).

**Returns:** `str` - Result as JSON string

**Raises:** `ValueError` - If evaluation fails, or a guardrail is exceeded

**Example:**
```python
import json

data = jsonatapy.JsonataData.from_json('{"orders": [{"price": 150}, {"price": 50}]}')
expr = jsonatapy.compile("orders[price > 100]")

result_str = expr.evaluate_data_to_json(data)
result = json.loads(result_str)
```

### `register(name, func)` / `register_override(name, func)`

Register a Python callable that the expression can invoke as `$name(...)` — the
equivalent of jsonata-js's `registerFunction`. Use it to expose enrichment/lookup,
formatting, or scoring logic to an otherwise-pure expression. Both methods return the
expression, so calls can be chained.

**Parameters:**
- `name` (str): The function name, called as `$name(...)` in the expression.
- `func` (Callable): Receives the already-evaluated arguments as positional Python
  values and must return a JSON-compatible value **synchronously**.

**Returns:** the `JsonataExpression` (for chaining)

**Raises:**
- `TypeError` — if `func` is not callable.
- `ValueError` — if `name` collides with a built-in (`register`), or if the built-in
  cannot be safely overridden (`register_override`).

**Behavior:**
- Host functions resolve **after** the expression's own `:=` bindings and lambdas, and
  **before** built-ins.
- `register` rejects a name that collides with a built-in; `register_override` replaces
  one deliberately — the intended uses being determinism injection for the impure
  built-ins (`$now`, `$millis`, `$random`) and sandboxing (disabling `$eval`).
- Evaluation is synchronous, so an `async def` (which returns a coroutine) is rejected
  at call time. For async I/O, await it outside jsonata and pass the result via
  `bindings`.

**Example:**
```python
# Enrichment lookup backed by host-owned data
catalog = {"A-1": "Widget", "B-2": "Gadget"}
expr = jsonatapy.compile("items.{ 'sku': sku, 'name': $productName(sku) }")
expr.register("productName", lambda sku: catalog.get(sku, "Unknown"))
expr.evaluate({"items": [{"sku": "A-1"}, {"sku": "B-2"}]})
# [{'sku': 'A-1', 'name': 'Widget'}, {'sku': 'B-2', 'name': 'Gadget'}]

# Determinism injection: freeze $now() for reproducible output
expr = jsonatapy.compile("{ 'generatedAt': $now() }")
expr.register_override("now", lambda: "2020-01-01T00:00:00.000Z")
expr.evaluate(None)
# {'generatedAt': '2020-01-01T00:00:00.000Z'}
```

See `examples/host_functions.py` for a runnable walkthrough.

#### Why host functions are synchronous

Host functions run synchronously: the callable is invoked mid-evaluation, and if
it does I/O it simply blocks until it returns. This is a deliberate design choice,
not a limitation. The evaluator is a synchronous, single-threaded engine — that is
where its speed comes from — and a blocking call stack (your code → `evaluate()` →
your host function → I/O) is a correct, ordinary way to run it. Concurrency comes
from running independent evaluations across threads or processes, exactly as you
would parallelize any other CPU-bound work. Async would only ever help a host
function that is *I/O-bound* — a CPU-bound one gains nothing from it — and even an
I/O-bound host function can run concurrently by dispatching evaluations to a thread
pool (a blocked thread waiting on I/O lets others proceed), so blocking is rarely a
real constraint. This is why an `async def` is rejected:
the synchronous core has no event loop to await a coroutine on, so a coroutine
return has no meaningful value. (jsonata-js is async only because JavaScript has no
threads and no blocking I/O — its event loop is the *only* concurrency primitive
available, so it had no choice. Python has real threads, so it does not inherit that
constraint.)

If a host function genuinely needs async I/O, do the I/O **outside** the expression
rather than inside a callback. Gather what the transform needs with `asyncio` up
front, then pass the results in through `bindings` (or bake them into small
synchronous lookups closed over that data) and run `evaluate()` normally. If you are
inside an event loop and don't want to block it, run the whole `evaluate()` call in a
thread with `loop.run_in_executor(...)`. Both patterns keep the fast synchronous core
intact while letting the async work live where it belongs — in your application, not
in the expression engine.

Synchronous execution was chosen for speed on the majority of use cases, which are
CPU-bound data transformations that never touch I/O. If enough users need genuinely
asynchronous host functions, we will consider adding that capability — but it would be
an opt-in path alongside the synchronous default, not a replacement for it.

## JsonataData Class

Pre-converted data handle for efficient repeated evaluation. Convert Python data
to jsonatapy's internal representation once, then reuse it across multiple
evaluations (via `evaluate_with_data`/`evaluate_data_to_json`) to avoid repeated
Python↔Rust conversion overhead — see [Performance Tips](#performance-tips).

!!! note "Using jsonata-core directly from Rust?"
    `JsonataData` exists purely to avoid *Python↔Rust* marshalling overhead — it
    has no separate equivalent on the Rust side, because there's no such boundary
    to cross there. In pure Rust you already work with `JValue` natively at zero
    conversion cost; see the [Quick start](rust-crate.md#quick-start) in the
    `jsonata-core` docs, which is the direct equivalent of everything below.

### `JsonataData(data)`

Create a handle from a Python object.

**Parameters:**
- `data` (Any): The data to pre-convert (typically a dict or list)

**Example:**
```python
data = jsonatapy.JsonataData({"orders": [{"price": 150}, {"price": 50}]})
expr = jsonatapy.compile("orders[price > 100]")
result = expr.evaluate_with_data(data)
```

### `JsonataData.from_json(json_str)`

Create a handle from a JSON string directly — the fastest way to construct one,
since it skips Python object conversion entirely and parses JSON straight into
jsonatapy's internal representation.

**Parameters:**
- `json_str` (str): Input data as a JSON string

**Returns:** `JsonataData` - A pre-converted data handle

**Raises:** `ValueError` - If the JSON string is invalid

**Example:**
```python
data = jsonatapy.JsonataData.from_json('{"orders": [{"price": 150}, {"price": 50}]}')
```

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

## Guardrails

`compile()`, `JsonataExpression.compile()`, and every `evaluate*()` method accept three optional
keyword-only arguments that protect against runaway or adversarial expressions:

| Parameter             | Limits                                                       | Error code |
|------------------------|--------------------------------------------------------------|------------|
| `timeout`              | Max evaluation time, in milliseconds                          | `D1012`    |
| `max_stack_depth`      | Max recursion depth (e.g. recursive lambdas)                   | `D1011`    |
| `max_sequence_length`  | Max length of a query-result sequence (`$map`/`$filter`/wildcards/descendants/etc) | `D2015` |

All three default to `None` (unlimited), matching behavior with no guardrails configured. Set
them at `compile()` time as defaults for every subsequent `evaluate*()` call, or pass them to an
individual `evaluate*()` call to override the compile-time default for that call only:

```python
import jsonatapy

# Compile-time default
expr = jsonatapy.compile("$sum(items.price)", timeout=1000, max_sequence_length=1_000_000)

# Per-call override
result = expr.evaluate(data, timeout=5000)

# Raised on violation
try:
    jsonatapy.evaluate("($inf := function(){$inf()}; $inf())", None, timeout=100)
except ValueError as e:
    print(e)  # D1012: Evaluation timeout after 100 milliseconds. Check for infinite loop
```

See [Guardrail Errors](error-handling.md#guardrail-errors) for the full list of error codes and
messages.

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
