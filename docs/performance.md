# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.16 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.16 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.16 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.2.2 | Reference implementation (Node.js v20.20.2) |
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
| Simple Paths | **5.4x faster** |
| Array Operations | **3.6x faster** |
| Complex Transformations | **8.1x faster** |
| Deep Nesting | **3.2x faster** |
| String Operations | **6.9x faster** |
| Higher-Order Functions | **12.3x faster** |
| Realistic Workload | **9.5x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.463 | 4.618 | 0.662 | 16.870 | 1831.429 | 61.225 | **4.9x faster** |
| Deep Path (5 levels) | tiny | 4.408 | 6.806 | 1.216 | 25.800 | 3015.916 | 71.672 | **5.9x faster** |
| Array Index Access | 100 elements | 5.241 | 9.359 | 1.451 | 11.310 | 940.257 | 96.555 | **2.2x faster** |
| Arithmetic Expression | tiny | 2.457 | 3.923 | 0.713 | 21.870 | 2604.675 | 56.123 | **8.9x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.556 | 2.379 | 0.769 | 6.750 | 304.305 | 20.035 | **4.3x faster** |
| Array Max (100 elements) | 100 elements | 1.327 | 2.143 | 0.527 | 6.180 | 297.935 | 20.031 | **4.7x faster** |
| Array Count (100 elements) | 100 elements | 2.130 | 3.766 | 0.525 | 8.850 | 529.247 | 39.083 | **4.2x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.450 | 3.991 | 1.053 | 4.690 | 179.356 | 28.459 | **1.9x faster** |
| Array Max (1000 elements) | 1000 elements | 1.950 | 3.496 | 0.556 | 3.850 | 166.293 | 28.440 | **2.0x faster** |
| Array Sum (10000 elements) | 10000 elements | 5.995 | 10.257 | 2.521 | 8.890 | 354.246 | 70.499 | **1.5x faster** |
| Array Mapping (extract field) | 100 objects | 8.124 | 26.934 | 1.257 | 21.560 | 2675.784 | 207.640 | **2.7x faster** |
| Array Mapping + Sum | 100 objects | 7.750 | 26.141 | 1.988 | 24.700 | 2982.222 | 207.545 | **3.2x faster** |
| Array Filtering (predicate) | 100 objects | 6.545 | 17.007 | 1.959 | 50.870 | 6745.873 | 107.902 | **7.8x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.990 | 3.828 | 3.005 | 27.200 | 2535.414 | 33.660 | **6.8x faster** |
| Object Construction (nested) | tiny | 6.141 | 5.087 | 4.530 | 33.200 | 2901.811 | 36.441 | **5.4x faster** |
| Conditional Expression | tiny | 1.114 | 1.687 | 0.403 | 14.050 | 1355.437 | 26.095 | **12.6x faster** |
| Multiple Nested Functions | tiny | 2.542 | 2.919 | 2.717 | 19.330 | 1774.822 | 27.607 | **7.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 4.690 | 6.501 | 1.001 | 25.830 | 2919.315 | 55.011 | **5.5x faster** |
| Nested Array Access | 4-level nested arrays | 8.768 | 13.513 | 0.632 | 7.870 | 611.674 | 107.693 | 1.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.941 | 4.647 | 3.278 | 22.620 | 2398.836 | 52.899 | **5.7x faster** |
| String Lowercase | tiny | 3.942 | 4.663 | 3.262 | 22.460 | 2401.295 | 52.749 | **5.7x faster** |
| String Length | tiny | 3.572 | 4.388 | 2.717 | 24.150 | 2596.304 | 54.170 | **6.8x faster** |
| String Concatenation | tiny | 3.070 | 3.190 | 2.827 | 26.030 | 1924.254 | 29.909 | **8.5x faster** |
| String Substring | tiny | 2.817 | 3.225 | 2.965 | 19.450 | 1685.387 | 28.191 | **6.9x faster** |
| String Contains | tiny | 2.115 | 2.530 | 1.647 | 16.160 | 1418.107 | 27.868 | **7.6x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.604 | 1.756 | 1.494 | 23.930 | 2687.222 | 7.661 | **14.9x faster** |
| $filter with lambda | 100 elements | 1.629 | 1.761 | 1.509 | 23.740 | 2690.601 | 7.513 | **14.6x faster** |
| $reduce with lambda | 100 elements | 3.038 | 3.200 | 3.101 | 22.680 | 2702.709 | 8.159 | **7.5x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.835 | 42.804 | 2.065 | 51.830 | 7447.800 | 315.757 | **6.6x faster** |
| Calculate total value | 100 products | 7.828 | 41.876 | 4.295 | 37.730 | 5161.724 | 315.656 | **4.8x faster** |
| Complex transformation | 100 products | 14.723 | 24.317 | 8.679 | 85.510 | 9311.830 | 136.166 | **5.8x faster** |
| Group by category (aggregate) | 100 products | 11.540 | 24.196 | 7.874 | 87.160 | N/A | 133.890 | **7.6x faster** |
| Top rated products | 100 products | 1.641 | 9.340 | 1.077 | 36.930 | 4525.308 | 67.107 | **22.5x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 11.744 | 500 |
| Filter by category (data→json) | 5.652 | 500 |
| Complex transformation (data handle) | 27.552 | 500 |
| Complex transformation (data→json) | 23.392 | 500 |
| Aggregate (data handle) | 4.745 | 500 |
| Aggregate (data→json) | 4.687 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.4x faster**)
- Array Operations (**3.6x faster**)
- Complex Transformations (**8.1x faster**)
- Deep Nesting (**3.2x faster**)
- String Operations (**6.9x faster**)
- Higher-Order Functions (**12.3x faster**)
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

- **Date:** 2026-11-27
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
