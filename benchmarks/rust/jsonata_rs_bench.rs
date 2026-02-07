// Benchmark harness for jsonata-rs
// Accepts JSON input with expression, data, and iterations,
// runs the benchmark, and outputs timing in milliseconds.

use std::io::{self, Read};
use std::time::Instant;
use bumpalo::Bump;
use jsonata_rs::JsonAta;

fn main() {
    // Read JSON from stdin
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).expect("Failed to read stdin");

    // Parse input JSON
    let input_json: serde_json::Value = serde_json::from_str(&input).expect("Invalid input JSON");

    let expression = input_json["expression"].as_str().expect("Missing expression");
    let data_json = serde_json::to_string(&input_json["data"]).expect("Failed to serialize data");
    let iterations = input_json["iterations"].as_u64().expect("Missing iterations") as usize;
    let warmup = input_json.get("warmup").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

    // Create arena for allocations
    let arena = Bump::new();

    // Parse expression once
    let jsonata = match JsonAta::new(expression, &arena) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("{{\"error\": \"Compilation failed: {}\"}}", e);
            std::process::exit(1);
        }
    };

    // Warmup
    for _ in 0..warmup {
        match jsonata.evaluate(Some(&data_json), None) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("{{\"error\": \"Warmup failed: {}\"}}", e);
                std::process::exit(1);
            }
        }
    }

    // Benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        match jsonata.evaluate(Some(&data_json), None) {
            Ok(_) => {},
            Err(e) => {
                eprintln!("{{\"error\": \"Evaluation failed: {}\"}}", e);
                std::process::exit(1);
            }
        }
    }
    let elapsed = start.elapsed();

    // Output timing in milliseconds
    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    println!("{{\"elapsed_ms\": {}}}", elapsed_ms);
}
