# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.6 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.6 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-19.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **3.7x faster** |
| Array Operations | **4.5x faster** |
| Complex Transformations | **6.4x faster** |
| Deep Nesting | **1.7x faster** |
| String Operations | **5.9x faster** |
| Higher-Order Functions | **9.1x faster** |
| Realistic Workload | **6.2x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 5.579 | 6.396 | 17.030 | 1843.228 | 61.996 | **3.1x faster** |
| Deep Path (5 levels) | tiny | 7.860 | 9.476 | 25.340 | 3026.361 | 72.107 | **3.2x faster** |
| Array Index Access | 100 elements | 6.466 | 9.516 | 11.350 | 943.793 | 86.626 | **1.8x faster** |
| Arithmetic Expression | tiny | 3.394 | 5.071 | 22.450 | 2615.748 | 74.922 | **6.6x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 2.318 | 2.829 | 9.500 | 470.055 | 19.236 | **4.1x faster** |
| Array Max (100 elements) | 100 elements | 1.524 | 2.151 | 19.250 | 449.277 | 19.322 | **12.6x faster** |
| Array Count (100 elements) | 100 elements | 2.852 | 4.097 | 23.510 | 532.941 | 37.900 | **8.2x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.828 | 4.313 | 4.860 | 180.359 | 24.390 | **1.7x faster** |
| Array Max (1000 elements) | 1000 elements | 2.240 | 3.499 | 3.820 | 167.805 | 24.345 | **1.7x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.584 | 10.052 | 8.870 | 354.893 | 60.233 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 11.574 | 35.423 | 21.540 | 2729.081 | 208.589 | **1.9x faster** |
| Array Mapping + Sum | 100 objects | 10.892 | 34.294 | 24.770 | 2994.227 | 209.092 | **2.3x faster** |
| Array Filtering (predicate) | 100 objects | 7.478 | 21.726 | 50.830 | 6801.184 | 108.274 | **6.8x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.687 | 5.209 | 27.110 | 2550.795 | 33.842 | **4.8x faster** |
| Object Construction (nested) | tiny | 8.182 | 7.091 | 33.780 | 2919.207 | 36.462 | **4.1x faster** |
| Conditional Expression | tiny | 1.420 | 2.056 | 13.650 | 1367.435 | 26.117 | **9.6x faster** |
| Multiple Nested Functions | tiny | 2.689 | 2.697 | 19.170 | 1787.299 | 27.463 | **7.1x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.834 | 8.924 | 25.840 | 2934.571 | 55.287 | **2.9x faster** |
| Nested Array Access | 4-level nested arrays | 16.812 | 18.583 | 8.020 | 619.820 | 108.448 | 2.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.664 | 5.116 | 22.680 | 2420.045 | 53.450 | **4.9x faster** |
| String Lowercase | tiny | 4.654 | 5.156 | 22.570 | 2420.561 | 53.349 | **4.9x faster** |
| String Length | tiny | 4.165 | 4.758 | 24.300 | 2620.290 | 54.939 | **5.8x faster** |
| String Concatenation | tiny | 4.155 | 3.670 | 25.610 | 1934.971 | 30.016 | **6.2x faster** |
| String Substring | tiny | 3.260 | 3.394 | 19.440 | 1696.209 | 28.290 | **6.0x faster** |
| String Contains | tiny | 2.122 | 2.403 | 15.920 | 1424.941 | 28.278 | **7.5x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.452 | 2.550 | 23.370 | 2708.130 | 7.154 | **9.5x faster** |
| $filter with lambda | 100 elements | 1.984 | 2.037 | 23.450 | 2703.237 | 7.063 | **11.8x faster** |
| $reduce with lambda | 100 elements | 3.917 | 4.031 | 22.900 | 2718.528 | 7.715 | **5.8x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 15.022 | 55.634 | 51.540 | 7483.672 | 317.468 | **3.4x faster** |
| Calculate total value | 100 products | 8.740 | 55.055 | 37.420 | 5175.579 | 318.050 | **4.3x faster** |
| Complex transformation | 100 products | 19.497 | 31.625 | 86.900 | 9355.943 | 137.095 | **4.5x faster** |
| Group by category (aggregate) | 100 products | 13.878 | 29.671 | 87.570 | N/A | 134.514 | **6.3x faster** |
| Top rated products | 100 products | 2.964 | 13.134 | 37.230 | 4551.650 | 67.641 | **12.6x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.764 | 500 |
| Filter by category (data→json) | 6.237 | 500 |
| Complex transformation (data handle) | 33.804 | 500 |
| Complex transformation (data→json) | 29.548 | 500 |
| Aggregate (data handle) | 5.920 | 500 |
| Aggregate (data→json) | 5.885 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**3.7x faster**)
- Array Operations (**4.5x faster**)
- Complex Transformations (**6.4x faster**)
- Deep Nesting (**1.7x faster**)
- String Operations (**5.9x faster**)
- Higher-Order Functions (**9.1x faster**)
- Realistic Workload (**6.2x faster**)

**Comparable to JavaScript:**

- (none this run)

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

- **Date:** 2026-07-19
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
