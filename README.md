# jsonata-core + jsonatapy

High-performance [JSONata](https://jsonata.org/) implementation in Rust, with Python bindings.

> Much of this project was built with human guidance using Claude Code. There was no performant
> JSONata implementation in Python, so the goal was to port JSONata to Rust (with a PyO3 wrapper
> for Python) and see how fast it could go. The answer: faster than V8 for most expression
> workloads, and faster than the next pure-Rust implementation.

[![Crates.io](https://img.shields.io/crates/v/jsonata-core.svg)](https://crates.io/crates/jsonata-core)
[![PyPI version](https://badge.fury.io/py/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![Python versions](https://img.shields.io/pypi/pyversions/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

---

## Two packages, one implementation

| | **jsonata-core** | **jsonatapy** |
|---|---|---|
| Language | Rust | Python |
| Published on | [crates.io](https://crates.io/crates/jsonata-core) | [PyPI](https://pypi.org/project/jsonatapy/) |
| Install | `cargo add jsonata-core` | `pip install jsonatapy` |
| Use when | You're writing Rust | You're writing Python |

`jsonatapy` is a thin PyO3 wrapper around `jsonata-core`. Both live in this repo.

---

## Rust quick start

```rust
use jsonata_core::evaluator::Evaluator;
use jsonata_core::parser;
use jsonata_core::value::JValue;

let ast = parser::parse("orders[price > 100].product")?;
let data = JValue::from_json_str(r#"{"orders":[
    {"product":"Laptop","price":1200},
    {"product":"Mouse","price":25}
]}"#)?;

let result = Evaluator::new().evaluate(&ast, &data)?;
```

```toml
# Cargo.toml
[dependencies]
jsonata-core = "2.1.2"          # pure Rust, no Python dependency

# Optional: disable SIMD for constrained targets
jsonata-core = { version = "2.1.2", default-features = false }
```

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

# Compile once, evaluate many times (10–1000x faster for repeated use)
expr = jsonatapy.compile("$sum(orders.(quantity * price))")
result = expr.evaluate({
    "orders": [
        {"product": "Laptop", "quantity": 2, "price": 1200},
        {"product": "Mouse",  "quantity": 5, "price": 25},
    ]
})
print(result)  # 2450

# Pre-convert data once for maximum throughput
data = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data)   # 4–15x faster than evaluate(dict)
```

Supports Python 3.10, 3.11, 3.12, 3.13 on Linux, macOS (Intel & ARM), and Windows.

---

## What is JSONata?

JSONata is a query and transformation language for JSON data:

- **Query** — `person.name`
- **Filter** — `products[price > 50]`
- **Transform** — `items.{"name": title, "cost": price}`
- **Aggregate** — `$sum(orders.total)`
- **Conditionals** — `price > 100 ? "expensive" : "affordable"`

See [official JSONata docs](https://docs.jsonata.org/) for the full language reference.

---

## Performance

`jsonata-core` passes **1682/1682** JSONata reference tests and is the fastest JSONata
implementation available in either Rust or Python.

### Pure Rust (Criterion benchmarks, no Python overhead)

| Category | jsonata-core | vs jsonata-rs |
|----------|-------------|----------------|
| Simple path lookup | 81 ns | ~40x faster |
| Arithmetic expression | 140 ns | ~40x faster |
| Conditional | 106 ns | ~30x faster |
| String operations | 126–284 ns | ~30x faster |
| $sum (100 elements) | 287 ns | ~70x faster |
| Filter predicate (100 objects) | 7.9 µs | ~50x faster |
| Realistic workload (100 products) | 9–44 µs | ~40x faster |

Run the benchmarks yourself:
```bash
cargo bench --no-default-features --features simd
```

### Python path (`jsonatapy`)

`jsonatapy` is the fastest Python JSONata implementation by a large margin, and faster than
the JavaScript reference implementation for most pure expression workloads:

| Category | vs JavaScript (V8) | vs jsonata-python |
|----------|--------------------|-------------------|
| Simple paths | **5–8x faster** (array index access: roughly tied) | ~20–40x faster |
| Conditionals | **15x faster** | ~40x faster |
| String operations | **8–15x faster** | ~30–45x faster |
| Complex transformations | **5–15x faster** | ~20–40x faster |
| Higher-order functions | **12–19x faster** | ~50–70x faster |
| Array-heavy workloads | **~1–3x slower to ~4x faster**, depending on access pattern | ~10–50x faster |

### The Python boundary

For large array workloads, the dominant cost is converting Python dicts to Rust values
on each `evaluate()` call — not expression evaluation itself. Two API paths avoid this:

```python
# Path 1: Pre-convert data once, reuse across many queries (4–15x faster than evaluate(dict))
data = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data)

# Path 2: Data arrives as a raw JSON string — pass it directly
result_str = expr.evaluate_json(raw_json_string)
```

With pre-converted data, realistic workloads (filtering, transforming, aggregating over
100-object arrays) run **~5–8x faster than V8**, not slower — even the raw `evaluate(dict)`
path without pre-conversion is roughly at parity with V8 (0.5–2x either direction).

See [Performance docs](docs/performance.md) for full benchmark results and methodology.

---

## Features

- **1682/1682 JSONata reference tests passing**
- **Pure Rust core** — no JavaScript runtime, no Node.js dependency
- **Optional Python bindings** — PyO3/maturin, zero-copy where possible
- **Cross-platform** — Linux, macOS (Intel & ARM), Windows; Python 3.10–3.13
- **SIMD-accelerated JSON parsing** — via `simd-json` (optional feature)

---

## Documentation

- [Installation](docs/installation.md)
- [API Reference](docs/api.md)
- [Usage Guide](docs/usage.md)
- [Performance](docs/performance.md)
- [Optimization Tips](docs/optimization-tips.md)
- [Building from Source](docs/development/building.md)

---

## Building from source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone
git clone https://github.com/txjmb/jsonata-core.git
cd jsonata-core

# Build and install Python extension
pip install maturin
maturin develop --release

# Run Python tests
pytest tests/python/ -v

# Run Rust benchmarks (no Python required)
cargo bench --no-default-features --features simd
```

---

## License

MIT — see [LICENSE](LICENSE).

This project implements the JSONata specification.
[jsonata-js](https://github.com/jsonata-js/jsonata) (the reference implementation) is also MIT licensed.
