// Phase 1 tests for host-callable custom functions (register_fn /
// register_fn_override). Design: docs/superpowers/specs/
// 2026-07-19-host-callable-functions-design.md
//
// v1 scope: direct calls `$name(args)`. Host-fn-as-first-class-value passed to
// a higher-order function is phase 2; overrides applied in value position
// (`$f := $now; $f()`) are covered here because they route through the same
// value-dispatch seam.

use jsonata_core::evaluator::{Evaluator, EvaluatorError};
use jsonata_core::parser::parse;
use jsonata_core::value::JValue;
use serde_json::json;

fn eval_with<F>(expr: &str, data: serde_json::Value, register: F) -> Result<JValue, EvaluatorError>
where
    F: FnOnce(&mut Evaluator),
{
    let ast = parse(expr).unwrap();
    let mut ev = Evaluator::new();
    register(&mut ev);
    ev.evaluate(&ast, &JValue::from(data))
}

#[test]
fn direct_call_single_string_arg() {
    let out = eval_with("$greet(name)", json!({ "name": "Ada" }), |ev| {
        ev.register_fn("greet", |args: &[JValue]| {
            let n = args.first().and_then(|v| v.as_str()).unwrap_or("world");
            Ok(JValue::from(format!("hello {n}")))
        })
        .unwrap();
    })
    .unwrap();
    assert_eq!(out, JValue::from("hello Ada"));
}

#[test]
fn direct_call_computes_over_multiple_args() {
    // $fxRate(amount, "USD") style: host multiplies by a looked-up rate.
    let out = eval_with(
        "$convert(amount, currency)",
        json!({ "amount": 10, "currency": "EUR" }),
        |ev| {
            ev.register_fn("convert", |args: &[JValue]| {
                let amount = args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                let rate = match args.get(1).and_then(|v| v.as_str()) {
                    Some("EUR") => 1.1,
                    _ => 1.0,
                };
                Ok(JValue::from_f64(amount * rate))
            })
            .unwrap();
        },
    )
    .unwrap();
    assert_eq!(out, JValue::from_f64(11.0));
}

#[test]
fn zero_arg_host_fn() {
    let out = eval_with("$token()", json!(null), |ev| {
        ev.register_fn("token", |_args: &[JValue]| Ok(JValue::from("abc-123")))
            .unwrap();
    })
    .unwrap();
    assert_eq!(out, JValue::from("abc-123"));
}

#[test]
fn host_fn_maps_over_a_sequence() {
    // `items.$double(qty)` maps the host fn across each item's context.
    let out = eval_with(
        "items.$double(qty)",
        json!({ "items": [ { "qty": 2 }, { "qty": 5 } ] }),
        |ev| {
            ev.register_fn("double", |args: &[JValue]| {
                let q = args.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
                Ok(JValue::from_f64(q * 2.0))
            })
            .unwrap();
        },
    )
    .unwrap();
    assert_eq!(
        out,
        JValue::from(vec![JValue::from_f64(4.0), JValue::from_f64(10.0)])
    );
}

#[test]
fn host_fn_error_propagates() {
    let err = eval_with("$boom(1)", json!(null), |ev| {
        ev.register_fn("boom", |_args: &[JValue]| {
            Err(EvaluatorError::EvaluationError("host blew up".into()))
        })
        .unwrap();
    })
    .unwrap_err();
    assert!(format!("{err}").contains("host blew up"), "got: {err}");
}

#[test]
fn register_fn_rejects_builtin_collision() {
    let mut ev = Evaluator::new();
    let err = ev
        .register_fn("sum", |_args: &[JValue]| Ok(JValue::from_f64(0.0)))
        .unwrap_err();
    assert!(
        format!("{err}").contains("shadows a built-in"),
        "got: {err}"
    );
}

#[test]
fn override_rejects_compilable_builtin() {
    // `round` is on the compiled fast path — overriding it is refused in v1.
    let mut ev = Evaluator::new();
    let err = ev
        .register_fn_override("round", |_args: &[JValue]| Ok(JValue::from_f64(0.0)))
        .unwrap_err();
    assert!(
        format!("{err}").contains("compiled fast path"),
        "got: {err}"
    );
}

#[test]
fn override_impure_builtin_now_direct() {
    // Determinism injection: freeze $now() for reproducible output.
    let out = eval_with("$now()", json!(null), |ev| {
        ev.register_fn_override("now", |_args: &[JValue]| {
            Ok(JValue::from("2020-01-01T00:00:00.000Z"))
        })
        .unwrap();
    })
    .unwrap();
    assert_eq!(out, JValue::from("2020-01-01T00:00:00.000Z"));
}

#[test]
fn override_applies_in_value_position() {
    // `$f := $now; $f()` routes through the value-dispatch seam; the override
    // must still win there.
    let out = eval_with("($f := $now; $f())", json!(null), |ev| {
        ev.register_fn_override("now", |_args: &[JValue]| {
            Ok(JValue::from("2020-01-01T00:00:00.000Z"))
        })
        .unwrap();
    })
    .unwrap();
    assert_eq!(out, JValue::from("2020-01-01T00:00:00.000Z"));
}

#[test]
fn in_expression_lambda_shadows_host_fn() {
    // A function defined in the expression wins over a host fn of the same name
    // (host fns resolve after the expression's own bindings/lambdas).
    let out = eval_with(
        "($greet := function($n){ 'local ' & $n }; $greet('x'))",
        json!(null),
        |ev| {
            ev.register_fn("greet", |_args: &[JValue]| Ok(JValue::from("HOST")))
                .unwrap();
        },
    )
    .unwrap();
    assert_eq!(out, JValue::from("local x"));
}

#[test]
fn unregistered_function_still_errors() {
    // Registering one host fn must not swallow the unknown-function error for a
    // different, unregistered name.
    let err = eval_with("$nope(1)", json!(null), |ev| {
        ev.register_fn("greet", |_args: &[JValue]| Ok(JValue::from("hi")))
            .unwrap();
    })
    .unwrap_err();
    assert!(format!("{err}").contains("nope"), "got: {err}");
}

#[test]
fn no_host_fns_leaves_builtins_intact() {
    // Sanity: an evaluator with no host fns behaves exactly as before.
    let out = eval_with("$sum([1,2,3])", json!(null), |_ev| {}).unwrap();
    assert_eq!(out, JValue::from_f64(6.0));
}
