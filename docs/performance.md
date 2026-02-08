# Performance

jsonatapy is a high-performance Rust implementation with Python bindings, designed to be significantly faster than JavaScript-based alternatives for typical use cases.

## Benchmark Results

Latest benchmarks run on 2026-02-07. Comparing jsonatapy against the reference JavaScript implementation.

### Summary

| Category | Average Speedup vs JS |
|----------|----------------------|
| Simple Paths | 6.2x faster |
| Arithmetic Operations | 10.1x faster |
| String Operations | 8.0x faster |
| Array Aggregations | ~1x (comparable) |
| Object Transformations | 3.5x faster |

### Detailed Results

#### Simple Path Access

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Simple path (`user.name`) | tiny | 0.005 | 0.029 | 6.2x |
| Deep path (5 levels) | tiny | 0.007 | 0.026 | 3.6x |
| Array index access | 100 elements | 0.016 | 0.015 | 1.0x |

#### Arithmetic and Logic

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| Arithmetic (`price * quantity`) | tiny | 0.004 | 0.037 | 10.1x |
| Comparison operators | tiny | 0.005 | 0.032 | 6.4x |
| Boolean logic | tiny | 0.006 | 0.030 | 5.0x |

#### Array Operations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| `$sum(values)` | 100 elements | 0.003 | 0.003 | 1.1x |
| `$max(values)` | 100 elements | 0.003 | 0.003 | 1.1x |
| `$count(values)` | 100 elements | 0.006 | 0.006 | 1.1x |
| Array filter | 100 elements | 0.015 | 0.012 | 0.8x |
| Array map | 100 elements | 0.018 | 0.014 | 0.8x |

#### String Operations

| Operation | Data Size | jsonatapy (ms) | JavaScript (ms) | Speedup |
|-----------|-----------|----------------|-----------------|---------|
| String concatenation | tiny | 0.004 | 0.032 | 8.0x |
| String functions | tiny | 0.006 | 0.028 | 4.7x |

## Comparison with Other Implementations

| Implementation | Language | Relative Performance |
|----------------|----------|---------------------|
| **jsonatapy** | Rust + Python | Baseline (fastest) |
| jsonata-js | JavaScript | 0.2x - 10x slower (varies by operation) |
| jsonata-python | Python wrapper | Not tested |
| jsonata-rs | Rust | Not tested |

## Performance Characteristics

jsonatapy excels at:
- Simple path queries
- Arithmetic and logical operations
- String manipulation
- Object transformations

Comparable performance on:
- Array aggregation functions
- Large array operations

## Notes

- Benchmarks run on Ubuntu Linux with Python 3.12
- JavaScript benchmarks use Node.js v20+
- Times shown are per operation in milliseconds
- "Speedup" shows how many times faster jsonatapy is compared to JavaScript
- Values less than 1.0 indicate JavaScript is faster for that specific operation
