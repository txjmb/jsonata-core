# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.15 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.15 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.15 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.2.2 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-11-18.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.6x faster** |
| Array Operations | **3.6x faster** |
| Complex Transformations | **8.1x faster** |
| Deep Nesting | **3.4x faster** |
| String Operations | **7.0x faster** |
| Higher-Order Functions | **12.3x faster** |
| Realistic Workload | **9.6x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.337 | 4.535 | 0.668 | 16.990 | 1842.972 | 61.048 | **5.1x faster** |
| Deep Path (5 levels) | tiny | 4.281 | 6.774 | 1.211 | 25.810 | 3029.410 | 72.000 | **6.0x faster** |
| Array Index Access | 100 elements | 5.222 | 9.301 | 1.445 | 11.330 | 943.832 | 97.112 | **2.2x faster** |
| Arithmetic Expression | tiny | 2.499 | 3.886 | 0.724 | 22.340 | 2626.758 | 56.439 | **8.9x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.558 | 2.357 | 0.769 | 6.820 | 305.303 | 20.016 | **4.4x faster** |
| Array Max (100 elements) | 100 elements | 1.329 | 2.127 | 0.534 | 6.430 | 299.020 | 20.068 | **4.8x faster** |
| Array Count (100 elements) | 100 elements | 2.136 | 3.742 | 0.542 | 8.790 | 530.430 | 39.114 | **4.1x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.482 | 3.996 | 1.055 | 4.670 | 179.555 | 28.448 | **1.9x faster** |
| Array Max (1000 elements) | 1000 elements | 1.986 | 3.491 | 0.557 | 3.860 | 167.448 | 28.458 | **1.9x faster** |
| Array Sum (10000 elements) | 10000 elements | 5.992 | 10.211 | 2.521 | 8.850 | 355.656 | 70.512 | **1.5x faster** |
| Array Mapping (extract field) | 100 objects | 7.853 | 26.917 | 1.252 | 21.400 | 2677.402 | 207.108 | **2.7x faster** |
| Array Mapping + Sum | 100 objects | 7.485 | 26.013 | 1.990 | 24.750 | 2987.685 | 207.253 | **3.3x faster** |
| Array Filtering (predicate) | 100 objects | 6.392 | 17.130 | 1.951 | 51.140 | 6763.492 | 107.694 | **8.0x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.921 | 3.797 | 3.027 | 27.060 | 2546.313 | 33.465 | **6.9x faster** |
| Object Construction (nested) | tiny | 5.969 | 5.049 | 4.549 | 33.820 | 2913.120 | 36.415 | **5.7x faster** |
| Conditional Expression | tiny | 1.107 | 1.649 | 0.401 | 13.590 | 1362.448 | 26.134 | **12.3x faster** |
| Multiple Nested Functions | tiny | 2.521 | 2.857 | 2.735 | 19.090 | 1786.302 | 27.582 | **7.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 4.433 | 6.468 | 1.004 | 25.800 | 2927.479 | 55.004 | **5.8x faster** |
| Nested Array Access | 4-level nested arrays | 8.492 | 13.192 | 0.635 | 8.050 | 611.959 | 107.804 | 1.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.869 | 4.501 | 3.267 | 22.630 | 2407.441 | 52.922 | **5.8x faster** |
| String Lowercase | tiny | 3.886 | 4.543 | 3.365 | 22.360 | 2410.499 | 52.856 | **5.8x faster** |
| String Length | tiny | 3.529 | 4.276 | 2.753 | 24.170 | 2609.259 | 54.281 | **6.8x faster** |
| String Concatenation | tiny | 3.026 | 3.166 | 2.821 | 26.070 | 1933.986 | 30.066 | **8.6x faster** |
| String Substring | tiny | 2.780 | 3.137 | 2.975 | 19.460 | 1691.797 | 28.298 | **7.0x faster** |
| String Contains | tiny | 2.092 | 2.469 | 1.662 | 16.150 | 1424.657 | 27.944 | **7.7x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.574 | 1.727 | 1.473 | 23.820 | 2695.202 | 7.605 | **15.1x faster** |
| $filter with lambda | 100 elements | 1.653 | 1.738 | 1.528 | 23.610 | 2693.778 | 7.502 | **14.3x faster** |
| $reduce with lambda | 100 elements | 3.090 | 3.247 | 3.099 | 22.970 | 2709.251 | 8.164 | **7.4x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.571 | 42.325 | 2.077 | 52.290 | 7460.071 | 315.785 | **6.9x faster** |
| Calculate total value | 100 products | 7.715 | 41.536 | 4.263 | 37.580 | 5168.735 | 316.190 | **4.9x faster** |
| Complex transformation | 100 products | 14.631 | 24.078 | 8.726 | 85.900 | 9336.593 | 136.053 | **5.9x faster** |
| Group by category (aggregate) | 100 products | 11.585 | 24.041 | 7.780 | 87.980 | N/A | 133.922 | **7.6x faster** |
| Top rated products | 100 products | 1.625 | 9.258 | 1.059 | 37.360 | 4540.872 | 66.999 | **23.0x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 11.757 | 500 |
| Filter by category (data→json) | 5.636 | 500 |
| Complex transformation (data handle) | 27.432 | 500 |
| Complex transformation (data→json) | 23.350 | 500 |
| Aggregate (data handle) | 4.714 | 500 |
| Aggregate (data→json) | 4.678 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.6x faster**)
- Array Operations (**3.6x faster**)
- Complex Transformations (**8.1x faster**)
- Deep Nesting (**3.4x faster**)
- String Operations (**7.0x faster**)
- Higher-Order Functions (**12.3x faster**)
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

- **Date:** 2026-11-18
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
