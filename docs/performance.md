# Performance Benchmarks

jsonatapy is a high-performance Rust implementation of JSONata with Python bindings. This page presents benchmark comparisons against other JSONata implementations.

## Implementations Tested

| Implementation | Language | Version | Description |
|----------------|----------|---------|-------------|
| **jsonatapy** | Rust + Python | 2.2.1 | This project (compiled Rust extension via PyO3) |
| **jsonatapy** (rust-only) | Rust + Python | 2.2.1 | Same library, JSON string I/O path (bypasses Python object conversion) |
| **jsonata-js** | JavaScript | 2.1.0 | Reference implementation (Node.js v20.20.2) |
| **jsonata-python** | Python | unknown | Pure Python implementation |
| **jsonata-rs** | Rust | 0.3 | Pure Rust implementation (CLI benchmark, no Python overhead) |

### Methodology: compile-once, evaluate-many

Every implementation below is measured the way a real caller who evaluates the same expression repeatedly would use it, not its slowest possible one-off call:

- **jsonatapy** — `jsonatapy.compile(expr)` once, then `.evaluate(data)` in the timed loop. No further reuse is available; the compiled bytecode is already cached on the expression object.
- **jsonata-js** — `jsonata(expr)` once, then `.evaluate(data)` in the timed loop. Same story: this is already the library's fastest repeated-call path.

- **jsonata-python** — uses its documented `Context` object (`ctx = jsonata.Context()`, then `ctx(expr, data)` in the loop) rather than the one-off `transform()` convenience function. `transform()` re-bootstraps an embedded Duktape engine — reloading the `jsonata.js` library into it — on every single call; reusing a `Context` keeps that engine warm and is the library's own documented path for repeated evaluation. It is *not* a true compile-once equivalent, since `Context.__call__` still re-parses the expression string on every call, so some of the remaining gap to jsonatapy/jsonata-js is real parsing cost this library doesn't let a caller amortize away.


Benchmarks run on 2026-07-08.

## Summary by Category

| Category | jsonatapy vs JS |
|----------|----------------|
| Simple Paths | **3.0x faster** |
| Array Operations | **2.0x faster** |
| Complex Transformations | **5.6x faster** |
| Deep Nesting | **1.5x faster** |
| String Operations | **5.5x faster** |
| Higher-Order Functions | **9.4x faster** |
| Realistic Workload | **1.5x faster** |

## Detailed Results

### Simple Paths

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Simple Path | tiny | 6.191 | 7.539 | 20.330 | 1837.303 | 62.540 | **3.3x faster** |
| Deep Path (5 levels) | tiny | 8.738 | 10.540 | 23.230 | 3019.426 | 72.417 | **2.7x faster** |
| Array Index Access | 100 elements | 6.666 | 11.595 | 9.710 | 964.753 | 87.202 | **1.5x faster** |
| Arithmetic Expression | tiny | 4.290 | 6.100 | 19.680 | 2616.865 | 56.848 | **4.6x faster** |

### Array Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Array Sum (100 elements) | 100 elements | 1.699 | 2.692 | 6.810 | 308.909 | 18.025 | **4.0x faster** |
| Array Max (100 elements) | 100 elements | 1.447 | 2.439 | 5.610 | 301.938 | 17.915 | **3.9x faster** |
| Array Count (100 elements) | 100 elements | 2.666 | 4.678 | 7.910 | 534.835 | 35.148 | **3.0x faster** |
| Array Sum (1000 elements) | 1000 elements | 2.791 | 4.307 | 4.520 | 180.493 | 24.616 | **1.6x faster** |
| Array Max (1000 elements) | 1000 elements | 2.291 | 3.804 | 3.700 | 168.879 | 24.484 | **1.6x faster** |
| Array Sum (10000 elements) | 10000 elements | 6.748 | 10.031 | 8.800 | 355.112 | 60.133 | **1.3x faster** |
| Array Mapping (extract field) | 100 objects | 42.465 | 37.934 | 18.540 | 2681.884 | 212.013 | 2.3x slower |
| Array Mapping + Sum | 100 objects | 41.857 | 36.865 | 21.910 | 2987.879 | 212.143 | 1.9x slower |
| Array Filtering (predicate) | 100 objects | 30.325 | 22.648 | 44.520 | 6756.098 | 110.885 | **1.5x faster** |

### Complex Transformations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Object Construction (simple) | tiny | 5.571 | 5.845 | 24.760 | 2555.121 | 34.140 | **4.4x faster** |
| Object Construction (nested) | tiny | 7.678 | 7.389 | 30.710 | 2915.911 | 36.828 | **4.0x faster** |
| Conditional Expression | tiny | 1.752 | 2.585 | 11.820 | 1359.944 | 26.296 | **6.7x faster** |
| Multiple Nested Functions | tiny | 2.468 | 3.333 | 17.720 | 1785.198 | 27.909 | **7.2x faster** |

### Deep Nesting

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Deep Path (12 levels) | 12 levels | 9.314 | 9.850 | 23.500 | 2934.038 | 55.456 | **2.5x faster** |
| Nested Array Access | 4-level nested arrays | 17.116 | 20.388 | 6.500 | 612.842 | 108.141 | 2.6x slower |

### String Operations

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| String Uppercase | tiny | 4.525 | 5.967 | 20.410 | 2414.192 | 53.689 | **4.5x faster** |
| String Lowercase | tiny | 4.574 | 5.988 | 19.840 | 2414.158 | 53.818 | **4.3x faster** |
| String Length | tiny | 4.248 | 5.641 | 22.520 | 2691.569 | 55.393 | **5.3x faster** |
| String Concatenation | tiny | 3.692 | 4.328 | 24.220 | 1931.206 | 30.226 | **6.6x faster** |
| String Substring | tiny | 3.184 | 4.221 | 17.750 | 1695.038 | 28.617 | **5.6x faster** |
| String Contains | tiny | 2.138 | 3.185 | 14.710 | 1429.845 | 28.437 | **6.9x faster** |

### Higher-Order Functions

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| $map with lambda | 100 elements | 1.943 | 2.135 | 21.560 | 2782.357 | 7.128 | **11.1x faster** |
| $filter with lambda | 100 elements | 1.917 | 2.110 | 21.670 | 2697.181 | 7.090 | **11.3x faster** |
| $reduce with lambda | 100 elements | 3.654 | 3.852 | 20.740 | 2708.791 | 7.748 | **5.7x faster** |

### Realistic Workload

| Operation | Data Size | jsonatapy | jsonatapy (rust) | jsonata-js | jsonata-python | jsonata-rs | vs JS |
|-----------|-----------|-----------|------------------|------------|----------------|------------|-------|
| Filter by category | 100 products | 75.529 | 57.725 | 47.690 | 7470.772 | 323.064 | 1.6x slower |
| Calculate total value | 100 products | 69.550 | 57.666 | 34.090 | 5173.942 | 323.379 | 2.0x slower |
| Complex transformation | 100 products | 38.756 | 32.623 | 79.990 | 9345.108 | 139.019 | **2.1x faster** |
| Group by category (aggregate) | 100 products | 35.152 | 30.562 | 82.700 | N/A | 137.042 | **2.4x faster** |
| Top rated products | 100 products | 17.645 | 13.645 | 35.040 | 4554.272 | 68.579 | **2.0x faster** |

### Path Comparison

| Operation | jsonatapy (ms) | Iterations |
|-----------|---------------|------------|
| Filter by category (data handle) | 12.433 | 500 |
| Filter by category (data→json) | 5.819 | 500 |
| Complex transformation (data handle) | 34.385 | 500 |
| Complex transformation (data→json) | 28.735 | 500 |
| Aggregate (data handle) | 6.293 | 500 |
| Aggregate (data→json) | 6.322 | 500 |

## Performance Characteristics

**Faster than JavaScript:**

- Simple Paths (**3.0x faster**)
- Array Operations (**2.0x faster**)
- Complex Transformations (**5.6x faster**)
- Deep Nesting (**1.5x faster**)
- String Operations (**5.5x faster**)
- Higher-Order Functions (**9.4x faster**)
- Realistic Workload (**1.5x faster**)

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

- **Date:** 2026-07-08
- **Platform:** GitHub Actions (macOS, ARM64)
- **Python:** 3.14.6
- **Node.js:** v20.20.2
- All times are total wall-clock time for the stated number of iterations
- Each benchmark includes a warmup phase before measurement
- 'vs JS' column shows jsonatapy speedup relative to the JavaScript reference implementation
- Values > 1x mean jsonatapy is faster; < 1x means JavaScript is faster
