# jsonata-core (rust) + jsonatapy + jsonata C-ABI library + jsonata cli
#### Pypi stats
[![jsonatapy Downloads Last Month](https://assets.piptrends.com/get-last-month-downloads-badge/jsonatapy.svg 'jsonatapy Downloads Last Month by pip Trends')](https://piptrends.com/package/jsonatapy) [![jsonatapy Downloads Last Week](https://assets.piptrends.com/get-last-week-downloads-badge/jsonatapy.svg 'jsonatapy Downloads Last Week by pip Trends')](https://piptrends.com/package/jsonatapy) [![jsonatapy Average Daily Downloads](https://assets.piptrends.com/get-average-downloads-badge/jsonatapy.svg 'jsonatapy Average Daily Downloads by pip Trends')](https://piptrends.com/package/jsonatapy)

#### Crates.io stats
[![downloads](https://shieldcn.dev/crates/d/jsonata-core.svg)](https://crates.io/crates/jsonata-core)

> ### ⚠️ Upgrading to 2.2.8 — please test before you deploy
>
> 2.2.8 is primarily a **conformance** release: it corrects a number of places where this
> library disagreed with the jsonata-js reference. These issues were found during routine quality checks and documentation improvements.  The test harness was inadvertently missing some edge cases.  Many of those corrections **change the
> result** of expressions that already run today, so an upgrade is not guaranteed to be
> drop-in.  A lot of work went into this release and a significant amount of test coverage was added to improve future results with the goal of maintaining the highest level of conformance to the jsonata standard.  If you encounter any issues in testing that are not caused by the conformance changes or if you believe a change is in error, please file an issue and we'll work on it as soon as possible.  If you use the library or the CLI, please add a star to the project!
>
> The areas most likely to affect you:
>
> - **Boolean coercion of containers.** A one-element array holding a falsy value — `[0]`,
>   `[""]`, `[false]`, `[null]` — is now falsy, as the reference has it. This affects `? :`,
>   `and`, `or`, `$not`, filter predicates and `$filter`.
> - **`$formatNumber` rounds half-to-even.** `$formatNumber(12.345, "#,##0.00")` was `"12.35"`
>   and is now `"12.34"`.
> - **Sequence and null handling in paths**, including `*`, `#$i`, `[]` versus `[true]`, and
>   how an explicit `null` behaves in object construction.
> - **Stricter argument validation** on six builtins that previously validated nothing, and
>   **more accurate error codes** across roughly 112 error shapes.
>
> Every one of these moves *toward* the reference implementation — but if your expressions were
> written against the old behaviour, they may need review. **Read the
> [release notes](CHANGELOG.md) before upgrading**; the 2.2.8 section opens with a summary of
> what changed and who is affected by each item, ahead of the detailed entries.

High-performance [JSONata](https://jsonata.org/) implementation in Rust, with Python binding and C ABI/library.  If you use this library, please add a github star!

Much of this project was built using Claude Code with significant human oversight. There was no performant
JSONata implementation in Python, so the goal was to port JSONata to Rust (with a PyO3 wrapper
for Python) and see how fast it could go. The answer: faster than V8 for most expression
workloads, and faster than the next pure-Rust implementation.  The rust versions are published on crates.io, and the python wheels on pypi.  There is also a command-line binary and Python command-line available (works great with uvx) for use in scripting.  The Python library is also usable in a command-line fashion, and a C-compatible library is available for those who want to easily use jsonata in C/C++.

Many, many thanks to the incredible work of all the maintainers of the [JSONata](https://github.com/jsonata-js/jsonata) reference library.  JSONata is a very powerful, well-designed, and useful language that has made an impact on many projects.  This project leverages their outstanding work to extend that capability to Python and Rust and would not be possible without that project.  The implementation in Rust was strongly influenced by their implementation.  The 1600+ (1686/1686 passing for last build of this project) tests they created provided the scaffolding and validation for all of this project.  This project will continue to follow and be a derivative of the reference project as the JSONata reference library evolves.

Release versions will follow the reference jsonata-js project major and minor release numbers, but not necessarily patches.  This will make it easier for adopters of this library to understand each release's JSONata API compatibility.  As an example, 2.2.8 should be compliant with 2.2.x jsonata-js tests, but may have fixes specific to this library.  If a patch release for jsonata-js is relevant for this project, it will be included in a patch release that may or may not follow the patch numbers of the upstream project.

This project currently chooses not to implement async at this time, because it has limited value for most of the most common use-cases and the overhead of the async functionality would slow down synchronous use cases.  We focused on sync performance instead.

[![Test Suite](https://github.com/txjmb/jsonata-core/actions/workflows/test.yml/badge.svg)](https://github.com/txjmb/jsonata-core/actions/workflows/test.yml)
[![Code Quality](https://github.com/txjmb/jsonata-core/actions/workflows/lint.yml/badge.svg)](https://github.com/txjmb/jsonata-core/actions/workflows/lint.yml)
[![Security](https://github.com/txjmb/jsonata-core/actions/workflows/security.yml/badge.svg)](https://github.com/txjmb/jsonata-core/actions/workflows/security.yml)

[![Crates.io](https://img.shields.io/crates/v/jsonata-core.svg)](https://crates.io/crates/jsonata-core)
[![PyPI version](https://badge.fury.io/py/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![Python versions](https://img.shields.io/pypi/pyversions/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![docs.rs](https://img.shields.io/docsrs/jsonata-core.svg)](https://docs.rs/jsonata-core)
[![Docs](https://img.shields.io/badge/docs-online-blue.svg)](https://txjmb.github.io/jsonata-core)
[![JSONata conformance](https://img.shields.io/badge/JSONata%20conformance-1686%2F1686-brightgreen.svg)](https://github.com/jsonata-js/jsonata/tree/master/test/test-suite)
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
jsonata-core = "2.2.8"          # pure Rust, no Python dependency

# Optional: disable SIMD for constrained targets
jsonata-core = { version = "2.2.8", default-features = false }
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
result = expr.evaluate_with_data(data)   # 3–15x faster than evaluate(dict)
```

Supports Python 3.10, 3.11, 3.12, 3.13, 3.14 on Linux, macOS (Intel & ARM), and Windows.

---

## Command-line quick start

Both packages also ship a binary CLI and a Python-based CLI, `jq`-shaped, with an identical contract:

```bash
pip install jsonatapy
echo '{"orders":[{"product":"Laptop","price":1200}]}' | jsonatapy 'orders[price > 100].product'
# "Laptop"
```

See [CLI Reference](docs/cli.md) for the full flag/exit-code contract.

## C / C++ quick start

The engine exposes a small C ABI (8 functions, JSON text in/out), usable
from C, C++, or any language with C interop:

```c
JsonataExpr *expr = jsonata_compile("$sum(items.price)");
char *result = jsonata_evaluate(expr, "{\"items\":[{\"price\":2},{\"price\":3}]}");
// result: "5"
jsonata_free_string(result);
jsonata_free_expr(expr);
```

Build with `cargo build --release --features capi` and include
[`bindings/c/jsonata.h`](bindings/c/jsonata.h). See the
[C API guide](bindings/c/README.md) for linking (gcc/clang, Makefile,
CMake), the memory/threading contract, and error handling.

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
implementation available in either Rust or Python:

- **~6x faster on average** than the JavaScript reference implementation (V8), across all
  benchmark categories — up to ~16x for complex transformations and string operations
- **~40x faster** than jsonata-rs (the next pure-Rust JSONata implementation) on pure-Rust
  Criterion benchmarks with no Python overhead on either side (`cargo bench`)
- **hundreds of times faster** than jsonata-python, even when it reuses its fastest
  (`Context`-based) repeated-evaluation path

For large array workloads, pre-convert data once with `jsonatapy.JsonataData` and reuse it
across queries — this avoids the Python↔Rust conversion cost that otherwise dominates:

```python
data = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data)   # 3–15x faster than evaluate(dict)
```

See [Performance docs](docs/performance.md) for the full category-by-category breakdown and
benchmark methodology.

---

## Features

- **1682/1682 JSONata reference tests passing**
- **Pure Rust core** — no JavaScript runtime, no Node.js dependency
- **Optional Python bindings** — PyO3/maturin, zero-copy where possible
- **Cross-platform** — Linux, macOS (Intel & ARM), Windows; Python 3.10–3.14
- **SIMD-accelerated JSON parsing** — via `simd-json`, enabled by default (disable with `--no-default-features`)

---

## Documentation

- [Installation](docs/installation.md)
- [API Reference](docs/api.md)
- [Usage Guide](docs/usage.md)
- [CLI Reference](docs/cli.md)
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
