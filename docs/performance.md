# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

**These numbers come from a dedicated, self-hosted Mac Mini (Apple Silicon), not a shared cloud CI runner** — single-tenant physical hardware with no other workloads competing for CPU. This matters: single-sample measurements on a shared/virtualized runner were previously noisy enough that identical code, measured twice, swung -66% to +120%. Every number below is also the *minimum* of 5 independent measurement trials per test (not an average) — for CPU-bound microbenchmarks, interference can only make a run slower than the code's true achievable speed, never faster, so the minimum across repeated trials is the best available estimate of that true speed.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.13 | This project (compiled Rust extension via PyO3); data crosses the boundary as Python dicts |
| **jsonatapy** (JSON string I/O) | Rust + Python | 2.2.13 | Same library via `evaluate_json`: data crosses as JSON strings, parsed/serialized by serde per call |
| **jsonata-core** (pure Rust) | Rust | 2.2.13 | This project's engine measured as a Rust library — no Python at all, data pre-parsed, expression pre-compiled (criterion methodology, per table row) |
| **jsonata-js** | JavaScript | 2.2.2 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Python wrapper embedding a JS engine (Duktape) |
| **jsonata-rs** | Rust | 0.3 | Third-party Rust implementation (Stedi's crate — not this project; CLI harness, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-core (pure Rust)** — `Expression::compile(expr)` once, input parsed to a `JValue` once, then `Expression::evaluate(&data)` in an in-process timed loop with warmup — the same methodology as the criterion suite (`benches/`), reported per table row. The gap between this column and the jsonatapy columns *is* the Python boundary cost; the engine is identical.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-10-24.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **5.5x faster** |
| Array Operations | **3.8x faster** |
| Complex Transformations | **8.1x faster** |
| Deep Nesting | **3.4x faster** |
| String Operations | **7.0x faster** |
| Higher-Order Functions | **12.3x faster** |
| Realistic Workload | **9.8x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 3.386 | 4.474 | 0.664 | 16.830 | 1832.398 | 61.919 | **5.0x faster** |
| Deep Path (5 levels) | tiny | 4.278 | 6.443 | 1.145 | 25.570 | 3019.097 | 72.431 | **6.0x faster** |
| Array Index Access | 100 elements | 5.152 | 9.021 | 1.348 | 11.340 | 940.223 | 86.619 | **2.2x faster** |
| Arithmetic Expression | tiny | 2.488 | 3.773 | 0.721 | 22.250 | 2616.748 | 56.676 | **8.9x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.559 | 2.327 | 0.729 | 6.820 | 304.260 | 18.034 | **4.4x faster** |
| Array Max (100 elements) | 100 elements | 1.328 | 2.087 | 0.512 | 6.400 | 298.039 | 18.536 | **4.8x faster** |
| Array Count (100 elements) | 100 elements | 2.174 | 3.741 | 0.485 | 8.790 | 528.277 | 35.076 | **4.0x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.448 | 3.911 | 1.046 | 4.680 | 178.601 | 24.129 | **1.9x faster** |
| Array Max (1000 elements) | 1000 elements | 1.952 | 3.411 | 0.551 | 3.820 | 166.827 | 24.124 | **2.0x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.003 | 9.960 | 2.520 | 8.840 | 354.652 | 59.875 | **1.5x faster** |
| Array Mapping (extract field) | 100 objects | 7.848 | 25.866 | 1.126 | 21.560 | 2685.686 | 205.796 | **2.7x faster** |
| Array Mapping + Sum | 100 objects | 7.555 | 25.179 | 1.813 | 24.710 | 2988.570 | 205.826 | **3.3x faster** |
| Array Filtering (predicate) | 100 objects | 5.213 | 16.577 | 1.855 | 50.700 | 6747.586 | 107.142 | **9.7x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 3.878 | 3.703 | 2.942 | 27.410 | 2541.370 | 34.027 | **7.1x faster** |
| Object Construction (nested) | tiny | 6.059 | 4.882 | 4.234 | 33.450 | 2909.645 | 36.949 | **5.5x faster** |
| Conditional Expression | tiny | 1.114 | 1.632 | 0.404 | 13.660 | 1357.382 | 26.370 | **12.3x faster** |
| Multiple Nested Functions | tiny | 2.550 | 2.898 | 2.534 | 19.380 | 1777.184 | 27.908 | **7.6x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 4.431 | 6.140 | 0.986 | 25.680 | 2923.662 | 55.351 | **5.8x faster** |
| Nested Array Access | 4-level nested arrays | 8.446 | 13.137 | 0.605 | 7.990 | 612.404 | 107.348 | 1.1x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 3.881 | 4.475 | 3.042 | 22.840 | 2397.208 | 53.416 | **5.9x faster** |
| String Lowercase | tiny | 3.890 | 4.499 | 3.045 | 22.470 | 2396.763 | 53.517 | **5.8x faster** |
| String Length | tiny | 3.609 | 4.321 | 2.456 | 24.520 | 2597.458 | 54.815 | **6.8x faster** |
| String Concatenation | tiny | 3.019 | 3.109 | 2.749 | 25.540 | 1924.715 | 30.334 | **8.5x faster** |
| String Substring | tiny | 2.798 | 3.121 | 2.892 | 19.510 | 1684.446 | 28.565 | **7.0x faster** |
| String Contains | tiny | 2.063 | 2.459 | 1.562 | 16.130 | 1416.221 | 28.293 | **7.8x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.596 | 1.737 | 1.485 | 23.550 | 2687.759 | 7.305 | **14.8x faster** |
| $filter with lambda | 100 elements | 1.623 | 1.725 | 1.473 | 23.540 | 2692.735 | 7.163 | **14.5x faster** |
| $reduce with lambda | 100 elements | 3.032 | 3.185 | 3.077 | 22.970 | 2700.852 | 7.783 | **7.6x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (json I/O) | jsonata-core (pure Rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|----------------------|--------------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 7.508 | 41.289 | 2.061 | 51.620 | 7456.994 | 314.265 | **6.9x faster** |
| Calculate total value | 100 products | 7.661 | 40.274 | 4.200 | 37.780 | 5171.325 | 314.276 | **4.9x faster** |
| Complex transformation | 100 products | 14.458 | 23.448 | 8.537 | 86.910 | 9339.702 | 135.515 | **6.0x faster** |
| Group by category (aggregate) | 100 products | 11.501 | 23.311 | 7.709 | 88.470 | N/A | 133.051 | **7.7x faster** |
| Top rated products | 100 products | 1.611 | 9.086 | 1.059 | 37.570 | 4533.237 | 66.951 | **23.3x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 11.736 | 500 |
| Filter by category (data→json) | 6.258 | 500 |
| Complex transformation (data handle) | 26.929 | 500 |
| Complex transformation (data→json) | 22.905 | 500 |
| Aggregate (data handle) | 4.634 | 500 |
| Aggregate (data→json) | 4.607 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**5.5x faster**)
- Array Operations (**3.8x faster**)
- Complex Transformations (**8.1x faster**)
- Deep Nesting (**3.4x faster**)
- String Operations (**7.0x faster**)
- Higher-Order Functions (**12.3x faster**)
- Realistic Workload (**9.8x faster**)

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

- **Date:** 2026-10-24
- **Platform:** GitHub Actions (self-hosted Michaels-Mini, physical/dedicated hardware, macOS ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
