# jsonata-core + jsonatapy

High-performance [JSONata](https://jsonata.org/) implementation in Rust, with Python bindings.

## Two packages, one implementation

| | **jsonata-core** | **jsonatapy** |
|---|---|---|
| Language | Rust | Python |
| Published on | [crates.io](https://crates.io/crates/jsonata-core) | [PyPI](https://pypi.org/project/jsonatapy/) |
| Install | `cargo add jsonata-core` | `pip install jsonatapy` |
| Use when | You're writing Rust | You're writing Python |

`jsonatapy` is a thin PyO3 wrapper around `jsonata-core`. Both live in the
[same repository](https://github.com/txjmb/jsonata-core).

---

## Python quick start

```bash
pip install jsonatapy
```

```python
import jsonatapy

# One-off evaluation
result = jsonatapy.evaluate('"Hello, " & name', {"name": "World"})
print(result)  # "Hello, World"

# Compile once, evaluate many times
expr = jsonatapy.compile("orders[price > 100].product")
result = expr.evaluate({"orders": [{"product": "Laptop", "price": 1200}]})

# Pre-convert data for maximum throughput (6–15x faster for repeated queries)
data = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data)
```

## Rust quick start

```rust
use jsonata_core::evaluator::Evaluator;
use jsonata_core::parser;
use jsonata_core::value::JValue;

let ast = parser::parse("orders[price > 100].product")?;
let data = JValue::from_json_str(r#"{"orders":[{"product":"Laptop","price":1200}]}"#)?;
let result = Evaluator::new().evaluate(&ast, &data)?;
```

---

## Performance highlights

- **1258/1258** JSONata reference tests passing
- **up to 17x faster** than the JavaScript reference implementation for pure expression workloads
- **~40x faster** than jsonata-rs (the next pure-Rust JSONata implementation)
- **~10–65x faster** than jsonata-python across all categories

See [Performance](performance.md) for full benchmark results.

---

## What is JSONata?

JSONata is a query and transformation language for JSON data:

- **Query** — `person.name`
- **Filter** — `products[price > 50]`
- **Transform** — `items.{"name": title, "cost": price}`
- **Aggregate** — `$sum(orders.total)`
- **Conditionals** — `price > 100 ? "expensive" : "affordable"`

See the [official JSONata docs](https://docs.jsonata.org/) for the full language reference.
