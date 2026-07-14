# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.5 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.5 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-14.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **3.2x faster** |
| Array Operations | **2.8x faster** |
| Complex Transformations | **5.6x faster** |
| Deep Nesting | **1.5x faster** |
| String Operations | **5.3x faster** |
| Higher-Order Functions | **8.4x faster** |
| Realistic Workload | **5.7x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 5.590 | 6.509 | 14.760 | 1837.258 | 61.756 | **2.6x faster** |
| Deep Path (5 levels) | tiny | 7.935 | 9.436 | 22.680 | 3022.341 | 72.110 | **2.9x faster** |
| Array Index Access | 100 elements | 6.392 | 9.598 | 9.450 | 943.776 | 86.465 | **1.5x faster** |
| Arithmetic Expression | tiny | 3.411 | 5.150 | 19.220 | 2616.343 | 56.717 | **5.6x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.657 | 2.285 | 5.950 | 305.482 | 17.993 | **3.6x faster** |
| Array Max (100 elements) | 100 elements | 1.404 | 2.026 | 5.560 | 298.797 | 17.954 | **4.0x faster** |
| Array Count (100 elements) | 100 elements | 2.618 | 3.854 | 7.900 | 531.448 | 34.924 | **3.0x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.804 | 4.049 | 4.530 | 180.282 | 24.423 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.312 | 3.538 | 3.730 | 167.525 | 24.402 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.657 | 10.086 | 8.800 | 357.430 | 60.001 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 11.479 | 35.592 | 18.450 | 2675.130 | 209.443 | **1.6x faster** |
| Array Mapping + Sum | 100 objects | 10.822 | 34.514 | 21.580 | 2979.790 | 209.446 | **2.0x faster** |
| Array Filtering (predicate) | 100 objects | 7.414 | 21.566 | 45.080 | 6746.894 | 108.692 | **6.1x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.853 | 5.257 | 24.920 | 2538.859 | 33.775 | **4.3x faster** |
| Object Construction (nested) | tiny | 8.242 | 6.519 | 30.620 | 2904.174 | 36.382 | **3.7x faster** |
| Conditional Expression | tiny | 1.420 | 2.139 | 11.520 | 1361.360 | 26.156 | **8.1x faster** |
| Multiple Nested Functions | tiny | 2.665 | 2.740 | 17.340 | 1783.393 | 27.730 | **6.5x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.899 | 8.944 | 23.080 | 2923.017 | 55.326 | **2.6x faster** |
| Nested Array Access | 4-level nested arrays | 16.793 | 18.535 | 6.450 | 613.302 | 108.248 | 2.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.683 | 5.209 | 19.960 | 2408.240 | 53.364 | **4.3x faster** |
| String Lowercase | tiny | 4.644 | 5.205 | 20.190 | 2406.503 | 53.093 | **4.3x faster** |
| String Length | tiny | 4.166 | 4.879 | 21.950 | 2604.154 | 54.398 | **5.3x faster** |
| String Concatenation | tiny | 4.194 | 3.717 | 23.760 | 1925.403 | 30.190 | **5.7x faster** |
| String Substring | tiny | 3.283 | 3.615 | 17.700 | 1690.230 | 28.360 | **5.4x faster** |
| String Contains | tiny | 2.140 | 2.475 | 14.200 | 1422.077 | 28.042 | **6.6x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.455 | 2.550 | 21.310 | 2688.354 | 7.181 | **8.7x faster** |
| $filter with lambda | 100 elements | 1.954 | 2.031 | 22.110 | 2690.887 | 7.080 | **11.3x faster** |
| $reduce with lambda | 100 elements | 3.918 | 4.027 | 20.620 | 2704.743 | 7.717 | **5.3x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 15.166 | 55.990 | 45.400 | 7451.391 | 318.144 | **3.0x faster** |
| Calculate total value | 100 products | 8.734 | 55.534 | 33.650 | 5162.454 | 317.637 | **3.9x faster** |
| Complex transformation | 100 products | 19.642 | 31.711 | 77.960 | 9311.040 | 137.000 | **4.0x faster** |
| Group by category (aggregate) | 100 products | 13.492 | 29.895 | 79.900 | N/A | 134.796 | **5.9x faster** |
| Top rated products | 100 products | 2.979 | 13.111 | 34.440 | 4524.175 | 67.565 | **11.6x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.933 | 500 |
| Filter by category (data→json) | 6.177 | 500 |
| Complex transformation (data handle) | 33.826 | 500 |
| Complex transformation (data→json) | 30.965 | 500 |
| Aggregate (data handle) | 5.954 | 500 |
| Aggregate (data→json) | 5.883 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**3.2x faster**)
- Array Operations (**2.8x faster**)
- Complex Transformations (**5.6x faster**)
- Deep Nesting (**1.5x faster**)
- String Operations (**5.3x faster**)
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

- **Date:** 2026-07-14
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
