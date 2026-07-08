# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.1 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.1 | Same library, JSON string I/O path (bypasses Python object conversion) |
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
| Simple Paths | **2.8x faster** |
| Array Operations | **1.9x faster** |
| Complex Transformations | **5.6x faster** |
| Deep Nesting | **1.4x faster** |
| String Operations | **5.6x faster** |
| Higher-Order Functions | **9.2x faster** |
| Realistic Workload | **1.5x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 6.186 | 7.253 | 14.810 | 1852.046 | 61.544 | **2.4x faster** |
| Deep Path (5 levels) | tiny | 8.660 | 10.310 | 22.790 | 3045.207 | 72.703 | **2.6x faster** |
| Array Index Access | 100 elements | 6.604 | 11.288 | 9.360 | 945.105 | 86.451 | **1.4x faster** |
| Arithmetic Expression | tiny | 4.112 | 5.947 | 19.140 | 2628.575 | 57.017 | **4.7x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.693 | 2.603 | 5.940 | 306.058 | 17.942 | **3.5x faster** |
| Array Max (100 elements) | 100 elements | 1.423 | 2.358 | 5.560 | 299.607 | 17.984 | **3.9x faster** |
| Array Count (100 elements) | 100 elements | 2.645 | 4.527 | 7.380 | 532.832 | 34.960 | **2.8x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.837 | 4.308 | 4.540 | 179.406 | 24.328 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.341 | 3.805 | 3.730 | 165.973 | 24.332 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.826 | 10.049 | 8.860 | 355.343 | 59.832 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 42.405 | 37.819 | 18.360 | 2675.942 | 211.145 | 2.3x slower |
| Array Mapping + Sum | 100 objects | 41.688 | 36.775 | 21.560 | 2984.003 | 211.905 | 1.9x slower |
| Array Filtering (predicate) | 100 objects | 30.254 | 22.604 | 44.660 | 6766.286 | 109.784 | **1.5x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.643 | 5.634 | 24.370 | 2545.354 | 33.840 | **4.3x faster** |
| Object Construction (nested) | tiny | 7.567 | 7.337 | 30.400 | 2912.646 | 37.132 | **4.0x faster** |
| Conditional Expression | tiny | 1.684 | 2.520 | 11.580 | 1359.814 | 26.229 | **6.9x faster** |
| Multiple Nested Functions | tiny | 2.425 | 3.274 | 17.420 | 1783.356 | 28.188 | **7.2x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.236 | 9.766 | 23.140 | 2930.772 | 55.557 | **2.5x faster** |
| Nested Array Access | 4-level nested arrays | 17.187 | 20.302 | 6.530 | 612.403 | 107.991 | 2.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.579 | 5.892 | 20.040 | 2404.077 | 53.278 | **4.4x faster** |
| String Lowercase | tiny | 4.521 | 5.898 | 19.940 | 2407.688 | 53.586 | **4.4x faster** |
| String Length | tiny | 3.942 | 5.540 | 21.900 | 2605.586 | 55.049 | **5.6x faster** |
| String Concatenation | tiny | 3.617 | 4.175 | 23.640 | 1932.035 | 30.299 | **6.5x faster** |
| String Substring | tiny | 3.142 | 4.235 | 17.730 | 1689.710 | 28.693 | **5.6x faster** |
| String Contains | tiny | 2.065 | 3.107 | 14.250 | 1422.688 | 28.328 | **6.9x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.981 | 2.137 | 21.650 | 2692.803 | 7.122 | **10.9x faster** |
| $filter with lambda | 100 elements | 1.966 | 2.111 | 21.680 | 2696.689 | 7.040 | **11.0x faster** |
| $reduce with lambda | 100 elements | 3.658 | 3.839 | 20.810 | 2707.666 | 7.717 | **5.7x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 75.777 | 58.029 | 45.360 | 7476.573 | 320.368 | 1.7x slower |
| Calculate total value | 100 products | 69.548 | 57.974 | 33.660 | 5178.122 | 320.302 | 2.1x slower |
| Complex transformation | 100 products | 38.716 | 32.960 | 78.410 | 9347.952 | 137.858 | **2.0x faster** |
| Group by category (aggregate) | 100 products | 35.251 | 30.728 | 79.470 | N/A | 135.739 | **2.3x faster** |
| Top rated products | 100 products | 17.581 | 13.588 | 34.550 | 4548.381 | 68.220 | **2.0x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.441 | 500 |
| Filter by category (data→json) | 5.828 | 500 |
| Complex transformation (data handle) | 34.588 | 500 |
| Complex transformation (data→json) | 28.689 | 500 |
| Aggregate (data handle) | 6.285 | 500 |
| Aggregate (data→json) | 6.354 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**2.8x faster**)
- Array Operations (**1.9x faster**)
- Complex Transformations (**5.6x faster**)
- Deep Nesting (**1.4x faster**)
- String Operations (**5.6x faster**)
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
