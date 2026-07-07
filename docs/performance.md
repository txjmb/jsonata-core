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

Benchmarks run on 2026-07-07.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **6.1x faster** |
| Array Operations | **3.0x faster** |
| Complex Transformations | **12.4x faster** |
| Deep Nesting | **2.1x faster** |
| String Operations | **13.1x faster** |
| Higher-Order Functions | **16.3x faster** |
| Realistic Workload | **1.6x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 4.657 | 6.980 | 29.720 | 176768.759 | 91.988 | **6.4x faster** |
| Deep Path (5 levels) | tiny | 7.305 | 9.867 | 42.020 | 177526.796 | 112.100 | **5.8x faster** |
| Array Index Access | 100 elements | 8.710 | 25.254 | 22.490 | 87595.760 | 138.992 | **2.6x faster** |
| Arithmetic Expression | tiny | 3.546 | 6.197 | 34.450 | 176309.776 | 85.923 | **9.7x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.976 | 5.146 | 12.920 | 17717.992 | 29.602 | **6.5x faster** |
| Array Max (100 elements) | 100 elements | 1.839 | 5.175 | 12.860 | 17716.862 | 29.440 | **7.0x faster** |
| Array Count (100 elements) | 100 elements | 3.381 | 9.904 | 17.870 | 35357.813 | 58.375 | **5.3x faster** |
| Array Sum (1000 elements) | 1000 elements | 3.327 | 8.965 | 7.590 | 3749.975 | 42.540 | **2.3x faster** |
| Array Max (1000 elements) | 1000 elements | 3.018 | 8.790 | 6.680 | 3730.907 | 43.724 | **2.2x faster** |
| Array Sum (10000 elements) | 10000 elements | 7.930 | 22.522 | 12.170 | 1449.541 | 109.411 | **1.5x faster** |
| Array Mapping (extract field) | 100 objects | 66.757 | 71.630 | 28.390 | 21511.802 | 314.866 | 2.4x slower |
| Array Mapping + Sum | 100 objects | 65.882 | 69.541 | 32.990 | 22027.033 | 317.092 | 2.0x slower |
| Array Filtering (predicate) | 100 objects | 44.768 | 40.164 | 64.960 | 20534.808 | 164.393 | **1.5x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 6.017 | 6.304 | 47.350 | 90811.794 | 51.940 | **7.9x faster** |
| Object Construction (nested) | tiny | 8.520 | 8.573 | 57.380 | 91268.322 | 55.529 | **6.7x faster** |
| Conditional Expression | tiny | 1.488 | 2.502 | 26.240 | 88291.497 | 40.400 | **17.6x faster** |
| Multiple Nested Functions | tiny | 2.234 | 3.312 | 38.440 | 88996.139 | 42.973 | **17.2x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 11.170 | 13.167 | 39.900 | 90636.909 | 86.197 | **3.6x faster** |
| Nested Array Access | 4-level nested arrays | 27.161 | 47.111 | 16.930 | 35317.012 | 179.809 | 1.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.789 | 5.987 | 42.300 | 175940.520 | 80.702 | **11.2x faster** |
| String Lowercase | tiny | 3.774 | 5.943 | 39.650 | 176222.449 | 82.965 | **10.5x faster** |
| String Length | tiny | 3.537 | 5.625 | 43.760 | 177382.926 | 83.207 | **12.4x faster** |
| String Concatenation | tiny | 3.968 | 4.962 | 58.030 | 89778.975 | 45.888 | **14.6x faster** |
| String Substring | tiny | 2.993 | 4.307 | 38.610 | 89538.426 | 44.132 | **12.9x faster** |
| String Contains | tiny | 1.859 | 3.175 | 31.600 | 88600.195 | 43.197 | **17.0x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.296 | 2.876 | 39.380 | 8293.113 | 10.607 | **17.1x faster** |
| $filter with lambda | 100 elements | 2.047 | 2.677 | 38.210 | 8264.912 | 10.067 | **18.7x faster** |
| $reduce with lambda | 100 elements | 2.858 | 3.480 | 37.180 | 8329.256 | 11.129 | **13.0x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 114.397 | 108.915 | 69.180 | 21580.404 | 483.149 | 1.7x slower |
| Calculate total value | 100 products | 103.825 | 108.759 | 52.060 | 17313.594 | 481.700 | 2.0x slower |
| Complex transformation | 100 products | 55.728 | 53.747 | 117.760 | 20456.753 | 210.769 | **2.1x faster** |
| Group by category (aggregate) | 100 products | 51.494 | 52.684 | 123.210 | N/A | 205.521 | **2.4x faster** |
| Top rated products | 100 products | 26.174 | 24.026 | 62.610 | 9981.728 | 103.921 | **2.4x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 15.811 | 500 |
| Filter by category (data→json) | 7.012 | 500 |
| Complex transformation (data handle) | 40.102 | 500 |
| Complex transformation (data→json) | 33.758 | 500 |
| Aggregate (data handle) | 6.884 | 500 |
| Aggregate (data→json) | 6.773 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (6.1x faster)
- Array Operations (3.0x faster)
- Complex Transformations (12.4x faster)
- Deep Nesting (2.1x faster)
- String Operations (13.1x faster)
- Higher-Order Functions (16.3x faster)

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

# Reuse many times (3-16x faster than evaluate(dict))
result = expr.evaluate_with_data(jdata)
```

## Methodology

- **Date:** 2026-07-07
- **Platform:** GitHub Actions (Linux, X64)
- **Python:** 3.11
- **Node.js:** v24.14.0
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
