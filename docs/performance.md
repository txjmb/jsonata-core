# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.17 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.17 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.17 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
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
| Simple Paths | **5.5x faster** |
| Array Operations | **3.6x faster** |
| Complex Transformations | **8.2x faster** |
| Deep Nesting | **3.3x faster** |
| String Operations | **6.8x faster** |
| Higher-Order Functions | **12.4x faster** |
| Realistic Workload | **9.6x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.420 | 4.572 | 0.671 | 16.640 | 1830.434 | 61.417 | **4.9x faster** |
| Deep Path (5 levels) | tiny | 4.307 | 6.865 | 1.230 | 25.820 | 3023.537 | 71.930 | **6.0x faster** |
| Array Index Access | 100 elements | 5.281 | 9.305 | 1.484 | 11.310 | 941.115 | 97.349 | **2.1x faster** |
| Arithmetic Expression | tiny | 2.477 | 3.906 | 0.725 | 21.960 | 2615.547 | 56.569 | **8.9x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.582 | 2.400 | 0.769 | 6.790 | 305.269 | 20.105 | **4.3x faster** |
| Array Max (100 elements) | 100 elements | 1.352 | 2.156 | 0.528 | 6.210 | 298.389 | 20.040 | **4.6x faster** |
| Array Count (100 elements) | 100 elements | 2.121 | 3.746 | 0.529 | 8.800 | 530.572 | 39.202 | **4.1x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.511 | 4.047 | 1.054 | 4.710 | 179.825 | 28.474 | **1.9x faster** |
| Array Max (1000 elements) | 1000 elements | 2.014 | 3.553 | 0.555 | 3.890 | 167.244 | 28.529 | **1.9x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.189 | 10.320 | 2.517 | 8.920 | 354.435 | 70.406 | **1.4x faster** |
| Array Mapping (extract field) | 100 objects | 7.945 | 26.944 | 1.297 | 21.470 | 2684.371 | 207.013 | **2.7x faster** |
| Array Mapping + Sum | 100 objects | 7.598 | 26.112 | 2.020 | 24.770 | 2986.701 | 207.764 | **3.3x faster** |
| Array Filtering (predicate) | 100 objects | 6.450 | 17.054 | 1.964 | 51.350 | 6762.573 | 107.578 | **8.0x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 4.006 | 3.819 | 3.012 | 27.090 | 2544.607 | 33.898 | **6.8x faster** |
| Object Construction (nested) | tiny | 6.093 | 5.119 | 4.399 | 33.230 | 2905.528 | 36.558 | **5.5x faster** |
| Conditional Expression | tiny | 1.108 | 1.703 | 0.401 | 14.200 | 1358.599 | 26.267 | **12.8x faster** |
| Multiple Nested Functions | tiny | 2.548 | 2.937 | 2.672 | 19.420 | 1784.479 | 27.753 | **7.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 4.561 | 6.514 | 1.001 | 25.940 | 2926.694 | 55.045 | **5.7x faster** |
| Nested Array Access | 4-level nested arrays | 8.724 | 13.454 | 0.639 | 7.880 | 612.405 | 107.809 | 1.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.938 | 4.656 | 3.289 | 22.810 | 2404.207 | 52.890 | **5.8x faster** |
| String Lowercase | tiny | 3.936 | 4.673 | 3.294 | 22.360 | 2400.864 | 53.159 | **5.7x faster** |
| String Length | tiny | 3.601 | 4.403 | 2.693 | 24.220 | 2605.268 | 54.536 | **6.7x faster** |
| String Concatenation | tiny | 3.092 | 3.244 | 2.831 | 25.810 | 1927.341 | 30.080 | **8.3x faster** |
| String Substring | tiny | 2.836 | 3.269 | 3.006 | 19.590 | 1687.334 | 28.360 | **6.9x faster** |
| String Contains | tiny | 2.127 | 2.631 | 1.658 | 16.240 | 1419.828 | 28.078 | **7.6x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.600 | 1.761 | 1.509 | 23.680 | 2696.190 | 7.728 | **14.8x faster** |
| $filter with lambda | 100 elements | 1.579 | 1.766 | 1.477 | 23.470 | 2697.945 | 7.533 | **14.9x faster** |
| $reduce with lambda | 100 elements | 3.036 | 3.197 | 3.106 | 22.620 | 2709.336 | 8.192 | **7.5x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.707 | 42.471 | 2.203 | 51.460 | 7470.703 | 315.936 | **6.7x faster** |
| Calculate total value | 100 products | 7.795 | 41.480 | 4.230 | 38.810 | 5171.128 | 316.036 | **5.0x faster** |
| Complex transformation | 100 products | 14.629 | 24.209 | 8.477 | 86.040 | 9347.543 | 136.603 | **5.9x faster** |
| Group by category (aggregate) | 100 products | 11.504 | 24.100 | 7.853 | 87.630 | N/A | 134.238 | **7.6x faster** |
| Top rated products | 100 products | 1.606 | 9.256 | 1.038 | 36.560 | 4537.427 | 67.151 | **22.8x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 11.714 | 500 |
| Filter by category (data→json) | 5.639 | 500 |
| Complex transformation (data handle) | 27.740 | 500 |
| Complex transformation (data→json) | 23.641 | 500 |
| Aggregate (data handle) | 4.736 | 500 |
| Aggregate (data→json) | 4.699 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.5x faster**)
- Array Operations (**3.6x faster**)
- Complex Transformations (**8.2x faster**)
- Deep Nesting (**3.3x faster**)
- String Operations (**6.8x faster**)
- Higher-Order Functions (**12.4x faster**)
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
