# Java/.NET FFI Benchmark Experiment — Report (2026-07-13)

Spike per docs/superpowers/specs/2026-07-13-java-dotnet-ffi-benchmark-experiment-design.md.
jsonata-core consumed over the C ABI (`capi` feature), JSON-as-string boundary.
Java: JMH 1.37, 1 fork, 3×1s warmup, 5×1s measure. .NET: BenchmarkDotNet 0.15.8, ShortRun job.
Spike-scale iteration counts — treat small (<1.2x) differences as noise.

**Reading the modes:** *s→s* = JSON text in/out for both sides (symmetric).
*Home-turf* = competitor evaluates pre-parsed native objects with no result
serialization (its best realistic case) while core still pays the full string
boundary (its worst realistic case).

## Java — core (FFM) vs dashjoin/jsonata-java 0.9.10

### Correctness gate

32/33 scenarios match. Excluded from aggregates (flagged):

- **Top rated products**: mismatch — ours=[{"id":2,"name":"Product 2","category":"Books","price":21,"inStock":true,"rating":4,"reviews":4,"tags":["tag0","tag1"],"vendor":{"name":"Vendor 2","rating":4.4}},{"id":5,"name":"Product 5","category":... theirs=[{"id":98,"name":"Product 98","category":"Books","price":549,"inStock":true,"rating":4,"reviews":196,"tags":["tag0","tag1","tag2"],"vendor":{"name":"Vendor 8","rating":4.6}},{"id":95,"name":"Product 9...


### Per-scenario results (µs/op, lower is better)

| Scenario | core s→s | comp s→s | speedup | core s→s (compile-each) | comp s→s (compile-each) | speedup | comp home-turf | core-vs-home-turf |
|---|---|---|---|---|---|---|---|---|
| Array Count (100 elements) | 3.14 | 3.56 | 1.13x | 4.41 | 4.92 | 1.12x | 0.48 | 0.15x |
| Array Filtering (predicate) | 55.23 | 42.16 | 0.76x | 62.69 | 43.66 | 0.70x | 12.13 | 0.22x |
| Array Mapping (extract field) | 48.96 | 27.74 | 0.57x | 54.05 | 28.86 | 0.53x | 4.80 | 0.10x |
| Array Mapping + Sum | 46.94 | 26.23 | 0.56x | 52.22 | 28.41 | 0.54x | 5.22 | 0.11x |
| Array Max (100 elements) | 3.30 | 4.06 | 1.23x | 4.50 | 5.36 | 1.19x | 0.90 | 0.27x |
| Array Max (1000 elements) | 28.36 | 37.00 | 1.30x | 30.28 | 39.22 | 1.30x | 5.25 | 0.19x |
| Array Sum (100 elements) | 3.30 | 4.06 | 1.23x | 4.56 | 5.86 | 1.29x | 0.87 | 0.26x |
| Array Sum (1000 elements) | 29.31 | 36.81 | 1.26x | 31.31 | 38.29 | 1.22x | 4.88 | 0.17x |
| Array Sum (10000 elements) | 293.15 | 424.46 | 1.45x | 301.39 | 431.38 | 1.43x | 44.77 | 0.15x |
| Conditional Expression | 0.33 | 0.40 | 1.20x | 2.02 | 1.87 | 0.93x | 0.13 | 0.41x |
| Multiple Nested Functions | 0.43 | 0.78 | 1.84x | 1.93 | 2.84 | 1.47x | 0.58 | 1.36x |
| Object Construction (nested) | 1.11 | 1.07 | 0.97x | 4.65 | 4.08 | 0.88x | 0.41 | 0.37x |
| Object Construction (simple) | 0.83 | 0.86 | 1.03x | 3.81 | 3.22 | 0.84x | 0.39 | 0.47x |
| Deep Path (12 levels) | 1.92 | 1.10 | 0.57x | 7.52 | 8.27 | 1.10x | 0.51 | 0.26x |
| Nested Array Access | 16.58 | 9.96 | 0.60x | 18.96 | 13.24 | 0.70x | 1.08 | 0.07x |
| $filter with lambda | 9.17 | 17.71 | 1.93x | 11.98 | 19.40 | 1.62x | 10.96 | 1.19x |
| $map with lambda | 9.80 | 17.99 | 1.83x | 12.20 | 19.36 | 1.59x | 11.25 | 1.15x |
| $reduce with lambda | 11.64 | 18.30 | 1.57x | 15.01 | 21.49 | 1.43x | 12.58 | 1.08x |
| Calculate total value | 143.75 | 74.48 | 0.52x | 159.54 | 78.19 | 0.49x | 14.84 | 0.10x |
| Complex transformation | 200.65 | 118.52 | 0.59x | 207.47 | 122.78 | 0.59x | 49.38 | 0.25x |
| Filter by category | 143.86 | 87.29 | 0.61x | 165.30 | 88.69 | 0.54x | 18.27 | 0.13x |
| Group by category (aggregate) | 190.07 | 119.00 | 0.63x | 214.10 | 130.16 | 0.61x | 52.61 | 0.28x |
| Top rated products ⚠️excluded | 167.90 | 105.36 | 0.63x | 179.69 | 113.05 | 0.63x | 29.38 | 0.18x |
| Arithmetic Expression | 0.40 | 0.50 | 1.26x | 1.83 | 1.69 | 0.92x | 0.15 | 0.37x |
| Array Index Access | 3.12 | 3.23 | 1.03x | 4.18 | 4.55 | 1.09x | 0.23 | 0.07x |
| Deep Path (5 levels) | 0.68 | 0.59 | 0.88x | 3.15 | 3.45 | 1.09x | 0.22 | 0.32x |
| Simple Path | 0.46 | 0.44 | 0.95x | 1.73 | 1.57 | 0.91x | 0.11 | 0.24x |
| String Concatenation | 0.70 | 1.04 | 1.49x | 2.94 | 3.20 | 1.09x | 0.77 | 1.11x |
| String Contains | 0.39 | 0.81 | 2.08x | 1.74 | 2.32 | 1.34x | 0.53 | 1.36x |
| String Length | 0.41 | 0.56 | 1.35x | 1.51 | 1.85 | 1.23x | 0.32 | 0.77x |
| String Lowercase | 0.44 | 0.54 | 1.24x | 1.55 | 1.84 | 1.19x | 0.32 | 0.74x |
| String Substring | 0.59 | 1.09 | 1.84x | 2.03 | 3.60 | 1.77x | 0.75 | 1.26x |
| String Uppercase | 0.40 | 0.56 | 1.38x | 1.55 | 1.91 | 1.23x | 0.34 | 0.84x |

### Aggregates (gate-passed scenarios only)

- **String→string, compiled (geomean): 1.06x** (dashjoin/jsonata-java 0.9.10 time ÷ core time; >1 means core is faster)
- String→string, compile-each (geomean): 1.00x
- Home-turf (competitor on pre-parsed native data, core still paying the string boundary; geomean): 0.33x
- Best 3 (s→s compiled): String Contains (2.08x), $filter with lambda (1.93x), Multiple Nested Functions (1.84x)
- Worst 3 (s→s compiled): Calculate total value (0.52x), Array Mapping + Sum (0.56x), Array Mapping (extract field) (0.57x)

## .NET — core (LibraryImport) vs Jsonata.Net.Native 3.0.0

### Correctness gate

33/33 scenarios match. All scenarios included.


### Per-scenario results (µs/op, lower is better)

| Scenario | core s→s | comp s→s | speedup | core s→s (compile-each) | comp s→s (compile-each) | speedup | comp home-turf | core-vs-home-turf |
|---|---|---|---|---|---|---|---|---|
| Array Count (100 elements) | 3.46 | 10.56 | 3.05x | 4.70 | 10.98 | 2.33x | 0.43 | 0.12x |
| Array Filtering (predicate) | 56.80 | 123.52 | 2.17x | 61.72 | 125.64 | 2.04x | 21.31 | 0.38x |
| Array Mapping (extract field) | 49.12 | 100.06 | 2.04x | 51.84 | 97.76 | 1.89x | 4.30 | 0.09x |
| Array Mapping + Sum | 47.15 | 92.43 | 1.96x | 51.06 | 97.87 | 1.92x | 5.79 | 0.12x |
| Array Max (100 elements) | 3.45 | 11.23 | 3.26x | 4.68 | 12.06 | 2.58x | 1.53 | 0.44x |
| Array Max (1000 elements) | 29.93 | 106.58 | 3.56x | 31.56 | 107.11 | 3.39x | 10.59 | 0.35x |
| Array Sum (100 elements) | 3.53 | 11.89 | 3.37x | 4.75 | 12.19 | 2.57x | 1.61 | 0.45x |
| Array Sum (1000 elements) | 32.50 | 112.23 | 3.45x | 34.76 | 113.42 | 3.26x | 10.68 | 0.33x |
| Array Sum (10000 elements) | 329.94 | 2147.60 | 6.51x | 328.81 | 2105.94 | 6.40x | 98.35 | 0.30x |
| Conditional Expression | 0.31 | 0.54 | 1.74x | 1.92 | 1.30 | 0.68x | 0.28 | 0.89x |
| Multiple Nested Functions | 0.42 | 1.12 | 2.68x | 1.82 | 2.03 | 1.12x | 0.75 | 1.79x |
| Object Construction (nested) | 1.18 | 1.83 | 1.55x | 4.48 | 3.77 | 0.84x | 1.04 | 0.88x |
| Object Construction (simple) | 0.87 | 1.42 | 1.63x | 3.71 | 2.63 | 0.71x | 0.71 | 0.82x |
| Deep Path (12 levels) | 2.17 | 4.33 | 1.99x | 7.46 | 7.85 | 1.05x | 1.15 | 0.53x |
| Nested Array Access | 17.68 | 47.81 | 2.70x | 19.66 | 48.67 | 2.48x | 0.29 | 0.02x |
| $filter with lambda | 9.48 | 27.00 | 2.85x | 11.96 | 28.72 | 2.40x | 16.42 | 1.73x |
| $map with lambda | 10.14 | 29.01 | 2.86x | 12.48 | 29.90 | 2.40x | 16.97 | 1.67x |
| $reduce with lambda | 13.00 | 27.26 | 2.10x | 14.88 | 29.21 | 1.96x | 17.70 | 1.36x |
| Calculate total value | 149.88 | 286.98 | 1.91x | 163.21 | 285.40 | 1.75x | 19.98 | 0.13x |
| Complex transformation | 209.29 | 363.26 | 1.74x | 220.23 | 364.82 | 1.66x | 88.41 | 0.42x |
| Filter by category | 150.20 | 292.97 | 1.95x | 164.64 | 296.50 | 1.80x | 18.38 | 0.12x |
| Group by category (aggregate) | 202.87 | 361.54 | 1.78x | 211.21 | 350.31 | 1.66x | 80.90 | 0.40x |
| Top rated products | 172.43 | 367.64 | 2.13x | 183.81 | 363.54 | 1.98x | 71.79 | 0.42x |
| Arithmetic Expression | 0.40 | 0.95 | 2.37x | 1.74 | 1.48 | 0.85x | 0.39 | 0.96x |
| Array Index Access | 3.36 | 9.68 | 2.88x | 4.22 | 10.59 | 2.51x | 0.25 | 0.07x |
| Deep Path (5 levels) | 0.71 | 1.68 | 2.36x | 2.99 | 2.86 | 0.96x | 0.55 | 0.77x |
| Simple Path | 0.45 | 0.87 | 1.91x | 1.61 | 1.36 | 0.84x | 0.28 | 0.61x |
| String Concatenation | 0.75 | 1.33 | 1.76x | 2.66 | 2.33 | 0.87x | 0.91 | 1.20x |
| String Contains | 0.36 | 1.10 | 3.06x | 1.62 | 1.76 | 1.09x | 0.61 | 1.71x |
| String Length | 0.36 | 0.87 | 2.43x | 1.42 | 1.42 | 1.00x | 0.47 | 1.31x |
| String Lowercase | 0.37 | 0.79 | 2.10x | 1.56 | 1.41 | 0.90x | 0.49 | 1.31x |
| String Substring | 0.53 | 1.17 | 2.21x | 1.96 | 2.31 | 1.18x | 0.70 | 1.32x |
| String Uppercase | 0.37 | 0.78 | 2.11x | 1.45 | 1.43 | 0.99x | 0.49 | 1.31x |

### Aggregates (gate-passed scenarios only)

- **String→string, compiled (geomean): 2.37x** (Jsonata.Net.Native 3.0.0 time ÷ core time; >1 means core is faster)
- String→string, compile-each (geomean): 1.57x
- Home-turf (competitor on pre-parsed native data, core still paying the string boundary; geomean): 0.48x
- Best 3 (s→s compiled): Array Sum (10000 elements) (6.51x), Array Max (1000 elements) (3.56x), Array Sum (1000 elements) (3.45x)
- Worst 3 (s→s compiled): Object Construction (nested) (1.55x), Object Construction (simple) (1.63x), Complex transformation (1.74x)
