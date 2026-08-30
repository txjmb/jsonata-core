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
    assert_eval("( $g := ( $f := function($x){ $x + 1 }; $f ); $g(41) )", "42");
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
    assert_error_contains("( $f := function($x)<n:n>{ $x * 2 }; $f(\"nope\") )", "T0410");
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
    assert_eval("( $sq := function($x){$x*$x}; $sq2 := $sq ~> $sq; $sq2(3) )", "81");
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
    assert_eval("( $f := function($x){$x}; $g := function($x){$x}; $f = $g )", "false");
}

#[test]
fn exists_on_function_values() {
    assert_eval("( $f := function(){1}; $exists($f) )", "true");
    assert_eval("$exists($undefined_thing)", "false");
}

// ── Divergences from jsonata-js (verified against the reference with node) ───
//
// Each test here pins what THIS engine currently does. The reference behavior
// is stated in the comment. Some of these are id-collision/dangling-tag
// artifacts of the side-table design and are expected to change with the
// value-carried closure redesign; update them deliberately when that lands.

mod divergent_from_reference {
    use super::*;

    #[test]
    fn alias_then_lambda_rebinding() {
        // Reference: ["old","new"] — $g copied the closure value; rebinding $f
        // doesn't affect it. Current engine: the alias's tag resolves by NAME
        // "f" through the side table, so $g() picks up the REBOUND lambda.
        assert_eval(
            "( $f := function(){ \"old\" }; $g := $f; $f := function(){ \"new\" }; [$g(), $f()] )",
            "[\"new\",\"new\"]",
        );
    }

    #[test]
    fn calling_name_rebound_to_non_function() {
        // Reference: T1006 (attempted to invoke a non-function; $f is 42).
        // Current engine: the stale side-table entry under "f" survives the
        // rebinding, so the OLD lambda is silently called.
        assert_eval("( $f := function(){ 1 }; $f := 42; $f() )", "1");
    }

    #[test]
    fn escaped_mutual_recursion() {
        // Reference: "mutual-esc" — the escaped closure's frame keeps $g
        // alive. Current engine: $g was not captured at $f's definition (it
        // didn't exist yet) and the block scope holding it has popped, so the
        // call dangles: T1006.
        assert_error_contains(
            "( $esc := ( $f := function(){ $g() }; $g := function(){ \"mutual-esc\" }; $f ); $esc() )",
            "T1006",
        );
    }

    #[test]
    fn escaped_partial_application() {
        // Reference: 42 — the partial holds the function it applies. Current
        // engine: the partial re-resolves "$add" BY NAME at invocation time,
        // and the defining scope has popped: T1006.
        assert_error_contains(
            "( $p := ( $add := function($a,$b){$a+$b}; $add(10, ?) ); $p(32) )",
            "T1006",
        );
    }

    #[test]
    fn transform_bound_to_variable_then_called() {
        // Reference: {"a":1,"b":2} — a transform is an ordinary function
        // value. Current engine: evaluating a bare transform yields the string
        // "<lambda>" plus a side-table entry the string doesn't reference, so
        // the call fails: T1006.
        assert_error_contains("( $t := |$|{\"b\":2}|; $t({\"a\":1}) )", "T1006");
    }

    #[test]
    fn transform_bound_to_variable_via_chain_pipe() {
        // Reference: {"a":1,"b":2}. Current engine: same "<lambda>" string
        // problem as above: T1006.
        assert_error_contains("( $t := |$|{\"b\":2}|; {\"a\":1} ~> $t )", "T1006");
    }

    #[test]
    fn captured_lambda_name_rebound_to_lambda_resolves_live() {
        // Reference: "B" (live frame). Current engine: also "B", but only
        // because the captured tag resolves by NAME through the side table —
        // the same accident that breaks alias_then_lambda_rebinding above.
        // A pure snapshot-capture design would return "A" here; if the
        // redesign changes this, this is the place to document it.
        assert_eval(
            "( $f := function(){ \"A\" }; $g := function(){ $f() }; $f := function(){ \"B\" }; $g() )",
            "\"B\"",
        );
    }

    #[test]
    fn captured_lambda_name_rebound_to_non_lambda() {
        // Reference: an error (the reference tries to call 42). Current
        // engine: the stale side-table entry under "f" makes $g() call the
        // ORIGINAL lambda: "A".
        assert_eval(
            "( $f := function(){ \"A\" }; $g := function(){ $f() }; $f := 42; $g() )",
            "\"A\"",
        );
    }

    #[test]
    fn closure_captured_in_other_closures_environment() {
        // Reference: 17 — the escaping anonymous closure keeps its captured
        // $sq alive. Current engine: the escape-analysis walk loses $sq
        // across the double scope pop (block exit + lambda return), so the
        // inner call dangles: T1006. This is the latent bug class issue #157
        // describes; the value-carried redesign makes it unrepresentable.
        assert_error_contains(
            "( $mk := function(){ ( $sq := function($x){ $x * $x }; function($y){ $sq($y) + 1 } ) }; \
               $mk()(4) )",
            "T1006",
        );
    }

    #[test]
    fn recursive_closure_called_through_alias_after_rebinding() {
        // Reference: "replaced" (the body's $f resolves through the live
        // frame, finding the rebound lambda). Current engine: same result,
        // via the name-tag accident. A snapshot + self-reference design
        // yields "done" (the recursive self-call always reaches the original
        // closure); if the redesign changes this, update deliberately.
        assert_eval(
            "( $f := function($x){ $x = 0 ? \"done\" : $f($x - 1) }; \
               $f2 := $f; $f := function($x){ \"replaced\" }; $f2(2) )",
            "\"replaced\"",
        );
    }
}
