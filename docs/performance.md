# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.9 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.9 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-08-30.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **4.9x faster** |
| Array Operations | **3.7x faster** |
| Complex Transformations | **7.6x faster** |
| Deep Nesting | **2.8x faster** |
| String Operations | **6.5x faster** |
| Higher-Order Functions | **11.9x faster** |
| Realistic Workload | **8.3x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.705 | 4.786 | 17.050 | 1834.458 | 62.221 | **4.6x faster** |
| Deep Path (5 levels) | tiny | 5.457 | 6.891 | 25.540 | 3017.851 | 72.612 | **4.7x faster** |
| Array Index Access | 100 elements | 4.345 | 8.419 | 11.180 | 939.902 | 86.993 | **2.6x faster** |
| Arithmetic Expression | tiny | 2.842 | 4.039 | 22.480 | 2609.409 | 57.034 | **7.9x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.502 | 2.313 | 6.880 | 304.604 | 18.076 | **4.6x faster** |
| Array Max (100 elements) | 100 elements | 1.247 | 2.060 | 6.440 | 297.723 | 18.031 | **5.2x faster** |
| Array Count (100 elements) | 100 elements | 1.960 | 3.611 | 8.670 | 529.518 | 35.176 | **4.4x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.285 | 3.836 | 4.680 | 178.638 | 24.276 | **2.0x faster** |
| Array Max (1000 elements) | 1000 elements | 1.787 | 3.337 | 3.850 | 167.090 | 24.276 | **2.2x faster** |
| Array Sum (10000 elements) | 10000 elements | 5.587 | 9.983 | 8.850 | 355.078 | 59.618 | **1.6x faster** |
| Array Mapping (extract field) | 100 objects | 8.912 | 25.835 | 21.600 | 2682.224 | 207.355 | **2.4x faster** |
| Array Mapping + Sum | 100 objects | 8.698 | 25.157 | 24.730 | 2986.538 | 208.144 | **2.8x faster** |
| Array Filtering (predicate) | 100 objects | 6.236 | 16.734 | 50.510 | 6762.662 | 107.826 | **8.1x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.961 | 3.693 | 27.110 | 2540.786 | 34.202 | **6.8x faster** |
| Object Construction (nested) | tiny | 5.484 | 4.564 | 33.440 | 2906.557 | 37.007 | **6.1x faster** |
| Conditional Expression | tiny | 1.280 | 1.769 | 13.660 | 1352.130 | 26.479 | **10.7x faster** |
| Multiple Nested Functions | tiny | 2.755 | 2.942 | 19.080 | 1778.242 | 27.971 | **6.9x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 5.738 | 6.683 | 25.760 | 2920.553 | 55.548 | **4.5x faster** |
| Nested Array Access | 4-level nested arrays | 6.891 | 12.844 | 7.890 | 611.757 | 108.345 | **1.1x faster** |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.059 | 4.670 | 22.480 | 2400.899 | 53.552 | **5.5x faster** |
| String Lowercase | tiny | 4.055 | 4.704 | 22.270 | 2401.560 | 53.764 | **5.5x faster** |
| String Length | tiny | 3.742 | 4.556 | 24.400 | 2604.379 | 55.135 | **6.5x faster** |
| String Concatenation | tiny | 3.314 | 3.226 | 26.160 | 1927.662 | 30.321 | **7.9x faster** |
| String Substring | tiny | 2.949 | 3.220 | 19.460 | 1688.168 | 28.631 | **6.6x faster** |
| String Contains | tiny | 2.287 | 2.664 | 16.040 | 1417.707 | 28.421 | **7.0x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.693 | 1.846 | 23.390 | 2689.302 | 7.222 | **13.8x faster** |
| $filter with lambda | 100 elements | 1.714 | 1.858 | 23.570 | 2693.158 | 7.125 | **13.8x faster** |
| $reduce with lambda | 100 elements | 2.763 | 2.927 | 22.640 | 2704.402 | 7.805 | **8.2x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 8.673 | 41.451 | 51.750 | 7466.338 | 316.613 | **6.0x faster** |
| Calculate total value | 100 products | 7.953 | 40.473 | 37.500 | 5172.275 | 315.575 | **4.7x faster** |
| Complex transformation | 100 products | 14.243 | 23.842 | 86.030 | 9327.921 | 136.365 | **6.0x faster** |
| Group by category (aggregate) | 100 products | 8.938 | 22.008 | 88.460 | N/A | 134.507 | **9.9x faster** |
| Top rated products | 100 products | 2.512 | 10.184 | 37.430 | 4538.419 | 67.470 | **14.9x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.299 | 500 |
| Filter by category (data→json) | 5.904 | 500 |
| Complex transformation (data handle) | 28.120 | 500 |
| Complex transformation (data→json) | 23.661 | 500 |
| Aggregate (data handle) | 5.208 | 500 |
| Aggregate (data→json) | 5.291 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**4.9x faster**)
- Array Operations (**3.7x faster**)
- Complex Transformations (**7.6x faster**)
- Deep Nesting (**2.8x faster**)
- String Operations (**6.5x faster**)
- Higher-Order Functions (**11.9x faster**)
- Realistic Workload (**8.3x faster**)

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

- **Date:** 2026-08-30
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
