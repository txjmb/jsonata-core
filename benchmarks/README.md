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

## Expected Performance

Based on initial benchmarks, typical performance characteristics:

| Category | jsonatapy vs JS | Notes |
|----------|----------------|-------|
| Simple Paths | 2-8x faster | String operations particularly fast |
| Array Operations (small) | 0.5-1x | Similar performance |
| Array Operations (large) | 0.02-0.1x | JS V8 optimizes array ops heavily |
| Complex Transformations | 2-12x faster | Conditionals and functions excel |
| Deep Nesting | 0.5-2x | Path traversal competitive |
| String Operations | 3-6x faster | Native Rust string handling |
| Higher-Order Functions | 0.04-0.06x | Lambda performance needs optimization |
| Realistic Workload | 0.01-0.1x | Complex queries need work |

**Key Insights:**
- jsonatapy excels at simple operations and string manipulation
- V8 JavaScript engine has highly optimized array operations
- Higher-order functions (lambdas) need performance optimization
- Complex realistic queries show room for improvement

## Performance Analysis

### Areas Where jsonatapy Excels
- Simple path navigation and field access
- String operations (uppercase, lowercase, substring)
- Basic arithmetic and conditionals
- Shallow object transformations

### Areas Needing Optimization
- Large array aggregations (1000+ elements)
- Higher-order functions with lambdas
- Complex filtering and mapping operations
- Nested array/object traversals

### Future Optimizations
- Implement lazy evaluation for large arrays
- Optimize lambda function execution
- Add SIMD operations for numeric arrays
- Improve memory allocation patterns

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
