# Benchmarks

jsonatapy includes a comprehensive benchmark suite comparing performance across multiple JSONata implementations.

## Running Benchmarks

```bash
# Build jsonatapy
maturin develop --release

# Install dependencies
uv pip install --system pytest

# Run benchmark suite
uv run python benchmarks/benchmark.py
```

## Implementations Compared

The benchmark suite compares four JSONata implementations:

1. **jsonatapy** - Rust-based Python extension (this project)
2. **JavaScript** - Official jsonata-js reference implementation (Node.js)
3. **jsonata-rs** - Pure Rust implementation
4. **jsonata-python** - Python wrapper around Node.js

## Benchmark Categories

- **Simple Paths** - Basic field access and navigation
- **Array Operations** - Filtering, mapping, aggregation
- **String Functions** - String manipulation operations
- **Numeric Functions** - Mathematical operations
- **Object Construction** - Creating new object structures
- **Conditionals** - Ternary operators and branching
- **Higher-Order Functions** - Map, filter, reduce with lambdas

## Results

See [Performance](performance.md) for detailed benchmark results and analysis.

## Custom Benchmarks

To add new benchmark cases, edit `benchmarks/benchmark.py` and add test cases to the suite.

```python
{
    "name": "My custom test",
    "category": "custom",
    "expression": "items[price > 100].name",
    "data": {"items": [...]},
    "iterations": 1000,
    "warmup": 100
}
```

## Visualization

Generate enhanced reports with charts:

```bash
uv run python benchmarks/enhanced_report.py
```

This creates charts in `benchmarks/charts/` showing performance comparisons.
