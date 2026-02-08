# Architecture

Technical overview of jsonatapy's architecture and design.

## High-Level Overview

jsonatapy is a Rust-based Python extension implementing the JSONata query and transformation language.

### Architecture Layers

```
┌─────────────────────────────────────────┐
│         Python Application              │
│         (User Code)                     │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│    Python API (python/jsonatapy/)       │
│    - compile(), evaluate()              │
│    - JsonataExpression wrapper          │
│    - JsonataData wrapper                │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│    PyO3 Bindings (src/lib.rs)           │
│    - Python↔Rust boundary               │
│    - Type conversions                   │
│    - Exception handling                 │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│         Rust Core                       │
│    ┌─────────────────────────────┐     │
│    │  Parser (src/parser.rs)     │     │
│    │  - Tokenization             │     │
│    │  - Expression parsing       │     │
│    │  - AST generation           │     │
│    └────────────┬────────────────┘     │
│                 │                       │
│    ┌────────────▼────────────────┐     │
│    │  Evaluator (src/evaluator.rs)│    │
│    │  - Expression evaluation    │     │
│    │  - Context management       │     │
│    │  - Lambda execution         │     │
│    └────────────┬────────────────┘     │
│                 │                       │
│    ┌────────────▼────────────────┐     │
│    │  Functions (src/functions.rs)│    │
│    │  - Built-in functions       │     │
│    │  - Datetime (src/datetime.rs)│    │
│    │  - Signature validation     │     │
│    └─────────────────────────────┘     │
└─────────────────────────────────────────┘
```

### Key Components

1. **Python API Layer** - User-facing Python interface
2. **PyO3 Bindings** - Python-Rust interop layer
3. **Parser** - Converts JSONata expressions to AST
4. **Evaluator** - Executes AST against data
5. **Functions** - Built-in function implementations

## Module Structure

jsonatapy mirrors the structure of the JavaScript reference implementation for maintainability.

```
src/
├── lib.rs          # PyO3 bindings, Python API entry point
├── parser.rs       # Expression parser (mirrors parser.js)
├── ast.rs          # AST node definitions
├── evaluator.rs    # Main evaluation engine (mirrors jsonata.js)
├── functions.rs    # Built-in functions (mirrors functions.js)
├── datetime.rs     # Date/time functions (mirrors datetime.js)
├── signature.rs    # Function signature validation (mirrors signature.js)
└── value.rs        # JValue type system (custom to Rust)
```

### Module Responsibilities

**lib.rs** - Python Bindings
```rust
#[pymodule]
fn _jsonatapy(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate, m)?)?;
    m.add_class::<JsonataExpression>()?;
    m.add_class::<JsonataData>()?;
    Ok(())
}
```

**parser.rs** - Lexical Analysis and Parsing
- Tokenizes JSONata expression strings
- Builds Abstract Syntax Tree (AST)
- Handles operator precedence
- Reports syntax errors

**evaluator.rs** - Expression Evaluation (~7500 lines)
- Main evaluation engine
- Context and scope management
- Lambda storage and execution
- Path traversal and predicate evaluation
- Object construction

**functions.rs** - Built-in Functions
- 40+ built-in JSONata functions
- String manipulation ($uppercase, $lowercase, $substring, etc.)
- Array operations ($map, $filter, $reduce, etc.)
- Numeric functions ($sum, $average, $max, etc.)
- Object functions ($keys, $values, $merge, etc.)

**value.rs** - Type System
- JValue enum representing all JSONata types
- Rc-wrapped for O(1) cloning
- Conversion to/from Python types

## JValue Type System

### JValue Enum

jsonatapy uses a custom `JValue` type instead of `serde_json::Value` for performance:

```rust
pub enum JValue {
    Null,
    Bool(bool),
    Number(f64),
    String(Rc<str>),              // Rc-wrapped for O(1) clone
    Array(Rc<Vec<JValue>>),       // Rc-wrapped for O(1) clone
    Object(Rc<IndexMap<String, JValue>>),  // Rc-wrapped
    Undefined,                    // JSONata undefined value
    Lambda {                      // First-class lambda function
        lambda_id: usize,
        params: Vec<String>,
        name: Option<String>,
        signature: Option<String>,
    },
    Builtin {                     // First-class built-in function
        name: String,
    },
    Regex {                       // First-class regex value
        pattern: String,
        flags: String,
    },
}
```

### Key Design Decisions

**1. Rc Wrapping for Zero-Copy Cloning**

```rust
// O(1) clone - just increments reference count
let s1 = JValue::string("hello");
let s2 = s1.clone();  // No data copy

// Compare to String clone: O(n)
let s1 = String::from("hello");
let s2 = s1.clone();  // Copies entire string
```

**Impact:** 20-100x performance improvement on realistic workloads.

**2. First-Class Lambda/Builtin/Regex**

Instead of wrapping in JSON objects:

```rust
// First-class variant (fast)
JValue::Lambda { lambda_id: 1, params: vec![], name: None, signature: None }

// Tagged JSON object (slow)
JValue::Object(map! {
    "type" => "lambda",
    "id" => 1,
})
```

**Impact:** Enum discriminant check vs hash map lookup.

**3. Undefined as Distinct Type**

```rust
// JSONata undefined != null
JValue::Undefined  // No value (not an error)
JValue::Null       // Explicit null value
```

### Type Conversions

**Python → JValue:**
```rust
fn from_py(py: Python, obj: &PyAny) -> PyResult<JValue> {
    if obj.is_none() {
        Ok(JValue::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(JValue::Bool(b))
    } else if let Ok(n) = obj.extract::<f64>() {
        Ok(JValue::Number(n))
    // ...
}
```

**JValue → Python:**
```rust
fn to_py(&self, py: Python) -> PyResult<PyObject> {
    match self {
        JValue::Null => Ok(py.None()),
        JValue::Bool(b) => Ok(b.to_object(py)),
        JValue::Number(n) => Ok(n.to_object(py)),
        JValue::String(s) => Ok(s.to_object(py)),
        // ...
    }
}
```

## Evaluation Pipeline

### Expression Compilation

```
JSONata String  →  Tokenizer  →  Parser  →  AST
"items[price>100]"              →  Node tree
```

### Expression Evaluation

```
AST  →  Evaluator  →  Context  →  Result
     ↓
  Functions
     ↓
  Scope Stack
```

### Evaluation Steps

1. **Parse** expression to AST (once)
2. **Create** evaluation context with data
3. **Traverse** AST nodes recursively
4. **Evaluate** each node with context
5. **Return** result value

### Example: Path Expression

```rust
// Expression: "items[price > 100].name"
// AST: Path(Identifier("items"),
//           Predicate(Comparison(field("price"), ">", Number(100))),
//           Identifier("name"))

fn evaluate_path(ctx: &mut Context, path: &PathNode) -> Result<JValue> {
    // 1. Evaluate base: items
    let base = evaluate_identifier(ctx, "items")?;

    // 2. Apply predicate: [price > 100]
    let filtered = apply_predicate(base, predicate)?;

    // 3. Extract field: .name
    let result = extract_field(filtered, "name")?;

    Ok(result)
}
```

## Scope Stack and Lambda Storage

### Context Structure

```rust
pub struct Context {
    scopes: Vec<Scope>,          // Stack of scopes
    lambdas: HashMap<usize, StoredLambda>,  // Lambda storage
    next_lambda_id: usize,
}

pub struct Scope {
    bindings: HashMap<String, JValue>,  // Variable bindings
}

pub struct StoredLambda {
    params: Vec<String>,
    body: Rc<AstNode>,
    captured_env: HashMap<String, JValue>,  // Captured variables
}
```

### Scope Management

Push/pop pattern (not clone/restore):
```rust
// Efficient - push/pop
ctx.push_scope();
let result = evaluate_expression(ctx, expr)?;
ctx.pop_scope();

// Inefficient - clone/restore
let old_ctx = ctx.clone();
let result = evaluate_expression(ctx, expr)?;
*ctx = old_ctx;
```

### Lambda Storage

Lambdas stored separately from values:

```rust
// Create lambda
let lambda_id = ctx.next_lambda_id();
let stored = StoredLambda {
    params: vec!["x".into()],
    body: Rc::new(body_ast),
    captured_env: capture_free_vars(ctx, &body_ast),
};
ctx.lambdas.insert(lambda_id, stored);

// Lambda value
let lambda_value = JValue::Lambda {
    lambda_id,
    params: vec!["x".into()],
    name: None,
    signature: None,
};

// Call lambda
let stored = ctx.lambdas.get(&lambda_id).unwrap();
ctx.push_scope_with_bindings(stored.captured_env.clone());
ctx.bind("x", argument);
let result = evaluate(ctx, &stored.body)?;
ctx.pop_scope();
```

### Selective Capture

Only capture free variables:
```rust
fn capture_free_vars(ctx: &Context, body: &AstNode) -> HashMap<String, JValue> {
    let free_vars = find_free_variables(body);
    let mut captured = HashMap::new();

    for var in free_vars {
        if let Some(value) = ctx.lookup(&var) {
            captured.insert(var, value.clone());
        }
    }

    captured
}
```

## Performance Optimizations

### Applied Optimizations

1. **Rc-wrapped JValue** - O(1) clone (20-100x speedup)
2. **First-class Lambda/Builtin/Regex** - Enum variants vs hash maps
3. **Scope stack push/pop** - Instead of HashMap clone
4. **Selective lambda capture** - Only capture free variables
5. **Zero-copy field extraction** - Reference-based access
6. **Predicate short-circuit** - Skip numeric evaluation for booleans
7. **Specialized predicates** - Optimize simple comparisons
8. **Merge sort** - O(n log n) sorting algorithm
9. **Iterator-based aggregation** - Zero-clone $sum/$max/$min
10. **HOF selective args** - Only pass needed arguments

### Predicate Optimization

Simple field comparisons are optimized:

```rust
// Optimized path
items[price > 100]      // Direct field comparison
items[category = "A"]   // Direct field equality

// General path (not optimized)
items[$contains(name, "widget")]  // Function call
```

### Memory Efficiency

Rc sharing prevents unnecessary copies:

```rust
let data = JValue::array(vec![...]);  // Large array

// Rc increment only - no data copy
let filtered = filter_array(&data, predicate)?;
let mapped = map_array(&filtered, mapper)?;
let sorted = sort_array(&mapped, comparator)?;
```

## Key Design Patterns

### 1. Mirror JavaScript Structure

Code organization mirrors jsonata-js for maintainability:

```
JavaScript              Rust
----------              ----
parser.js        →      parser.rs
jsonata.js       →      evaluator.rs
functions.js     →      functions.rs
datetime.js      →      datetime.rs
signature.rs     →      signature.rs
```

### 2. Pattern Matching for Type Safety

```rust
match value {
    JValue::Number(n) => Ok(n * 2.0),
    JValue::String(s) => parse_number(&s),
    JValue::Array(arr) => sum_array(&arr),
    _ => Err(EvalError::TypeError("Expected number".into())),
}
```

### 3. Result-Based Error Handling

```rust
pub fn evaluate(ctx: &mut Context, node: &AstNode) -> Result<JValue, EvalError> {
    match node {
        AstNode::Number(n) => Ok(JValue::Number(*n)),
        AstNode::Path(path) => evaluate_path(ctx, path),
        // ...
    }
}
```

### 4. Context Threading

```rust
// Context passed through call chain
fn evaluate(ctx: &mut Context, node: &AstNode) -> Result<JValue> {
    let result = evaluate_child(ctx, child_node)?;
    // ...
}

fn evaluate_child(ctx: &mut Context, node: &AstNode) -> Result<JValue> {
    // Same context, can access bindings/lambdas
}
```

### 5. Lazy Evaluation

```rust
// Only evaluate branches that are needed
fn evaluate_conditional(ctx: &mut Context, cond: &AstNode,
                       then_branch: &AstNode,
                       else_branch: &AstNode) -> Result<JValue> {
    let cond_value = evaluate(ctx, cond)?;

    if is_truthy(&cond_value) {
        evaluate(ctx, then_branch)  // Only evaluate then
    } else {
        evaluate(ctx, else_branch)  // Only evaluate else
    }
}
```

## Testing Strategy

### Three-Layer Testing

1. **Rust Unit Tests** (31 tests)
   - Test individual functions
   - Located in `src/*.rs` with `#[cfg(test)]`

2. **Python Integration Tests** (~50 tests)
   - Test Python API
   - Located in `tests/python/`

3. **Reference Test Suite** (1258 tests)
   - Official JSONata compatibility tests
   - 100% pass rate

### Test Coverage

Target: 100% coverage (matching upstream jsonata-js)

## Next Steps

- [Learn about testing](testing.md)
- [Understand build process](building.md)
- [Contribute to the project](contributing.md)
