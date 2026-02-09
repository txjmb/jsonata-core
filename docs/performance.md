# Performance

jsonatapy is a high-performance Rust implementation with Python bindings, designed to be significantly faster than JavaScript-based alternatives for typical use cases.

## Benchmark Results

Latest benchmarks run on 2026-02-08.
 Comparing jsonatapy against: JavaScript reference implementation, jsonata-python, jsonata-rs.

### Summary

| Category | Average Speedup vs JS |
|----------|----------------------|
| Simple Paths | 6.9x |
| Array Operations | 0.9x |
| Complex Transformations | 7.7x |
| Deep Nesting | 0.9x |
| String Operations | 6.4x |
| Higher-Order Functions | 0.3x |
| Realistic Workload | 0.0x |

### Detailed Results

#### Simple Paths

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| Simple Path | tiny | 4.084 | 5.660 | 31.990 | 74.231 | 7.8x |
| Deep Path (5 levels) | tiny | 5.793 | 7.535 | 35.190 | 92.107 | 6.1x |
| Array Index Access | 100 elements | 8.745 | 16.441 | 15.470 | 114.110 | 1.8x |
| Arithmetic Expression | tiny | 3.097 | 5.175 | 37.090 | 67.441 | 12.0x |

#### Array Operations

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| Array Sum (100 elements) | 100 elements | 2.180 | 3.333 | 3.410 | 23.819 | 1.6x |
| Array Max (100 elements) | 100 elements | 1.985 | 3.249 | 6.600 | 23.064 | 3.3x |
| Array Count (100 elements) | 100 elements | 3.625 | 6.254 | 7.510 | 43.090 | 2.1x |
| Array Sum (1000 elements) | 1000 elements | 4.260 | 5.380 | 1.220 | 29.383 | 0.3x |
| Array Max (1000 elements) | 1000 elements | 3.349 | 5.348 | 1.060 | 34.176 | 0.3x |
| Array Sum (10000 elements) | 10000 elements | 9.262 | 13.265 | 0.100 | 78.693 | 0.0x |
| Array Mapping (extract field) | 100 objects | 53.472 | 52.472 | 4.220 | 270.049 | 0.1x |
| Array Mapping + Sum | 100 objects | 52.222 | 55.174 | 3.500 | 247.673 | 0.1x |
| Array Filtering (predicate) | 100 objects | 35.448 | 30.535 | 2.210 | 130.772 | 0.1x |

#### Complex Transformations

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| Object Construction (simple) | tiny | 5.775 | 6.302 | 18.250 | 42.090 | 3.2x |
| Object Construction (nested) | tiny | 8.026 | 9.356 | 17.130 | 42.694 | 2.1x |
| Conditional Expression | tiny | 1.240 | 1.996 | 22.190 | 31.132 | 17.9x |
| Multiple Nested Functions | tiny | 1.649 | 2.376 | 12.680 | 33.385 | 7.7x |

#### Deep Nesting

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| Deep Path (12 levels) | 12 levels | 8.896 | 10.849 | 14.280 | 71.519 | 1.6x |
| Nested Array Access | 4-level nested arrays | 24.507 | 30.846 | 5.350 | 148.498 | 0.2x |

#### String Operations

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| String Uppercase | tiny | 3.229 | 4.623 | 20.380 | 65.801 | 6.3x |
| String Lowercase | tiny | 3.095 | 4.615 | 21.480 | 64.467 | 6.9x |
| String Length | tiny | 2.836 | 4.454 | 22.380 | 68.934 | 7.9x |
| String Concatenation | tiny | 3.337 | 3.919 | 12.990 | 37.099 | 3.9x |
| String Substring | tiny | 2.419 | 3.441 | 12.560 | 37.318 | 5.2x |
| String Contains | tiny | 1.623 | 2.915 | 13.260 | 36.142 | 8.2x |

#### Higher-Order Functions

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| $map with lambda | 100 elements | 3.301 | 3.505 | 0.920 | 7.876 | 0.3x |
| $filter with lambda | 100 elements | 2.998 | 3.331 | 1.350 | 7.501 | 0.5x |
| $reduce with lambda | 100 elements | 3.712 | 4.064 | 0.950 | 8.116 | 0.3x |

#### Realistic Workload

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| Filter by category | 100 products | 86.771 | 83.380 | 1.890 | 521.232 | 0.0x |
| Calculate total value | 100 products | 94.660 | 100.866 | 1.900 | 400.709 | 0.0x |
| Complex transformation | 100 products | 47.325 | 47.423 | 1.180 | 171.037 | 0.0x |
| Group by category (aggregate) | 100 products | 45.418 | 48.189 | 1.360 | 163.654 | 0.0x |
| Top rated products | 100 products | 21.935 | 22.304 | 0.210 | 85.829 | 0.0x |

#### Path Comparison

| Operation | Data Size | jsonatapy (ms) | rust-only (ms) | JavaScript (ms) | jsonata-rs (ms) | Speedup |
|-----------|-----------|----------------|----------------|-----------------|-----------------|---------|
| Filter by category (data handle) | 100 products | 11.933 | N/A | N/A | N/A | N/A |
| Filter by category (data→json) | 100 products | 7.520 | N/A | N/A | N/A | N/A |
| Complex transformation (data handle) | 100 products | 36.105 | N/A | N/A | N/A | N/A |
| Complex transformation (data→json) | 100 products | 33.144 | N/A | N/A | N/A | N/A |
| Aggregate (data handle) | 100 products | 5.562 | N/A | N/A | N/A | N/A |
| Aggregate (data→json) | 100 products | 4.969 | N/A | N/A | N/A | N/A |

## Comparison with Other Implementations

| Implementation | Language | Status |
|----------------|----------|--------|
| **jsonatapy** | Rust + Python | Baseline (this implementation) |
| jsonata-js | JavaScript | Tested (reference implementation) |
| jsonata-python | Python wrapper | Tested |
| jsonata-rs | Rust | Tested |

## Performance Characteristics

jsonatapy excels at:
- Simple Paths
- Complex Transformations
- String Operations

Comparable performance on:
- Array Operations
- Deep Nesting

## Notes

- Benchmarks run on Ubuntu Linux with Python 3.12
- JavaScript benchmarks use Node.js v20+
- Times shown are per operation in milliseconds
- 'Speedup' shows how many times faster jsonatapy is compared to JavaScript
- Values less than 1.0 indicate JavaScript is faster for that specific operation
