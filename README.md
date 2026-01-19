# jsonatapy

A high-performance Python implementation of [JSONata](https://jsonata.org/) - the JSON query and transformation language.

[![PyPI version](https://badge.fury.io/py/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![Python versions](https://img.shields.io/pypi/pyversions/jsonatapy.svg)](https://pypi.org/project/jsonatapy/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

## Overview

**jsonatapy** is a Rust-based Python extension that brings the power of JSONata to Python with native performance. Unlike existing Python wrappers that embed a JavaScript engine, jsonatapy implements JSONata directly in Rust, providing:

- **Performance**: **4.5x faster than JavaScript** on average (8-18x faster on simple operations)
- **Native Integration**: Pure Python API with full type hint support
- **Zero JavaScript Dependencies**: No Node.js or JavaScript engine required
- **Memory Efficient**: Rust's zero-cost abstractions minimize overhead
- **Full JSONata 2.1.0 Support**: Lambda functions, higher-order functions, all built-in functions

## What is JSONata?

JSONata is a lightweight query and transformation language for JSON data. It enables you to:

- Extract and transform data with concise expressions
- Perform complex queries and aggregations
- Transform JSON structures declaratively
- Apply functional programming patterns to data

**Example:**
```python
import jsonatapy

data = {
    "orders": [
        {"product": "Laptop", "quantity": 2, "price": 1200},
        {"product": "Mouse", "quantity": 5, "price": 25},
        {"product": "Keyboard", "quantity": 3, "price": 75}
    ]
}

# Calculate total order value
expr = jsonatapy.compile("$sum(orders.(quantity * price))")
result = expr.evaluate(data)
print(result)  # 2750
```

## Installation

### From PyPI

```bash
pip install jsonatapy
```

Pre-built wheels available for:
- **Python**: 3.8, 3.9, 3.10, 3.11, 3.12
- **Platforms**: Windows, Linux (x86_64, aarch64), macOS (x86_64, arm64)

### From Source

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install maturin
pip install maturin

# Build and install
maturin develop --release
```

See **[Installation Guide](docs/installation.md)** for detailed instructions, troubleshooting, and platform-specific notes.

## Quick Start

```python
import jsonatapy

# Simple path query
data = {"name": "World"}
result = jsonatapy.evaluate('"Hello, " & name', data)
print(result)  # "Hello, World"

# Compile once, evaluate many times
expr = jsonatapy.compile("orders[price > 100].product")
result = expr.evaluate(data)

# High-performance JSON string API (10-50x faster for large data)
import json
json_str = json.dumps(data)
result_str = expr.evaluate_json(json_str)
result = json.loads(result_str)
```

See **[Usage Guide](docs/usage.md)** for more examples and patterns.

## Features

### ✅ Full JSONata 2.1.0 Support

- **Path Expressions**: Navigate JSON with dot notation and predicates
- **Filtering & Mapping**: Array operations with predicates and transformations
- **Lambda Functions**: First-class functions with parameter binding
- **Higher-Order Functions**: `$map`, `$filter`, `$reduce`, `$single`, `$sift`
- **Object Construction**: Build new objects with computed fields
- **Built-in Functions**: 40+ string, numeric, array, and object functions
- **Aggregations**: `$sum`, `$average`, `$min`, `$max`, `$count`
- **Conditional Expressions**: Ternary operators and logical operations

### ⚡ Performance

**4.5x faster than JavaScript on average!**

| Operation Type | vs JavaScript | Use Case |
|----------------|---------------|----------|
| Simple paths | 8-10x faster | Field access |
| Arithmetic | 14x faster | Calculations |
| Conditionals | 19x faster | Business logic |
| String operations | 8x faster | Text processing |
| Array mapping | 5-6x slower | Large arrays only |

See **[Performance Guide](docs/performance.md)** for detailed benchmarks and optimization tips.

## Documentation

📚 **[Full Documentation](docs/README.md)**

- **[Installation Guide](docs/installation.md)** - Installation for all platforms
- **[API Reference](docs/api.md)** - Complete Python API documentation
- **[Usage Guide](docs/usage.md)** - Common patterns and examples
- **[Performance Guide](docs/performance.md)** - Optimization and benchmarks
- **[Building from Source](docs/building.md)** - Development setup

### External Resources

- **[JSONata Language Reference](https://docs.jsonata.org/)** - Official JSONata syntax
- **[Try JSONata Online](https://try.jsonata.org/)** - Interactive playground
- **[Reference Implementation](https://github.com/jsonata-js/jsonata)** - JavaScript jsonata-js

## Use Cases

### API Response Transformation

```python
# Transform complex API response
expr = jsonatapy.compile('''
    {
        "userId": data.user.id,
        "fullName": data.user.firstName & " " & data.user.lastName,
        "totalSpent": $sum(data.user.orders.total)
    }
''')
```

### Data Filtering & Aggregation

```python
# Filter and aggregate sales data
expr = jsonatapy.compile('''
    $sum(transactions[region="North"].amount)
''')
```

### ETL Pipelines

```python
# Transform data in pipeline
transform = jsonatapy.compile('''
    records[status="active"].{
        "id": id,
        "name": $uppercase(name),
        "value": amount * 1.1
    }
''')
```

See **[Usage Guide](docs/usage.md)** for more real-world examples.

## Development

### Quick Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install maturin
pip install maturin

# Build and test
maturin develop --release
pytest tests/python/ -v
```

See **[Building Guide](docs/building.md)** for detailed development instructions.

## Contributing

Contributions welcome! See:
- **[CLAUDE.MD](CLAUDE.MD)** - Architecture and design guidelines
- **[Building Guide](docs/building.md)** - Development setup
- Code mirrors JavaScript reference for maintainability
- Pull requests should include tests

## License

MIT License - See [LICENSE](LICENSE) for details.

This project is inspired by and compatible with [jsonata-js](https://github.com/jsonata-js/jsonata) (also MIT licensed).

## Acknowledgments

- The JSONata team for creating and maintaining the reference implementation
- The Rust and PyO3 communities for excellent tooling
