# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.1.0 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.1.0 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v24.13.1) |
| **jsonata-python** | Python | 0.6.1 | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

Benchmarks run on 2026-02-21.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **7.8x faster** |
| Array Operations | 1.4x slower |
| Complex Transformations | **7.4x faster** |
| Deep Nesting | **1.1x faster** |
| String Operations | **6.6x faster** |
| Higher-Order Functions | 1.0x slower |
| Realistic Workload | 45.4x slower |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 9.021 | 8.564 | 44.200 | 78.539 | 70.449 | **4.9x faster** |
| Deep Path (5 levels) | tiny | 5.266 | 6.717 | 34.200 | 137.189 | 83.225 | **6.5x faster** |
| Array Index Access | 100 elements | 9.219 | 18.315 | 14.410 | 174.187 | 120.285 | **1.6x faster** |
| Arithmetic Expression | tiny | 2.493 | 4.401 | 45.870 | 107.788 | 69.231 | **18.4x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 2.074 | 3.765 | 3.830 | 109.599 | 22.420 | **1.8x faster** |
| Array Max (100 elements) | 100 elements | 1.952 | 4.093 | 3.100 | 110.582 | 23.031 | **1.6x faster** |
| Array Count (100 elements) | 100 elements | 3.602 | 7.334 | 7.090 | 86.943 | 47.060 | **2.0x faster** |
| Array Sum (1000 elements) | 1000 elements | 3.521 | 6.148 | 1.130 | 186.281 | 31.486 | 3.1x slower |
| Array Max (1000 elements) | 1000 elements | 3.399 | 6.150 | 1.070 | 180.448 | 31.599 | 3.2x slower |
| Array Sum (10000 elements) | 10000 elements | 8.860 | 15.243 | 0.100 | 443.108 | 93.549 | 88.6x slower |
| Array Mapping (extract field) | 100 objects | 51.925 | 49.433 | 4.340 | 383.866 | 255.278 | 12.0x slower |
| Array Mapping + Sum | 100 objects | 46.768 | 47.093 | 2.760 | 436.320 | 257.956 | 16.9x slower |
| Array Filtering (predicate) | 100 objects | 33.842 | 31.634 | 1.880 | 437.531 | 127.079 | 18.0x slower |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 4.070 | 4.349 | 17.100 | 94.880 | 38.762 | **4.2x faster** |
| Object Construction (nested) | tiny | 6.222 | 7.007 | 15.820 | 110.687 | 41.217 | **2.5x faster** |
| Conditional Expression | tiny | 1.182 | 1.967 | 17.850 | 44.218 | 29.957 | **15.1x faster** |
| Multiple Nested Functions | tiny | 1.629 | 2.190 | 12.340 | 70.934 | 34.202 | **7.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.175 | 10.220 | 14.730 | 161.699 | 69.405 | **1.8x faster** |
| Nested Array Access | 4-level nested arrays | 22.124 | 33.092 | 10.310 | 254.956 | 143.335 | 2.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 2.818 | 3.748 | 19.880 | 96.378 | 58.804 | **7.1x faster** |
| String Lowercase | tiny | 2.754 | 3.840 | 17.500 | 95.594 | 63.988 | **6.4x faster** |
| String Length | tiny | 2.471 | 3.811 | 20.270 | 95.153 | 63.183 | **8.2x faster** |
| String Concatenation | tiny | 2.727 | 3.291 | 12.780 | 102.903 | 34.760 | **4.7x faster** |
| String Substring | tiny | 2.062 | 2.866 | 11.930 | 67.350 | 31.911 | **5.8x faster** |
| String Contains | tiny | 1.208 | 2.046 | 9.230 | 53.357 | 32.195 | **7.6x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.123 | 2.010 | 0.950 | 103.854 | 7.249 | 2.2x slower |
| $filter with lambda | 100 elements | 1.599 | 2.060 | 0.960 | 109.189 | 7.051 | 1.7x slower |
| $reduce with lambda | 100 elements | 2.263 | 2.454 | 4.310 | 96.655 | 8.181 | **1.9x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 77.889 | 72.628 | 1.780 | 728.531 | 385.372 | 43.8x slower |
| Calculate total value | 100 products | 69.572 | 74.099 | 1.680 | 655.096 | 365.980 | 41.4x slower |
| Complex transformation | 100 products | 37.658 | 37.565 | 1.040 | 554.077 | 161.372 | 36.2x slower |
| Group by category (aggregate) | 100 products | 43.074 | 42.752 | 1.060 | 648.065 | 154.126 | 40.6x slower |
| Top rated products | 100 products | 19.336 | 16.239 | 0.210 | 193.656 | 78.264 | 92.1x slower |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.849 | 500 |
| Filter by category (data→json) | 6.892 | 500 |
| Complex transformation (data handle) | 28.975 | 500 |
| Complex transformation (data→json) | 23.497 | 500 |
| Aggregate (data handle) | 5.404 | 500 |
| Aggregate (data→json) | 5.249 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (7.8x faster)
- Complex Transformations (7.4x faster)
- String Operations (6.6x faster)

**Comparable to JavaScript:**

- Deep Nesting
- Higher-Order Functions

**Slower than JavaScript:**

- Array Operations (1.4x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)
- Realistic Workload (45.4x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)

### Optimizing Array Workloads

For array-heavy workloads, the dominant cost is converting Python dicts to Rust values on every call. Use `JsonataData` to pre-convert data once and reuse across multiple evaluations:

```python
import jsonatapy

data = {...}  # your data
expr = jsonatapy.compile("products[price > 100]")

# Pre-convert once
jdata = jsonatapy.JsonataData(data)

# Reuse many times (6-15x faster than evaluate(dict))
result = expr.evaluate_with_data(jdata)
```

## Methodology

- **Date:** 2026-02-21
- **Platform:** Linux (WSL2) on x86_64
- **Python:** 3.13
- **Node.js:** v24.13.1
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
