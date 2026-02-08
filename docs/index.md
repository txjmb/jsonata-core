# jsonatapy Documentation

High-performance Python implementation of JSONata, the JSON query and transformation language.

## Quick Start

### Installation

```bash
pip install jsonatapy
```

Supports Python 3.10, 3.11, 3.12, 3.13 on Linux, macOS, and Windows.

### Basic Usage

```python
import jsonatapy

# Simple query
data = {"name": "World"}
result = jsonatapy.evaluate('"Hello, " & name', data)
print(result)  # "Hello, World"

# Compile once, evaluate many times
expr = jsonatapy.compile("orders[price > 100].product")
result = expr.evaluate(data)
```

## Features

- **Full JSONata 2.1.0 Support** - 1258/1258 reference tests passing
- **Rust Implementation** - Type-safe with Python bindings via PyO3
- **Cross-Platform** - Linux, macOS (Intel & ARM), Windows
- **Production Ready** - Comprehensive test suite and error handling

## Documentation

- [Installation](installation.md) - Setup and requirements
- [API Reference](api.md) - Complete Python API
- [Usage Guide](usage.md) - Examples and patterns
- [Performance](performance.md) - Benchmarks and optimization
- [Development](development/building.md) - Building and contributing

## What is JSONata?

JSONata is a query and transformation language for JSON data:

- **Query** - Extract data: `person.name`
- **Filter** - Select items: `products[price > 50]`
- **Transform** - Reshape data: `items.{"name": title, "cost": price}`
- **Aggregate** - Calculate: `$sum(orders.total)`
- **Conditionals** - Logic: `price > 100 ? "expensive" : "affordable"`

See [official JSONata docs](https://docs.jsonata.org/) for complete language reference.

## Version

- **jsonatapy**: 2.1.0
- **JSONata specification**: 2.1.0
- **Python**: 3.10+

## License

MIT License - See [LICENSE](license.md) for details.
