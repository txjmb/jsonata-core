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
2. **jsonatapy (JSON string I/O)** - Same library, `evaluate_json(str)` path (data crosses the boundary as JSON strings instead of Python dicts)
3. **jsonata-core (pure Rust)** - This project's engine measured as a Rust library with no Python boundary at all: data pre-parsed to a `JValue`, expression pre-compiled, in-process timed loop (the criterion methodology, reported per benchmark row)
4. **jsonata-js** - Official JavaScript reference implementation (Node.js v24.13.1)
5. **jsonata-python** - Python wrapper embedding a JS engine (Duktape)
6. **jsonata-rs** - Third-party pure-Rust implementation (Stedi's crate — not this project; optional, requires building the binary)

To include the jsonata-core and jsonata-rs columns in benchmarks:
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

---

## Pure-Rust Criterion Benchmarks

The Python benchmark suite measures end-to-end performance including the Python↔Rust
boundary cost. To measure the Rust evaluator in isolation — with no Python interpreter,
no PyO3, no GIL, no object conversion — the crate includes a
[Criterion](https://bheisler.github.io/criterion.rs/book/) benchmark suite.

This is the most direct measure of what `jsonata-core` costs as a Rust library.

### Running

```bash
cargo bench --no-default-features --features simd
```

Results are written to `target/criterion/`. Open
`target/criterion/report/index.html` for the full HTML report.

### What is measured

Each benchmark parses the expression once (outside the timed loop), then measures
repeated `Evaluator::new().evaluate(&ast, &data)` calls using Criterion's statistical
framework (outlier detection, confidence intervals, automatically tuned sample count).

### Results (release build, AMD Ryzen / Intel Core, SIMD enabled)

| Benchmark | Time |
|-----------|------|
| Simple field lookup (`name`) | 81 ns |
| Deep path 5 levels (`a.b.c.d.e`) | 140 ns |
| Arithmetic (`price * quantity`) | 140 ns |
| Conditional (`price > 100 ? "expensive" : "affordable"`) | 106 ns |
| String operations (`$uppercase`, `$substring`) | 126–284 ns |
| `$sum` (100 elements) | 287 ns |
| `$sum` (1000 elements) | 1.88 µs |
| Filter predicate (100 objects) | 7.9 µs |
| Filter by category (100-product dataset) | 9.3 µs |
| Complex transformation (100-product dataset) | 44 µs |
| `$sort` / top-rated (100-product dataset) | 18 µs |

### Comparison with jsonata-rs

[jsonata-rs](https://crates.io/crates/jsonata-rs) is the only other pure-Rust JSONata
implementation. Based on its published benchmarks, `jsonata-core` is approximately
**40x faster** across typical workloads:

| Category | jsonata-core | jsonata-rs (est.) |
|----------|-------------|-------------------|
| Simple path lookup | 81 ns | ~3 µs |
| `$sum` (100 elements) | 287 ns | ~20 µs |
| Filter predicate (100 objects) | 7.9 µs | ~385 µs |

The gap comes from `jsonata-core`'s JValue type (O(1) `Rc` clones, no heap allocation
for common operations) and a compile-once expression cache that eliminates repeated
predicate recompilation.

### Clarification: the "jsonatapy (json I/O)" column

The `evaluate_json(json_string)` path (previously mislabeled "rust-only") is **not**
a pure-Rust measurement. Both jsonatapy columns use the Rust evaluator; the
difference is how data enters and exits:

- **`evaluate(dict)`** — PyO3 walks the Python dict tree → JValue → evaluate → JValue → Python object
- **`evaluate_json(str)`** — serde_json parses the JSON string → JValue → evaluate → JValue → serde_json serializes

For small payloads, serde_json parse+serialize overhead can exceed the PyO3 traversal
cost, so `evaluate_json` is sometimes *slower* than `evaluate(dict)` on tiny inputs.
Neither path eliminates the Python boundary; they just cross it differently.

The measurements that eliminate the Python boundary entirely are the Criterion
benchmarks above and the **jsonata-core (pure Rust)** column in the
[performance tables](performance.md), which applies the same criterion methodology
per benchmark row. The gap between that column and the jsonatapy columns is the
Python boundary cost itself — the engine is identical.

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
