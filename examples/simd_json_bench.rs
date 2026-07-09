//! One-off benchmark: how much does the `simd` feature speed up JSON parsing?
//!
//! Run with the feature on (default) vs off, e.g.:
//!   cargo run --release --example simd_json_bench
//!   cargo run --release --example simd_json_bench --no-default-features
//!
//! Not part of the permanent benchmark suite - a throwaway comparison.

use jsonata_core::value::JValue;
use std::time::Instant;

fn ecommerce_json(n: usize) -> String {
    let products: Vec<String> = (0..n)
        .map(|i| {
            format!(
                r#"{{"id":{i},"name":"Product {i}","category":"{cat}","price":{price},"inStock":{stock},"rating":{rating},"reviews":{reviews},"tags":["tag1","tag2","tag3"],"vendor":{{"name":"Vendor {v}","rating":4.2}}}}"#,
                i = i,
                cat = ["Electronics", "Clothing", "Books", "Home"][i % 4],
                price = 10.0 + i as f64 * 5.5,
                stock = i % 3 != 0,
                rating = 3.0 + (i % 3) as f64 * 0.5,
                reviews = i * 2,
                v = i % 10,
            )
        })
        .collect();
    format!(r#"{{"products":[{}]}}"#, products.join(","))
}

const REPEAT_TRIALS: usize = 5;

fn bench_parse(label: &str, json: &str, iterations: usize) {
    // warmup (shared across all trials)
    for _ in 0..(iterations / 10).max(10) {
        let _ = JValue::from_json_str(json).unwrap();
    }

    // Take the min across REPEAT_TRIALS independent trials - single-sample
    // wall-clock timing is noisy enough to be meaningless on its own (see
    // this project's own benchmarks/python/benchmark.py for why).
    let mut best_ms: Option<f64> = None;
    let mut all_ms = Vec::with_capacity(REPEAT_TRIALS);
    for _ in 0..REPEAT_TRIALS {
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = JValue::from_json_str(json).unwrap();
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        all_ms.push(ms);
        best_ms = Some(best_ms.map_or(ms, |b: f64| b.min(ms)));
    }
    let best_ms = best_ms.unwrap();
    let per_iter = best_ms / iterations as f64;
    println!(
        "{label:<28} {bytes:>8} bytes  {iterations:>6} iters  min={min:>8.2}ms  all={all:?}  {per:>10.5} ms/iter",
        label = label,
        bytes = json.len(),
        iterations = iterations,
        min = best_ms,
        all = all_ms.iter().map(|v| format!("{v:.1}")).collect::<Vec<_>>(),
        per = per_iter,
    );
}

#[cfg(feature = "simd")]
fn bench_parse_reused_buffer(label: &str, json: &str, iterations: usize) {
    use simd_json::Buffers;

    let mut buffers = Buffers::new(json.len() + 64);
    let mut bytes = json.as_bytes().to_vec();

    // warmup
    for _ in 0..(iterations / 10).max(10) {
        let _: JValue =
            simd_json::serde::from_slice_with_buffers(&mut bytes, &mut buffers).unwrap();
    }

    let mut best_ms: Option<f64> = None;
    let mut all_ms = Vec::with_capacity(REPEAT_TRIALS);
    for _ in 0..REPEAT_TRIALS {
        let start = Instant::now();
        for _ in 0..iterations {
            bytes.clear();
            bytes.extend_from_slice(json.as_bytes());
            let _: JValue =
                simd_json::serde::from_slice_with_buffers(&mut bytes, &mut buffers).unwrap();
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        all_ms.push(ms);
        best_ms = Some(best_ms.map_or(ms, |b: f64| b.min(ms)));
    }
    let best_ms = best_ms.unwrap();
    let per_iter = best_ms / iterations as f64;
    println!(
        "{label:<28} {bytes:>8} bytes  {iterations:>6} iters  min={min:>8.2}ms  all={all:?}  {per:>10.5} ms/iter",
        label = label,
        bytes = json.len(),
        iterations = iterations,
        min = best_ms,
        all = all_ms.iter().map(|v| format!("{v:.1}")).collect::<Vec<_>>(),
        per = per_iter,
    );
}

fn main() {
    let feature = if cfg!(feature = "simd") {
        "simd-json"
    } else {
        "serde_json (simd off)"
    };
    println!("=== JSON parsing benchmark - active parser: {feature} ===\n");

    bench_parse("tiny (1 product)", &ecommerce_json(1), 200_000);
    bench_parse("small (10 products)", &ecommerce_json(10), 50_000);
    bench_parse("medium (100 products)", &ecommerce_json(100), 5_000);
    bench_parse("large (1000 products)", &ecommerce_json(1000), 500);

    #[cfg(feature = "simd")]
    {
        println!("\n=== HYPOTHESIS TEST: simd-json with a REUSED Buffers scratch ===\n");
        bench_parse_reused_buffer("tiny (1 product)", &ecommerce_json(1), 200_000);
        bench_parse_reused_buffer("small (10 products)", &ecommerce_json(10), 50_000);
        bench_parse_reused_buffer("medium (100 products)", &ecommerce_json(100), 5_000);
        bench_parse_reused_buffer("large (1000 products)", &ecommerce_json(1000), 500);
    }
}
