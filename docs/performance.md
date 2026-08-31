# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.9 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.9 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.9 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-08-31.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **4.9x faster** |
| Array Operations | **3.7x faster** |
| Complex Transformations | **7.6x faster** |
| Deep Nesting | **2.9x faster** |
| String Operations | **6.5x faster** |
| Higher-Order Functions | **11.9x faster** |
| Realistic Workload | **8.1x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.705 | 4.877 | 0.768 | 16.820 | 1824.042 | 61.484 | **4.5x faster** |
| Deep Path (5 levels) | tiny | 5.479 | 6.812 | 1.232 | 25.360 | 3005.573 | 72.071 | **4.6x faster** |
| Array Index Access | 100 elements | 4.320 | 8.413 | 0.465 | 11.190 | 933.850 | 97.630 | **2.6x faster** |
| Arithmetic Expression | tiny | 2.841 | 4.032 | 0.857 | 22.380 | 2603.586 | 56.913 | **7.9x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.499 | 2.319 | 0.781 | 6.890 | 304.144 | 20.184 | **4.6x faster** |
| Array Max (100 elements) | 100 elements | 1.243 | 2.064 | 0.534 | 6.410 | 297.163 | 20.172 | **5.2x faster** |
| Array Count (100 elements) | 100 elements | 1.946 | 3.596 | 0.545 | 8.700 | 527.289 | 39.419 | **4.5x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.290 | 3.849 | 1.057 | 4.660 | 178.617 | 28.586 | **2.0x faster** |
| Array Max (1000 elements) | 1000 elements | 1.787 | 3.336 | 0.558 | 3.840 | 166.545 | 28.630 | **2.1x faster** |
| Array Sum (10000 elements) | 10000 elements | 5.586 | 9.973 | 2.519 | 8.810 | 354.240 | 70.519 | **1.6x faster** |
| Array Mapping (extract field) | 100 objects | 8.833 | 25.838 | 1.306 | 21.620 | 2676.758 | 208.870 | **2.4x faster** |
| Array Mapping + Sum | 100 objects | 8.805 | 25.215 | 2.047 | 24.760 | 2982.977 | 209.306 | **2.8x faster** |
| Array Filtering (predicate) | 100 objects | 6.241 | 16.739 | 2.698 | 50.870 | 6738.023 | 112.088 | **8.2x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 4.101 | 3.817 | 2.959 | 27.590 | 2531.848 | 34.017 | **6.7x faster** |
| Object Construction (nested) | tiny | 5.491 | 4.545 | 4.045 | 33.400 | 2898.268 | 36.853 | **6.1x faster** |
| Conditional Expression | tiny | 1.275 | 1.765 | 0.452 | 13.550 | 1350.644 | 26.355 | **10.6x faster** |
| Multiple Nested Functions | tiny | 2.769 | 2.973 | 2.931 | 19.210 | 1775.537 | 27.779 | **6.9x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 5.686 | 6.678 | 1.232 | 25.920 | 2910.910 | 55.381 | **4.6x faster** |
| Nested Array Access | 4-level nested arrays | 6.884 | 12.924 | 0.314 | 7.900 | 609.672 | 107.454 | **1.1x faster** |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.046 | 4.679 | 3.451 | 22.680 | 2395.514 | 53.345 | **5.6x faster** |
| String Lowercase | tiny | 4.062 | 4.675 | 3.359 | 22.230 | 2395.335 | 53.213 | **5.5x faster** |
| String Length | tiny | 3.735 | 4.583 | 2.886 | 24.550 | 2596.091 | 55.030 | **6.6x faster** |
| String Concatenation | tiny | 3.311 | 3.232 | 3.048 | 26.150 | 1921.544 | 30.205 | **7.9x faster** |
| String Substring | tiny | 2.940 | 3.238 | 3.023 | 19.500 | 1682.958 | 28.586 | **6.6x faster** |
| String Contains | tiny | 2.294 | 2.676 | 1.755 | 16.160 | 1412.139 | 28.356 | **7.0x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.730 | 1.879 | 1.293 | 23.760 | 2679.104 | 7.651 | **13.7x faster** |
| $filter with lambda | 100 elements | 1.720 | 1.845 | 1.498 | 23.600 | 2684.242 | 7.539 | **13.7x faster** |
| $reduce with lambda | 100 elements | 2.765 | 2.929 | 2.776 | 22.940 | 2696.682 | 8.227 | **8.3x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 8.673 | 41.494 | 2.685 | 51.550 | 7441.033 | 316.541 | **5.9x faster** |
| Calculate total value | 100 products | 7.203 | 40.611 | 5.632 | 37.260 | 5153.651 | 315.792 | **5.2x faster** |
| Complex transformation | 100 products | 14.573 | 24.130 | 9.758 | 89.030 | 9305.808 | 136.543 | **6.1x faster** |
| Group by category (aggregate) | 100 products | 10.464 | 22.070 | 8.079 | 88.450 | N/A | 134.061 | **8.5x faster** |
| Top rated products | 100 products | 2.512 | 9.976 | 2.047 | 37.160 | 4520.874 | 67.508 | **14.8x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.343 | 500 |
| Filter by category (data→json) | 5.934 | 500 |
| Complex transformation (data handle) | 28.303 | 500 |
| Complex transformation (data→json) | 23.938 | 500 |
| Aggregate (data handle) | 5.381 | 500 |
| Aggregate (data→json) | 5.316 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**4.9x faster**)
- Array Operations (**3.7x faster**)
- Complex Transformations (**7.6x faster**)
- Deep Nesting (**2.9x faster**)
- String Operations (**6.5x faster**)
- Higher-Order Functions (**11.9x faster**)
- Realistic Workload (**8.1x faster**)

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

- **Date:** 2026-08-31
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
