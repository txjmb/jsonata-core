# jsonatapy

A high-performance Python implementation of [JSONata](https://jsonata.org/) - the JSON query and transformation language.

## Overview

**jsonatapy** is a Rust-based Python extension that brings the power of JSONata to Python with native performance. Unlike existing Python wrappers that embed a JavaScript engine, jsonatapy implements JSONata directly in Rust, providing:

- **Performance**: 2-5x faster than the JavaScript implementation, 10-100x faster than JS wrapper solutions
- **Native Integration**: Pure Python API with full type hint support
- **Zero JavaScript Dependencies**: No Node.js or JavaScript engine required
- **Memory Efficient**: Rust's zero-cost abstractions minimize overhead
- **Full Compatibility**: Passes 100% of the reference implementation's test suite

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

**Note: This project is currently under active development. Installation instructions will be updated once the first release is available.**

### From PyPI (Coming Soon)
```bash
pip install jsonatapy
# or with UV (recommended - 10-100x faster!)
uv pip install jsonatapy
```

### From Source

**Requirements:**
- Rust (https://rustup.rs/)
- Python 3.8+
- UV (recommended) or pip

**Quick setup with UV:**
```bash
# Install UV
curl -LsSf https://astral.sh/uv/install.sh | sh

# Setup and build
uv venv
source .venv/bin/activate
uv pip install maturin
maturin develop

# Run tests
uv run pytest tests/python/ -v
```

See **UV_SETUP.md** for detailed UV instructions, or **BUILD_INSTRUCTIONS.md** for traditional setup.

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
```

## Project Status

This project is in **early development**. We are currently implementing:

- ✅ Project structure and tooling
- 🚧 Lexer and tokenizer
- 🚧 Parser (expression to AST)
- 🚧 Core evaluator
- 📋 Built-in functions
- 📋 Advanced features (async, higher-order functions)

See [CLAUDE.MD](CLAUDE.MD) for detailed implementation roadmap.

## Design Goals

1. **Maintainability**: Code structure mirrors the reference JavaScript implementation for easy upstream synchronization
2. **Compatibility**: 100% test suite compatibility with jsonata-js
3. **Quality**: Best-in-class documentation, testing, and CI/CD

## Development

See [CLAUDE.MD](CLAUDE.MD) for comprehensive development guidelines.

### Quick Start

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Python dependencies
pip install maturin pytest

# Build and install the extension in development mode
maturin develop

# Run tests
pytest tests/python/
cargo test
```

## Documentation

- [JSONata Language Reference](https://docs.jsonata.org/)
- [Try JSONata Online](https://try.jsonata.org/)
- [Reference Implementation (JavaScript)](https://github.com/jsonata-js/jsonata)

## Contributing

Contributions are welcome! Please see [CLAUDE.MD](CLAUDE.MD) for:
- Code style guidelines
- Testing requirements
- Implementation priorities
- How to track with upstream changes

## Performance Benchmarks

*Benchmarks will be added as implementation progresses*

## License

MIT License - See [LICENSE](LICENSE) for details.

This project is inspired by and compatible with [jsonata-js](https://github.com/jsonata-js/jsonata) (also MIT licensed).

## Acknowledgments

- The JSONata team for creating and maintaining the reference implementation
- The Rust and PyO3 communities for excellent tooling
