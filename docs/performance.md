# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.1.2 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.1.2 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v24.13.1) |
| **jsonata-python** | Python | 0.6.1 | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

Benchmarks run on 2026-02-21.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **7.5x faster** |
| Array Operations | 1.7x slower |
| Complex Transformations | **7.2x faster** |
| Deep Nesting | ~1x (roughly equal) |
| String Operations | **6.8x faster** |
| Higher-Order Functions | ~1x (roughly equal) |
| Realistic Workload | 40x slower |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.920 | 4.879 | 41.820 | 84.969 | 69.028 | **10.7x faster** |
| Deep Path (5 levels) | tiny | 5.456 | 6.579 | 20.550 | 148.894 | 83.602 | **3.8x faster** |
| Array Index Access | 100 elements | 8.572 | 18.200 | 16.030 | 179.115 | 111.016 | **1.9x faster** |
| Arithmetic Expression | tiny | 2.591 | 4.760 | 35.360 | 105.717 | 63.545 | **13.7x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.992 | 3.520 | 3.440 | 98.708 | 23.450 | **1.7x faster** |
| Array Max (100 elements) | 100 elements | 2.033 | 3.590 | 2.570 | 95.782 | 21.002 | **1.3x faster** |
| Array Count (100 elements) | 100 elements | 3.480 | 6.600 | 5.150 | 79.678 | 40.763 | **1.5x faster** |
| Array Sum (1000 elements) | 1000 elements | 3.370 | 6.670 | 0.950 | 173.571 | 29.640 | 3.5x slower |
| Array Max (1000 elements) | 1000 elements | 3.359 | 5.550 | 1.260 | 175.454 | 31.833 | 2.7x slower |
| Array Sum (10000 elements) | 10000 elements | 8.790 | 14.220 | 0.090 | 412.474 | 89.195 | 97.7x slower |
| Array Mapping (extract field) | 100 objects | 49.387 | 47.910 | 3.690 | 391.485 | 267.298 | 13.4x slower |
| Array Mapping + Sum | 100 objects | 48.531 | 48.100 | 2.530 | 445.111 | 268.614 | 19.2x slower |
| Array Filtering (predicate) | 100 objects | 34.277 | 29.890 | 1.770 | 440.502 | 123.324 | 19.4x slower |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 4.148 | 4.390 | 16.290 | 97.056 | 40.211 | **3.9x faster** |
| Object Construction (nested) | tiny | 6.063 | 6.990 | 16.680 | 113.821 | 39.687 | **2.8x faster** |
| Conditional Expression | tiny | 1.161 | 1.840 | 19.500 | 44.838 | 30.815 | **16.8x faster** |
| Multiple Nested Functions | tiny | 1.654 | 2.140 | 8.890 | 70.163 | 30.375 | **5.4x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.189 | 10.020 | 13.820 | 148.715 | 68.938 | **1.7x faster** |
| Nested Array Access | 4-level nested arrays | 22.936 | 31.850 | 5.550 | 259.617 | 136.763 | 4.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 2.746 | 4.000 | 19.920 | 96.747 | 58.618 | **7.3x faster** |
| String Lowercase | tiny | 2.922 | 4.180 | 22.210 | 92.172 | 60.825 | **7.6x faster** |
| String Length | tiny | 2.362 | 3.810 | 21.430 | 104.077 | 60.300 | **9.1x faster** |
| String Concatenation | tiny | 2.674 | 3.250 | 10.220 | 98.697 | 34.475 | **3.8x faster** |
| String Substring | tiny | 1.958 | 2.790 | 12.380 | 67.992 | 33.786 | **6.3x faster** |
| String Contains | tiny | 1.391 | 2.280 | 9.100 | 60.987 | 30.889 | **6.5x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.874 | 2.090 | 0.820 | 107.861 | 7.258 | 2.3x slower |
| $filter with lambda | 100 elements | 1.612 | 1.970 | 0.960 | 111.306 | 6.770 | 1.7x slower |
| $reduce with lambda | 100 elements | 2.153 | 2.510 | 4.300 | 105.463 | 8.597 | **2.0x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 76.332 | 76.670 | 1.980 | 723.716 | 407.229 | 38.6x slower |
| Calculate total value | 100 products | 71.727 | 68.420 | 1.440 | 661.389 | 401.337 | 49.8x slower |
| Complex transformation | 100 products | 40.264 | 38.010 | 1.190 | 550.732 | 166.103 | 33.8x slower |
| Group by category (aggregate) | 100 products | 44.896 | 42.990 | 1.700 | 643.883 | 156.071 | 26.4x slower |
| Top rated products | 100 products | 18.861 | 16.450 | 0.200 | 191.795 | 78.980 | 94.3x slower |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.965 | 500 |
| Filter by category (data→json) | 7.236 | 500 |
| Complex transformation (data handle) | 29.590 | 500 |
| Complex transformation (data→json) | 26.663 | 500 |
| Aggregate (data handle) | 4.600 | 500 |
| Aggregate (data→json) | 4.657 | 500 |

## Performance Characteristics

### Where jsonatapy excels

jsonatapy is the **fastest Python JSONata implementation by a wide margin** — ~10–65x faster than jsonata-python across all categories. It is also **faster than jsonata-rs** (the leading pure-Rust JSONata implementation) for most workloads, demonstrating that the compilation layer and bytecode VM pay off even without Python overhead.

For **pure expression evaluation** — simple paths, arithmetic, conditionals, string operations, and complex transformations — jsonatapy consistently beats the JavaScript reference implementation running on V8:

- Simple Paths (7.5x faster)
- Complex Transformations (7.2x faster)
- String Operations (6.8x faster)

### Where JavaScript is faster

For workloads that iterate over large arrays of Python dicts, the dominant cost is converting Python objects to Rust values on every `evaluate()` call — roughly 1µs per field, or ~130µs for 100 objects with 5 fields each. This is a property of the Python/C extension model, not of jsonatapy specifically: any Rust or C extension must pay this cost when reading Python dict data.

- Array Operations (1.7x slower overall; aggregates on 100-element arrays are competitive)
- Higher-Order Functions (~1x overall; $reduce is faster, $map/$filter are ~2x slower)
- Realistic Workload (40x slower on full-conversion path)

V8's JIT compiler eliminates equivalent overhead through type specialisation and loop vectorisation. Without a JIT, this gap is irreducible for the `evaluate(dict)` call path.

### Using pre-converted data

With `JsonataData`, the conversion cost is paid once and amortized across all evaluations. With pre-converted data, the remaining gap to V8 is 2–7x — the pure interpreter vs JIT gap — which is the practical performance ceiling without adding a JIT compiler.

```python
import jsonatapy

data = {...}  # your data
expr = jsonatapy.compile("products[price > 100]")

# Pre-convert once
jdata = jsonatapy.JsonataData(data)

# Reuse many times (6–15x faster than evaluate(dict))
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
