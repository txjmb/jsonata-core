// Host-callable custom functions
//
// Shows how a host application registers native Rust functions that a JSONata
// expression can call as `$name(...)`:
//   - enrichment/lookup functions (the canonical use case)
//   - determinism injection: overriding an impure built-in ($now) with a frozen
//     implementation for reproducible output
//   - sandboxing: overriding a powerful built-in ($eval) to disable it
//
// Run with: cargo run --example host_functions

use jsonata_core::value::JValue;
use jsonata_core::{evaluator::Evaluator, parser::parse};
use serde_json::json;

fn main() {
    demo_enrichment_lookup();
    demo_multi_arg();
    demo_override_now();
    demo_sandbox_eval();
    demo_collision_is_rejected();
}

fn eval(ev: &mut Evaluator, expr: &str, data: serde_json::Value) -> JValue {
    let ast = parse(expr).expect("expression parses");
    ev.evaluate(&ast, &JValue::from(data))
        .expect("evaluation succeeds")
}

/// The canonical use case: a lookup backed by host-owned data. The expression
/// stays a clean artifact; the host owns the (here trivial) data source.
fn demo_enrichment_lookup() {
    let mut ev = Evaluator::new();
    ev.register_fn("productName", |args: &[JValue]| {
        let sku = args.first().and_then(|v| v.as_str()).unwrap_or("");
        let name = match sku {
            "A-1" => "Widget",
            "B-2" => "Gadget",
            _ => "Unknown",
        };
        Ok(JValue::from(name))
    })
    .unwrap();

    let out = eval(
        &mut ev,
        "items.{ 'sku': sku, 'name': $productName(sku) }",
        json!({ "items": [ { "sku": "A-1" }, { "sku": "B-2" } ] }),
    );
    println!("enrichment lookup: {}", out.to_json_string().unwrap());
}

/// Host functions receive all arguments already evaluated.
fn demo_multi_arg() {
    let mut ev = Evaluator::new();
    ev.register_fn("convert", |args: &[JValue]| {
        let amount = args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
        let rate = match args.get(1).and_then(|v| v.as_str()) {
            Some("EUR") => 1.1,
            _ => 1.0,
        };
        Ok(JValue::from_f64(amount * rate))
    })
    .unwrap();

    let out = eval(
        &mut ev,
        "$convert(amount, currency)",
        json!({ "amount": 10, "currency": "EUR" }),
    );
    println!("multi-arg convert: {}", out.to_json_string().unwrap());
}

/// Determinism injection: freeze `$now()` so output is reproducible in tests.
/// `$now` is a non-compilable (impure) built-in, so overriding it is allowed.
fn demo_override_now() {
    let mut ev = Evaluator::new();
    ev.register_fn_override("now", |_args: &[JValue]| {
        Ok(JValue::from("2020-01-01T00:00:00.000Z"))
    })
    .unwrap();

    let out = eval(&mut ev, "{ 'generatedAt': $now() }", json!(null));
    println!("frozen $now: {}", out.to_json_string().unwrap());
}

/// Sandboxing: disable the dynamic-evaluation built-in `$eval` when running
/// semi-trusted expressions.
fn demo_sandbox_eval() {
    let mut ev = Evaluator::new();
    ev.register_fn_override("eval", |_args: &[JValue]| {
        Err(jsonata_core::evaluator::EvaluatorError::EvaluationError(
            "$eval is disabled in this environment".to_string(),
        ))
    })
    .unwrap();

    let ast = parse("$eval('1 + 1')").unwrap();
    match ev.evaluate(&ast, &JValue::Null) {
        Ok(v) => println!("sandbox $eval: unexpectedly returned {v:?}"),
        Err(e) => println!("sandbox $eval: blocked as expected -> {e}"),
    }
}

/// Registering a name that collides with a built-in is refused; use
/// `register_fn_override` to replace a built-in deliberately.
fn demo_collision_is_rejected() {
    let mut ev = Evaluator::new();
    match ev.register_fn("sum", |_args: &[JValue]| Ok(JValue::Null)) {
        Ok(()) => println!("collision: unexpectedly accepted"),
        Err(e) => println!("collision: rejected as expected -> {e}"),
    }
}
