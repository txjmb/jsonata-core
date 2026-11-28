# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.3.0 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.3.0 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.3.0 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.3.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-11-27.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.2x faster** |
| Array Operations | **3.7x faster** |
| Complex Transformations | **8.1x faster** |
| Deep Nesting | **2.7x faster** |
| String Operations | **6.7x faster** |
| Higher-Order Functions | **11.9x faster** |
| Realistic Workload | **9.6x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.605 | 4.556 | 0.689 | 16.900 | 1829.066 | 62.203 | **4.7x faster** |
| Deep Path (5 levels) | tiny | 4.953 | 6.869 | 1.041 | 25.830 | 3014.069 | 72.436 | **5.2x faster** |
| Array Index Access | 100 elements | 5.273 | 9.353 | 1.449 | 11.470 | 939.415 | 97.954 | **2.2x faster** |
| Arithmetic Expression | tiny | 2.535 | 3.912 | 0.718 | 22.130 | 2604.933 | 57.043 | **8.7x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.555 | 2.373 | 0.771 | 6.870 | 304.434 | 20.223 | **4.4x faster** |
| Array Max (100 elements) | 100 elements | 1.337 | 2.140 | 0.533 | 6.220 | 297.919 | 20.174 | **4.7x faster** |
| Array Count (100 elements) | 100 elements | 2.128 | 3.759 | 0.539 | 8.860 | 528.941 | 39.494 | **4.2x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.466 | 3.985 | 1.054 | 4.710 | 178.695 | 28.527 | **1.9x faster** |
| Array Max (1000 elements) | 1000 elements | 1.972 | 3.490 | 0.557 | 3.880 | 167.143 | 28.532 | **2.0x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.001 | 10.129 | 2.521 | 8.860 | 353.498 | 70.759 | **1.5x faster** |
| Array Mapping (extract field) | 100 objects | 8.907 | 27.120 | 1.290 | 21.620 | 2672.415 | 207.394 | **2.4x faster** |
| Array Mapping + Sum | 100 objects | 8.556 | 26.395 | 2.021 | 24.870 | 2978.651 | 207.556 | **2.9x faster** |
| Array Filtering (predicate) | 100 objects | 5.288 | 17.235 | 2.056 | 51.920 | 6728.224 | 107.689 | **9.8x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.973 | 3.812 | 3.032 | 27.340 | 2534.738 | 34.155 | **6.9x faster** |
| Object Construction (nested) | tiny | 6.020 | 5.096 | 4.428 | 33.520 | 2898.632 | 36.901 | **5.6x faster** |
| Conditional Expression | tiny | 1.137 | 1.687 | 0.414 | 13.990 | 1354.423 | 26.398 | **12.3x faster** |
| Multiple Nested Functions | tiny | 2.577 | 2.913 | 2.766 | 19.330 | 1773.898 | 28.062 | **7.5x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 5.724 | 6.474 | 1.010 | 25.880 | 2917.717 | 55.028 | **4.5x faster** |
| Nested Array Access | 4-level nested arrays | 8.717 | 13.397 | 0.657 | 7.930 | 611.187 | 107.932 | 1.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.040 | 4.555 | 3.351 | 22.580 | 2396.378 | 53.760 | **5.6x faster** |
| String Lowercase | tiny | 4.045 | 4.584 | 3.309 | 22.780 | 2397.803 | 53.580 | **5.6x faster** |
| String Length | tiny | 3.668 | 4.347 | 2.785 | 24.430 | 2598.920 | 55.030 | **6.7x faster** |
| String Concatenation | tiny | 3.173 | 3.166 | 2.853 | 25.270 | 1919.235 | 30.330 | **8.0x faster** |
| String Substring | tiny | 2.870 | 3.192 | 3.023 | 19.610 | 1682.636 | 28.530 | **6.8x faster** |
| String Contains | tiny | 2.175 | 2.507 | 1.696 | 16.160 | 1416.281 | 28.273 | **7.4x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.655 | 1.814 | 1.562 | 23.560 | 2683.289 | 7.629 | **14.2x faster** |
| $filter with lambda | 100 elements | 1.687 | 1.838 | 1.495 | 23.430 | 2685.242 | 7.509 | **13.9x faster** |
| $reduce with lambda | 100 elements | 3.049 | 3.205 | 3.097 | 22.810 | 2696.486 | 8.201 | **7.5x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.666 | 42.552 | 2.173 | 50.880 | 7435.288 | 315.969 | **6.6x faster** |
| Calculate total value | 100 products | 7.802 | 41.596 | 4.150 | 37.770 | 5149.464 | 315.424 | **4.8x faster** |
| Complex transformation | 100 products | 14.846 | 24.293 | 8.931 | 85.660 | 9306.174 | 136.290 | **5.8x faster** |
| Group by category (aggregate) | 100 products | 11.539 | 24.007 | 7.810 | 87.730 | N/A | 133.968 | **7.6x faster** |
| Top rated products | 100 products | 1.622 | 9.279 | 1.097 | 37.500 | 4514.001 | 67.132 | **23.1x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.012 | 500 |
| Filter by category (data→json) | 5.684 | 500 |
| Complex transformation (data handle) | 28.027 | 500 |
| Complex transformation (data→json) | 23.695 | 500 |
| Aggregate (data handle) | 4.688 | 500 |
| Aggregate (data→json) | 4.709 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.2x faster**)
- Array Operations (**3.7x faster**)
- Complex Transformations (**8.1x faster**)
- Deep Nesting (**2.7x faster**)
- String Operations (**6.7x faster**)
- Higher-Order Functions (**11.9x faster**)
- Realistic Workload (**9.6x faster**)

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

- **Date:** 2026-11-27
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
