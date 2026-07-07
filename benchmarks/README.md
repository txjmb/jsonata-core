# JSONata Comprehensive Benchmark Suite

Performance benchmarks comparing multiple JSONata implementations:
1. **jsonatapy** (this project - Rust/PyO3) ✅
2. **jsonata** (JavaScript reference - via Node.js) ✅
3. **jsonata-python** (rayokota wrapper - optional) ✅
4. **jsonata-rs** (Stedi pure Rust - optional) ✅

All four implementations are now fully integrated!

## Quick Start

```bash
# 1. Build jsonatapy (from project root)
maturin develop --release

# 2. Install JavaScript dependencies
cd benchmarks/javascript
npm install
cd ../..

# 3. (Optional) Install jsonata-python for comparison
uv pip install jsonata

# 4. (Optional) Build jsonata-rs for Rust-only comparison
cd benchmarks/rust
cargo build --release
cd ../..

# 5. (Optional) Install visualization tools
uv pip install rich matplotlib

# 6. Run benchmarks
uv run python benchmarks/python/benchmark.py

# 7. Generate enhanced report with charts
uv run python benchmarks/python/enhanced_report.py
```

## Tools Included

1. **python/benchmark.py** - Full benchmark suite with 30+ tests across 8 categories
2. **python/enhanced_report.py** - Generate rich tables and performance charts (NEW!)
3. **python/analyze_results.py** - Analyze and compare benchmark results
4. **python/quick_benchmark.py** - Quick ad-hoc performance testing
5. **rust/jsonata_rs_bench.rs** - Rust binary for pure Rust benchmarking (NEW!)

## Features

- **Comprehensive Test Coverage**: 30+ benchmarks across 8 categories
- **Multiple Implementations**: Compares 4 different implementations (jsonatapy, JS, jsonata-python, jsonata-rs)
- **Memory Profiling**: Tracks peak memory usage for all implementations (NEW!)
- **Rich Output**: Color-coded tables with rich library (NEW!)
- **Enhanced Reporting**: Generate beautiful charts and visualizations (NEW!)
- **Automatic Fallback**: Gracefully handles missing implementations
- **Historical Tracking**: JSON output for tracking performance over time
- **Visual Reports**: Category comparisons and speedup charts
- **Interactive Tools**: Quick benchmark and results analysis utilities

## Benchmark Categories

### 1. Simple Paths (Warm-up)
- Simple path navigation (`user.name`)
- Deep path access (5-12 levels)
- Array index access
- Basic arithmetic expressions

### 2. Array Operations
- Aggregation functions (`$sum`, `$max`, `$count`)
- Array sizes: 100, 1,000, 10,000 elements
- Array mapping and filtering
- Field extraction from object arrays

### 3. Complex Transformations
- Object construction (simple and nested)
- Conditional expressions (ternary operators)
- Multiple nested function calls
- Composite string operations

### 4. Deep Nesting (10+ Levels)
- Deeply nested object access (12 levels)
- Nested array access (4-level arrays)
- Performance under deep structure traversal

### 5. String Operations
- Case conversion (`$uppercase`, `$lowercase`)
- String manipulation (`$length`, `$substring`, `$contains`)
- String concatenation and joining
- Complex string transformations

### 6. Higher-Order Functions
- `$map` with lambda functions
- `$filter` with predicates
- `$reduce` with accumulators
- Function composition

### 7. Realistic Workload (E-Commerce)
- Product catalog filtering
- Price aggregation with conditions
- Complex object transformations
- Category-based grouping
- Sorting with custom comparators

## Output

### Console Output

The benchmark suite provides detailed console output with:
- Individual test timings (total and per-iteration)
- Speedup calculations vs JavaScript reference
- Category-wise grouping
- Overall statistics and averages
- Rich formatted tables (if `rich` library is available)

Example output:
```
======================================================================
Benchmark: Array Sum (100 elements)
Category: Array Operations
Expression: $sum(values)
Data Size: 100 elements
Iterations: 1,000
======================================================================
jsonatapy:       4.77 ms (  0.0048 ms/iter)
JavaScript:      2.87 ms (  0.0029 ms/iter)
  → jsonatapy is 0.60x slower than JS
```

### JSON Results

Results are automatically saved to `benchmarks/results/benchmark_results_YYYYMMDD_HHMMSS.json`:

```json
{
  "timestamp": "2026-02-05T07:56:51.061787",
  "implementations": {
    "jsonatapy": true,
    "javascript": true,
    "jsonata_python": false
  },
  "results": [
    {
      "name": "Simple Path",
      "category": "Simple Paths",
      "expression": "user.name",
      "data_size": "tiny",
      "iterations": 10000,
      "jsonatapy_ms": 6.01,
      "js_ms": 35.21,
      "jsonatapy_speedup": 5.86
    }
  ]
}
```

### Graphs and Charts

If `matplotlib` is installed, the suite generates:

1. **Speedup Comparison** (`speedup_comparison.png`)
   - Horizontal bar chart showing speedup vs JavaScript for each test
   - Green bars = faster, Red bars = slower

2. **Category Comparison** (`category_comparison.png`)
   - Side-by-side bar charts for each category
   - Compares absolute timings across implementations

3. **Statistics** (`statistics.png`)
   - Pie chart: Percentage of tests where jsonatapy is faster
   - Bar chart: Distribution of speedup ranges

## Installation Options

### Core Dependencies (Required)

```bash
# Build jsonatapy
maturin develop --release

# Install Node.js (for JavaScript benchmarks)
# - Windows: https://nodejs.org/
# - Linux: sudo apt install nodejs npm
# - macOS: brew install node

# Install JavaScript dependencies
cd benchmarks/javascript
npm install
```

### Optional Dependencies

```bash
# For rich formatted output
pip install rich

# For graphs and charts
pip install matplotlib

# For comparison with jsonata-python wrapper
pip install jsonata
```

## Running Benchmarks

### Full Benchmark Suite

Run all 30+ benchmarks across all categories:

```bash
python benchmarks/python/benchmark.py
```

### Quick Performance Test

Test a single expression interactively:

```bash
python benchmarks/python/quick_benchmark.py "expression" '{"data": "json"}' [iterations]

# Examples:
python benchmarks/python/quick_benchmark.py "user.name" '{"user": {"name": "Alice"}}' 1000
python benchmarks/python/quick_benchmark.py '$sum(values)' '{"values": [1, 2, 3, 4, 5]}' 500
python benchmarks/python/quick_benchmark.py '$uppercase(text)' '{"text": "hello"}' 2000
```

### Analyze Results

Analyze the most recent benchmark results:

```bash
python benchmarks/python/analyze_results.py
```

Analyze a specific results file:

```bash
python benchmarks/python/analyze_results.py benchmarks/results/benchmark_results_20260205_075832.json
```

Compare two benchmark runs:

```bash
python benchmarks/python/analyze_results.py benchmarks/results/file1.json benchmarks/results/file2.json
```

### With Virtual Environment

```bash
source .venv/bin/activate  # Linux/Mac
# OR
.venv\Scripts\activate     # Windows

python benchmarks/python/benchmark.py
```

### Customize Benchmarks

Edit `benchmarks/python/benchmark.py` to add your own tests:

```python
suite.benchmark(
    name="My Custom Test",
    category="Custom Category",
    expression="$.products[price > 100]",
    data={"products": [...]},
    data_size="custom",
    iterations=1000
)
```

## Performance

Current results (2026-07-07, full benchmark run against jsonata-js 2.1.0 on Node.js v24.14,
recorded by the release CI job on a clean GitHub Actions runner):

> **Note:** results before 2026-07-07 came from either an unawaited JS harness or a stale
> release git tag. `benchmarks/javascript/benchmark.js` previously called
> `compiled.evaluate(data)` without `await`, and jsonata-js's `evaluate()` is `async` with a
> genuine `await` per recursion step (including once per array element) — without awaiting, the
> timed loop only ran each call up to its first internal suspension point, making JS look
> implausibly fast on anything that iterates arrays. Separately, the `v2.1.6` release tag was
> initially left pointing at a commit that predated this fix, so the first release-time
> benchmark run re-measured the same stale numbers. Both are fixed; the table below is from a
> run against the corrected tag.

| Category | jsonatapy vs JS | Notes |
|----------|----------------|-------|
| Simple Paths | **2.6–9.7x faster** | Array index access lowest (2.6x); arithmetic highest (9.7x) |
| Array Operations (100–1,000 elements) | **2.2–7.0x faster** | $sum/$max/$count/filter all faster |
| Array Operations (10,000 elements) | **1.5x faster** | |
| Array Mapping (100 objects) | ~2–2.4x slower | Python↔Rust dict conversion cost per field |
| Complex Transformations | **6.7–17.6x faster** | Conditional 17.6x, object construction 6.7–7.9x |
| Deep Nesting | 3.6x faster – 1.6x slower | Deep field path faster; nested array access slower |
| String Operations | **10.5–17.0x faster** | Native Rust string handling |
| Higher-Order Functions | **13.0–18.7x faster** | $map, $filter, $reduce all faster than V8 |
| Realistic Workload (dict input) | Mixed: 2.0x slower – 2.4x faster | Filter/aggregate slightly slower; transform/group-by/sort faster |
| Realistic Workload (JsonataData) | **4.4–9.9x faster** | Pre-converted data beats V8 across the board |

**Key Insights:**
- jsonatapy is the fastest Python JSONata implementation by a wide margin
- For pure expression evaluation (paths, arithmetic, conditionals, strings, HOFs), jsonatapy consistently beats V8
- Per-element object-field mapping over Python dicts is the main category where V8 still wins outright
- Use `jsonatapy.JsonataData` to pre-convert data once and amortize Python↔Rust conversion cost across repeated queries — this turns the mixed raw-dict realistic-workload results into a clear win

## Performance Analysis

### Where jsonatapy excels
- Simple path navigation and field access (2.6–9.7x faster than JS)
- String operations — uppercase, lowercase, substring, contains (10.5–17.0x faster)
- Arithmetic and conditionals (up to 17.6x faster)
- Complex object transformations (6.7–17.6x faster)
- Higher-order functions — $map/$filter/$reduce (13.0–18.7x faster)
- Realistic workloads with pre-converted data (4.4–9.9x faster)

### Where JavaScript is faster
- Per-element object-field mapping over Python dict arrays (~2–2.4x) — Python↔Rust conversion cost dominates
- Two of five raw-dict realistic-workload operations (filter, aggregate) are ~1.7–2.0x slower — use `JsonataData` to pull ahead

### The Python boundary
For large array workloads, the dominant remaining cost is converting Python objects to Rust
values on each `evaluate()` call. Two API paths avoid this:

```python
# Pre-convert once, reuse many times (3–16x faster than evaluate(dict))
data = jsonatapy.JsonataData(large_dataset)
result = expr.evaluate_with_data(data)

# Or pass raw JSON string directly
result = expr.evaluate_json(raw_json_string)
```

See [docs/performance.md](../docs/performance.md) for the full table of results.

## Troubleshooting

### jsonatapy not available
```bash
# From project root
maturin develop --release

# Verify installation
python -c "import jsonatapy; print('Success')"
```

### Node.js not found
```bash
# Check installation
node --version

# Install if needed
# - Windows: Download from https://nodejs.org/
# - Linux: sudo apt install nodejs npm
# - macOS: brew install node
```

### JavaScript benchmark fails
```bash
# Install dependencies
cd benchmarks
npm install

# Verify jsonata is installed
node -e "require('jsonata'); console.log('OK')"
```

### Graphs not generated
```bash
# Install matplotlib
pip install matplotlib

# Verify installation
python -c "import matplotlib; print('OK')"
```

### Permission errors on WSL
```bash
# If you see permission errors on WSL
chmod +x benchmarks/python/benchmark.py
chmod +x benchmarks/javascript/benchmark.js
```

## Continuous Performance Tracking

### Track Performance Over Time

```bash
# Run benchmarks regularly and save results
python benchmarks/python/benchmark.py

# Results are saved with timestamps in benchmarks/results/
ls benchmarks/results/
```

### Compare Historical Results

```python
import json
import glob

# Load all benchmark results
results = []
for file in sorted(glob.glob("benchmarks/results/*.json")):
    with open(file) as f:
        results.append(json.load(f))

# Compare average speedups over time
for r in results:
    timestamp = r["timestamp"]
    avg_speedup = sum(
        res["jsonatapy_speedup"]
        for res in r["results"]
        if res["jsonatapy_speedup"]
    ) / len([res for res in r["results"] if res["jsonatapy_speedup"]])
    print(f"{timestamp}: {avg_speedup:.2f}x average speedup")
```

## Contributing

To add new benchmark categories:

1. Create test data in the `main()` function
2. Add `suite.benchmark()` calls with descriptive names
3. Group related tests by category
4. Use appropriate iteration counts (more for fast operations)
5. Document expected performance characteristics

## References

- JSONata Documentation: https://docs.jsonata.org/
- JavaScript Reference: https://github.com/jsonata-js/jsonata
- jsonata-python: https://github.com/rayokota/jsonata-python
- jsonata-rs: https://github.com/Stedi/jsonata-rs
