# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.4 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.4 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-13.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **2.7x faster** |
| Array Operations | **2.7x faster** |
| Complex Transformations | **4.8x faster** |
| Deep Nesting | **1.4x faster** |
| String Operations | **4.5x faster** |
| Higher-Order Functions | **8.3x faster** |
| Realistic Workload | **5.6x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 6.417 | 7.216 | 14.550 | 1833.677 | 61.511 | **2.3x faster** |
| Deep Path (5 levels) | tiny | 8.712 | 10.301 | 22.590 | 3048.156 | 72.126 | **2.6x faster** |
| Array Index Access | 100 elements | 6.909 | 9.949 | 9.420 | 948.388 | 86.643 | **1.4x faster** |
| Arithmetic Expression | tiny | 4.302 | 5.926 | 19.110 | 2636.091 | 56.729 | **4.4x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.748 | 2.341 | 5.950 | 306.621 | 17.932 | **3.4x faster** |
| Array Max (100 elements) | 100 elements | 1.493 | 2.094 | 5.580 | 300.206 | 17.907 | **3.7x faster** |
| Array Count (100 elements) | 100 elements | 2.788 | 4.004 | 7.360 | 532.966 | 34.977 | **2.6x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.764 | 4.063 | 4.520 | 179.076 | 24.463 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.268 | 3.541 | 3.740 | 166.493 | 24.450 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.569 | 10.070 | 8.810 | 353.859 | 59.966 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 11.723 | 35.586 | 18.290 | 2685.749 | 208.914 | **1.6x faster** |
| Array Mapping + Sum | 100 objects | 11.052 | 34.570 | 21.660 | 2992.943 | 209.623 | **2.0x faster** |
| Array Filtering (predicate) | 100 objects | 7.568 | 21.653 | 45.290 | 6766.535 | 108.938 | **6.0x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 6.301 | 5.785 | 24.340 | 2547.324 | 33.880 | **3.9x faster** |
| Object Construction (nested) | tiny | 8.506 | 6.976 | 30.660 | 2914.864 | 36.655 | **3.6x faster** |
| Conditional Expression | tiny | 1.846 | 2.476 | 11.540 | 1362.177 | 26.250 | **6.3x faster** |
| Multiple Nested Functions | tiny | 3.103 | 3.124 | 17.380 | 1782.651 | 27.651 | **5.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.287 | 9.336 | 23.230 | 2926.390 | 55.400 | **2.5x faster** |
| Nested Array Access | 4-level nested arrays | 17.207 | 18.627 | 6.500 | 613.555 | 108.591 | 2.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 5.488 | 5.989 | 19.880 | 2409.003 | 53.224 | **3.6x faster** |
| String Lowercase | tiny | 5.524 | 6.046 | 19.570 | 2409.619 | 53.190 | **3.5x faster** |
| String Length | tiny | 4.966 | 5.594 | 22.150 | 2608.713 | 54.413 | **4.5x faster** |
| String Concatenation | tiny | 4.541 | 4.124 | 23.500 | 1929.474 | 30.141 | **5.2x faster** |
| String Substring | tiny | 3.808 | 3.877 | 17.480 | 1692.159 | 28.358 | **4.6x faster** |
| String Contains | tiny | 2.572 | 2.813 | 14.090 | 1424.230 | 28.186 | **5.5x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.464 | 2.564 | 21.650 | 2696.672 | 7.192 | **8.8x faster** |
| $filter with lambda | 100 elements | 1.988 | 2.078 | 21.530 | 2696.940 | 7.116 | **10.8x faster** |
| $reduce with lambda | 100 elements | 3.974 | 4.074 | 20.650 | 2709.486 | 7.710 | **5.2x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 15.102 | 56.038 | 45.530 | 7474.232 | 318.242 | **3.0x faster** |
| Calculate total value | 100 products | 8.832 | 55.450 | 33.840 | 5172.220 | 317.715 | **3.8x faster** |
| Complex transformation | 100 products | 19.981 | 31.916 | 79.620 | 9332.391 | 137.000 | **4.0x faster** |
| Group by category (aggregate) | 100 products | 14.037 | 29.793 | 78.810 | N/A | 134.683 | **5.6x faster** |
| Top rated products | 100 products | 2.974 | 13.156 | 34.350 | 4537.937 | 67.534 | **11.6x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.886 | 500 |
| Filter by category (data→json) | 6.284 | 500 |
| Complex transformation (data handle) | 33.857 | 500 |
| Complex transformation (data→json) | 29.345 | 500 |
| Aggregate (data handle) | 5.899 | 500 |
| Aggregate (data→json) | 5.924 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**2.7x faster**)
- Array Operations (**2.7x faster**)
- Complex Transformations (**4.8x faster**)
- Deep Nesting (**1.4x faster**)
- String Operations (**4.5x faster**)
- Higher-Order Functions (**8.3x faster**)
- Realistic Workload (**5.6x faster**)

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

- **Date:** 2026-07-13
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
