# jsonata-core (Rust crate)

`jsonata-core` is the pure Rust implementation of JSONata that powers `jsonatapy`.
It is published separately on [crates.io](https://crates.io/crates/jsonata-core) for
use in Rust projects that don't need Python bindings.

## Installation

```toml
[dependencies]
jsonata-core = "2.1.0"
```

By default this enables SIMD-accelerated JSON parsing. To disable:

```toml
jsonata-core = { version = "2.1.0", default-features = false }
```

To enable Python bindings (used internally by `jsonatapy`):

```toml
jsonata-core = { version = "2.1.0", features = ["python"] }
```

## Quick start

```rust
use jsonata_core::evaluator::Evaluator;
use jsonata_core::parser;
use jsonata_core::value::JValue;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse expression once
    let ast = parser::parse("products[price > 100].name")?;

    // Build data directly or parse from JSON
    let data = JValue::from_json_str(r#"{
        "products": [
            {"name": "Laptop", "price": 1200},
            {"name": "Mouse",  "price": 25},
            {"name": "Monitor","price": 450}
        ]
    }"#)?;

    // Evaluate
    let result = Evaluator::new().evaluate(&ast, &data)?;
    println!("{}", result.to_json_string()?);
    // ["Laptop","Monitor"]

    Ok(())
}
```

## Public API

### `parser::parse`

```rust
pub fn parse(expression: &str) -> Result<AstNode, ParserError>
```

Parses a JSONata expression string into an AST. Parsing is the expensive step —
do it once and reuse the `AstNode` across many evaluations.

### `Evaluator`

```rust
pub struct Evaluator { ... }

impl Evaluator {
    pub fn new() -> Self;
    pub fn with_context(context: Context) -> Self;
    pub fn evaluate(&mut self, node: &AstNode, data: &JValue)
        -> Result<JValue, EvaluatorError>;
}
```

`Evaluator` is stateful (holds the scope stack). Construct a fresh one per
top-level `evaluate()` call, or reuse one if you're managing scope manually.

### `JValue`

The runtime value type. All JSONata values are represented as `JValue`.

```rust
// Constructors
JValue::from_json_str(s: &str) -> Result<JValue, ...>  // parse JSON string
JValue::object(map: IndexMap<String, JValue>) -> JValue
JValue::array(vec: Vec<JValue>) -> JValue
JValue::string(s: impl Into<Rc<str>>) -> JValue
JValue::from(n: f64) -> JValue                          // Number
JValue::Bool(b: bool)
JValue::Null
JValue::Undefined

// Serialisation
value.to_json_string() -> Result<String, ...>
```

`JValue` clones are O(1) — heap variants (`String`, `Array`, `Object`) use
`Rc` reference counting, so cloning shares the allocation.

### `Context`

```rust
pub struct Context { ... }

impl Context {
    pub fn new() -> Self;
    pub fn bind(&mut self, name: String, value: JValue);
}
```

Used to inject variable bindings before evaluation:

```rust
let mut ctx = Context::new();
ctx.bind("threshold".to_string(), JValue::from(100.0));

let mut ev = Evaluator::with_context(ctx);
let result = ev.evaluate(&ast, &data)?;  // $threshold available in expression
```

## Performance

Criterion benchmark results (pure Rust, no Python, release build):

| Expression | Time |
|-----------|------|
| Simple field lookup | 81 ns |
| Arithmetic | 140 ns |
| Conditional | 106 ns |
| String operations | 126–284 ns |
| `$sum` (100 elements) | 287 ns |
| `$sum` (1000 elements) | 1.88 µs |
| Filter predicate (100 objects) | 7.9 µs |
| Realistic workload (100 products) | 9–79 µs |

Compared to `jsonata-rs` (the next fastest Rust implementation): **~40x faster**
across typical workloads.

Run benchmarks:

```bash
git clone https://github.com/txjmb/jsonata-core.git
cd jsonata-core
cargo bench --no-default-features --features simd
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `simd` | yes | SIMD-accelerated JSON parsing via `simd-json` |
| `python` | no | PyO3 Python bindings (used by `jsonatapy`) |

## Compatibility

- Rust stable 1.70+
- Passes all 1258 JSONata 2.1.0 reference tests
- `!Send` — uses `Rc` internally; not safe to send across threads.
  For parallel workloads, create one `Evaluator` per thread.
