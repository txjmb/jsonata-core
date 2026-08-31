// Benchmark harness for jsonata-core measured as a pure Rust library —
// no Python interpreter, no PyO3 boundary, no per-iteration JSON I/O.
//
// Mirrors jsonata_rs_bench.rs (same stdin protocol, same output shape) so
// benchmark.py can drive both identically, and mirrors the criterion suite's
// methodology (benches/evaluator_bench.rs): the expression is compiled once,
// the input data is parsed to a JValue once, and the timed loop measures
// only Expression::evaluate. This is the number that answers "what does the
// Rust engine itself cost?", per table row, alongside the Python-boundary
// columns.

use std::hint::black_box;
use std::io::{self, Read};
use std::time::Instant;

use jsonata_core::value::JValue;
use jsonata_core::Expression;

fn main() {
    // Read JSON from stdin
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .expect("Failed to read stdin");

    // Parse input JSON
    let input_json: serde_json::Value = serde_json::from_str(&input).expect("Invalid input JSON");

    let expression = input_json["expression"].as_str().expect("Missing expression");
    let data_json = serde_json::to_string(&input_json["data"]).expect("Failed to serialize data");
    let iterations = input_json["iterations"].as_u64().expect("Missing iterations") as usize;
    let warmup = input_json
        .get("warmup")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    // Parse data once — the timed loop measures evaluation only
    let data: JValue = match JValue::from_json_str(&data_json) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{{\"error\": \"Data parse failed: {}\"}}", e);
            std::process::exit(1);
        }
    };

    // Compile once
    let expr = match Expression::compile(expression) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{{\"error\": \"Compilation failed: {}\"}}", e);
            std::process::exit(1);
        }
    };

    // Warmup
    for _ in 0..warmup {
        if let Err(e) = expr.evaluate(&data) {
            eprintln!("{{\"error\": \"Warmup failed: {}\"}}", e);
            std::process::exit(1);
        }
    }

    // Benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        match expr.evaluate(&data) {
            Ok(v) => {
                black_box(v);
            }
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
