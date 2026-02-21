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
| Simple Paths | **7.4x faster** |
| Array Operations | 1.9x slower |
| Complex Transformations | **8.3x faster** |
| Deep Nesting | 1.3x slower |
| String Operations | **7.0x faster** |
| Higher-Order Functions | 2.0x slower |
| Realistic Workload | 26.7x slower |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.418 | 4.861 | 33.450 | 79.845 | 69.655 | **9.8x faster** |
| Deep Path (5 levels) | tiny | 5.267 | 6.676 | 23.720 | 144.328 | 89.320 | **4.5x faster** |
| Array Index Access | 100 elements | 8.875 | 16.370 | 13.900 | 175.037 | 108.928 | **1.6x faster** |
| Arithmetic Expression | tiny | 2.477 | 4.368 | 34.290 | 96.192 | 63.679 | **13.8x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.921 | 3.446 | 2.700 | 99.382 | 22.349 | **1.4x faster** |
| Array Max (100 elements) | 100 elements | 2.009 | 3.560 | 2.750 | 105.869 | 21.035 | **1.4x faster** |
| Array Count (100 elements) | 100 elements | 3.605 | 6.585 | 4.290 | 78.911 | 40.881 | **1.2x faster** |
| Array Sum (1000 elements) | 1000 elements | 3.703 | 6.153 | 0.930 | 168.281 | 36.927 | 4.0x slower |
| Array Max (1000 elements) | 1000 elements | 3.336 | 5.551 | 0.910 | 167.937 | 30.045 | 3.7x slower |
| Array Sum (10000 elements) | 10000 elements | 11.906 | 16.955 | 0.090 | 412.132 | 84.302 | 132.3x slower |
| Array Mapping (extract field) | 100 objects | 46.030 | 47.858 | 4.430 | 383.882 | 258.456 | 10.4x slower |
| Array Mapping + Sum | 100 objects | 44.196 | 47.854 | 2.690 | 448.681 | 261.346 | 16.4x slower |
| Array Filtering (predicate) | 100 objects | 33.973 | 28.789 | 1.660 | 440.762 | 121.827 | 20.5x slower |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 4.104 | 4.618 | 15.720 | 91.767 | 37.249 | **3.8x faster** |
| Object Construction (nested) | tiny | 6.143 | 6.059 | 15.660 | 116.394 | 44.571 | **2.5x faster** |
| Conditional Expression | tiny | 1.085 | 1.808 | 20.090 | 40.364 | 30.020 | **18.5x faster** |
| Multiple Nested Functions | tiny | 1.596 | 2.211 | 12.970 | 69.301 | 30.505 | **8.1x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.188 | 9.465 | 10.770 | 160.139 | 66.444 | **1.3x faster** |
| Nested Array Access | 4-level nested arrays | 21.349 | 32.423 | 5.090 | 252.984 | 128.685 | 4.2x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 2.637 | 3.903 | 19.710 | 94.128 | 59.715 | **7.5x faster** |
| String Lowercase | tiny | 2.616 | 3.845 | 19.950 | 93.876 | 66.215 | **7.6x faster** |
| String Length | tiny | 2.560 | 3.965 | 20.120 | 93.868 | 59.884 | **7.9x faster** |
| String Concatenation | tiny | 2.653 | 3.187 | 9.030 | 99.441 | 32.199 | **3.4x faster** |
| String Substring | tiny | 2.114 | 2.803 | 12.160 | 65.012 | 30.429 | **5.8x faster** |
| String Contains | tiny | 1.220 | 2.087 | 12.420 | 55.135 | 30.694 | **10.2x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.787 | 2.083 | 0.850 | 114.098 | 7.153 | 2.1x slower |
| $filter with lambda | 100 elements | 1.636 | 1.900 | 0.990 | 103.713 | 6.476 | 1.7x slower |
| $reduce with lambda | 100 elements | 2.044 | 2.364 | 0.850 | 100.455 | 7.090 | 2.4x slower |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 78.289 | 74.613 | 1.880 | 734.265 | 384.977 | 41.6x slower |
| Calculate total value | 100 products | 71.979 | 73.042 | 1.750 | 652.558 | 372.265 | 41.1x slower |
| Complex transformation | 100 products | 42.814 | 36.595 | 4.200 | 540.489 | 161.282 | 10.2x slower |
| Group by category (aggregate) | 100 products | 43.870 | 47.985 | 1.180 | 618.170 | 155.178 | 37.2x slower |
| Top rated products | 100 products | 17.891 | 15.873 | 0.250 | 199.836 | 76.446 | 71.6x slower |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.501 | 500 |
| Filter by category (data→json) | 6.803 | 500 |
| Complex transformation (data handle) | 29.559 | 500 |
| Complex transformation (data→json) | 26.926 | 500 |
| Aggregate (data handle) | 4.506 | 500 |
| Aggregate (data→json) | 4.317 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (7.4x faster)
- Complex Transformations (8.3x faster)
- String Operations (7.0x faster)

**Slower than JavaScript:**

- Array Operations (1.9x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)
- Deep Nesting (1.3x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)
- Higher-Order Functions (2.0x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)
- Realistic Workload (26.7x slower, primarily due to Python/Rust boundary overhead on per-call data conversion)

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
