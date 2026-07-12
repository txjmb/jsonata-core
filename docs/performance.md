# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.3 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.3 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-12.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **2.7x faster** |
| Array Operations | **1.9x faster** |
| Complex Transformations | **5.5x faster** |
| Deep Nesting | **1.4x faster** |
| String Operations | **5.4x faster** |
| Higher-Order Functions | **9.6x faster** |
| Realistic Workload | **1.5x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 6.237 | 6.457 | 14.700 | 1835.106 | 61.758 | **2.4x faster** |
| Deep Path (5 levels) | tiny | 9.261 | 9.504 | 22.680 | 3015.204 | 73.094 | **2.4x faster** |
| Array Index Access | 100 elements | 6.769 | 10.603 | 9.710 | 938.867 | 86.804 | **1.4x faster** |
| Arithmetic Expression | tiny | 4.163 | 5.141 | 19.040 | 2606.022 | 56.756 | **4.6x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.665 | 2.314 | 5.970 | 305.129 | 17.970 | **3.6x faster** |
| Array Max (100 elements) | 100 elements | 1.409 | 2.063 | 5.540 | 298.254 | 18.007 | **3.9x faster** |
| Array Count (100 elements) | 100 elements | 2.589 | 3.926 | 7.340 | 530.227 | 35.056 | **2.8x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.795 | 4.282 | 4.550 | 178.364 | 24.397 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.280 | 3.774 | 3.700 | 166.380 | 24.416 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.641 | 10.478 | 8.780 | 354.275 | 59.910 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 43.081 | 35.646 | 18.370 | 2671.036 | 209.100 | 2.3x slower |
| Array Mapping + Sum | 100 objects | 42.333 | 34.476 | 21.730 | 2977.095 | 209.341 | 1.9x slower |
| Array Filtering (predicate) | 100 objects | 30.586 | 21.339 | 44.730 | 6731.095 | 108.713 | **1.5x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.537 | 5.224 | 24.540 | 2534.029 | 33.875 | **4.4x faster** |
| Object Construction (nested) | tiny | 7.740 | 6.568 | 30.540 | 2901.421 | 36.969 | **3.9x faster** |
| Conditional Expression | tiny | 1.720 | 2.098 | 11.570 | 1357.944 | 26.282 | **6.7x faster** |
| Multiple Nested Functions | tiny | 2.448 | 2.707 | 17.210 | 1780.800 | 27.899 | **7.0x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.429 | 8.767 | 23.100 | 2912.070 | 55.236 | **2.4x faster** |
| Nested Array Access | 4-level nested arrays | 16.915 | 18.975 | 6.480 | 611.807 | 108.467 | 2.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.721 | 5.182 | 19.940 | 2401.853 | 53.229 | **4.2x faster** |
| String Lowercase | tiny | 4.691 | 5.151 | 19.630 | 2401.833 | 53.340 | **4.2x faster** |
| String Length | tiny | 4.062 | 4.809 | 21.750 | 2600.960 | 54.637 | **5.4x faster** |
| String Concatenation | tiny | 3.637 | 3.702 | 23.680 | 1925.406 | 30.183 | **6.5x faster** |
| String Substring | tiny | 3.260 | 3.485 | 17.780 | 1687.054 | 28.418 | **5.5x faster** |
| String Contains | tiny | 2.155 | 2.458 | 14.030 | 1420.293 | 28.178 | **6.5x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.879 | 1.999 | 21.650 | 2683.356 | 7.136 | **11.5x faster** |
| $filter with lambda | 100 elements | 1.874 | 1.969 | 21.620 | 2686.865 | 7.067 | **11.5x faster** |
| $reduce with lambda | 100 elements | 3.555 | 3.694 | 20.620 | 2697.090 | 7.725 | **5.8x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 76.652 | 55.306 | 45.000 | 7439.112 | 317.477 | 1.7x slower |
| Calculate total value | 100 products | 69.813 | 55.126 | 33.450 | 5152.369 | 316.801 | 2.1x slower |
| Complex transformation | 100 products | 38.850 | 31.347 | 78.720 | 9293.462 | 136.910 | **2.0x faster** |
| Group by category (aggregate) | 100 products | 35.630 | 29.684 | 79.710 | N/A | 134.567 | **2.2x faster** |
| Top rated products | 100 products | 17.706 | 13.173 | 34.300 | 4519.983 | 67.399 | **1.9x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.401 | 500 |
| Filter by category (data→json) | 5.832 | 500 |
| Complex transformation (data handle) | 32.895 | 500 |
| Complex transformation (data→json) | 28.856 | 500 |
| Aggregate (data handle) | 5.861 | 500 |
| Aggregate (data→json) | 5.976 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**2.7x faster**)
- Array Operations (**1.9x faster**)
- Complex Transformations (**5.5x faster**)
- Deep Nesting (**1.4x faster**)
- String Operations (**5.4x faster**)
- Higher-Order Functions (**9.6x faster**)
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

- **Date:** 2026-07-12
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
