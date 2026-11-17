//! Reusing one `Evaluator` after an evaluation fails (#195).
//!
//! A failed evaluation must leave no trace: the next evaluation on the same
//! evaluator sees the bindings it would see on a fresh one.

use jsonata_core::evaluator::Evaluator;
use jsonata_core::value::JValue;
use jsonata_core::Expression;

fn eval(ev: &mut Evaluator, src: &str, data: &JValue) -> Result<JValue, String> {
    let expr = Expression::compile(src).map_err(|e| e.to_string())?;
    ev.evaluate(expr.ast(), data).map_err(|e| e.to_string())
}

/// Each failing expression binds `$v`, `$x` or `$` somewhere inside before
/// it raises, through a different construct.
const FAILING: &[&str] = &[
    // Block scope.
    r#"($x := 1; $error("e"))"#,
    // Lambda invocation scope.
    r#"$map([1], function($v) { $error("e") })"#,
    // Tail-call trampoline scope.
    r#"($f := function($v) { $v > 0 ? $f($v - 1) : $error("e") }; $f(3))"#,
    // Sort comparator scope.
    r#"$sort([2, 1], function($v, $w) { $error("e") })"#,
    // Partial application temporaries.
    r#"($f := $substring(?, 1); $f(true))"#,
    // Group-by key over a tuple stream, binding `$x`.
    r#"a@$x{$error("e"): 1}"#,
    // Group-by key binding `$`.
    r#"a{$error("e"): 1}"#,
    // A `.(...)` step binding `$`.
    r#"a.($error("e"))"#,
];

/// What a fresh evaluator gives for each probe.
const PROBES: &[&str] = &["$x", "$v", "$w", "$", "$f"];

#[test]
fn failed_evaluation_leaves_no_bindings() {
    let data = JValue::from(serde_json::json!({"a": [1, 2]}));
    let expected: Vec<_> = PROBES
        .iter()
        .map(|probe| eval(&mut Evaluator::new(), probe, &data))
        .collect();

    for failing in FAILING {
        let mut ev = Evaluator::new();
        assert!(
            eval(&mut ev, failing, &data).is_err(),
            "{failing} should fail"
        );
        for (probe, expected) in PROBES.iter().zip(&expected) {
            assert_eq!(
                &eval(&mut ev, probe, &data),
                expected,
                "after `{failing}`, `{probe}` differs from a fresh evaluator"
            );
        }
    }
}

#[test]
fn an_error_inside_a_successful_evaluation_leaves_no_bindings() {
    // A builtin's path argument used to be evaluated twice, the first time
    // discarding any error, so the failed first pass left `$x` bound for the
    // second pass and for later evaluations. jsonata-js raises the error.
    let data = JValue::from(serde_json::json!({"a": [1, 2]}));
    let mut ev = Evaluator::new();
    let src = r#"$string(a.($exists($x) ? "ok" : ($x := 1; $error("e"))))"#;
    assert!(eval(&mut ev, src, &data).is_err(), "{src} should fail");
    assert_eq!(eval(&mut ev, "$x", &data), Ok(JValue::Undefined));
}

#[test]
fn bindings_from_a_successful_evaluation_do_not_outlive_it() {
    // jsonata-js runs each evaluation in a new frame of its environment.
    let data = JValue::Null;
    let mut ev = Evaluator::new();
    assert_eq!(eval(&mut ev, "$x := 1", &data), Ok(JValue::from(1i64)));
    assert_eq!(eval(&mut ev, "$x", &data), Ok(JValue::Undefined));
}

#[test]
fn parent_is_each_evaluation_s_own_input() {
    let mut ev = Evaluator::new();
    let first = JValue::from(serde_json::json!({"k": 1}));
    let second = JValue::from(serde_json::json!({"k": 2}));
    assert_eq!(eval(&mut ev, "$$.k", &first), Ok(JValue::from(1i64)));
    assert_eq!(eval(&mut ev, "$$.k", &second), Ok(JValue::from(2i64)));
}

#[test]
fn many_failed_evaluations_do_not_grow_the_scope_stack() {
    // Each failure inside a lambda used to leave its scope pushed, so an
    // evaluator reused after many failures carried a growing stack of stale
    // scopes. Evaluation afterwards must behave as on a fresh evaluator.
    let data = JValue::Null;
    let mut ev = Evaluator::new();
    for _ in 0..1000 {
        assert!(eval(&mut ev, r#"$map([1], function($v) { $error("e") })"#, &data).is_err());
    }
    assert_eq!(
        eval(
            &mut ev,
            "($f := function($n) { $n = 0 ? 0 : 1 + $f($n - 1) }; $f(50))",
            &data
        ),
        Ok(JValue::from(50i64))
    );
    assert_eq!(eval(&mut ev, "$v", &data), Ok(JValue::Undefined));
}

/// A sort that fails partway leaves no state for the next path (#221): a
/// failing predicate on a first-step sort of a single consarray value once
/// left a flag set that made the next evaluation's first path undefined.
#[test]
fn failed_sort_leaves_no_path_state() {
    let data = JValue::from(serde_json::json!({"a": {"b": 1}}));
    let mut ev = Evaluator::new();
    assert!(eval(&mut ev, r#"[3][0]^($)[$error("boom")].$"#, &data).is_err());
    assert_eq!(eval(&mut ev, "a.b", &data), Ok(JValue::from(1.0)));
    assert!(eval(&mut ev, r#"([3].$)^($)[$error("boom")].$"#, &data).is_err());
    assert_eq!(eval(&mut ev, "a.b", &data), Ok(JValue::from(1.0)));
}
