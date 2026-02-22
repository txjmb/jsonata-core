# jsonata-core annd jsonatapy

High-performance Python/Rust implementation of [JSONata](https://jsonata.org/) - the JSON query and transformation language.  Yes, much of this project was written with a lot of human guidance by Claude in Claude Code.  There wasn't any performant implementation of jsonata in Python, so I thought I'd see if Claude could pull of a jsonata port and decided on Rust with a PyO3 wrapper. Jsonata-core is available as a Rust crate and jsonatpy (the wrapper/extension) on Pypi.

[![PyPI version](https://badge.fury.io/py/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![Python versions](https://img.shields.io/pypi/pyversions/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

jsonatapy is a Python extension implementing JSONata directly in Rust via jsonata-core for native performance.

- **Full JSONata 2.1.0 Support** - 1258/1258 reference tests passing
- **Native Python API** - Type hints, zero JavaScript dependencies
- **Cross-Platform** - Linux, macOS (Intel & ARM), Windows
- **Production Ready** - Comprehensive test suite and error handling
- **Performant** - Faster than native javascript library on many operations (see documentation section on [Performance](https://txjmb.github.io/jsonatapy/performance/) for benchmark results vs other libraries)

## Installation

```bash
pip install jsonatapy
```

Supports Python 3.10, 3.11, 3.12, 3.13 on all major platforms.

## Quick Start

```python
import jsonatapy

# Simple query
data = {"name": "World"}
result = jsonatapy.evaluate('"Hello, " & name', data)
print(result)  # "Hello, World"

# Compile once, reuse many times
expr = jsonatapy.compile("orders[price > 100].product")
result = expr.evaluate(data)

# Complex transformation
data = {
    "orders": [
        {"product": "Laptop", "quantity": 2, "price": 1200},
        {"product": "Mouse", "quantity": 5, "price": 25},
        {"product": "Keyboard", "quantity": 3, "price": 75}
    ]
}

result = jsonatapy.evaluate("$sum(orders.(quantity * price))", data)
print(result)  # 2750
```

## What is JSONata?

JSONata is a query and transformation language for JSON:

- **Query** - Extract data: `person.name`
- **Filter** - Select items: `products[price > 50]`
- **Transform** - Reshape data: `items.{"name": title, "cost": price}`
- **Aggregate** - Calculate: `$sum(orders.total)`
- **Conditionals** - Logic: `price > 100 ? "expensive" : "affordable"`

See [official JSONata docs](https://docs.jsonata.org/) for language reference.

## Performance

jsonatapy is the **fastest JSONata implementation available for Python** — by a wide margin — and faster than the JavaScript reference implementation for most pure expression workloads.

| Category | vs JavaScript | vs jsonata-python |
|----------|--------------|-------------------|
| Simple paths | **7–10x faster** | ~25x faster |
| Conditionals | **18x faster** | ~50x faster |
| String operations | **7x faster** | ~30x faster |
| Complex transformations | **8x faster** | ~25x faster |
| Higher-order functions | ~2x slower | ~60x faster |
| Array-heavy workloads | varies (see below) | ~10–50x faster |

For **pure expression evaluation** (simple queries, conditionals, string and math operations), jsonatapy consistently beats V8 JavaScript. It is also significantly faster than jsonata-rs, the leading pure-Rust JSONata implementation.

### The Python Boundary

For workloads that iterate over large arrays of Python dicts (filtering, mapping, aggregation), the dominant cost is converting Python objects to Rust values on each call — not expression evaluation. This is a fundamental property of the Python/C extension model, not specific to jsonatapy. Two paths avoid it:

```python
# Path 1: Pre-convert data once, reuse across many evaluations (6–15x faster)
data_handle = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data_handle)

# Path 2: Keep data as a JSON string, skip Python object creation entirely
result_str = expr.evaluate_json(json_string)
```

With pre-converted data, even array-heavy expressions run within 2–7x of V8, which is the irreducible gap between a Rust bytecode interpreter and V8's JIT compiler.

See [Performance](docs/performance.md) for full benchmark results.

## Features

### JSONata 2.1.0 Specification

- Path expressions and queries
- Array filtering and mapping
- Lambda functions and higher-order functions
- Object construction and transformation
- Built-in functions (string, numeric, array, object)
- Conditional expressions

### Python API

```python
# One-off evaluation
result = jsonatapy.evaluate(expression, data)

# Compiled expressions (faster for repeated use)
expr = jsonatapy.compile(expression)
result = expr.evaluate(data)

# Check version
print(jsonatapy.__version__)  # 2.1.0
```

## Documentation

- [Installation](docs/installation.md) - Setup and requirements
- [API Reference](docs/api.md) - Complete Python API
- [Usage Guide](docs/usage.md) - Examples and patterns
- [Performance](docs/performance.md) - Benchmarks
- [Building](docs/development/building.md) - Development setup

## Use Cases

- API response transformation
- ETL pipelines and data processing
- Configuration file processing
- Data filtering and aggregation

## Building from Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install maturin
pip install maturin

# Build and install
git clone https://github.com/txjmb/jsonata-core.git
cd jsonatapy
maturin develop --release
```

See [Building Guide](docs/development/building.md) for details.

## Testing

```bash
# Install dependencies
uv pip install --system pytest pytest-xdist

# Run tests
pytest tests/python/ -v
```

## Contributing

Contributions welcome! See [CLAUDE.MD](CLAUDE.MD) for architecture guidelines.

## License

MIT License - See [LICENSE](LICENSE) for details.

This project implements the JSONata specification. JSONata and the reference implementation [jsonata-js](https://github.com/jsonata-js/jsonata) are also MIT licensed.

## Acknowledgments

- JSONata team for the specification and reference implementation
- Rust and PyO3 communities
