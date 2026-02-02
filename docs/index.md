# jsonatapy Documentation

Welcome to **jsonatapy** - a high-performance Python implementation of JSONata, the JSON query and transformation language.

## What is jsonatapy?

jsonatapy is a Rust-based Python extension that provides native Python support for [JSONata](https://jsonata.org/), a lightweight query and transformation language for JSON data. It offers **4.5x faster performance** than the JavaScript implementation for typical use cases.

## Quick Links

- **[Installation Guide](installation.md)** - Get started with jsonatapy
- **[Python API Reference](api.md)** - Complete API documentation
- **[Usage Guide](usage.md)** - Common patterns and examples
- **[Performance Guide](performance.md)** - Optimization tips and benchmarks
- **[Compatibility Report](compatibility.md)** - JSONata 2.1.0 compliance status
- **[Build Guide](building.md)** - Development and building from source
- **[JSONata Language Reference](https://docs.jsonata.org/)** - Official JSONata syntax documentation

## Quick Start

### Installation

```bash
pip install jsonatapy
```

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

## Key Features

### ✅ Full JSONata 2.1.0 Support
- Path expressions and queries
- Array filtering and mapping
- Lambda functions and higher-order functions
- Object construction and transformation
- Built-in functions (string, numeric, array, object)
- Conditional expressions
- **Validated against 1,273+ reference tests** ([Compatibility Report](compatibility.md))

### ⚡ High Performance
- **4.5x faster than JavaScript** on average
- **8-18x faster** on simple operations
- Type-safe Rust implementation
- JSON string API for maximum performance

### 🐍 Pythonic API
- Simple `evaluate()` function for one-off queries
- `compile()` for reusable expressions
- Type hints and comprehensive docstrings
- Zero configuration required

### 🔧 Production Ready
- Comprehensive test suite
- Detailed error messages
- Memory efficient
- Cross-platform support (Windows, Linux, macOS)

## What is JSONata?

JSONata is a lightweight query and transformation language for JSON data. It allows you to:

- **Query** - Extract data using path expressions: `person.name`
- **Filter** - Select items matching conditions: `products[price > 50]`
- **Transform** - Map and reshape data: `items.{"name": title, "cost": price}`
- **Aggregate** - Perform calculations: `$sum(orders.total)`
- **Conditionals** - Make decisions: `price > 100 ? "expensive" : "affordable"`

For complete JSONata language documentation, see the [official JSONata docs](https://docs.jsonata.org/).

## Performance Highlights

jsonatapy delivers exceptional performance for typical use cases:

| Operation Type | vs JavaScript | Notes |
|----------------|---------------|-------|
| Simple paths | **8-10x faster** | Direct Rust execution |
| Arithmetic | **14x faster** | No type coercion overhead |
| Conditionals | **19x faster** | Fast boolean evaluation |
| String operations | **8x faster** | Efficient string handling |
| Array mapping | 5-6x slower | Value cloning overhead |
| **Average** | **4.5x faster** | Real-world workloads |

For detailed benchmarks and optimization tips, see the [Performance Guide](performance.md).

## When to Use jsonatapy

### ✅ Perfect For:
- Server-side data transformation in Python applications
- ETL pipelines and data processing
- API response transformation
- Configuration file processing
- Small to medium datasets (< 1000 items)
- Performance-critical Python code

### ⚠️ Consider Alternatives:
- Pure browser/Node.js environments → Use JavaScript JSONata
- Extremely large arrays (10,000+ items) → Consider custom solutions
- When you need exact JavaScript behavior quirks

## Architecture

jsonatapy consists of three main components:

1. **Parser** (Rust) - Converts JSONata expression strings into Abstract Syntax Trees (AST)
2. **Evaluator** (Rust) - Executes AST against JSON data with full JSONata semantics
3. **Python Bindings** (PyO3) - Exposes Rust functionality to Python with zero-copy where possible

```
┌─────────────────┐
│  Python Code    │
└────────┬────────┘
         │ PyO3
┌────────▼────────┐
│  Rust Extension │
│  - Parser       │
│  - Evaluator    │
│  - Functions    │
└─────────────────┘
```

## Getting Help

- **Documentation Issues**: File an issue on [GitHub](https://github.com/jsonata-js/jsonata/issues)
- **JSONata Syntax**: Refer to [JSONata docs](https://docs.jsonata.org/)
- **Bug Reports**: Include minimal reproduction case
- **Performance Questions**: See [Performance Guide](performance.md)

## Contributing

jsonatapy is designed to mirror the reference JavaScript implementation for easier maintenance. See [Build Guide](building.md) for development setup.

## License

MIT License - Same as the upstream JSONata project for maximum compatibility.

## Version

Current version: **0.1.0**
JSONata specification: **2.1.0**

---

**Next Steps:**
- [Install jsonatapy](installation.md)
- [Learn the Python API](api.md)
- [Explore usage patterns](usage.md)
- [Understand performance](performance.md)
