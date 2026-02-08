# Performance

jsonatapy is a high-performance Rust implementation with Python bindings, designed to be significantly faster than JavaScript-based alternatives for typical use cases.

## Benchmark Results

Latest benchmarks run on 2026-02-07.
 Comparing jsonatapy against: JavaScript reference implementation.

### Summary

| Category | Average Speedup vs JS |
|----------|----------------------|
| Simple Paths | 5.2x |
| Array Operations | 0.4x |
| Complex Transformations | 6.2x |
| Deep Nesting | 1.1x |
| String Operations | 5.3x |
| Higher-Order Functions | 0.3x |
| Realistic Workload | 0.0x |

### Detailed Results

#### Simple Paths

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Simple Path | tiny | 4.620 | 28.750 | 6.2x |
| Deep Path (5 levels) | tiny | 7.139 | 25.560 | 3.6x |
| Array Index Access | 100 elements | 15.680 | 15.190 | 1.0x |
| Arithmetic Expression | tiny | 3.718 | 37.410 | 10.1x |

#### Array Operations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Array Sum (100 elements) | 100 elements | 3.188 | 2.860 | 0.9x |
| Array Max (100 elements) | 100 elements | 3.152 | 2.950 | 0.9x |
| Array Count (100 elements) | 100 elements | 6.087 | 5.580 | 0.9x |
| Array Sum (1000 elements) | 1000 elements | 5.794 | 0.940 | 0.2x |
| Array Max (1000 elements) | 1000 elements | 5.445 | 1.050 | 0.2x |
| Array Sum (10000 elements) | 10000 elements | 14.342 | 0.140 | 0.0x |
| Array Mapping (extract field) | 100 objects | 51.420 | 8.050 | 0.2x |
| Array Mapping + Sum | 100 objects | 56.496 | 2.840 | 0.1x |
| Array Filtering (predicate) | 100 objects | 33.548 | 2.010 | 0.1x |

#### Complex Transformations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Object Construction (simple) | tiny | 5.755 | 17.020 | 3.0x |
| Object Construction (nested) | tiny | 7.659 | 16.960 | 2.2x |
| Conditional Expression | tiny | 1.541 | 20.240 | 13.1x |
| Multiple Nested Functions | tiny | 2.010 | 13.260 | 6.6x |

#### Deep Nesting

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Deep Path (12 levels) | 12 levels | 8.726 | 15.670 | 1.8x |
| Nested Array Access | 4-level nested arrays | 26.420 | 9.130 | 0.3x |

#### String Operations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| String Uppercase | tiny | 3.543 | 21.040 | 5.9x |
| String Lowercase | tiny | 3.714 | 21.330 | 5.7x |
| String Length | tiny | 3.455 | 20.750 | 6.0x |
| String Concatenation | tiny | 3.664 | 9.110 | 2.5x |
| String Substring | tiny | 2.437 | 12.320 | 5.1x |
| String Contains | tiny | 1.736 | 11.850 | 6.8x |

#### Higher-Order Functions

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| $map with lambda | 100 elements | 3.279 | 0.910 | 0.3x |
| $filter with lambda | 100 elements | 3.457 | 1.420 | 0.4x |
| $reduce with lambda | 100 elements | 4.032 | 1.060 | 0.3x |

#### Realistic Workload

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Filter by category | 100 products | 91.551 | 2.050 | 0.0x |
| Calculate total value | 100 products | 87.814 | 4.630 | 0.1x |
| Complex transformation | 100 products | 46.740 | 4.100 | 0.1x |
| Group by category (aggregate) | 100 products | 56.129 | 1.400 | 0.0x |
| Top rated products | 100 products | 25.183 | 0.210 | 0.0x |

#### Path Comparison

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Filter by category (data handle) | 100 products | 13.670 | N/A | N/A |
| Filter by category (data→json) | 100 products | 8.725 | N/A | N/A |
| Complex transformation (data handle) | 100 products | 33.402 | N/A | N/A |
| Complex transformation (data→json) | 100 products | 30.125 | N/A | N/A |
| Aggregate (data handle) | 100 products | 8.814 | N/A | N/A |
| Aggregate (data→json) | 100 products | 9.257 | N/A | N/A |

## Comparison with Other Implementations

| Implementation | Language | Status |
|----------------|----------|--------|
| **jsonatapy** | Rust + Python | Baseline (this implementation) |
| jsonata-js | JavaScript | Tested (reference implementation) |
| jsonata-python | Python wrapper | Not tested |
| jsonata-rs | Rust | Not tested |

## Performance Characteristics

jsonatapy excels at:
- Simple Paths
- Complex Transformations
- String Operations

Comparable performance on:
- Deep Nesting

## Notes

- Benchmarks run on Ubuntu Linux with Python 3.12
- JavaScript benchmarks use Node.js v20+
- Times shown are per operation in milliseconds
- 'Speedup' shows how many times faster jsonatapy is compared to JavaScript
- Values less than 1.0 indicate JavaScript is faster for that specific operation
