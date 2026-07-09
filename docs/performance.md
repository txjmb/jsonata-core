# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.2 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.2 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-08.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **2.7x faster** |
| Array Operations | **1.9x faster** |
| Complex Transformations | **5.5x faster** |
| Deep Nesting | **1.4x faster** |
| String Operations | **5.3x faster** |
| Higher-Order Functions | **9.2x faster** |
| Realistic Workload | **1.5x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 6.326 | 6.588 | 14.740 | 1842.276 | 61.632 | **2.3x faster** |
| Deep Path (5 levels) | tiny | 9.409 | 9.425 | 22.610 | 3025.274 | 72.117 | **2.4x faster** |
| Array Index Access | 100 elements | 6.427 | 9.300 | 9.440 | 942.855 | 86.428 | **1.5x faster** |
| Arithmetic Expression | tiny | 4.241 | 5.153 | 19.100 | 2622.020 | 56.809 | **4.5x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.685 | 2.226 | 5.920 | 305.974 | 17.968 | **3.5x faster** |
| Array Max (100 elements) | 100 elements | 1.432 | 1.980 | 5.550 | 299.518 | 17.966 | **3.9x faster** |
| Array Count (100 elements) | 100 elements | 2.626 | 3.764 | 7.510 | 532.242 | 35.079 | **2.9x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.787 | 3.878 | 4.550 | 178.736 | 24.349 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.306 | 3.374 | 3.700 | 166.936 | 24.341 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.617 | 9.635 | 8.770 | 354.684 | 59.872 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 42.549 | 35.726 | 18.250 | 2678.886 | 210.859 | 2.3x slower |
| Array Mapping + Sum | 100 objects | 41.801 | 34.591 | 21.560 | 2987.670 | 211.051 | 1.9x slower |
| Array Filtering (predicate) | 100 objects | 30.075 | 21.448 | 44.770 | 6759.223 | 109.573 | **1.5x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.630 | 5.303 | 24.760 | 2545.947 | 33.754 | **4.4x faster** |
| Object Construction (nested) | tiny | 7.734 | 6.607 | 30.580 | 2913.775 | 36.875 | **4.0x faster** |
| Conditional Expression | tiny | 1.762 | 2.073 | 11.610 | 1363.979 | 26.235 | **6.6x faster** |
| Multiple Nested Functions | tiny | 2.442 | 2.708 | 17.410 | 1788.894 | 27.879 | **7.1x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.562 | 8.914 | 23.150 | 2929.821 | 55.372 | **2.4x faster** |
| Nested Array Access | 4-level nested arrays | 16.914 | 19.027 | 6.390 | 612.692 | 108.031 | 2.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.863 | 5.173 | 20.150 | 2413.651 | 53.489 | **4.1x faster** |
| String Lowercase | tiny | 4.807 | 5.216 | 19.920 | 2416.669 | 53.246 | **4.1x faster** |
| String Length | tiny | 4.211 | 4.838 | 21.950 | 2617.585 | 55.043 | **5.2x faster** |
| String Concatenation | tiny | 3.849 | 3.759 | 24.010 | 1932.848 | 30.280 | **6.2x faster** |
| String Substring | tiny | 3.259 | 3.520 | 17.910 | 1694.051 | 28.720 | **5.5x faster** |
| String Contains | tiny | 2.165 | 2.482 | 14.630 | 1427.701 | 28.338 | **6.8x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.973 | 2.072 | 21.450 | 2693.451 | 7.102 | **10.9x faster** |
| $filter with lambda | 100 elements | 1.927 | 2.011 | 21.370 | 2698.443 | 7.043 | **11.1x faster** |
| $reduce with lambda | 100 elements | 3.659 | 3.745 | 20.510 | 2707.532 | 7.699 | **5.6x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 75.515 | 55.814 | 45.200 | 7477.472 | 320.311 | 1.7x slower |
| Calculate total value | 100 products | 69.418 | 56.145 | 33.590 | 5175.303 | 319.921 | 2.1x slower |
| Complex transformation | 100 products | 38.575 | 32.048 | 78.590 | 9345.187 | 137.732 | **2.0x faster** |
| Group by category (aggregate) | 100 products | 35.215 | 29.877 | 79.550 | N/A | 135.798 | **2.3x faster** |
| Top rated products | 100 products | 17.500 | 13.144 | 34.710 | 4539.881 | 68.134 | **2.0x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.909 | 500 |
| Filter by category (data→json) | 6.166 | 500 |
| Complex transformation (data handle) | 33.787 | 500 |
| Complex transformation (data→json) | 28.536 | 500 |
| Aggregate (data handle) | 6.279 | 500 |
| Aggregate (data→json) | 6.299 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**2.7x faster**)
- Array Operations (**1.9x faster**)
- Complex Transformations (**5.5x faster**)
- Deep Nesting (**1.4x faster**)
- String Operations (**5.3x faster**)
- Higher-Order Functions (**9.2x faster**)
- Realistic Workload (**1.5x faster**)

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

- **Date:** 2026-07-08
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
