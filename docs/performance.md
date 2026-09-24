# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.10 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.10 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.10 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-09-23.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **4.9x faster** |
| Array Operations | **3.7x faster** |
| Complex Transformations | **7.6x faster** |
| Deep Nesting | **2.8x faster** |
| String Operations | **6.5x faster** |
| Higher-Order Functions | **12.0x faster** |
| Realistic Workload | **8.1x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.710 | 4.761 | 0.772 | 16.880 | 1827.702 | 61.881 | **4.6x faster** |
| Deep Path (5 levels) | tiny | 5.514 | 6.782 | 1.239 | 25.640 | 3008.876 | 72.363 | **4.6x faster** |
| Array Index Access | 100 elements | 4.354 | 8.444 | 0.467 | 11.300 | 937.946 | 97.517 | **2.6x faster** |
| Arithmetic Expression | tiny | 2.847 | 4.048 | 0.858 | 22.280 | 2602.653 | 56.977 | **7.8x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.512 | 2.320 | 0.785 | 6.900 | 303.819 | 20.175 | **4.6x faster** |
| Array Max (100 elements) | 100 elements | 1.251 | 2.056 | 0.540 | 6.450 | 297.146 | 20.155 | **5.2x faster** |
| Array Count (100 elements) | 100 elements | 1.958 | 3.594 | 0.554 | 8.700 | 527.634 | 39.454 | **4.4x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.293 | 3.849 | 1.057 | 4.670 | 178.813 | 28.559 | **2.0x faster** |
| Array Max (1000 elements) | 1000 elements | 1.793 | 3.330 | 0.559 | 3.830 | 166.808 | 28.584 | **2.1x faster** |
| Array Sum (10000 elements) | 10000 elements | 5.588 | 9.978 | 2.522 | 8.840 | 355.657 | 70.553 | **1.6x faster** |
| Array Mapping (extract field) | 100 objects | 8.961 | 25.921 | 1.312 | 21.300 | 2679.028 | 209.480 | **2.4x faster** |
| Array Mapping + Sum | 100 objects | 8.726 | 25.235 | 2.054 | 24.630 | 2981.042 | 210.194 | **2.8x faster** |
| Array Filtering (predicate) | 100 objects | 6.219 | 16.783 | 2.610 | 51.310 | 6740.508 | 108.751 | **8.3x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.961 | 3.681 | 2.971 | 27.130 | 2532.052 | 34.070 | **6.8x faster** |
| Object Construction (nested) | tiny | 5.602 | 4.541 | 4.048 | 33.410 | 2903.882 | 36.805 | **6.0x faster** |
| Conditional Expression | tiny | 1.267 | 1.764 | 0.450 | 13.560 | 1352.213 | 26.343 | **10.7x faster** |
| Multiple Nested Functions | tiny | 2.772 | 2.952 | 2.952 | 19.210 | 1774.524 | 27.871 | **6.9x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 5.703 | 6.599 | 1.232 | 25.610 | 2917.278 | 55.513 | **4.5x faster** |
| Nested Array Access | 4-level nested arrays | 6.859 | 12.823 | 0.304 | 8.060 | 611.064 | 108.055 | **1.2x faster** |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.043 | 4.720 | 3.460 | 22.530 | 2394.488 | 53.335 | **5.6x faster** |
| String Lowercase | tiny | 4.044 | 4.680 | 3.468 | 22.450 | 2394.383 | 53.279 | **5.6x faster** |
| String Length | tiny | 3.738 | 4.565 | 2.957 | 24.380 | 2590.814 | 54.945 | **6.5x faster** |
| String Concatenation | tiny | 3.327 | 3.229 | 3.066 | 25.790 | 1921.044 | 30.355 | **7.8x faster** |
| String Substring | tiny | 2.949 | 3.240 | 3.083 | 19.560 | 1683.702 | 28.595 | **6.6x faster** |
| String Contains | tiny | 2.299 | 2.686 | 1.776 | 16.110 | 1414.351 | 28.457 | **7.0x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.638 | 1.772 | 1.288 | 23.490 | 2681.648 | 7.632 | **14.3x faster** |
| $filter with lambda | 100 elements | 1.734 | 1.875 | 1.511 | 23.460 | 2686.796 | 7.524 | **13.5x faster** |
| $reduce with lambda | 100 elements | 2.759 | 2.905 | 2.778 | 22.620 | 2698.461 | 8.166 | **8.2x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 8.772 | 41.468 | 2.698 | 51.890 | 7449.646 | 317.120 | **5.9x faster** |
| Calculate total value | 100 products | 7.245 | 40.621 | 5.631 | 37.670 | 5156.155 | 317.365 | **5.2x faster** |
| Complex transformation | 100 products | 14.196 | 23.664 | 9.706 | 85.870 | 9313.624 | 136.579 | **6.0x faster** |
| Group by category (aggregate) | 100 products | 10.440 | 22.096 | 8.003 | 88.600 | N/A | 134.272 | **8.5x faster** |
| Top rated products | 100 products | 2.546 | 9.979 | 2.079 | 37.480 | 4522.942 | 67.417 | **14.7x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.255 | 500 |
| Filter by category (data→json) | 5.914 | 500 |
| Complex transformation (data handle) | 28.165 | 500 |
| Complex transformation (data→json) | 24.029 | 500 |
| Aggregate (data handle) | 5.200 | 500 |
| Aggregate (data→json) | 5.355 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**4.9x faster**)
- Array Operations (**3.7x faster**)
- Complex Transformations (**7.6x faster**)
- Deep Nesting (**2.8x faster**)
- String Operations (**6.5x faster**)
- Higher-Order Functions (**12.0x faster**)
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

- **Date:** 2026-09-23
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
