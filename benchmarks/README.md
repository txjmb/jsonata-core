# JSONataPy Benchmarks

Performance benchmarks comparing JSONataPy (Rust implementation) against the reference JSONata JavaScript implementation.

## Setup

### Prerequisites

1. **Python environment** with jsonatapy installed:
   ```bash
   cd ..
   maturin develop --release
   ```

2. **Node.js** (for JavaScript benchmarks):
   ```bash
   # Check if Node.js is installed
   node --version

   # Install if needed:
   # - Windows: https://nodejs.org/
   # - Linux: sudo apt install nodejs npm
   # - macOS: brew install node
   ```

3. **Install JavaScript dependencies**:
   ```bash
   cd benchmarks
   npm install
   ```

## Running Benchmarks

### Full Benchmark Suite

Compare both implementations:
```bash
python benchmark.py
```

### Rust Only

If Node.js is not available, the benchmark will automatically run Rust-only mode.

## Benchmark Categories

The suite tests performance across four categories:

### 1. Simple Queries
- Path navigation
- Deep nesting
- Basic arithmetic
- String functions

### 2. Array Operations
- Sum, max, min
- Array indexing
- Array filtering

### 3. Large Data
- 1000+ element arrays
- Aggregation functions
- Nested access patterns

### 4. Complex Queries
- Multiple function composition
- Conditional expressions
- String manipulation chains

## Expected Results

Typical performance characteristics:

- **Simple queries**: Rust is 10-50x faster
- **Array operations**: Rust is 5-20x faster
- **Large data**: Rust is 3-15x faster
- **Complex queries**: Rust is 8-30x faster

The Rust implementation benefits from:
- No JIT warmup time
- Compiled native code
- Efficient memory management
- Zero-cost abstractions

## Customization

Edit `benchmark.py` to add custom benchmarks:

```python
suite.benchmark(
    "My Custom Test",
    "expression here",
    {"data": "here"},
    iterations=1000
)
```

## Output Format

```
====================================================================
Benchmark: Simple Path
Expression: user.name
Iterations: 10,000
====================================================================
Rust:          15.23 ms (  0.0015 ms/iter)
JavaScript:   234.56 ms (  0.0235 ms/iter)

Speedup:       15.40x faster
```

## Troubleshooting

**Node.js not found:**
- Install Node.js from https://nodejs.org/
- Ensure `node` is in your PATH

**jsonata module not found:**
- Run `npm install` in the benchmarks directory

**jsonatapy not installed:**
- Run `maturin develop --release` from the project root
