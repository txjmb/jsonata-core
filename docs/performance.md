# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.1.6 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.1.6 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v24.14.0) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-07.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.4x faster** |
| Array Operations | **2.6x faster** |
| Complex Transformations | **11.1x faster** |
| Deep Nesting | **2.3x faster** |
| String Operations | **11.7x faster** |
| Higher-Order Functions | **11.4x faster** |
| Realistic Workload | **1.7x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 5.863 | 8.611 | 31.490 | 3653.558 | 111.037 | **5.4x faster** |
| Deep Path (5 levels) | tiny | 8.789 | 11.800 | 44.240 | 5794.521 | 124.900 | **5.0x faster** |
| Array Index Access | 100 elements | 10.703 | 30.058 | 21.450 | 1782.947 | 171.808 | **2.0x faster** |
| Arithmetic Expression | tiny | 4.432 | 7.048 | 40.640 | 4923.814 | 99.603 | **9.2x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 2.722 | 6.464 | 15.490 | 588.664 | 35.692 | **5.7x faster** |
| Array Max (100 elements) | 100 elements | 2.415 | 6.146 | 14.640 | 573.499 | 35.912 | **6.1x faster** |
| Array Count (100 elements) | 100 elements | 4.347 | 11.902 | 19.360 | 993.885 | 67.321 | **4.5x faster** |
| Array Sum (1000 elements) | 1000 elements | 4.488 | 11.323 | 7.660 | 347.104 | 52.435 | **1.7x faster** |
| Array Max (1000 elements) | 1000 elements | 3.972 | 10.765 | 6.710 | 322.313 | 51.510 | **1.7x faster** |
| Array Sum (10000 elements) | 10000 elements | 11.003 | 28.389 | 12.990 | 662.799 | 123.151 | **1.2x faster** |
| Array Mapping (extract field) | 100 objects | 66.283 | 70.789 | 29.060 | 4655.782 | 362.445 | 2.3x slower |
| Array Mapping + Sum | 100 objects | 64.260 | 70.721 | 35.600 | 5332.053 | 362.288 | 1.8x slower |
| Array Filtering (predicate) | 100 objects | 46.223 | 40.155 | 79.450 | 12993.246 | 188.738 | **1.7x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 6.686 | 7.203 | 49.640 | 4824.624 | 59.825 | **7.4x faster** |
| Object Construction (nested) | tiny | 9.344 | 9.253 | 60.500 | 5997.621 | 64.125 | **6.5x faster** |
| Conditional Expression | tiny | 1.804 | 2.825 | 28.060 | 2532.945 | 47.606 | **15.6x faster** |
| Multiple Nested Functions | tiny | 2.694 | 3.765 | 40.430 | 3410.445 | 49.822 | **15.0x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 11.717 | 15.436 | 45.670 | 5659.908 | 99.439 | **3.9x faster** |
| Nested Array Access | 4-level nested arrays | 24.020 | 49.557 | 16.670 | 1211.052 | 215.052 | 1.4x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.609 | 6.631 | 45.250 | 4587.840 | 94.983 | **9.8x faster** |
| String Lowercase | tiny | 4.706 | 6.563 | 44.190 | 4553.222 | 94.799 | **9.4x faster** |
| String Length | tiny | 4.397 | 6.381 | 48.230 | 5029.112 | 95.180 | **11.0x faster** |
| String Concatenation | tiny | 4.382 | 5.298 | 56.180 | 3686.211 | 53.355 | **12.8x faster** |
| String Substring | tiny | 3.451 | 4.803 | 39.270 | 3289.984 | 51.075 | **11.4x faster** |
| String Contains | tiny | 2.275 | 4.015 | 36.320 | 2778.005 | 50.087 | **16.0x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 3.417 | 4.378 | 39.380 | 5196.790 | 13.110 | **11.5x faster** |
| $filter with lambda | 100 elements | 2.803 | 3.520 | 40.440 | 5196.723 | 12.154 | **14.4x faster** |
| $reduce with lambda | 100 elements | 4.594 | 5.144 | 37.790 | 5255.713 | 13.649 | **8.2x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 114.011 | 119.613 | 74.810 | 13787.057 | 538.790 | 1.5x slower |
| Calculate total value | 100 products | 103.546 | 107.499 | 55.510 | 9189.353 | 547.523 | 1.9x slower |
| Complex transformation | 100 products | 58.270 | 56.387 | 133.180 | 17784.095 | 252.694 | **2.3x faster** |
| Group by category (aggregate) | 100 products | 53.469 | 55.180 | 139.690 | N/A | 239.745 | **2.6x faster** |
| Top rated products | 100 products | 26.848 | 24.977 | 63.380 | 8881.117 | 118.815 | **2.4x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 17.735 | 500 |
| Filter by category (data→json) | 7.286 | 500 |
| Complex transformation (data handle) | 45.897 | 500 |
| Complex transformation (data→json) | 38.712 | 500 |
| Aggregate (data handle) | 7.900 | 500 |
| Aggregate (data→json) | 7.927 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (5.4x faster)
- Array Operations (2.6x faster)
- Complex Transformations (11.1x faster)
- Deep Nesting (2.3x faster)
- String Operations (11.7x faster)
- Higher-Order Functions (11.4x faster)

**Comparable to JavaScript:**

- Realistic Workload

### Optimizing Array Workloads

For array-heavy workloads, the dominant cost is converting Python dicts to Rust values on every call. Use `JsonataData` to pre-convert data once and reuse across multiple evaluations:

```python
import jsonatapy

data = {...}  # your data
expr = jsonatapy.compile("products[price > 100]")

# Pre-convert once
jdata = jsonatapy.JsonataData(data)

# Reuse many times (3-15x faster than evaluate(dict))
result = expr.evaluate_with_data(jdata)
```

## Methodology

- **Date:** 2026-07-07
- **Platform:** GitHub Actions (Linux, X64)
- **Python:** 3.11.15
- **Node.js:** v24.14.0
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
