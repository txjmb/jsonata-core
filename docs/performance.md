# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.11 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.11 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.11 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-10-01.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.7x faster** |
| Array Operations | **4.1x faster** |
| Complex Transformations | **8.5x faster** |
| Deep Nesting | **3.4x faster** |
| String Operations | **7.1x faster** |
| Higher-Order Functions | **14.3x faster** |
| Realistic Workload | **9.4x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.310 | 4.422 | 0.595 | 16.820 | 1826.709 | 61.611 | **5.1x faster** |
| Deep Path (5 levels) | tiny | 4.387 | 6.579 | 0.940 | 25.480 | 3011.270 | 71.503 | **5.8x faster** |
| Array Index Access | 100 elements | 4.397 | 8.615 | 0.421 | 11.310 | 938.110 | 96.992 | **2.6x faster** |
| Arithmetic Expression | tiny | 2.442 | 3.819 | 0.641 | 22.330 | 2602.561 | 56.731 | **9.1x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.458 | 2.310 | 0.721 | 6.860 | 304.457 | 20.027 | **4.7x faster** |
| Array Max (100 elements) | 100 elements | 1.237 | 2.082 | 0.497 | 6.460 | 297.065 | 20.016 | **5.2x faster** |
| Array Count (100 elements) | 100 elements | 1.938 | 3.635 | 0.465 | 8.750 | 526.673 | 39.166 | **4.5x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.372 | 4.039 | 1.044 | 4.870 | 180.112 | 28.352 | **2.1x faster** |
| Array Max (1000 elements) | 1000 elements | 1.793 | 3.402 | 0.549 | 3.830 | 166.018 | 28.399 | **2.1x faster** |
| Array Sum (10000 elements) | 10000 elements | 5.586 | 9.967 | 2.514 | 8.850 | 353.802 | 70.109 | **1.6x faster** |
| Array Mapping (extract field) | 100 objects | 7.569 | 25.590 | 0.847 | 21.410 | 2670.599 | 209.092 | **2.8x faster** |
| Array Mapping + Sum | 100 objects | 7.239 | 24.810 | 1.538 | 24.710 | 2975.382 | 208.922 | **3.4x faster** |
| Array Filtering (predicate) | 100 objects | 4.941 | 16.099 | 1.829 | 50.470 | 6731.878 | 108.424 | **10.2x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.711 | 3.541 | 2.755 | 27.450 | 2529.953 | 33.837 | **7.4x faster** |
| Object Construction (nested) | tiny | 5.460 | 4.430 | 3.785 | 33.250 | 2897.885 | 36.775 | **6.1x faster** |
| Conditional Expression | tiny | 1.066 | 1.617 | 0.353 | 13.680 | 1352.683 | 26.262 | **12.8x faster** |
| Multiple Nested Functions | tiny | 2.468 | 2.809 | 2.498 | 19.090 | 1774.459 | 27.655 | **7.7x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 4.527 | 6.291 | 0.967 | 25.680 | 2918.190 | 55.389 | **5.7x faster** |
| Nested Array Access | 4-level nested arrays | 6.805 | 12.552 | 0.246 | 8.000 | 610.479 | 107.673 | **1.2x faster** |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.764 | 4.385 | 2.949 | 22.470 | 2394.845 | 53.413 | **6.0x faster** |
| String Lowercase | tiny | 3.773 | 4.342 | 2.983 | 22.470 | 2393.904 | 53.420 | **6.0x faster** |
| String Length | tiny | 3.439 | 4.169 | 2.360 | 24.530 | 2592.447 | 54.602 | **7.1x faster** |
| String Concatenation | tiny | 3.104 | 3.133 | 2.658 | 25.590 | 1918.898 | 30.204 | **8.2x faster** |
| String Substring | tiny | 2.740 | 3.059 | 2.838 | 19.610 | 1681.747 | 28.393 | **7.2x faster** |
| String Contains | tiny | 2.077 | 2.488 | 1.495 | 16.350 | 1413.179 | 28.164 | **7.9x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.272 | 1.477 | 1.223 | 22.880 | 2674.199 | 7.647 | **18.0x faster** |
| $filter with lambda | 100 elements | 1.445 | 1.607 | 1.382 | 23.630 | 2676.702 | 7.520 | **16.4x faster** |
| $reduce with lambda | 100 elements | 2.664 | 2.807 | 2.827 | 22.570 | 2695.178 | 8.148 | **8.5x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.268 | 40.157 | 2.001 | 51.680 | 7438.791 | 316.889 | **7.1x faster** |
| Calculate total value | 100 products | 6.499 | 40.223 | 5.656 | 37.160 | 5154.582 | 315.982 | **5.7x faster** |
| Complex transformation | 100 products | 12.611 | 22.411 | 8.308 | 86.060 | 9303.946 | 136.340 | **6.8x faster** |
| Group by category (aggregate) | 100 products | 8.148 | 21.677 | 7.940 | 89.450 | N/A | 134.763 | **11.0x faster** |
| Top rated products | 100 products | 2.239 | 9.638 | 1.872 | 37.080 | 4526.039 | 67.505 | **16.6x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 11.688 | 500 |
| Filter by category (data→json) | 5.343 | 500 |
| Complex transformation (data handle) | 25.277 | 500 |
| Complex transformation (data→json) | 20.849 | 500 |
| Aggregate (data handle) | 5.271 | 500 |
| Aggregate (data→json) | 5.103 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.7x faster**)
- Array Operations (**4.1x faster**)
- Complex Transformations (**8.5x faster**)
- Deep Nesting (**3.4x faster**)
- String Operations (**7.1x faster**)
- Higher-Order Functions (**14.3x faster**)
- Realistic Workload (**9.4x faster**)

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

- **Date:** 2026-10-01
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
