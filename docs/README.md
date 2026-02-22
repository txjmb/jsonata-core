# jsonatapy Documentation

Comprehensive documentation for jsonatapy - the high-performance Python implementation of JSONata.

## Documentation Index

### Getting Started
- **[index.md](index.md)** - Project overview and introduction
- **[installation.md](installation.md)** - Installation guide for all platforms
- **[Quick Start](#quick-start)** - Get up and running in 5 minutes

### Using jsonatapy
- **[api.md](api.md)** - Complete Python API reference
- **[usage.md](usage.md)** - Common patterns and examples
- **[performance.md](performance.md)** - Performance guide and benchmarks

### Development
- **[development/building.md](development/building.md)** - Building from source and development guide
- **[development/contributing.md](development/contributing.md)** - Contribution guidelines

### Reference
- **[JSONata Language Docs](https://docs.jsonata.org/)** - Official JSONata syntax reference

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

# Compile once, use many times
expr = jsonatapy.compile("orders[price > 100].product")
result = expr.evaluate(data)
```

## What is JSONata?

JSONata is a lightweight query and transformation language for JSON data. Think of it as "SQL for JSON" or "jq with more power".

**Key Features:**
- Path expressions: `person.address.city`
- Filtering: `products[price > 50]`
- Transformations: `items.{"name": title, "cost": price}`
- Aggregations: `$sum(orders.total)`
- Lambda functions: `$map(items, function($x) { $x * 2 })`

## Why jsonatapy?

### Performance
- Compiled Rust implementation
- Native Python extension

### Pythonic
- Simple API: `evaluate()` and `compile()`
- Type hints and docstrings
- Works with native Python types

### Complete
- Full JSONata 2.1.0 support
- All built-in functions
- Lambda functions and higher-order functions

### Production Ready
- 1258/1258 test suite compatibility
- Detailed error messages
- Cross-platform support (Linux, macOS, Windows)

## Documentation Guide

### For New Users

1. Read [Installation Guide](installation.md)
2. Try [Quick Start](#quick-start) examples
3. Browse [Usage Guide](usage.md) for patterns
4. Check [API Reference](api.md) for details

### For Performance Optimization

1. Read [Performance Guide](performance.md)
2. Learn when to use `compile()` vs `evaluate()`
3. Understand JSON string API for large datasets
4. Review benchmark results

### For Contributors

1. Read [Building Guide](development/building.md)
2. Review [CLAUDE.md](../CLAUDE.md) for architecture
3. Set up development environment
4. Run test suite

## Common Use Cases

### API Response Transformation

Transform complex API responses into simpler structures:

```python
import jsonatapy

api_data = {
    "data": {
        "user": {
            "id": 123,
            "attributes": {"firstName": "Alice", "lastName": "Smith"}
        }
    }
}

expr = jsonatapy.compile('''
    {
        "userId": data.user.id,
        "fullName": data.user.attributes.firstName & " " & data.user.attributes.lastName
    }
''')

result = expr.evaluate(api_data)
# {"userId": 123, "fullName": "Alice Smith"}
```

### Data Filtering and Aggregation

Filter and aggregate large datasets:

```python
import jsonatapy

sales = {
    "transactions": [
        {"product": "A", "amount": 100},
        {"product": "B", "amount": 200},
        {"product": "A", "amount": 150}
    ]
}

# Total sales for product A
total = jsonatapy.evaluate(
    "$sum(transactions[product='A'].amount)",
    sales
)
print(total)  # 250
```

### ETL Pipelines

Use in data transformation pipelines:

```python
import jsonatapy
import json

# Compile transformation
transform = jsonatapy.compile('''
    records[status="active"].{
        "id": id,
        "name": $uppercase(name),
        "value": amount * 1.1
    }
''')

# For large datasets, use JSON string API
if len(data["records"]) > 1000:
    json_str = json.dumps(data)
    result_str = transform.evaluate_json(json_str)
    result = json.loads(result_str)
else:
    result = transform.evaluate(data)
```

## Getting Help

### Documentation Issues
- Check the relevant guide in this directory
- Search existing GitHub issues
- File a new issue with details

### JSONata Syntax Questions
- Review [JSONata docs](https://docs.jsonata.org/)
- Try expressions in [JSONata Exerciser](https://try.jsonata.org/)
- Check [usage.md](usage.md) for examples

### Bug Reports

Include:
- jsonatapy version: `jsonatapy.__version__`
- Python version: `python --version`
- Platform: Windows/Linux/macOS
- Minimal reproduction code
- Expected vs actual behavior

## Contributing

We welcome contributions! See [development/contributing.md](development/contributing.md) and [development/building.md](development/building.md) for:
- Development environment setup
- Building from source
- Running tests
- Code style guidelines

## License

MIT License - Compatible with upstream JSONata project.

## Links

- **GitHub**: https://github.com/txjmb/jsonata-core
- **PyPI**: https://pypi.org/project/jsonatapy/
- **JSONata Spec**: https://docs.jsonata.org/
- **Reference Implementation**: https://github.com/jsonata-js/jsonata
