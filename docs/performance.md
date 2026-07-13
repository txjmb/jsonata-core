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
| Complex Transformations | **4.9x faster** |
| Deep Nesting | **1.4x faster** |
| String Operations | **4.5x faster** |
| Higher-Order Functions | **8.4x faster** |
| Realistic Workload | **5.7x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 6.384 | 7.073 | 14.600 | 1838.379 | 61.814 | **2.3x faster** |
| Deep Path (5 levels) | tiny | 8.750 | 10.322 | 22.880 | 3021.317 | 72.248 | **2.6x faster** |
| Array Index Access | 100 elements | 6.928 | 10.065 | 9.520 | 941.509 | 86.874 | **1.4x faster** |
| Arithmetic Expression | tiny | 4.285 | 5.833 | 19.080 | 2615.682 | 56.603 | **4.5x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.740 | 2.360 | 5.960 | 305.940 | 17.932 | **3.4x faster** |
| Array Max (100 elements) | 100 elements | 1.487 | 2.104 | 5.530 | 299.300 | 17.954 | **3.7x faster** |
| Array Count (100 elements) | 100 elements | 2.775 | 4.020 | 7.530 | 532.231 | 34.980 | **2.7x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.761 | 4.040 | 4.540 | 179.058 | 24.490 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.261 | 3.525 | 3.710 | 166.978 | 24.488 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.551 | 10.104 | 8.820 | 355.566 | 59.935 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 11.632 | 35.524 | 18.450 | 2681.594 | 209.872 | **1.6x faster** |
| Array Mapping + Sum | 100 objects | 10.887 | 34.485 | 21.420 | 2989.225 | 210.399 | **2.0x faster** |
| Array Filtering (predicate) | 100 objects | 7.620 | 21.603 | 45.410 | 6750.763 | 108.913 | **6.0x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 6.101 | 5.579 | 24.760 | 2543.278 | 33.785 | **4.1x faster** |
| Object Construction (nested) | tiny | 8.363 | 6.814 | 30.720 | 2912.100 | 36.642 | **3.7x faster** |
| Conditional Expression | tiny | 1.811 | 2.454 | 11.530 | 1362.430 | 26.335 | **6.4x faster** |
| Multiple Nested Functions | tiny | 3.103 | 3.113 | 17.290 | 1783.805 | 27.725 | **5.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.336 | 9.283 | 23.310 | 2921.408 | 55.658 | **2.5x faster** |
| Nested Array Access | 4-level nested arrays | 17.028 | 18.663 | 6.410 | 613.160 | 108.202 | 2.7x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 5.451 | 5.937 | 20.300 | 2412.908 | 53.083 | **3.7x faster** |
| String Lowercase | tiny | 5.494 | 5.970 | 19.590 | 2410.855 | 53.147 | **3.6x faster** |
| String Length | tiny | 4.943 | 5.593 | 22.080 | 2611.875 | 54.454 | **4.5x faster** |
| String Concatenation | tiny | 4.505 | 4.048 | 23.420 | 1932.914 | 30.190 | **5.2x faster** |
| String Substring | tiny | 3.893 | 3.854 | 17.820 | 1694.911 | 28.283 | **4.6x faster** |
| String Contains | tiny | 2.560 | 2.871 | 14.260 | 1426.273 | 28.108 | **5.6x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.483 | 2.557 | 21.730 | 2694.330 | 7.146 | **8.8x faster** |
| $filter with lambda | 100 elements | 1.989 | 2.062 | 21.550 | 2695.176 | 7.054 | **10.8x faster** |
| $reduce with lambda | 100 elements | 3.803 | 3.945 | 20.780 | 2706.515 | 7.744 | **5.5x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 15.242 | 56.146 | 45.590 | 7466.944 | 317.414 | **3.0x faster** |
| Calculate total value | 100 products | 8.942 | 55.666 | 33.450 | 5157.552 | 318.061 | **3.7x faster** |
| Complex transformation | 100 products | 19.795 | 31.955 | 80.760 | 9324.558 | 137.296 | **4.1x faster** |
| Group by category (aggregate) | 100 products | 13.453 | 29.844 | 79.580 | N/A | 135.065 | **5.9x faster** |
| Top rated products | 100 products | 2.959 | 13.190 | 35.000 | 4534.602 | 67.659 | **11.8x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.748 | 500 |
| Filter by category (data→json) | 6.293 | 500 |
| Complex transformation (data handle) | 33.980 | 500 |
| Complex transformation (data→json) | 29.565 | 500 |
| Aggregate (data handle) | 5.931 | 500 |
| Aggregate (data→json) | 5.888 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**2.7x faster**)
- Array Operations (**2.7x faster**)
- Complex Transformations (**4.9x faster**)
- Deep Nesting (**1.4x faster**)
- String Operations (**4.5x faster**)
- Higher-Order Functions (**8.4x faster**)
- Realistic Workload (**5.7x faster**)

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
