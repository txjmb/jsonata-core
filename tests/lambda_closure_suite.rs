// Letrec / closure stress suite (issue #157, prerequisite work).
//
// Pins the CURRENT behavior of lambda storage, closure capture, recursion,
// escaping closures, rebinding, partial application, transforms-as-values and
// the TCO trampoline, so the JValue::Lambda redesign (carrying the closure in
// the value instead of a lambda_id tag into a per-scope side table) has a
// behavioral baseline to diff against.
//
// Tests whose current behavior DIVERGES from jsonata-js are grouped in the
// `divergent_from_reference` module at the bottom, each with a comment stating
// what the reference implementation (jsonata-js 2.x, verified directly with
// node against tests/jsonata-js/src/jsonata.js) returns. The redesign is
// expected to change some of these; when it does, the assertions there should
// be updated deliberately, in the same commit, with the rationale.

use jsonata_core::{evaluator::Evaluator, parser::parse, value::JValue};

/// Evaluate `expr` against `{}` and return the result.
fn eval(expr: &str) -> Result<JValue, String> {
    let ast = parse(expr).map_err(|e| format!("parse error: {e:?}"))?;
    let mut evaluator = Evaluator::new();
    let data = JValue::from_json_str("{}").unwrap();
    evaluator.evaluate(&ast, &data).map_err(|e| format!("{e}"))
}

/// Evaluate and compare against an expected JSON literal.
fn assert_eval(expr: &str, expected_json: &str) {
    let result = eval(expr).unwrap_or_else(|e| panic!("{expr}\n  unexpected error: {e}"));
    let expected = JValue::from_json_str(expected_json).unwrap();
    assert_eq!(result, expected, "expression: {expr}");
}

/// Evaluate and expect an error whose message contains `fragment`.
fn assert_error_contains(expr: &str, fragment: &str) {
    match eval(expr) {
        Ok(v) => panic!("{expr}\n  expected error containing {fragment:?}, got value {v}"),
        Err(msg) => assert!(
            msg.contains(fragment),
            "{expr}\n  expected error containing {fragment:?}, got: {msg}"
        ),
    }
}

// ── Direct recursion ─────────────────────────────────────────────────────────

#[test]
fn direct_recursion_factorial() {
    assert_eval(
        "( $f := function($x){ $x <= 1 ? 1 : $x * $f($x-1) }; $f(5) )",
        "120",
    );
}

#[test]
fn direct_recursion_building_arrays() {
    assert_eval(
        "( $fn := function($m){ $m = 0 ? [] : $append($fn($m-1), [$m]) }; $fn(4) )",
        "[1,2,3,4]",
    );
}

#[test]
fn deep_tail_recursion_uses_tco() {
    // 50k frames would overflow the real stack; the TCO trampoline must kick in.
    assert_eval(
        "( $count := function($n, $acc){ $n = 0 ? $acc : $count($n-1, $acc+1) }; $count(50000, 0) )",
        "50000",
    );
}

// ── Mutual recursion / late binding ──────────────────────────────────────────

#[test]
fn mutual_recursion_in_scope() {
    assert_eval(
        "( $even := function($n){ $n = 0 ? true : $odd($n-1) }; \
           $odd := function($n){ $n = 0 ? false : $even($n-1) }; $even(10) )",
        "true",
    );
}

#[test]
fn late_bound_name_resolves_at_call_time() {
    // $f2 is not yet bound when $f is defined; the body resolves it by name
    // at call time through the live scope. Matches jsonata-js.
    assert_eval(
        "( $f := function(){ $f2() }; $f2 := function(){ \"late\" }; $f() )",
        "\"late\"",
    );
}

// ── Closures escaping their defining scope ───────────────────────────────────

#[test]
fn closure_escapes_block() {
    assert_eval(
        "( $g := ( $f := function($x){ $x + 1 }; $f ); $g(41) )",
        "42",
    );
}

#[test]
fn closure_escapes_nested_blocks() {
    assert_eval(
        "( $mk := function(){ ( $h := function($x){ $x * 2 }; $h ) }; $mk()(21) )",
        "42",
    );
}

#[test]
fn closures_escape_inside_array() {
    assert_eval(
        "( $fs := ( $a := function(){ 1 }; $b := function(){ 2 }; [$a, $b] ); \
           $x := $fs[0]; $y := $fs[1]; [$x(), $y()] )",
        "[1,2]",
    );
}

#[test]
fn closure_escapes_inside_object() {
    assert_eval(
        "( $obj := ( $inc := function($x){ $x + 1 }; { \"fn\": $inc } ); \
           $z := $obj.fn; $z(41) )",
        "42",
    );
}

#[test]
fn escaped_closure_keeps_captured_local() {
    assert_eval(
        "( $outer := function(){ ( $a := 5; $b := function(){ $a * 10 }; $b ) }; $outer()() )",
        "50",
    );
}

#[test]
fn escaped_recursive_closure_with_captured_local() {
    // Recursion by self-name must survive the defining scope popping, together
    // with the captured $x.
    assert_eval(
        "( $g := ( $x := 3; $f := function($n){ $n = 0 ? 0 : $x + $f($n - 1) }; $f ); $g(4) )",
        "12",
    );
}

#[test]
fn escaped_closure_used_as_hof_callback() {
    assert_eval(
        "( $f := ( $inc := function($x){ $x + 1 }; $inc ); $map([1,2,3], $f) )",
        "[2,3,4]",
    );
}

#[test]
fn factory_closures_capture_independently() {
    assert_eval(
        "( $mk := function($m){ function($x){ $x * $m } }; \
           $double := $mk(2); $triple := $mk(3); [$double(5), $triple(5)] )",
        "[10,15]",
    );
}

// closure_captured_in_other_closures_environment lives in
// divergent_from_reference below: the current escape-analysis GC loses the
// inner captured lambda.

// ── Higher-order functions / parameters ──────────────────────────────────────

#[test]
fn lambda_passed_as_parameter_and_called() {
    assert_eval(
        "( $apply := function($f, $v){ $f($v) }; $apply(function($x){$x+100}, 1) )",
        "101",
    );
}

#[test]
fn y_combinator_fixed_point() {
    assert_eval(
        "( $y := function($f){ (function($x){ $x($x) })(function($x){ $f(function($y){ ($x($x))($y) }) }) }; \
           $fac := $y(function($self){ function($n){ $n <= 1 ? 1 : $n * $self($n-1) } }); $fac(6) )",
        "720",
    );
}

#[test]
fn hof_builtins_with_lambda_callbacks() {
    assert_eval("$map([1,2,3], function($v){ $v * 10 })", "[10,20,30]");
    assert_eval("$filter([1,2,3,4], function($v){ $v % 2 = 0 })", "[2,4]");
    assert_eval("$reduce([1,2,3,4], function($a,$b){ $a + $b })", "10");
    assert_eval(
        "( $cmp := function($l, $r){ $l > $r }; $sort([3,1,2], $cmp) )",
        "[1,2,3]",
    );
    assert_eval(
        "$sift({\"a\": 1, \"bb\": 2, \"c\": 3}, function($v, $k){ $length($k) = 1 })",
        "{\"a\":1,\"c\":3}",
    );
    assert_eval(
        "$each({\"a\":1,\"b\":2}, function($v,$k){ $k & \"=\" & $v })",
        "[\"a=1\",\"b=2\"]",
    );
    assert_eval("$single([1,2,3], function($v){ $v = 2 })", "2");
}

#[test]
fn stored_lambda_by_name_as_hof_callback() {
    assert_eval(
        "( $f := function($x){ $x + 1 }; $map([1,2,3], $f) )",
        "[2,3,4]",
    );
}

// ── Signatures through stored lambdas ────────────────────────────────────────

#[test]
fn signature_enforced_on_stored_lambda() {
    assert_eval("( $f := function($x)<n:n>{ $x * 2 }; $f(21) )", "42");
    assert_error_contains(
        "( $f := function($x)<n:n>{ $x * 2 }; $f(\"nope\") )",
        "T0410",
    );
}

// ── Partial application ──────────────────────────────────────────────────────

#[test]
fn partial_application_of_lambda() {
    assert_eval(
        "( $add := function($a,$b){$a+$b}; $inc := $add(1, ?); $inc(41) )",
        "42",
    );
}

#[test]
fn partial_application_of_builtin() {
    assert_eval(
        "( $first := $substringBefore(?, \" \"); $first(\"Hello World\") )",
        "\"Hello\"",
    );
}

// ── Function composition (~>) ────────────────────────────────────────────────

#[test]
fn composition_of_builtins() {
    assert_eval("( $u := $trim ~> $uppercase; $u(\"  hi \") )", "\"HI\"");
}

#[test]
fn composition_of_lambdas() {
    assert_eval(
        "( $f := function($x){$x+1}; $g := function($x){$x*2}; $h := $f ~> $g; $h(5) )",
        "12",
    );
}

#[test]
fn composition_with_self() {
    assert_eval(
        "( $sq := function($x){$x*$x}; $sq2 := $sq ~> $sq; $sq2(3) )",
        "81",
    );
}

#[test]
fn chain_pipe_into_map_with_function_reference() {
    assert_eval(
        "( $double := function($x){$x*2}; [1,2,3] ~> $map($double) )",
        "[2,4,6]",
    );
}

// ── Transforms ───────────────────────────────────────────────────────────────

#[test]
fn transform_applied_via_chain_pipe_literal() {
    assert_eval("{\"a\":1} ~> |$|{\"b\":2}|", "{\"a\":1,\"b\":2}");
}

// ── Rebinding, aliasing, shadowing ───────────────────────────────────────────

#[test]
fn alias_survives_non_lambda_rebinding() {
    // $g grabbed the old closure; rebinding $f to a number must not break $g.
    assert_eval(
        "( $f := function(){ \"old\" }; $g := $f; $f := 42; [$g(), $f] )",
        "[\"old\",42]",
    );
}

#[test]
fn rebinding_lambda_name_replaces_it_in_call_position() {
    assert_eval(
        "( $f := function(){ \"old\" }; $f := function(){ \"new\" }; $f() )",
        "\"new\"",
    );
}

#[test]
fn parameter_shadows_outer_lambda_of_same_name() {
    assert_eval(
        "( $g := function(){ \"outer\" }; $call := function($g){ $g() }; \
           $call(function(){ \"inner\" }) )",
        "\"inner\"",
    );
}

#[test]
fn plain_value_capture_is_a_snapshot() {
    // NOTE: jsonata-js returns 2 here (closures hold their defining frame by
    // reference, so a later := in the same frame is visible). This engine has
    // always captured plain values by snapshot at definition time; that
    // pre-existing divergence is orthogonal to the closure-storage redesign
    // and is pinned here so the redesign doesn't silently move it.
    assert_eval("( $x := 1; $g := function(){ $x }; $x := 2; $g() )", "1");
}

// ── Function values as data ──────────────────────────────────────────────────

#[test]
fn string_of_lambda_is_empty_string() {
    assert_eval("$string(function($x){$x})", "\"\"");
}

#[test]
fn lambdas_never_compare_equal() {
    assert_eval("( $f := function($x){$x}; $f = $f )", "false");
    assert_eval("( $f := function($x){$x}; $g := $f; $f = $g )", "false");
    assert_eval(
        "( $f := function($x){$x}; $g := function($x){$x}; $f = $g )",
        "false",
    );
}

#[test]
fn exists_on_function_values() {
    assert_eval("( $f := function(){1}; $exists($f) )", "true");
    assert_eval("$exists($undefined_thing)", "false");
}

// ── Fixed by the value-carried closure redesign (#157) ───────────────────────
//
// Before the redesign these were dangling-tag / id-collision artifacts of the
// lambda side table (the pre-redesign pins are in this suite's git history).
// They now match jsonata-js, verified directly against the reference with node.

mod fixed_by_redesign {
    use super::*;

    #[test]
    fn alias_survives_lambda_rebinding() {
        // $g copied the closure value; rebinding $f doesn't affect it.
        // (Previously ["new","new"]: the alias's tag resolved by NAME "f"
        // through the side table, picking up the rebound lambda.)
        assert_eval(
            "( $f := function(){ \"old\" }; $g := $f; $f := function(){ \"new\" }; [$g(), $f()] )",
            "[\"old\",\"new\"]",
        );
    }

    #[test]
    fn calling_name_rebound_to_non_function_is_an_error() {
        // $f is 42; calling it is T1006. (Previously the stale side-table
        // entry under "f" survived the rebinding and the OLD lambda was
        // silently called, returning 1.)
        assert_error_contains("( $f := function(){ 1 }; $f := 42; $f() )", "T1006");
    }

    #[test]
    fn escaped_partial_application() {
        // The partial captures the closure it applies, so it survives the
        // defining scope. (Previously it re-resolved "$add" by name at
        // invocation time: T1006.)
        assert_eval(
            "( $p := ( $add := function($a,$b){$a+$b}; $add(10, ?) ); $p(32) )",
            "42",
        );
    }

    #[test]
    fn transform_bound_to_variable_then_called() {
        // A transform is an ordinary function value. (Previously evaluating a
        // bare transform yielded the string "<lambda>" plus a side-table entry
        // the string didn't reference: T1006.)
        assert_eval(
            "( $t := |$|{\"b\":2}|; $t({\"a\":1}) )",
            "{\"a\":1,\"b\":2}",
        );
    }

    #[test]
    fn transform_bound_to_variable_via_chain_pipe() {
        assert_eval(
            "( $t := |$|{\"b\":2}|; {\"a\":1} ~> $t )",
            "{\"a\":1,\"b\":2}",
        );
    }

    #[test]
    fn closure_captured_in_other_closures_environment() {
        // The escaping anonymous closure carries its captured $sq inside the
        // value. (Previously the escape-analysis walk lost $sq across the
        // double scope pop — block exit + lambda return — and the inner call
        // dangled: T1006. That bug class is now unrepresentable.)
        assert_eval(
            "( $mk := function(){ ( $sq := function($x){ $x * $x }; function($y){ $sq($y) + 1 } ) }; \
               $mk()(4) )",
            "17",
        );
    }
}

// ── Divergences from jsonata-js (verified against the reference with node) ───
//
// jsonata-js closures hold their defining environment frame BY REFERENCE, so a
// later `:=` rebinding in that frame is visible through previously-defined
// closures. This engine captures free variables by VALUE SNAPSHOT at
// definition time (and always has, for plain values — see
// plain_value_capture_is_a_snapshot above). The redesign makes that snapshot
// rule uniform for lambda-valued variables too, where the old side table
// sometimes resolved them live by accident. Live-frame semantics would require
// closures to hold their defining frame (`Rc` cycles and per-cycle leaks —
// rejected in #157 for the long-lived Python interpreter case).

mod divergent_from_reference {
    use super::*;

    #[test]
    fn escaped_mutual_recursion() {
        // Reference: "mutual-esc" — the escaped closure's live frame keeps $g
        // reachable. Snapshot capture: $g did not exist when $f was defined,
        // so nothing captured it; after the defining scope pops the call
        // dangles: T1006. (Same behavior as before the redesign.)
        assert_error_contains(
            "( $esc := ( $f := function(){ $g() }; $g := function(){ \"mutual-esc\" }; $f ); $esc() )",
            "T1006",
        );
    }

    #[test]
    fn captured_lambda_name_rebound_to_lambda() {
        // Reference: "B" (live frame sees the rebinding). Snapshot capture:
        // $g captured $f's value at definition — "A". (Before the redesign
        // this returned "B", but only via the same name-tag accident that
        // broke alias_survives_lambda_rebinding.)
        assert_eval(
            "( $f := function(){ \"A\" }; $g := function(){ $f() }; $f := function(){ \"B\" }; $g() )",
            "\"A\"",
        );
    }

    #[test]
    fn captured_lambda_name_rebound_to_non_lambda() {
        // Reference: an error (the live frame's $f is 42; calling it fails).
        // Snapshot capture: $g captured the original closure — "A".
        assert_eval(
            "( $f := function(){ \"A\" }; $g := function(){ $f() }; $f := 42; $g() )",
            "\"A\"",
        );
    }

    #[test]
    fn recursive_closure_called_through_alias_after_rebinding() {
        // Reference: "replaced" (the body's recursive $f resolves through the
        // live frame, finding the rebound lambda). Snapshot + late-bound
        // self-reference: a recursive function always calls ITSELF — "done".
        assert_eval(
            "( $f := function($x){ $x = 0 ? \"done\" : $f($x - 1) }; \
               $f2 := $f; $f := function($x){ \"replaced\" }; $f2(2) )",
            "\"done\"",
        );
    }
}

// ── Memory: no closure leaks ─────────────────────────────────────────────────

#[test]
fn repeated_evaluation_does_not_leak_closures() {
    use jsonata_core::evaluator::live_lambda_count;

    // Shapes that exercised the old escape-analysis GC hardest: escaping
    // closures, closures capturing closures, self-recursion, aliasing,
    // partial application. Snapshot capture cannot form Rc cycles (a closure
    // only captures values that existed before it did; self-reference is
    // late-bound at invocation, never stored), so every closure must be freed
    // once the evaluation's result is dropped.
    let exprs = [
        "( $f := function($x){ $x <= 1 ? 1 : $x * $f($x-1) }; $f(10) )",
        "( $mk := function(){ ( $sq := function($x){ $x * $x }; function($y){ $sq($y) + 1 } ) }; $mk()(4) )",
        "( $g := ( $x := 3; $f := function($n){ $n = 0 ? 0 : $x + $f($n - 1) }; $f ); $g(4) )",
        "( $p := ( $add := function($a,$b){$a+$b}; $add(10, ?) ); $p(32) )",
        "( $f := function(){ \"old\" }; $g := $f; $f := function(){ \"new\" }; [$g(), $f()] )",
        "( $y := function($f){ (function($x){ $x($x) })(function($x){ $f(function($y){ ($x($x))($y) }) }) }; \
           $fac := $y(function($self){ function($n){ $n <= 1 ? 1 : $n * $self($n-1) } }); $fac(6) )",
    ];

    let baseline = live_lambda_count();
    for _ in 0..100 {
        for expr in &exprs {
            let _ = eval(expr).unwrap();
        }
    }
    assert_eq!(
        live_lambda_count(),
        baseline,
        "closures leaked across repeated evaluations"
    );
}
