# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.8 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.8 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-20.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **3.6x faster** |
| Array Operations | **3.0x faster** |
| Complex Transformations | **6.3x faster** |
| Deep Nesting | **1.7x faster** |
| String Operations | **5.8x faster** |
| Higher-Order Functions | **9.0x faster** |
| Realistic Workload | **6.1x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 5.672 | 6.609 | 16.940 | 1823.155 | 61.096 | **3.0x faster** |
| Deep Path (5 levels) | tiny | 7.823 | 9.518 | 25.310 | 3001.015 | 71.647 | **3.2x faster** |
| Array Index Access | 100 elements | 6.457 | 9.519 | 11.120 | 938.526 | 85.860 | **1.7x faster** |
| Arithmetic Expression | tiny | 3.399 | 5.113 | 22.220 | 2601.242 | 56.501 | **6.5x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.657 | 2.263 | 6.810 | 303.671 | 17.863 | **4.1x faster** |
| Array Max (100 elements) | 100 elements | 1.407 | 2.018 | 6.390 | 297.695 | 17.891 | **4.5x faster** |
| Array Count (100 elements) | 100 elements | 2.608 | 3.813 | 8.900 | 529.199 | 34.780 | **3.4x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.602 | 3.821 | 4.650 | 178.284 | 24.047 | **1.8x faster** |
| Array Max (1000 elements) | 1000 elements | 2.104 | 3.329 | 3.810 | 165.795 | 24.012 | **1.8x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.472 | 9.737 | 8.840 | 353.456 | 59.336 | **1.4x faster** |
| Array Mapping (extract field) | 100 objects | 11.455 | 34.593 | 21.410 | 2668.433 | 206.427 | **1.9x faster** |
| Array Mapping + Sum | 100 objects | 10.760 | 33.672 | 24.660 | 2971.003 | 206.451 | **2.3x faster** |
| Array Filtering (predicate) | 100 objects | 8.989 | 21.126 | 50.940 | 6714.903 | 107.646 | **5.7x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.843 | 5.275 | 26.750 | 2529.875 | 33.741 | **4.6x faster** |
| Object Construction (nested) | tiny | 8.230 | 6.577 | 33.370 | 2895.849 | 36.592 | **4.1x faster** |
| Conditional Expression | tiny | 1.415 | 2.150 | 13.450 | 1352.151 | 26.026 | **9.5x faster** |
| Multiple Nested Functions | tiny | 2.698 | 2.739 | 18.860 | 1771.814 | 27.537 | **7.0x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.889 | 8.819 | 25.710 | 2908.301 | 55.146 | **2.9x faster** |
| Nested Array Access | 4-level nested arrays | 16.969 | 18.412 | 7.930 | 609.109 | 107.753 | 2.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.688 | 5.182 | 22.460 | 2389.683 | 52.800 | **4.8x faster** |
| String Lowercase | tiny | 4.646 | 5.184 | 22.150 | 2387.992 | 52.960 | **4.8x faster** |
| String Length | tiny | 4.140 | 4.791 | 24.440 | 2590.140 | 54.446 | **5.9x faster** |
| String Concatenation | tiny | 4.127 | 3.803 | 25.780 | 1920.096 | 29.960 | **6.2x faster** |
| String Substring | tiny | 3.282 | 3.458 | 19.270 | 1682.234 | 28.505 | **5.9x faster** |
| String Contains | tiny | 2.158 | 2.430 | 15.840 | 1414.831 | 28.103 | **7.3x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 2.458 | 2.555 | 23.360 | 2678.138 | 7.142 | **9.5x faster** |
| $filter with lambda | 100 elements | 1.997 | 2.066 | 23.430 | 2677.115 | 7.011 | **11.7x faster** |
| $reduce with lambda | 100 elements | 3.897 | 4.011 | 22.420 | 2687.093 | 7.644 | **5.8x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 16.825 | 55.333 | 51.520 | 7430.772 | 315.032 | **3.1x faster** |
| Calculate total value | 100 products | 8.773 | 54.961 | 37.150 | 5152.192 | 315.215 | **4.2x faster** |
| Complex transformation | 100 products | 19.633 | 31.560 | 85.440 | 9282.907 | 136.240 | **4.4x faster** |
| Group by category (aggregate) | 100 products | 13.883 | 29.595 | 88.590 | N/A | 135.254 | **6.4x faster** |
| Top rated products | 100 products | 3.018 | 13.246 | 37.960 | 4516.204 | 67.149 | **12.6x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.540 | 500 |
| Filter by category (data→json) | 6.221 | 500 |
| Complex transformation (data handle) | 33.960 | 500 |
| Complex transformation (data→json) | 29.469 | 500 |
| Aggregate (data handle) | 5.906 | 500 |
| Aggregate (data→json) | 5.876 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**3.6x faster**)
- Array Operations (**3.0x faster**)
- Complex Transformations (**6.3x faster**)
- Deep Nesting (**1.7x faster**)
- String Operations (**5.8x faster**)
- Higher-Order Functions (**9.0x faster**)
- Realistic Workload (**6.1x faster**)

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

- **Date:** 2026-07-20
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
