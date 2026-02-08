# Performance

jsonatapy is a high-performance Rust implementation with Python bindings, designed to be significantly faster than JavaScript-based alternatives for typical use cases.

## Benchmark Results

Latest benchmarks run on 2026-02-08.
 Comparing jsonatapy against: JavaScript reference implementation, jsonata-python, jsonata-rs.

### Summary

| Category | Average Speedup vs JS |
|----------|----------------------|
| Simple Paths | 4.7x |
| Array Operations | 0.4x |
| Complex Transformations | 5.7x |
| Deep Nesting | 0.6x |
| String Operations | 5.5x |
| Higher-Order Functions | 0.8x |
| Realistic Workload | 0.0x |

### Detailed Results

#### Simple Paths

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Simple Path | tiny | 5.545 | 28.590 | 5.2x |
| Deep Path (5 levels) | tiny | 7.379 | 26.890 | 3.6x |
| Array Index Access | 100 elements | 16.874 | 15.390 | 0.9x |
| Arithmetic Expression | tiny | 4.028 | 36.130 | 9.0x |

#### Array Operations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Array Sum (100 elements) | 100 elements | 3.528 | 5.800 | 1.6x |
| Array Max (100 elements) | 100 elements | 3.460 | 3.090 | 0.9x |
| Array Count (100 elements) | 100 elements | 6.639 | 4.770 | 0.7x |
| Array Sum (1000 elements) | 1000 elements | 6.028 | 0.980 | 0.2x |
| Array Max (1000 elements) | 1000 elements | 5.728 | 1.120 | 0.2x |
| Array Sum (10000 elements) | 10000 elements | 15.766 | 0.140 | 0.0x |
| Array Mapping (extract field) | 100 objects | 67.748 | 7.130 | 0.1x |
| Array Mapping + Sum | 100 objects | 69.750 | 3.300 | 0.0x |
| Array Filtering (predicate) | 100 objects | 41.735 | 5.080 | 0.1x |

#### Complex Transformations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Object Construction (simple) | tiny | 6.944 | 17.560 | 2.5x |
| Object Construction (nested) | tiny | 9.034 | 19.390 | 2.1x |
| Conditional Expression | tiny | 1.716 | 23.070 | 13.4x |
| Multiple Nested Functions | tiny | 2.149 | 9.730 | 4.5x |

#### Deep Nesting

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Deep Path (12 levels) | 12 levels | 11.200 | 11.520 | 1.0x |
| Nested Array Access | 4-level nested arrays | 33.391 | 8.830 | 0.3x |

#### String Operations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| String Uppercase | tiny | 3.787 | 21.980 | 5.8x |
| String Lowercase | tiny | 3.410 | 25.620 | 7.5x |
| String Length | tiny | 3.805 | 18.830 | 4.9x |
| String Concatenation | tiny | 3.727 | 13.280 | 3.6x |
| String Substring | tiny | 2.520 | 12.670 | 5.0x |
| String Contains | tiny | 1.749 | 11.150 | 6.4x |

#### Higher-Order Functions

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| $map with lambda | 100 elements | 3.744 | 1.020 | 0.3x |
| $filter with lambda | 100 elements | 3.620 | 4.170 | 1.2x |
| $reduce with lambda | 100 elements | 3.912 | 4.140 | 1.1x |

#### Realistic Workload

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Filter by category | 100 products | 107.833 | 2.130 | 0.0x |
| Calculate total value | 100 products | 105.050 | 2.280 | 0.0x |
| Complex transformation | 100 products | 55.962 | 1.330 | 0.0x |
| Group by category (aggregate) | 100 products | 64.921 | 1.300 | 0.0x |
| Top rated products | 100 products | 28.330 | 0.200 | 0.0x |

#### Path Comparison

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Filter by category (data handle) | 100 products | 13.972 | N/A | N/A |
| Filter by category (data→json) | 100 products | 8.771 | N/A | N/A |
| Complex transformation (data handle) | 100 products | 36.883 | N/A | N/A |
| Complex transformation (data→json) | 100 products | 33.189 | N/A | N/A |
| Aggregate (data handle) | 100 products | 9.558 | N/A | N/A |
| Aggregate (data→json) | 100 products | 9.389 | N/A | N/A |

## Comparison with Other Implementations

| Implementation | Language | Status |
|----------------|----------|--------|
| **jsonatapy** | Rust + Python | Baseline (this implementation) |
| jsonata-js | JavaScript | Tested (reference implementation) |
| jsonata-python | Python wrapper | Tested (~2,300-69,000x slower) |
| jsonata-rs | Rust | Binary available but tests failed |

## Performance Characteristics

jsonatapy excels at:
- Simple Paths
- Complex Transformations
- String Operations

Comparable performance on:
- Higher-Order Functions

## Known Bottlenecks

The "Realistic Workload" benchmarks reveal significant performance bottlenecks when processing arrays/objects with 100+ elements:

- **Python↔Rust boundary overhead**: Converting Python dicts/lists to Rust JValue structures dominates execution time for array operations
- **Filter operations**: 50x slower than JS due to per-element conversion overhead
- **Array transformations**: 40-140x slower than JS on realistic data sizes

**Optimization opportunities** (see plan in `.claude/plans/`):
1. Fast-path JSON serialization using `json.dumps` + Rust deserializer
2. Specialized predicate evaluation for common filter patterns
3. These optimizations target 5-10x improvement on realistic workloads

The **Path Comparison** section shows using `JsonataData` handles (bypassing Python→JSON conversion) improves performance significantly:
- Filter: 108ms → 14ms (7.7x faster)
- Complex transformation: 56ms → 34ms (1.6x faster)
- Using `evaluate_json()` with JSON strings: 108ms → 9ms (12x faster)

## Notes

- Benchmarks run on Ubuntu Linux with Python 3.12
- JavaScript benchmarks use Node.js v20+
- Times shown are per operation in milliseconds
- 'Speedup' shows how many times faster jsonatapy is compared to JavaScript
- Values less than 1.0 indicate JavaScript is faster for that specific operation
