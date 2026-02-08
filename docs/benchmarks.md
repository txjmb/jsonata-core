# Benchmarks

jsonatapy includes a comprehensive benchmark suite comparing performance across multiple JSONata implementations.

## Running Benchmarks

```bash
# Build jsonatapy
maturin develop --release

# Install benchmark dependencies (includes jsonata-python for comparison)
uv sync --extra bench

# Run full benchmark suite
uv run python benchmarks/python/benchmark.py
```

## Implementations Compared

Latest benchmark results (2026-02-08) compare:

1. **jsonatapy** - Rust-based Python extension (this project)
2. **jsonata-js** - Official JavaScript reference implementation (Node.js)
3. **jsonata-python** - Python wrapper around JavaScript implementation
4. **jsonata-rs** - Pure Rust implementation (optional, requires building the binary)

To include jsonata-rs in benchmarks:
```bash
cd benchmarks/rust
cargo build --release
```

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
