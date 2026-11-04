# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.14 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.14 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.14 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.2.2 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-11-04.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.6x faster** |
| Array Operations | **3.8x faster** |
| Complex Transformations | **8.0x faster** |
| Deep Nesting | **3.3x faster** |
| String Operations | **6.9x faster** |
| Higher-Order Functions | **12.4x faster** |
| Realistic Workload | **9.5x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.378 | 4.534 | 0.705 | 16.990 | 1836.711 | 61.601 | **5.0x faster** |
| Deep Path (5 levels) | tiny | 4.366 | 6.763 | 1.280 | 25.620 | 3017.349 | 72.429 | **5.9x faster** |
| Array Index Access | 100 elements | 5.148 | 9.256 | 1.361 | 11.310 | 941.846 | 96.590 | **2.2x faster** |
| Arithmetic Expression | tiny | 2.451 | 3.840 | 0.739 | 22.500 | 2618.708 | 56.640 | **9.2x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.552 | 2.362 | 0.770 | 6.920 | 305.086 | 19.953 | **4.5x faster** |
| Array Max (100 elements) | 100 elements | 1.324 | 2.144 | 0.530 | 6.450 | 298.084 | 20.020 | **4.9x faster** |
| Array Count (100 elements) | 100 elements | 2.119 | 3.746 | 0.527 | 8.740 | 530.290 | 39.057 | **4.1x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.462 | 3.986 | 1.054 | 4.680 | 179.120 | 28.426 | **1.9x faster** |
| Array Max (1000 elements) | 1000 elements | 1.970 | 3.490 | 0.556 | 3.860 | 166.873 | 28.471 | **2.0x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.006 | 10.115 | 2.516 | 8.840 | 353.646 | 70.171 | **1.5x faster** |
| Array Mapping (extract field) | 100 objects | 7.973 | 26.540 | 1.297 | 21.460 | 2678.923 | 208.427 | **2.7x faster** |
| Array Mapping + Sum | 100 objects | 7.597 | 25.646 | 2.029 | 24.710 | 2987.199 | 208.740 | **3.3x faster** |
| Array Filtering (predicate) | 100 objects | 5.202 | 16.807 | 2.109 | 50.420 | 6779.029 | 108.186 | **9.7x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.963 | 3.704 | 3.009 | 26.990 | 2542.111 | 33.962 | **6.8x faster** |
| Object Construction (nested) | tiny | 6.087 | 4.954 | 4.380 | 33.470 | 2908.900 | 36.722 | **5.5x faster** |
| Conditional Expression | tiny | 1.110 | 1.663 | 0.408 | 13.550 | 1357.187 | 26.261 | **12.2x faster** |
| Multiple Nested Functions | tiny | 2.533 | 2.908 | 2.776 | 19.100 | 1783.288 | 27.816 | **7.5x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 4.539 | 6.369 | 1.024 | 25.750 | 2924.303 | 55.274 | **5.7x faster** |
| Nested Array Access | 4-level nested arrays | 8.450 | 13.121 | 0.610 | 8.020 | 613.889 | 107.729 | 1.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.824 | 4.528 | 3.304 | 22.630 | 2400.792 | 53.321 | **5.9x faster** |
| String Lowercase | tiny | 3.875 | 4.545 | 3.266 | 22.640 | 2402.033 | 53.020 | **5.8x faster** |
| String Length | tiny | 3.574 | 4.370 | 2.737 | 24.640 | 2606.004 | 54.715 | **6.9x faster** |
| String Concatenation | tiny | 3.010 | 3.092 | 2.788 | 25.660 | 1931.043 | 30.270 | **8.5x faster** |
| String Substring | tiny | 2.839 | 3.161 | 2.933 | 19.560 | 1690.300 | 28.501 | **6.9x faster** |
| String Contains | tiny | 2.116 | 2.484 | 1.669 | 16.080 | 1420.954 | 28.208 | **7.6x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.621 | 1.784 | 1.512 | 23.660 | 2697.424 | 7.584 | **14.6x faster** |
| $filter with lambda | 100 elements | 1.577 | 1.738 | 1.467 | 23.650 | 2698.732 | 7.506 | **15.0x faster** |
| $reduce with lambda | 100 elements | 3.016 | 3.174 | 3.081 | 22.660 | 2710.180 | 8.107 | **7.5x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.519 | 41.860 | 2.242 | 51.910 | 7467.123 | 316.500 | **6.9x faster** |
| Calculate total value | 100 products | 7.810 | 40.919 | 4.183 | 37.730 | 5178.163 | 316.344 | **4.8x faster** |
| Complex transformation | 100 products | 14.635 | 23.707 | 8.569 | 86.360 | 9349.318 | 136.270 | **5.9x faster** |
| Group by category (aggregate) | 100 products | 11.640 | 23.745 | 7.875 | 88.340 | N/A | 134.171 | **7.6x faster** |
| Top rated products | 100 products | 1.670 | 9.182 | 1.058 | 37.190 | 4546.555 | 67.116 | **22.3x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 11.792 | 500 |
| Filter by category (data→json) | 5.616 | 500 |
| Complex transformation (data handle) | 27.022 | 500 |
| Complex transformation (data→json) | 22.984 | 500 |
| Aggregate (data handle) | 4.650 | 500 |
| Aggregate (data→json) | 4.687 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.6x faster**)
- Array Operations (**3.8x faster**)
- Complex Transformations (**8.0x faster**)
- Deep Nesting (**3.3x faster**)
- String Operations (**6.9x faster**)
- Higher-Order Functions (**12.4x faster**)
- Realistic Workload (**9.5x faster**)

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

- **Date:** 2026-11-04
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
