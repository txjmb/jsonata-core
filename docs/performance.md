# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.8 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.8 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-08-25.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **3.7x faster** |
| Array Operations | **2.9x faster** |
| Complex Transformations | **5.7x faster** |
| Deep Nesting | **1.7x faster** |
| String Operations | **4.4x faster** |
| Higher-Order Functions | **10.6x faster** |
| Realistic Workload | **6.1x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 5.555 | 6.595 | 17.060 | 1833.028 | 62.481 | **3.1x faster** |
| Deep Path (5 levels) | tiny | 7.848 | 9.528 | 25.490 | 3016.710 | 72.907 | **3.2x faster** |
| Array Index Access | 100 elements | 6.179 | 9.720 | 11.400 | 940.889 | 87.415 | **1.8x faster** |
| Arithmetic Expression | tiny | 3.401 | 5.113 | 22.340 | 2611.785 | 57.413 | **6.6x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.941 | 2.642 | 6.740 | 305.087 | 18.112 | **3.5x faster** |
| Array Max (100 elements) | 100 elements | 1.677 | 2.386 | 6.430 | 298.223 | 18.179 | **3.8x faster** |
| Array Count (100 elements) | 100 elements | 2.831 | 4.242 | 8.890 | 530.256 | 35.426 | **3.1x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.862 | 4.238 | 4.670 | 178.593 | 24.328 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.350 | 3.743 | 3.820 | 167.318 | 24.269 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 7.219 | 10.963 | 8.810 | 354.105 | 60.006 | **1.2x faster** |
| Array Mapping (extract field) | 100 objects | 11.045 | 35.157 | 21.490 | 2679.407 | 209.399 | **1.9x faster** |
| Array Mapping + Sum | 100 objects | 10.830 | 34.572 | 24.670 | 2975.568 | 209.355 | **2.3x faster** |
| Array Filtering (predicate) | 100 objects | 7.676 | 21.293 | 51.350 | 6743.746 | 108.321 | **6.7x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.762 | 5.225 | 26.960 | 2540.414 | 34.234 | **4.7x faster** |
| Object Construction (nested) | tiny | 7.897 | 6.516 | 33.320 | 2910.548 | 37.221 | **4.2x faster** |
| Conditional Expression | tiny | 1.462 | 2.137 | 13.570 | 1356.223 | 26.614 | **9.3x faster** |
| Multiple Nested Functions | tiny | 4.063 | 4.274 | 18.920 | 1777.959 | 28.008 | **4.7x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 8.783 | 8.888 | 25.890 | 2917.217 | 56.130 | **2.9x faster** |
| Nested Array Access | 4-level nested arrays | 15.951 | 18.921 | 8.010 | 611.320 | 108.524 | 2.0x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 6.201 | 6.507 | 22.660 | 2397.988 | 54.048 | **3.7x faster** |
| String Lowercase | tiny | 6.165 | 6.549 | 22.660 | 2401.608 | 53.875 | **3.7x faster** |
| String Length | tiny | 5.649 | 6.244 | 24.750 | 2599.356 | 55.431 | **4.4x faster** |
| String Concatenation | tiny | 4.913 | 4.539 | 25.280 | 1927.866 | 30.665 | **5.1x faster** |
| String Substring | tiny | 4.341 | 4.498 | 19.640 | 1686.942 | 28.936 | **4.5x faster** |
| String Contains | tiny | 3.138 | 3.411 | 16.060 | 1419.796 | 28.712 | **5.1x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.856 | 1.949 | 23.840 | 2684.529 | 7.235 | **12.8x faster** |
| $filter with lambda | 100 elements | 1.858 | 1.928 | 23.510 | 2686.343 | 7.056 | **12.7x faster** |
| $reduce with lambda | 100 elements | 3.526 | 3.662 | 22.600 | 2701.940 | 7.742 | **6.4x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 14.894 | 56.370 | 51.820 | 7450.904 | 316.826 | **3.5x faster** |
| Calculate total value | 100 products | 8.721 | 55.564 | 37.530 | 5168.845 | 316.649 | **4.3x faster** |
| Complex transformation | 100 products | 19.418 | 31.444 | 86.530 | 9306.045 | 136.619 | **4.5x faster** |
| Group by category (aggregate) | 100 products | 13.522 | 30.724 | 88.320 | N/A | 134.274 | **6.5x faster** |
| Top rated products | 100 products | 3.151 | 13.314 | 37.450 | 4537.455 | 67.388 | **11.9x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.765 | 500 |
| Filter by category (data→json) | 6.320 | 500 |
| Complex transformation (data handle) | 32.814 | 500 |
| Complex transformation (data→json) | 28.243 | 500 |
| Aggregate (data handle) | 5.834 | 500 |
| Aggregate (data→json) | 6.692 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**3.7x faster**)
- Array Operations (**2.9x faster**)
- Complex Transformations (**5.7x faster**)
- Deep Nesting (**1.7x faster**)
- String Operations (**4.4x faster**)
- Higher-Order Functions (**10.6x faster**)
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

- **Date:** 2026-08-25
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
