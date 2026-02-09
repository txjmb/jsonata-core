# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.1.0 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.1.0 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v24.13.0) |
| **jsonata-python** | Python | 0.6.1 | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

Benchmarks run on 2026-02-08.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.7x faster** |
| Array Operations | 1.4x slower |
| Complex Transformations | **6.4x faster** |
| Deep Nesting | 1.1x slower |
| String Operations | **6.3x faster** |
| Higher-Order Functions | 3.4x slower |
| Realistic Workload | 28.1x slower |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.979 | 5.295 | 23.720 | 80.853 | 68.775 | **6.0x faster** |
| Deep Path (5 levels) | tiny | 5.470 | 6.962 | 24.150 | 147.993 | 92.053 | **4.4x faster** |
| Array Index Access | 100 elements | 9.587 | 15.426 | 15.500 | 186.515 | 106.663 | **1.6x faster** |
| Arithmetic Expression | tiny | 3.263 | 4.993 | 35.780 | 100.909 | 67.750 | **11.0x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 2.198 | 3.578 | 2.680 | 102.015 | 20.776 | **1.2x faster** |
| Array Max (100 elements) | 100 elements | 1.962 | 3.155 | 5.690 | 102.278 | 21.723 | **2.9x faster** |
| Array Count (100 elements) | 100 elements | 3.655 | 6.289 | 4.240 | 84.101 | 42.518 | **1.2x faster** |
| Array Sum (1000 elements) | 1000 elements | 3.643 | 5.905 | 1.300 | 178.662 | 31.185 | 2.8x slower |
| Array Max (1000 elements) | 1000 elements | 3.650 | 5.385 | 0.930 | 189.745 | 32.524 | 3.9x slower |
| Array Sum (10000 elements) | 10000 elements | 9.887 | 14.222 | 0.110 | 442.689 | 92.677 | 89.9x slower |
| Array Mapping (extract field) | 100 objects | 51.198 | 55.960 | 7.630 | 405.450 | 244.766 | 6.7x slower |
| Array Mapping + Sum | 100 objects | 53.457 | 51.161 | 3.680 | 468.919 | 249.063 | 14.5x slower |
| Array Filtering (predicate) | 100 objects | 34.179 | 30.816 | 5.010 | 460.752 | 126.282 | 6.8x slower |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.779 | 5.994 | 16.550 | 101.349 | 40.663 | **2.9x faster** |
| Object Construction (nested) | tiny | 8.964 | 8.469 | 15.630 | 114.156 | 43.904 | **1.7x faster** |
| Conditional Expression | tiny | 1.354 | 2.072 | 18.780 | 44.279 | 32.117 | **13.9x faster** |
| Multiple Nested Functions | tiny | 1.719 | 2.467 | 12.180 | 69.471 | 32.049 | **7.1x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.119 | 11.071 | 15.080 | 157.158 | 69.299 | **1.7x faster** |
| Nested Array Access | 4-level nested arrays | 22.598 | 31.174 | 5.630 | 258.326 | 133.304 | 4.0x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.038 | 4.228 | 20.610 | 96.609 | 66.030 | **6.8x faster** |
| String Lowercase | tiny | 3.016 | 4.427 | 20.410 | 94.675 | 64.486 | **6.8x faster** |
| String Length | tiny | 2.905 | 4.266 | 21.580 | 96.430 | 73.537 | **7.4x faster** |
| String Concatenation | tiny | 3.330 | 4.087 | 9.420 | 107.408 | 34.262 | **2.8x faster** |
| String Substring | tiny | 2.229 | 3.331 | 12.520 | 71.734 | 35.090 | **5.6x faster** |
| String Contains | tiny | 1.461 | 2.493 | 12.060 | 61.979 | 35.852 | **8.3x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 3.160 | 3.358 | 0.950 | 109.875 | 7.712 | 3.3x slower |
| $filter with lambda | 100 elements | 3.013 | 3.114 | 1.010 | 113.849 | 7.451 | 3.0x slower |
| $reduce with lambda | 100 elements | 3.696 | 4.326 | 0.920 | 107.816 | 8.616 | 4.0x slower |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 81.563 | 76.437 | 1.780 | 734.315 | 376.793 | 45.8x slower |
| Calculate total value | 100 products | 71.135 | 73.102 | 1.560 | 670.159 | 390.203 | 45.6x slower |
| Complex transformation | 100 products | 43.302 | 41.364 | 1.240 | 563.856 | 161.663 | 34.9x slower |
| Group by category (aggregate) | 100 products | 43.660 | 43.613 | 4.200 | 654.537 | 164.877 | 10.4x slower |
| Top rated products | 100 products | 22.192 | 18.907 | 0.210 | 192.178 | 85.421 | 105.7x slower |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.778 | 500 |
| Filter by category (data→json) | 8.475 | 500 |
| Complex transformation (data handle) | 33.812 | 500 |
| Complex transformation (data→json) | 31.929 | 500 |
| Aggregate (data handle) | 4.825 | 500 |
| Aggregate (data→json) | 4.526 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (5.7x faster)
- Complex Transformations (6.4x faster)
- String Operations (6.3x faster)

**Comparable to JavaScript:**

- Deep Nesting

**Slower than JavaScript:**

- Array Operations (1.4x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)
- Higher-Order Functions (3.4x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)
- Realistic Workload (28.1x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)

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

- **Date:** 2026-02-08
- **Platform:** Linux (WSL2) on x86_64
- **Python:** 3.13
- **Node.js:** v24.13.0
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
