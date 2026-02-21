# Benchmarks

jsonatapy includes a comprehensive benchmark suite comparing performance across multiple JSONata implementations.

## Running Benchmarks

```bash
# Build jsonatapy
maturin develop --release

# Install benchmark dependencies
uv sync --extra bench

# Run full benchmark suite
uv run python benchmarks/python/benchmark.py

# Update docs/performance.md with latest results
uv run python benchmarks/update_docs.py
```

## Implementations Compared

Latest benchmark results (2026-02-21) compare:

1. **jsonatapy** - Rust-based Python extension (this project), `evaluate(dict)` path
2. **jsonatapy (rust-only)** - Same library, `evaluate_json(str)` path (bypasses Python object conversion)
3. **jsonata-js** - Official JavaScript reference implementation (Node.js v24.13.1)
4. **jsonata-python** - Pure Python implementation
5. **jsonata-rs** - Pure Rust CLI implementation (optional, requires building the binary)

To include jsonata-rs in benchmarks:
```bash
cd benchmarks/rust
cargo build --release
```

## Benchmark Categories

- **Simple Paths** - Basic field access and arithmetic (`name`, `a.b.c`, `x + y`)
- **Array Operations** - Aggregation and filtering on arrays of scalars and objects
- **Complex Transformations** - Object construction, conditionals, nested function calls
- **Deep Nesting** - Paths 12+ levels deep, nested array access
- **String Operations** - `$uppercase`, `$substring`, `$contains`, etc.
- **Higher-Order Functions** - `$map`, `$filter`, `$reduce` with lambdas
- **Realistic Workload** - E-commerce dataset queries: filter, aggregate, transform, sort
- **Path Comparison** - Same expressions across all four evaluation paths (`evaluate`, `evaluate_json`, `evaluate_with_data`, `evaluate_data_to_json`)

## Results

See [Performance](performance.md) for detailed benchmark results, charts, and analysis of the performance characteristics of each category.

## Key Findings

jsonatapy is the fastest Python JSONata implementation available, and faster than the JavaScript reference for pure expression workloads. The performance ceiling for array-heavy workloads is set by Python's object model: converting Python dicts to Rust values costs roughly 1µs per field, which dominates evaluation time for large datasets. The `JsonataData` API and `evaluate_json` path avoid this cost.

## Custom Benchmarks

To add new benchmark cases, edit `benchmarks/python/benchmark.py` and add entries to the benchmark list:

```python
{
    "name": "My custom test",
    "category": "Custom",
    "expression": "items[price > 100].name",
    "data": {"items": [...]},
    "iterations": 1000,
    "warmup": 100,
}
```
