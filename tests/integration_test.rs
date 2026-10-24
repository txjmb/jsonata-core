// Integration tests for Parser + Evaluator
//
// These tests verify that the parser and evaluator work together correctly
// to process complete JSONata expressions.

use jsonata_core::{
    evaluator::{CancelToken, Context, Evaluator, EvaluatorOptions},
    parser::parse,
    value::JValue,
};
use serde_json::json;

#[test]
fn test_simple_field_access() {
    let data: JValue = json!({
        "name": "Alice",
        "age": 30
    })
    .into();

    let ast = parse("name").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("Alice")));
}

#[test]
fn test_nested_field_access() {
    let data: JValue = json!({
        "user": {
            "profile": {
                "name": "Bob"
            }
        }
    })
    .into();

    let ast = parse("user.profile.name").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("Bob")));
}

#[test]
fn test_arithmetic_expression() {
    let data: JValue = json!({
        "price": 100,
        "quantity": 5
    })
    .into();

    // Test basic multiplication - arithmetic produces f64 results
    let ast = parse("price * quantity").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::from(json!(500.0)));

    // Test complex arithmetic
    let ast = parse("(price + 10) * quantity").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::from(json!(550.0)));
}

#[test]
fn test_comparison_expression() {
    let data: JValue = json!({
        "age": 25,
        "threshold": 18
    })
    .into();

    let ast = parse("age > threshold").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Bool(true));
}

#[test]
fn test_logical_expression() {
    let data: JValue = json!({
        "age": 25,
        "has_license": true
    })
    .into();

    let ast = parse("age >= 18 and has_license").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Bool(true));
}

#[test]
fn test_string_concatenation() {
    let data: JValue = json!({
        "first": "Hello",
        "second": "World"
    })
    .into();

    let ast = parse(r#"first & " " & second"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("Hello World")));
}

#[test]
fn test_function_call() {
    let data: JValue = json!({
        "name": "alice"
    })
    .into();

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$uppercase(name)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("ALICE")));
}

#[test]
fn test_nested_function_calls() {
    let data: JValue = json!({
        "text": "HELLO"
    })
    .into();

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$length($lowercase(text))").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!(5)));
}

#[test]
fn test_conditional_expression() {
    let data: JValue = json!({
        "score": 85
    })
    .into();

    let ast = parse(r#"score >= 80 ? "Pass" : "Fail""#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("Pass")));
}

#[test]
fn test_array_literal() {
    let data: JValue = json!({
        "a": 1,
        "b": 2,
        "c": 3
    })
    .into();

    let ast = parse("[a, b, c]").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!([1, 2, 3])));
}

#[test]
fn test_object_literal() {
    let data: JValue = json!({
        "x": 10,
        "y": 20
    })
    .into();

    let ast = parse(r#"{"sum": x + y, "product": x * y}"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    // Arithmetic operations produce f64 results
    assert_eq!(result, JValue::from(json!({"sum": 30.0, "product": 200.0})));
}

#[test]
fn test_range_operator() {
    let data = JValue::Null;

    let ast = parse("1..5").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!([1, 2, 3, 4, 5])));
}

#[test]
fn test_in_operator() {
    let data: JValue = json!({
        "value": 3,
        "list": [1, 2, 3, 4, 5]
    })
    .into();

    let ast = parse("value in list").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Bool(true));
}

#[test]
fn test_complex_real_world_example() {
    let data: JValue = json!({
        "order": {
            "id": "ORD-123",
            "items": [
                {"name": "Laptop", "price": 1000, "quantity": 1},
                {"name": "Mouse", "price": 25, "quantity": 2}
            ],
            "customer": {
                "name": "Alice Smith",
                "type": "premium"
            },
            "discount_rate": 0.1
        }
    })
    .into();

    // Access nested fields
    let ast = parse("order.customer.name").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::from(json!("Alice Smith")));

    // Check customer type
    let ast = parse(r#"order.customer.type = "premium""#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::Bool(true));
}

#[test]
fn test_missing_field_returns_undefined() {
    let data: JValue = json!({
        "name": "Alice"
    })
    .into();

    let ast = parse("missing_field").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Undefined);
}

#[test]
fn test_deep_nesting() {
    let data: JValue = json!({
        "a": {
            "b": {
                "c": {
                    "d": {
                        "e": "deep value"
                    }
                }
            }
        }
    })
    .into();

    let ast = parse("a.b.c.d.e").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("deep value")));
}

#[test]
fn test_multiple_operations() {
    let data: JValue = json!({
        "x": 10,
        "y": 20,
        "z": 30
    })
    .into();

    let ast = parse("(x + y) * z / 2").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!(450.0)));
}

#[test]
fn test_sum_function() {
    let data: JValue = json!({
        "numbers": [1, 2, 3, 4, 5]
    })
    .into();

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$sum(numbers)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!(15.0)));
}

#[test]
fn test_count_function() {
    let data: JValue = json!({
        "items": [1, 2, 3, 4, 5]
    })
    .into();

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$count(items)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!(5)));
}

#[test]
fn test_nested_conditionals() {
    let data: JValue = json!({
        "score": 75
    })
    .into();

    let ast =
        parse(r#"score >= 90 ? "A" : (score >= 80 ? "B" : (score >= 70 ? "C" : "F"))"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!("C")));
}

#[test]
fn test_block_expression() {
    let data = JValue::Null;

    let ast = parse("(1; 2; 3)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    // Block should return the last expression
    assert_eq!(result, JValue::from(json!(3)));
}

#[test]
fn test_unary_negation() {
    let data: JValue = json!({
        "value": 42
    })
    .into();

    let ast = parse("-value").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!(-42.0)));
}

#[test]
fn test_modulo_operator() {
    let data: JValue = json!({
        "dividend": 17,
        "divisor": 5
    })
    .into();

    let ast = parse("dividend % divisor").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!(2.0)));
}

#[test]
fn test_comparison_operators() {
    let data: JValue = json!({
        "a": 10,
        "b": 20
    })
    .into();

    // Less than
    let ast = parse("a < b").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), JValue::Bool(true));

    // Less than or equal
    let ast = parse("a <= b").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), JValue::Bool(true));

    // Greater than
    let ast = parse("b > a").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), JValue::Bool(true));

    // Greater than or equal
    let ast = parse("b >= a").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), JValue::Bool(true));

    // Equal
    let ast = parse("a = 10").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), JValue::Bool(true));

    // Not equal
    let ast = parse("a != b").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), JValue::Bool(true));
}

#[test]
fn test_string_comparison() {
    let data: JValue = json!({
        "name1": "Alice",
        "name2": "Bob"
    })
    .into();

    let ast = parse("name1 < name2").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Bool(true));
}

#[test]
fn test_empty_array() {
    let data = JValue::Null;

    let ast = parse("[]").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!([])));
}

#[test]
fn test_empty_object() {
    let data = JValue::Null;

    let ast = parse("{}").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!({})));
}

#[test]
fn test_error_undefined_variable() {
    let data = JValue::Null;

    // An unbound variable is undefined -- not an error, and not null.
    // Verified against jsonata-js: `$undefined` is undefined, `{"a": $x}` is
    // `{}` (the key drops out), and `3 > $x` is undefined. This asserted null
    // before the null/undefined split was carried through to variables (#98);
    // with null the comparison would raise T2010 instead.
    let ast = parse("$undefined").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Undefined);

    // The consequences that make undefined the right value here.
    let ast = parse("3 > $x").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::Undefined
    );
    let ast = parse("{\"a\": $x}").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!({}))
    );
}

#[test]
fn test_error_type_mismatch() {
    let data: JValue = json!({
        "text": "hello",
        "number": 42
    })
    .into();

    let ast = parse("text + number").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);

    assert!(result.is_err());
}

#[test]
fn test_error_division_by_zero() {
    let data: JValue = json!({
        "value": 10
    })
    .into();

    // jsonata-js returns Infinity for division by zero rather than raising; the
    // D1001 surfaces only when that value is consumed as an operand (#102).
    let ast = parse("value / 0").unwrap();
    let mut evaluator = Evaluator::new();
    match evaluator.evaluate(&ast, &data) {
        Ok(JValue::Number(n)) => assert!(n.is_infinite() && n > 0.0),
        other => panic!("expected +inf, got {other:?}"),
    }

    // Consuming it does raise.
    let ast = parse("1 / (value / 0)").unwrap();
    assert!(Evaluator::new().evaluate(&ast, &data).is_err());
}

#[test]
fn test_with_variables() {
    let data: JValue = json!({
        "price": 100
    })
    .into();

    let ast = parse("price * $discount").unwrap();

    // Create context with discount variable
    let mut context = Context::new();
    context.bind("discount".to_string(), json!(0.9).into());
    let mut evaluator = Evaluator::with_context(context);

    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::from(json!(90.0)));
}

/// Run `f` on a thread with a 1MB stack, Windows' default for a main thread
/// (Linux gives 8MB), so the deep-recursion tests below fail by crashing the
/// test process if any recursive path overruns the real stack (GitHub issues
/// #34, #200). `JValue` is `!Send`, so only a `String` summary comes back.
fn on_small_stack(f: impl FnOnce() -> String + Send + 'static) -> String {
    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .expect("evaluation overflowed the native stack instead of returning")
}

/// Evaluate `expr` with the tree-walker and with the default dispatch
/// (`Expression`, which uses the bytecode VM when it can) and return both
/// outcomes as text.
fn eval_both_engines(expr: &str, options: EvaluatorOptions) -> (String, String) {
    eval_both_engines_with(expr, options, &JValue::Null)
}

/// `eval_both_engines` against `data`. An expression that does not parse
/// gives its parse error on both.
fn eval_both_engines_with(
    expr: &str,
    options: EvaluatorOptions,
    data: &JValue,
) -> (String, String) {
    fn show(r: Result<JValue, jsonata_core::evaluator::EvaluatorError>) -> String {
        match r {
            Ok(v) => format!("Ok({})", v.to_json_string().unwrap()),
            Err(e) => format!("Err({e})"),
        }
    }
    let tree_walker = match parse(expr) {
        Ok(ast) => {
            show(Evaluator::with_options(Context::new(), options.clone()).evaluate(&ast, data))
        }
        Err(e) => format!("Err({e})"),
    };
    let default = match jsonata_core::Expression::compile_with_options(expr, options) {
        Ok(compiled) => show(compiled.evaluate(data)),
        Err(e) => format!("Err({e})"),
    };
    (tree_walker, default)
}

const NON_TAIL_RECURSION: &str = "($f := function($n){ $n <= 0 ? 0 : 1 + $f($n - 1) }; $f(N))";

/// Non-tail recursion goes as deep as jsonata-js lets it (GitHub issue #200):
/// it used to stop at 302 evaluation levels, about 100 calls, with U1001.
/// jsonata-js has no fixed limit. On a 1MB stack, too.
#[test]
fn test_deep_non_tail_recursion_evaluates() {
    let outcome = on_small_stack(|| {
        let mut out = String::new();
        for n in [150, 1000, 5000, 9000] {
            let expr = NON_TAIL_RECURSION.replace('N', &n.to_string());
            let (tree_walker, default) = eval_both_engines(&expr, EvaluatorOptions::default());
            out.push_str(&format!("{n}: {tree_walker} {default}\n"));
        }
        out
    });
    assert_eq!(
        outcome,
        "150: Ok(150) Ok(150)\n1000: Ok(1000) Ok(1000)\n5000: Ok(5000) Ok(5000)\n\
         9000: Ok(9000) Ok(9000)\n"
    );
}

/// A recursive function can build a value as deep as its recursion. Comparing
/// it, turning it into a string or JSON, and dropping it must not overflow a
/// small stack either. (The array depth is kept low because dropping a
/// `JValue` is still plainly recursive, and an unoptimized build's drop of
/// nested arrays overflows 1MB at a few thousand levels.)
#[test]
fn test_deep_value_built_by_recursion() {
    let outcome = on_small_stack(|| {
        let expr = "($f := function($n){ $n <= 0 ? {} : {'a': $f($n - 1)} }; \
                    $v := $f(9000); [$v = $f(9000), $length($string($v))])";
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        let ast = parse("($f := function($n){ $n <= 0 ? [] : [[$f($n - 1)]] }; $f(1500))").unwrap();
        let deep = Evaluator::new().evaluate(&ast, &JValue::Null).unwrap();
        let json = deep.to_json_string().unwrap();
        let reparsed_equal = JValue::from_json_str(&json).map(|v| v == deep);
        format!("{tree_walker} {default} {} {reparsed_equal:?}", json.len())
    });
    // 9000 levels of `{"a":` plus the innermost `{}` and the closing braces.
    // The arrays nest 1500 deep, so their JSON is 3002 characters, which
    // serde_json's own 128-level parse limit rejects reading back.
    let string_len = 9000 * 6 + 2;
    assert!(
        outcome.starts_with(&format!(
            "Ok([true,{string_len}]) Ok([true,{string_len}]) 3002 Err("
        )),
        "got: {outcome}"
    );
}

/// Recursion with no base case stops with U1001 at the built-in limit, on
/// both engines, instead of exhausting the stack or memory.
#[test]
fn test_unbounded_recursion_raises_u1001() {
    let outcome = on_small_stack(|| {
        let (tree_walker, default) = eval_both_engines(
            "($inf := function($n){$n+$inf($n-1)}; $inf(5))",
            EvaluatorOptions::default(),
        );
        format!("{tree_walker}\n{default}")
    });
    assert_eq!(
        outcome,
        "Err(Evaluation error: U1001: Stack overflow error: evaluation nested deeper than 30000 levels. Check for non-terminating recursive function. Consider rewriting as tail-recursive.)\n\
         Err(Evaluation error: U1001: Stack overflow error: evaluation nested deeper than 30000 levels. Check for non-terminating recursive function. Consider rewriting as tail-recursive.)"
    );
}

/// The same through calls of an expression (`($f)(x)`, `($g := $f; $g)(x)`),
/// which go through `apply_call_value` rather than a named call: deep
/// recursion evaluates, and with no base case it is U1001, not a crash.
#[test]
fn test_recursion_through_expression_calls() {
    let outcome = on_small_stack(|| {
        let mut out = String::new();
        for expr in [
            "($f := function($n){ $n <= 0 ? 0 : 1 + ($f)($n - 1) }; ($f)(5000))",
            "($f := function($n){ $n <= 0 ? 0 : 1 + ($g := $f; $g)($n - 1) }; $f(3000))",
            "($inf := function($n){ $n + ($inf)($n - 1) }; ($inf)(5))",
        ] {
            let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
            out.push_str(&format!("{tree_walker} | {default}\n"));
        }
        out
    });
    let u1001 =
        "Err(Evaluation error: U1001: Stack overflow error: evaluation nested deeper than 30000 levels. Check for non-terminating recursive function. Consider rewriting as tail-recursive.)";
    assert_eq!(
        outcome,
        format!("Ok(5000) | Ok(5000)\nOk(3000) | Ok(3000)\n{u1001} | {u1001}\n")
    );
}

/// The same through the callback of a higher-order builtin called as a tail
/// call, which the trampoline runs without an ordinary evaluation step in
/// between (#191): each level is guarded all the same, so deep recursion
/// evaluates and with no base case it is U1001, not a crash.
#[test]
fn test_recursion_through_tail_called_builtin_callbacks() {
    let outcome = on_small_stack(|| {
        let mut out = String::new();
        for expr in [
            "($f := function($n){ $n = 0 ? 0 : $reduce([$n, 1], function($a, $b){ $f($a - 1) }) }; $f(5000))",
            "($g := function($x){ $f($x - 1) }; \
              $f := function($n){ $n = 0 ? 0 : $reduce([$n, 1], function($a, $b){ $g($a) }) }; $f(5000))",
            "($f := function($n){ $n = 0 ? 'done' : $map([$n], function($x){ $f($x - 1) }) }; $count($f(5000)))",
            "($f := function($n){ $n = 0 ? 'done' : $filter([$n], function($x){ $f($x - 1) }) }; $count($f(5000)))",
            "($f := function($n){ $n = 0 ? 'done' : $each({'k': $n}, function($x){ $f($x - 1) }) }; $count($f(5000)))",
            "($d := $reduce([1..3000], function($a, $i){ {'c': [$a]} }, {'v': 1}); \
              $f := function($n){ $n.c ? $map($n.c, $f) : $n.v }; $count($f($d)))",
            "($f := function($n){ $n <= 0 ? 0 : ($f ~> $number)($n - 1) }; $f(5000))",
            "($f := function($n){ $map([$n], function($x){ $f($x + 1) }) }; $f(0))",
            "($f := function($n){ $reduce([$n, 1], function($a, $b){ $f($a + 1) }) }; $f(0))",
            "($f := function($n){ ($f ~> $number)($n + 1) }; $f(0))",
        ] {
            let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
            out.push_str(&format!("{tree_walker} | {default}\n"));
        }
        out
    });
    let u1001 =
        "Err(Evaluation error: U1001: Stack overflow error: evaluation nested deeper than 30000 levels. Check for non-terminating recursive function. Consider rewriting as tail-recursive.)";
    assert_eq!(
        outcome,
        format!(
            "Ok(0) | Ok(0)\nOk(0) | Ok(0)\nOk(1) | Ok(1)\nOk(1) | Ok(1)\nOk(1) | Ok(1)\n\
             Ok(1) | Ok(1)\nOk(0) | Ok(0)\n{u1001} | {u1001}\n{u1001} | {u1001}\n{u1001} | {u1001}\n"
        )
    );
}

/// The same through a tail call of a value that is not a thunk function: a
/// partial application (`$f($n - 1, ?)()`, also when returned from another
/// call), a partial application of a builtin, and a builtin called as a value
/// (`($reduce)(...)`). These run outside the trampoline's ordinary evaluation
/// step and used to overflow the native stack at about 2,000 levels (#255);
/// each level is guarded now, so deep recursion evaluates and with no base
/// case it is U1001, not a crash.
#[test]
fn test_recursion_through_tail_called_partial_applications() {
    let outcome = on_small_stack(|| {
        let mut out = String::new();
        for expr in [
            "($f := function($n){ $n <= 0 ? 1 : $f($n - 1, ?)() }; $f(5000))",
            "($g := function($n){ $f($n - 1, ?) }; \
              $f := function($n){ $n <= 0 ? 1 : $g($n)() }; $f(5000))",
            "($f := function($n){ $n <= 0 ? 1 : $reduce(?, function($a, $b){ $f($b) }, 0)([$n - 1]) }; $f(5000))",
            "($f := function($n){ $n <= 0 ? 1 : ($reduce)([$n - 1], function($a, $b){ $f($b) }, 0) }; $f(5000))",
            "($f := function($n){ $n <= 0 ? 1 : $eval(?)('$f(' & ($n - 1) & ')') }; $f(5000))",
            "($f := function($n){ $f($n + 1, ?)() }; $f(0))",
            "($f := function($n){ ($reduce)([$n + 1], function($a, $b){ $f($b) }, 0) }; $f(0))",
        ] {
            let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
            out.push_str(&format!("{tree_walker} | {default}\n"));
        }
        out
    });
    let u1001 = "Err(Evaluation error: U1001: Stack overflow error: evaluation nested deeper \
                 than 30000 levels. Check for non-terminating recursive function. Consider \
                 rewriting as tail-recursive.)";
    assert_eq!(
        outcome,
        format!(
            "Ok(1) | Ok(1)\nOk(1) | Ok(1)\nOk(1) | Ok(1)\nOk(1) | Ok(1)\nOk(1) | Ok(1)\n\
             {u1001} | {u1001}\n{u1001} | {u1001}\n"
        )
    );
}

/// The same recursion stops at a timeout (D1012) when one is set.
#[test]
fn test_recursion_through_tail_called_builtin_callbacks_times_out() {
    let outcome = on_small_stack(|| {
        let options = EvaluatorOptions {
            timeout_ms: Some(1),
            ..Default::default()
        };
        let (tree_walker, default) = eval_both_engines(
            "($f := function($n){ $map([$n], function($x){ $f($x + 1) }) }; $f(0))",
            options,
        );
        format!("{tree_walker} | {default}")
    });
    assert!(
        outcome.matches("U1001").count() + outcome.matches("D1012").count() == 2,
        "expected U1001 or D1012 from both engines, got: {outcome}"
    );
}

/// A user-configured `max_stack_depth` raises D1011, jsonata-js's error for
/// its `stack` option, on both engines; recursion that fits under it runs.
#[test]
fn test_max_stack_depth_raises_d1011() {
    let outcome = on_small_stack(|| {
        let options = |depth| EvaluatorOptions {
            max_stack_depth: Some(depth),
            ..Default::default()
        };
        let mut out = String::new();
        for (expr, depth) in [
            ("($inf := function($n){$n+$inf($n-1)}; $inf(5))", 10),
            (&*NON_TAIL_RECURSION.replace('N', "3000"), 5000),
            (&*NON_TAIL_RECURSION.replace('N', "3000"), 10_000),
        ] {
            let (tree_walker, default) = eval_both_engines(expr, options(depth));
            out.push_str(&format!("{depth}: {tree_walker} | {default}\n"));
        }
        out
    });
    let d1011 = "Err(Evaluation error: D1011: Stack overflow. Check for non-terminating recursive function. \
                 Consider rewriting as tail-recursive)";
    assert_eq!(
        outcome,
        format!("10: {d1011} | {d1011}\n5000: {d1011} | {d1011}\n10000: Ok(3000) | Ok(3000)\n")
    );
}

/// A `max_stack_depth` at or above the built-in limit changes nothing: the
/// built-in limit (and its U1001) still applies.
#[test]
fn test_max_stack_depth_above_built_in_limit_still_raises_u1001() {
    let outcome = on_small_stack(|| {
        let options = EvaluatorOptions {
            max_stack_depth: Some(100_000),
            ..Default::default()
        };
        eval_both_engines("($inf := function($n){$n+$inf($n-1)}; $inf(5))", options).0
    });
    assert!(outcome.contains("U1001"), "expected U1001, got: {outcome}");
}

/// A deeply nested expression evaluated at every level of a deep recursion:
/// compiled sub-expressions (here a `$map` body) and tree-walked ones must
/// both fit in the stack left at that depth, and an expression nested past
/// the parser's limit is still a clean U1002.
#[test]
fn test_deeply_nested_expression_inside_deep_recursion() {
    let outcome = on_small_stack(|| {
        let mut out = String::new();
        for nesting in [50, 300] {
            let inner = format!("{}$x{}", "(".repeat(nesting), "+0)".repeat(nesting));
            let expr = format!(
                "($f := function($n){{ $n <= 0 ? 0 : 1 + $f($map([$n], function($x){{{inner}}}) - 1) }}; $f(3000))"
            );
            let (tree_walker, default) = eval_both_engines(&expr, EvaluatorOptions::default());
            out.push_str(&format!("{nesting}: {tree_walker} {default}\n"));
        }
        let too_deep = format!("{}1{}", "[".repeat(5000), "]".repeat(5000));
        match parse(&too_deep) {
            Ok(_) => out.push_str("parsed"),
            Err(e) => out.push_str(&e.to_string()),
        }
        out
    });
    assert!(
        outcome.starts_with("50: Ok(3000) Ok(3000)\n300: Ok(3000) Ok(3000)\n"),
        "got: {outcome}"
    );
    assert!(outcome.contains("U1002"), "got: {outcome}");
}

/// Object construction must drop keys whose value is an undefined (no-match)
/// path, matching reference JSONata and this crate's own VM backend - not
/// keep them as an explicit `null` (see GitHub issue #32).
#[test]
fn test_object_construction_drops_undefined_valued_keys() {
    let data: JValue = json!({"a": 1}).into();

    let ast = parse(r#"{ "keep": a, "drop": b }"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!({"keep": 1})));
}

/// Same as above, but through a multi-step dotted path (a.b.c) rather than a
/// bare name - a separate code path within the tree-walker that had the same
/// bug (see GitHub issue #32).
#[test]
fn test_object_construction_drops_undefined_valued_dotted_path() {
    let data: JValue = json!({"a": {}}).into();

    let ast = parse(r#"{ "k": a.b.c }"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!({})));
}

/// An *explicit* null value must still be kept - only undefined (missing
/// path) values are dropped.
#[test]
fn test_object_construction_keeps_explicit_null() {
    let data = JValue::Null;

    let ast = parse(r#"{ "k": $exists(x) ? x : null }"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!({"k": null})));
}

/// Lambda ids were derived from the AST node's pointer address, which is
/// constant for a given lambda expression but gets evaluated fresh on every
/// invocation of a recursive lambda (Y-combinator style). Two invocations of
/// the *same* lambda expression - each creating its own closure with its own
/// captured environment - collided on the same id and aliased each other,
/// intermittently producing wrong results or spurious recursion-depth errors
/// (see GitHub issue #35). Repeats many times since the bug was probabilistic.
#[test]
fn test_recursive_lambda_ids_do_not_collide() {
    let expr = "λ($f) { λ($x) { $x($x) }( λ($g) { $f( (λ($a) {$g($g)($a)}))})}\
                (λ($f) { λ($n) { $n < 2 ? 1 : $n * $f($n - 1) } })(6)";
    let ast = parse(expr).unwrap();
    let data = JValue::Null;

    for i in 0..3000 {
        let mut evaluator = Evaluator::new();
        let result = evaluator
            .evaluate(&ast, &data)
            .unwrap_or_else(|e| panic!("iteration {i} failed: {e}"));
        assert_eq!(result, JValue::from(json!(720.0)), "iteration {i}");
    }
}

// --- Task 6: `%` inside filter predicates and sort terms (runtime) ---
//
// Regression guard for the runtime half of Task 6. The `ast_transform` unit
// tests only assert AST *tagging*; these assert the resolved VALUES end-to-end,
// so the fragile runtime pieces (create_tuple_stream's incoming-tuple bindings
// for `%.%` chains, filter stages applied to the built tuple stream, and
// routing a `%` path-step over a tuple stream) fail loudly if broken. Expected values
// are from tests/jsonata-js/.../parent-operator/parent.json (dataset5).
//
// The result is still a tuple stream (final output-unwrap is Task 7), so we
// extract the `@` values recursively before comparing.
fn tuple_at_values(v: &JValue) -> Vec<JValue> {
    match v {
        JValue::Array(arr) => arr.iter().flat_map(tuple_at_values).collect(),
        JValue::Object(o) if o.get("__tuple__").and_then(|b| b.as_bool()) == Some(true) => {
            match o.get("@") {
                Some(inner) => tuple_at_values(inner),
                None => vec![],
            }
        }
        other => vec![other.clone()],
    }
}

fn dataset5() -> JValue {
    let s = include_str!("jsonata-js/test/test-suite/datasets/dataset5.json");
    serde_json::from_str::<serde_json::Value>(s).unwrap().into()
}

fn eval_tuple_at(expr: &str) -> Vec<JValue> {
    let ast = parse(expr).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &dataset5()).unwrap();
    tuple_at_values(&result)
}

#[test]
fn test_percent_in_predicate_on_parent_step() {
    // parent.json: "...Price.%[%.OrderID='order103'].SKU" -> the % step over a
    // tuple stream, with a predicate whose own % resolves to Product.
    let got = eval_tuple_at("Account.Order.Product.Price.%[%.OrderID='order103'].SKU");
    let want: Vec<JValue> = ["0406654608", "0406634348"]
        .iter()
        .map(|s| JValue::from(json!(s)))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn test_percent_chain_in_predicate() {
    // "...Product[%.%.`Account Name`='Firefly'].SKU" exercises the %.% chain
    // inside a predicate (first % -> Product, second % -> Order/Account name).
    let got = eval_tuple_at("Account.Order.Product[%.%.`Account Name`='Firefly'].SKU");
    let want: Vec<JValue> = ["0406654608", "0406634348", "040657863", "0406654603"]
        .iter()
        .map(|s| JValue::from(json!(s)))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn test_percent_in_two_sort_terms() {
    // "...SKU^(%.Price, >%.%.OrderID)" -- both sort terms use %; ordering must
    // match parent.json (primary %.Price asc, secondary %.%.OrderID desc).
    let got = eval_tuple_at("Account.Order.Product.SKU^(%.Price, >%.%.OrderID)");
    let want: Vec<JValue> = ["0406634348", "040657863", "0406654608", "0406654603"]
        .iter()
        .map(|s| JValue::from(json!(s)))
        .collect();
    assert_eq!(got, want);
}

/// A configured timeout must trip D1012 on a pathologically slow (but
/// terminating) expression. Note: the brief's originally proposed shape (a
/// 200,000-term chained-addition string, "1+1+1+...+1") was tried first and
/// rejected — see the task report for why (it overflows the native stack
/// inside the recursive-descent *parser* itself, before evaluation even
/// starts, regardless of any timeout). `$map` over a large range with cheap
/// per-element work is expensive to evaluate node-by-node (~200,000 lambda
/// invocations) without producing a deeply-nested AST, so it exercises the
/// D1012 checkpoint without tripping D1011/U1001 or a parser stack overflow.
#[test]
fn test_timeout_raises_d1012() {
    let data = JValue::Null;
    let expr_str = "$map([1..200000], function($x){$x*2})";
    let ast = parse(expr_str).unwrap();
    let options = jsonata_core::evaluator::EvaluatorOptions {
        timeout_ms: Some(1), // 1ms — must expire before 200k lambda invocations finish
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let result = evaluator.evaluate(&ast, &data);
    let err = result.expect_err("expected a D1012 timeout error");
    assert!(
        err.to_string().contains("D1012"),
        "expected D1012, got: {err}"
    );
}

/// No timeout configured (the default) must never raise D1012, however long
/// evaluation takes.
#[test]
fn test_no_timeout_configured_never_raises_d1012() {
    let data = JValue::Null;
    let expr_str = "$map([1..200000], function($x){$x*2})";
    let ast = parse(expr_str).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);
    assert!(result.is_ok(), "expected Ok, got: {result:?}");
}

/// Options carrying `token`, with a 10 s timeout so a cancel that is never
/// seen fails with D1012 instead of hanging the test run.
fn cancellable(token: &CancelToken) -> EvaluatorOptions {
    EvaluatorOptions {
        cancel: Some(token.clone()),
        timeout_ms: Some(10_000),
        ..Default::default()
    }
}

/// Evaluates `expr_str` with a host function `$tick` that cancels the
/// evaluation on its `cancel_on`th call. It returns `"x"`, since a string suits
/// every callback slot and a `$replace` replacement must return one. Returns
/// the error and how many times `$tick` ran.
fn evaluate_cancelled_on_call(
    expr_str: &str,
    cancel_on: usize,
) -> (jsonata_core::evaluator::EvaluatorError, usize) {
    let ast = parse(expr_str).unwrap();
    let token = CancelToken::new();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut evaluator = Evaluator::with_options(Context::new(), cancellable(&token));
    let (tick_token, tick_calls) = (token.clone(), calls.clone());
    evaluator
        .register_fn("tick", move |_args: &[JValue]| {
            tick_calls.set(tick_calls.get() + 1);
            if tick_calls.get() == cancel_on {
                tick_token.cancel();
            }
            Ok(JValue::string("x"))
        })
        .unwrap();
    let err = evaluator
        .evaluate(&ast, &JValue::Null)
        .expect_err("expected a U1003 cancellation");
    (err, calls.get())
}

/// A token cancelled before evaluation starts stops it at the first node.
#[test]
fn test_cancel_before_evaluation_raises_u1003() {
    let ast = parse("$sum([1..10])").unwrap();
    let token = CancelToken::new();
    token.cancel();
    let err = Evaluator::with_options(Context::new(), cancellable(&token))
        .evaluate(&ast, &JValue::Null)
        .expect_err("expected U1003");
    assert!(
        err.to_string().contains("U1003"),
        "expected U1003, got: {err}"
    );
}

/// A token cancelled before the run stops a compiled `Expression` too, whose
/// simple expressions run on the bytecode VM rather than the tree-walker.
#[test]
fn test_cancel_before_evaluation_raises_u1003_on_compiled_expression() {
    let token = CancelToken::new();
    token.cancel();
    let data = JValue::from(json!({"a": 1}));
    for expr_str in ["a + 1", "a", "$sum([a, 2])"] {
        let expr =
            jsonata_core::Expression::compile_with_options(expr_str, cancellable(&token)).unwrap();
        let err = expr.evaluate(&data).expect_err("expected U1003");
        assert!(
            err.to_string().contains("U1003"),
            "{expr_str}: expected U1003, got: {err}"
        );
    }
}

/// A runaway tail-recursive loop, which has no iteration cap, stops once
/// another thread cancels it. The trampoline checks on every pass.
#[test]
fn test_cancel_from_another_thread_stops_tail_recursion() {
    let ast = parse("($inf := function(){$inf()}; $inf())").unwrap();
    let token = CancelToken::new();
    let canceller = token.clone();
    let cancel_thread = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        canceller.cancel();
    });
    let err = Evaluator::with_options(Context::new(), cancellable(&token))
        .evaluate(&ast, &JValue::Null)
        .expect_err("expected U1003");
    cancel_thread.join().unwrap();
    assert!(
        err.to_string().contains("U1003"),
        "expected U1003, got: {err}"
    );
}

/// A cancel stops a higher-order builtin between elements, whether its
/// callback is a lambda, whose body is checked like any node, or a host
/// function, whose call evaluates no node and is checked by the loop.
#[test]
fn test_cancel_stops_higher_order_builtins_between_elements() {
    let cases = [
        "$map([1..2000], function($x){ $tick($x) })",
        "$map([1..2000], $tick)",
        "$filter([1..2000], $tick)",
        "$reduce([1..2000], $tick)",
        "$single([1..2000], $tick)",
        "$sift($merge([1..2000].{$string($): $}), $tick)",
        "$each($merge([1..2000].{$string($): $}), $tick)",
        "$sort([1..2000], $tick)",
        "$replace($pad('', 2000, 'a'), /a/, $tick)",
    ];
    for expr_str in cases {
        let (err, calls) = evaluate_cancelled_on_call(expr_str, 100);
        assert!(
            err.to_string().contains("U1003"),
            "{expr_str}: expected U1003, got: {err}"
        );
        assert_eq!(calls, 100, "{expr_str}: $tick ran {calls} times");
    }
}

/// `timeout_ms` stops `$map` over a host callback too. The loop had no
/// check, so a slow callback ran past the deadline to the end of the array.
#[test]
fn test_timeout_stops_map_over_a_host_callback() {
    let ast = parse("$map([1..1000], $slow)").unwrap();
    let options = EvaluatorOptions {
        timeout_ms: Some(50),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    evaluator
        .register_fn("slow", |args: &[JValue]| {
            std::thread::sleep(std::time::Duration::from_millis(1));
            Ok(args.first().cloned().unwrap_or(JValue::Null))
        })
        .unwrap();
    let err = evaluator
        .evaluate(&ast, &JValue::Null)
        .expect_err("expected D1012");
    assert!(
        err.to_string().contains("D1012"),
        "expected D1012, got: {err}"
    );
}

/// A token that is never cancelled changes no result.
#[test]
fn test_uncancelled_token_changes_nothing() {
    let ast = parse("$map([1..1000], function($x){$x*2})").unwrap();
    let token = CancelToken::new();
    let with_token = Evaluator::with_options(Context::new(), cancellable(&token))
        .evaluate(&ast, &JValue::Null)
        .unwrap();
    let without = Evaluator::new().evaluate(&ast, &JValue::Null).unwrap();
    assert_eq!(with_token, without);
}

/// jsonata-js's own canonical guardrails-documentation example: an infinite
/// tail-recursive call. `invoke_lambda_with_tco`'s trampoline loop previously
/// had no timeout check at all -- only a hardcoded 100,000-iteration cap,
/// since removed (#251) -- so this expression ignored `timeout_ms` and failed
/// with U1001 instead of D1012. Now the timeout is all that stops it.
#[test]
fn test_tco_trampoline_infinite_recursion_raises_d1012_timeout() {
    let data = JValue::Null;
    let expr_str = "($inf := function(){$inf()}; $inf())";
    let ast = parse(expr_str).unwrap();
    let options = jsonata_core::evaluator::EvaluatorOptions {
        timeout_ms: Some(5),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator
        .evaluate(&ast, &data)
        .expect_err("expected a D1012 timeout error");
    assert!(
        err.to_string().contains("D1012"),
        "expected D1012, got: {err}"
    );
}

/// Without a configured timeout, the new per-iteration `check_loop_timeout`
/// call in the TCO trampoline must remain a no-op (it early-returns when
/// `options.timeout_ms` is `None`), so a bounded tail-recursive loop still
/// completes normally and produces the correct result. Uses a moderate bound
/// to keep the test suite fast; `test_tail_loop_has_no_iteration_cap` covers
/// long loops.
#[test]
fn test_tco_trampoline_no_timeout_configured_completes_normally() {
    let data = JValue::Null;
    let expr_str = "(\
        $count := function($n, $acc){$n <= 0 ? $acc : $count($n - 1, $acc + 1)};\
        $count(1000, 0)\
    )";
    let ast = parse(expr_str).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::from(json!(1000.0)));
}

/// A tail loop has no iteration cap, as in jsonata-js (GitHub issue #251):
/// it used to stop at 100,000 iterations with "U1001: ... maximum recursion
/// depth (500) exceeded", a depth nothing enforced. 200,000 iterations, a
/// mutually tail-recursive pair, and a tail loop under a configured (but
/// generous) timeout all finish, on both engines, on a 1MB stack.
#[test]
fn test_tail_loop_has_no_iteration_cap() {
    let outcome = on_small_stack(|| {
        let mut out = String::new();
        for (expr, options) in [
            (
                "($loop := function($n){ $n = 0 ? \"done\" : $loop($n - 1) }; $loop(200000))",
                EvaluatorOptions::default(),
            ),
            (
                "($even := function($n){ $n = 0 ? true : $odd($n - 1) };\
                  $odd := function($n){ $n = 0 ? false : $even($n - 1) }; $odd(200001))",
                EvaluatorOptions::default(),
            ),
            (
                "($loop := function($n){ $n = 0 ? \"done\" : $loop($n - 1) }; $loop(200000))",
                EvaluatorOptions {
                    timeout_ms: Some(600_000),
                    ..Default::default()
                },
            ),
        ] {
            let (tree_walker, default) = eval_both_engines(expr, options);
            out.push_str(&format!("{tree_walker} {default}\n"));
        }
        out
    });
    assert_eq!(
        outcome,
        "Ok(\"done\") Ok(\"done\")\nOk(true) Ok(true)\nOk(\"done\") Ok(\"done\")\n"
    );
}

/// With no cap on tail calls, the guardrails still hold around them: a
/// `max_stack_depth` still raises D1011 for real (non-tail) recursion, even
/// inside a long tail loop, and a timeout still stops an infinite tail loop
/// (D1012) on both engines.
#[test]
fn test_tail_loop_guardrails_still_apply() {
    let outcome = on_small_stack(|| {
        let depth = EvaluatorOptions {
            max_stack_depth: Some(500),
            ..Default::default()
        };
        // 150,000 tail iterations within a depth limit of 500 levels...
        let (tw_tail, vm_tail) = eval_both_engines(
            "($loop := function($n){ $n = 0 ? \"done\" : $loop($n - 1) }; $loop(150000))",
            depth.clone(),
        );
        // ...while 500 nested non-tail calls exceed it.
        let (tw_deep, vm_deep) = eval_both_engines(&NON_TAIL_RECURSION.replace('N', "500"), depth);
        let timeout = EvaluatorOptions {
            timeout_ms: Some(50),
            ..Default::default()
        };
        let (tw_inf, vm_inf) = eval_both_engines("($inf := function(){$inf()}; $inf())", timeout);
        format!("{tw_tail} {vm_tail}\n{tw_deep}\n{vm_deep}\n{tw_inf}\n{vm_inf}")
    });
    let lines: Vec<&str> = outcome.lines().collect();
    assert_eq!(lines[0], "Ok(\"done\") Ok(\"done\")", "got: {outcome}");
    assert!(
        lines[1].contains("D1011") && lines[2].contains("D1011"),
        "got: {outcome}"
    );
    assert!(
        lines[3].contains("D1012") && lines[4].contains("D1012"),
        "got: {outcome}"
    );
}

/// The range operator must respect `max_sequence_length` (D2015) in addition
/// to its existing hardcoded 10-million-element cap (D2014) — mirrors
/// jsonata-js's `evaluateRangeExpression`, which checks D2014 then D2015.
#[test]
fn test_range_operator_raises_d2015_when_configured() {
    let data = JValue::Null;
    let ast = parse("[1..1000]").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// Without `max_sequence_length` set, ranges up to the existing 10M hardcoded
/// cap are unaffected.
#[test]
fn test_range_operator_unaffected_without_max_sequence_length() {
    let data = JValue::Null;
    let ast = parse("[1..1000]").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    match result {
        JValue::Array(arr) => assert_eq!(arr.len(), 1000),
        other => panic!("expected array, got {other:?}"),
    }
}

/// Plain field-path mapping over an array (e.g. `items.name`) is a
/// query-result sequence per jsonata-js's `evaluatePath`/`evaluateStep` and
/// must respect `max_sequence_length`.
#[test]
fn test_path_mapping_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!({"name": format!("item{i}")})).collect::<Vec<_>>()
    }).into();
    let ast = parse("items.name").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// Top-level wildcard `*` over a large object must respect max_sequence_length.
#[test]
fn test_wildcard_raises_d2015() {
    let mut obj = serde_json::Map::new();
    for i in 0..1000 {
        obj.insert(format!("k{i}"), serde_json::json!(i));
    }
    let data: JValue = serde_json::Value::Object(obj).into();
    let ast = parse("*").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// Top-level descendant `**` over a deeply-nested structure must respect
/// max_sequence_length.
#[test]
fn test_descendant_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!({"v": i})).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("**").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// `evaluate_path`'s single-step fast path (`steps.len() == 1`, `AstNode::Name`,
/// non-tuple `JValue::Array` branch, src/evaluator.rs ~4030-4066) is reached via
/// its own internal recursion for a NESTED array element (`items` is an array
/// containing one element that is itself a 1000-element array of `{name}`
/// objects). This exercises and covers the fast path's own D2015 check (its
/// `return` skips this `evaluate_path` invocation's shared final-return
/// chokepoint). Note: for this specific construction the *outer* `items.name`
/// call still separately hits the shared chokepoint too (its own `result`
/// accumulates the same 1000 elements via the general per-step loop before
/// returning), so this particular data shape alone doesn't prove the fast
/// path's own check is independently load-bearing — see
/// `test_bare_single_step_path_over_root_array_raises_d2015` below for the
/// top-level, no-enclosing-frame construction that does prove it. Both tests
/// are kept: this one covers the nested-recursion route into the fast path,
/// the other covers the top-level route.
#[test]
fn test_path_single_step_fast_path_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": [(0..1000).map(|i| serde_json::json!({"name": format!("item{i}")})).collect::<Vec<_>>()]
    }).into();
    let ast = parse("items.name").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// `evaluate_path`'s 2-step `$variable.field` fast path (src/evaluator.rs
/// ~4181-4206) `return`s directly and bypasses the shared final-return
/// chokepoint, so it needs its own independent D2015 check.
#[test]
fn test_path_variable_field_fast_path_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!({"name": format!("item{i}")})).collect::<Vec<_>>()
    }).into();
    let ast = parse("($v := items; $v.name)").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// Bare single-segment field access (`name`, no dots) over root data that IS
/// itself a large array is a genuine top-level call into evaluate_path's
/// steps.len()==1 fast path, with no enclosing evaluate_path frame to
/// backstop it via the shared final-exit chokepoint — this is the case that
/// makes the check at src/evaluator.rs's single-step fast path (non-tuple
/// array branch) load-bearing, not redundant.
#[test]
fn test_bare_single_step_path_over_root_array_raises_d2015() {
    let data: JValue = serde_json::json!((0..1000)
        .map(|i| serde_json::json!({"name": format!("item{i}")}))
        .collect::<Vec<_>>())
    .into();
    let ast = parse("name").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// $map's generic (non-compiled-fast-path) construction must respect
/// max_sequence_length. Using `$x.*` as the lambda body (not compilable, per
/// try_compile_expr_with_allowed_vars's lack of a Wildcard arm) forces this
/// through the generic per-element loop, not the CompiledExpr fast path.
#[test]
fn test_map_generic_path_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!({"a": i})).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("$map(items, function($x){$x.*})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// $map's CompiledExpr fast path (inline lambda, single param, compilable
/// arithmetic body) must ALSO respect max_sequence_length -- this is a
/// distinct return point from the generic loop above, and a prior task
/// (Task 5) found a real bug where only one of several return points was
/// instrumented. `$x*2` is compilable per try_compile_expr_with_allowed_vars.
#[test]
fn test_map_compiled_fast_path_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!(i)).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("$map(items, function($x){$x*2})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// $filter's generic (non-compiled-fast-path) loop must respect
/// max_sequence_length. `$x.*` is not compilable, forcing the generic path.
#[test]
fn test_filter_generic_path_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!({"a": i})).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("$filter(items, function($x){$x.* != null})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// $filter's CompiledExpr fast path (inline lambda, single param, compilable
/// comparison body) must ALSO respect max_sequence_length -- a distinct
/// return point from the generic loop above.
#[test]
fn test_filter_compiled_fast_path_raises_d2015() {
    let data: JValue = serde_json::json!({
        "items": (0..1000).map(|i| serde_json::json!(i)).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("$filter(items, function($x){$x >= 0})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

// ── Task 8: eval_compiled_inner's CompiledExpr::MapCall/FilterCall/ReduceCall
// and compiled_apply_filter guardrail coverage ──────────────────────────────
//
// The tests above (`test_map_compiled_fast_path_raises_d2015` /
// `test_filter_compiled_fast_path_raises_d2015`) exercise a *different* fast
// path: `evaluate_function_call`'s hand-rolled "map"/"filter" arms, which
// compile only the lambda *body* and loop over the array in the tree walker
// (already guarded by an earlier task's `check_sequence_length`). A bare
// top-level `$map(...)`/`$filter(...)` call is dispatched there directly and
// never reaches `eval_compiled_inner`'s `CompiledExpr::MapCall`/`FilterCall`/
// `ReduceCall` arms.
//
// A naive zero-arg IIFE wrapper (`(function(){...})()`) does NOT reach them
// either: the parser marks *any* lambda whose body's tail position is a
// function call as a TCO "thunk" (`parser.rs::is_tail_call`), and thunks
// unconditionally get `compiled_body: None` (both at `AstNode::Lambda`
// definition time and via `extract_inline_lambda`'s `thunk: false` guard),
// so they always fall back to the full tree-walker -- confirmed empirically
// (a temporary `eprintln!` in each target arm showed zero hits, and the
// "D2015" assertions were passing for the wrong reason: the tree-walker's
// already-guarded path #2 above, not this task's target).
//
// The reliable way to reach `eval_compiled_inner`'s HOF arms is the pattern
// the arms' own doc comment describes: nest the HOF/path call as a
// *non-tail* expression inside another (already thunk:false) lambda body
// compiled by path #2, e.g. `function($x){ (INNER_CALL; 1) }` -- wrapping in
// a `Block` whose last expression is a plain literal keeps the outer
// callback's tail position a non-function-call, so it stays thunk:false and
// compiles via `try_compile_expr_with_allowed_vars`. That compiled `Block`
// (`CompiledExpr::Block([inner_call_compiled, Literal(1)])`) is then run via
// `eval_compiled_inner`, which evaluates `inner_call_compiled` first --
// landing squarely in the target arm. Confirmed empirically the same way
// (temporary `eprintln!`s in each target arm all fired for these exact
// expressions, and all returned `is_err() == false` pre-implementation,
// i.e. genuinely unguarded fast paths, not false negatives).
//
// `outer` is a single-element array purely to drive one iteration of the
// (harmless, already-guarded) outer path #2 loop; all the guardrail action
// happens in the untouched inner call.

/// $map's CompiledExpr::MapCall arm must respect max_sequence_length.
/// Distinct return point from `test_map_compiled_fast_path_raises_d2015` above.
#[test]
fn test_map_eval_compiled_inner_raises_d2015() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..1000).collect::<Vec<i32>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){($map(items, function($y){$y*2}); 1)})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// A timeout must trip inside CompiledExpr::MapCall's per-element loop, even
/// though it never passes through evaluate_internal's per-node checkpoint.
///
/// `timeout_ms: 200` + 2,000,000 items (not a tighter/smaller budget): reaching
/// this arm first runs through `evaluate_function_call`'s generic "evaluate all
/// args" preamble for the OUTER `$map`, which itself evaluates+compiles the
/// callback `Lambda` -- setup that was empirically observed to occasionally
/// take several ms under load. 200ms comfortably clears that, while the full
/// uninstrumented loop reliably takes 700ms+, giving a wide, non-flaky margin.
#[test]
fn test_map_eval_compiled_inner_raises_d1012_timeout() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..2_000_000).collect::<Vec<i32>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){($map(items, function($y){$y*2}); 1)})").unwrap();
    let options = EvaluatorOptions {
        timeout_ms: Some(200),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D1012");
    assert!(err.to_string().contains("D1012"), "got: {err}");
}

/// $filter's CompiledExpr::FilterCall arm must respect max_sequence_length.
#[test]
fn test_filter_eval_compiled_inner_raises_d2015() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..1000).collect::<Vec<i32>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){($filter(items, function($y){$y>=0}); 1)})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// A timeout must trip inside CompiledExpr::FilterCall's per-element loop.
/// See `test_map_eval_compiled_inner_raises_d1012_timeout` for why this uses
/// a 200ms/2,000,000-item margin rather than a razor-thin budget.
#[test]
fn test_filter_eval_compiled_inner_raises_d1012_timeout() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..2_000_000).collect::<Vec<i32>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){($filter(items, function($y){$y>=0}); 1)})").unwrap();
    let options = EvaluatorOptions {
        timeout_ms: Some(200),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D1012");
    assert!(err.to_string().contains("D1012"), "got: {err}");
}

/// A timeout must trip inside CompiledExpr::ReduceCall's per-element loop.
/// No corresponding D2015 test: jsonata-js's own `reduce()` never calls
/// `createSequence()`, so the accumulator is intentionally uncapped upstream
/// too -- ReduceCall must NOT gain a check_sequence_length call. See
/// `test_map_eval_compiled_inner_raises_d1012_timeout` for why this uses a
/// 200ms/2,000,000-item margin rather than a razor-thin budget.
#[test]
fn test_reduce_eval_compiled_inner_raises_d1012_timeout() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..2_000_000).collect::<Vec<i32>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){($reduce(items, function($acc,$y){$acc+$y}); 1)})")
        .unwrap();
    let options = EvaluatorOptions {
        timeout_ms: Some(200),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D1012");
    assert!(err.to_string().contains("D1012"), "got: {err}");
}

/// A compiled path predicate (`items[val>=0]`, compiling to
/// CompiledExpr::FieldPath with a filter step) must respect
/// max_sequence_length via `compiled_apply_filter`.
#[test]
fn test_path_filter_eval_compiled_inner_raises_d2015() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..1000).map(|i| serde_json::json!({"val": i})).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){(items[val>=0]; 1)})").unwrap();
    let options = EvaluatorOptions {
        max_sequence_length: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D2015");
    assert!(err.to_string().contains("D2015"), "got: {err}");
}

/// A timeout must trip inside `compiled_apply_filter`'s per-element loop.
///
/// Uses a much larger margin than the sibling MapCall/FilterCall/ReduceCall
/// D1012 tests (`timeout_ms: 200` + 2,000,000 items, vs. their `1`ms +
/// 500,000): empirically, reaching this specific arm first runs through
/// `evaluate_function_call`'s generic "evaluate all args" preamble (which
/// evaluates the whole callback `Lambda` node, compiling its body) before
/// entering the fast path, and that setup alone was observed to occasionally
/// take several ms under load -- enough to make a 1ms budget flaky (it could
/// fire during setup, "passing" without ever exercising this arm's loop).
/// 200ms comfortably exceeds worst-case observed setup while the full
/// 2,000,000-element uninstrumented loop reliably takes 500-650ms, giving a
/// wide, non-flaky margin either side.
#[test]
fn test_path_filter_eval_compiled_inner_raises_d1012_timeout() {
    let data: JValue = serde_json::json!({
        "outer": [1],
        "items": (0..2_000_000).map(|i| serde_json::json!({"val": i})).collect::<Vec<_>>()
    })
    .into();
    let ast = parse("$map(outer, function($x){(items[val>=0]; 1)})").unwrap();
    let options = EvaluatorOptions {
        timeout_ms: Some(200),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D1012");
    assert!(err.to_string().contains("D1012"), "got: {err}");
}

/// A named/stored lambda (`$storedFn := function($x){$x*2}`) used as the
/// callback of a top-level `$map(...)` takes an entirely different bypass
/// than the CompiledExpr::MapCall/FilterCall/ReduceCall arms above: either
/// `evaluate_function_call`'s own "map" arm's dedicated "stored lambda
/// variable" fast path (a separate per-item loop calling `eval_compiled`
/// directly), or -- for other call shapes -- `invoke_stored_lambda`'s
/// compiled fast path (`src/evaluator.rs`, a single `eval_compiled` call per
/// invocation, reachable from any tree-walker per-element loop that resolves
/// a callback to a stored lambda). Neither of those loops is one of this
/// task's instrumented MapCall/FilterCall/ReduceCall/compiled_apply_filter
/// arms, so per-loop checks alone do not cover them.
///
/// This is closed structurally, not by enumerating more call sites: a single
/// `check_loop_timeout` at the top of `eval_compiled_inner` itself (before
/// its `match`) covers every route into compiled evaluation, since both
/// `eval_compiled` and `eval_compiled_shaped` -- and therefore every caller
/// of either -- funnel through that one function.
///
/// 2,000,000 elements / `timeout_ms: 50`: with the entry-point check firing
/// on literally every element (not just at loop boundaries), this trips far
/// more reliably/quickly than the coarser per-loop-only tests above -- a much
/// smaller timeout budget is safe here.
#[test]
fn test_map_stored_lambda_bypass_raises_d1012_timeout() {
    let data: JValue = serde_json::json!({
        "bigArray": (0..2_000_000).collect::<Vec<i32>>()
    })
    .into();
    let ast = parse("($storedFn := function($x){$x*2}; $map(bigArray, $storedFn))").unwrap();
    let options = EvaluatorOptions {
        timeout_ms: Some(50),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(Context::new(), options);
    let err = evaluator.evaluate(&ast, &data).expect_err("expected D1012");
    assert!(err.to_string().contains("D1012"), "got: {err}");
}

/// Deeply left-nested arithmetic (e.g. `1+1+1+...`) must hit a graceful
/// depth-guard error in ast_transform's post-parse pass, not crash the
/// whole process. Historical bug: `parser::parse()` unconditionally pipes
/// every parse through `ast_transform::resolve_ancestry`, whose recursive
/// AST walk had no depth guard (found during jsonata-js 2.2.1 Phase 2
/// guardrails work — see docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md).
/// Confirmed empirically (this session) that the raw Pratt parser handles
/// this input fine at n=200,000; the crash is entirely in the post-parse
/// ast_transform pass.
#[test]
fn test_deeply_nested_arithmetic_does_not_overflow_native_stack_at_parse_time() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024) // 1MB, matching Windows' default thread stack
        .spawn(|| {
            let expr_str = format!("({})", vec!["1"; 200_000].join("+"));
            match parse(&expr_str) {
                Ok(_) => "Ok".to_string(),
                Err(e) => format!("Err({e})"),
            }
        })
        .unwrap();

    let outcome = handle.join().expect(
        "expression parsing overflowed the native stack instead of returning a graceful error",
    );

    assert!(
        outcome.starts_with("Err"),
        "expected a graceful depth-limit error, got: {outcome}"
    );
    assert!(
        outcome.contains("U1002"),
        "expected the U1002 depth-limit error code, got: {outcome}"
    );
}

/// Companion to the crash-repro test above: reasonable nesting (well below
/// the ast_transform depth ceiling) must still parse successfully -- proves
/// the new guard doesn't fire on legitimate expressions. 200 arithmetic
/// terms produce ~400 recursive `transform_node`/`transform_children` stack
/// frames (each `+` adds two: `transform_node` -> `transform_children` ->
/// `transform_node`(lhs)), comfortably under the MAX_TRANSFORM_DEPTH=1000
/// ceiling -- empirically, the ceiling's actual cutoff for this shape of
/// expression is between n=500 (still `Ok`) and n=501 (`Err`), so 200 terms
/// leaves a healthy margin rather than sitting right at the edge.
#[test]
fn test_reasonable_nesting_still_parses_successfully() {
    let expr_str = format!("({})", vec!["1"; 200].join("+"));
    let result = parse(&expr_str);
    assert!(
        result.is_ok(),
        "expected reasonable nesting to parse fine, got: {result:?}"
    );
}

/// Deeply PARENTHESIZED nesting (e.g. `((((...))))`) must hit a graceful
/// depth-guard error during parsing itself, not crash the process. This is
/// a DIFFERENT crash than the flat-arithmetic-chain one Task 2 fixed in
/// ast_transform.rs: parens are real recursive descent inside
/// `parse_primary`/`parse_expression` (src/parser.rs), so this crashes
/// BEFORE ast_transform ever runs, and BEFORE ast_transform's own guard
/// gets a chance to protect anything. Found during Task 2's validation
/// (this session) — contradicts the original "crash is not in the parser"
/// framing for this specific shape (see the plan's Scope Amendment).
#[test]
fn test_deeply_nested_parens_does_not_overflow_native_stack_at_parse_time() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024) // 1MB, matching Windows' default thread stack
        .spawn(|| {
            let n = 5_000;
            let expr_str = format!("{}1{}", "(".repeat(n), ")".repeat(n));
            match parse(&expr_str) {
                Ok(_) => "Ok".to_string(),
                Err(e) => format!("Err({e})"),
            }
        })
        .unwrap();

    let outcome = handle.join().expect(
        "expression parsing overflowed the native stack instead of returning a graceful error",
    );

    assert!(
        outcome.starts_with("Err") && outcome.contains("U1002"),
        "expected a graceful U1002 depth-limit error, got: {outcome}"
    );
}

/// Reasonable paren nesting must still work (the guard must not fire on
/// legitimate expressions). Pick a depth comfortably below the chosen
/// ceiling (Step 4 picks the ceiling; this must stay well under it).
#[test]
fn test_reasonable_paren_nesting_still_parses() {
    let n = 50;
    let expr_str = format!("{}1{}", "(".repeat(n), ")".repeat(n));
    let result = parse(&expr_str);
    assert!(
        result.is_ok(),
        "reasonable paren nesting should parse fine, got: {result:?}"
    );
}

/// The flat left-associative arithmetic chain from Task 2 must now ALSO be
/// caught here, at parse-construction time, independent of ast_transform's
/// guard (which still runs afterward as defense-in-depth, but should never
/// see a tree deep enough to matter now).
#[test]
fn test_deeply_nested_arithmetic_caught_at_parse_construction_time() {
    let expr_str = format!("({})", vec!["1"; 200_000].join("+"));
    let result = parse(&expr_str);
    match result {
        Err(e) => {
            let msg = format!("{e}");
            assert!(msg.contains("U1002"), "expected U1002, got: {msg}");
            // U1002 is also used by ast_transform.rs's separate, earlier-added
            // post-parse guard ("while post-processing the parsed expression"),
            // so checking the error code alone isn't enough to prove the NEW
            // parser-level guard (added in this commit) is the one that fired.
            assert!(
                msg.contains("while parsing expression"),
                "expected the parser-level guard's message (not ast_transform's), got: {msg}"
            );
        }
        Ok(_) => panic!("expected a graceful depth-limit error"),
    }
}

/// Sibling subtrees must not inherit accumulated depth from an earlier
/// sibling. Each sibling here is a chain of 400 terms — safely under
/// MAX_PARSE_DEPTH (1000) on its own, but three of them summed (1200) would
/// exceed the ceiling if depth accumulated ACROSS siblings instead of being
/// restored to its pre-call value between them. This makes the test actually
/// load-bearing: a hypothetical "doesn't reset depth" bug would fail it,
/// unlike a shallow chain whose cumulative depth stays under the ceiling
/// regardless of whether reset happens.
#[test]
fn test_sibling_subtrees_do_not_inherit_depth() {
    let chain = vec!["1"; 400].join("+");
    let expr_str = format!("[{chain}, {chain}, {chain}]");
    let result = parse(&expr_str);
    assert!(
        result.is_ok(),
        "siblings should each parse independently without inheriting depth from a prior sibling, got: {result:?}"
    );
}

/// A literal array with more than u16::MAX (65,535) elements must not
/// silently truncate via Instr::MakeArray's u16 operand - it should either
/// evaluate correctly (falling back to the tree-walker, which has no such
/// limit) or be rejected with a clear error, but never silently return a
/// wrong-length result. Historical bug: compiler.rs:328's
/// `Instr::MakeArray(elems.len() as u16)` truncates silently (confirmed
/// empirically: a 100,000-element literal previously returned length 34,464
/// = 100,000 mod 65,536).
///
/// NOTE ON API CHOICE: this deliberately goes through `jsonata_core::_bench`
/// (compile-to-bytecode + VM run), NOT bare `Evaluator::evaluate`. Bare
/// `Evaluator::evaluate` never calls `try_compile_expr`/the VM at all for a
/// top-level array/object literal (`evaluate_internal_impl`'s
/// `AstNode::Array`/`AstNode::Object` arms are pure tree-walker code, with no
/// compiler involvement) - so a test built on it would pass identically
/// whether or not the u16-truncation guard exists, proving nothing. `_bench`
/// (feature = "bench", used by `benches/evaluator_bench.rs` for the same
/// reason) is the only way to reach the actual compiled path from outside
/// `src/`. This mirrors what production code (`JsonataExpression` in
/// `src/lib.rs`) really does: try to compile, and fall back to the
/// tree-walker when compilation declines (returns `None`).
#[cfg(feature = "bench")]
#[test]
fn test_large_literal_array_does_not_silently_truncate() {
    use jsonata_core::_bench;
    let n = 100_000;
    let expr_str = format!("[{}]", vec!["1"; n].join(","));
    let ast = parse(&expr_str).unwrap();
    let data = JValue::Null;
    let result = match _bench::compile(&ast) {
        Some(prog) => _bench::run(&prog, &data).unwrap(),
        None => Evaluator::new().evaluate(&ast, &data).unwrap(),
    };
    match result {
        JValue::Array(arr) => assert_eq!(arr.len(), n, "array literal was silently truncated"),
        other => panic!("expected array, got {other:?}"),
    }
}

/// Same shape for object literals and Instr::MakeObject. See the API-choice
/// note on `test_large_literal_array_does_not_silently_truncate` above.
#[cfg(feature = "bench")]
#[test]
fn test_large_literal_object_does_not_silently_truncate() {
    use jsonata_core::_bench;
    let n = 100_000;
    let pairs: Vec<String> = (0..n).map(|i| format!("\"k{i}\": {i}")).collect();
    let expr_str = format!("{{{}}}", pairs.join(","));
    let ast = parse(&expr_str).unwrap();
    let data = JValue::Null;
    let result = match _bench::compile(&ast) {
        Some(prog) => _bench::run(&prog, &data).unwrap(),
        None => Evaluator::new().evaluate(&ast, &data).unwrap(),
    };
    match result {
        JValue::Object(obj) => assert_eq!(obj.len(), n, "object literal was silently truncated"),
        other => panic!("expected object, got {other:?}"),
    }
}

/// A `(...; ...; ...)` block with more than u16::MAX sub-expressions must not
/// silently truncate via `Instr::BlockEnd`'s `u16` operand.
///
/// NOTE ON TEST SHAPE: a naive "does the block's OWN returned value survive"
/// test does not detect this bug. `Instr::BlockEnd` always `stack.pop()`s the
/// real top-of-stack first (the true last-evaluated element) regardless of
/// the (possibly truncated) `n` - so a bare block whose value is returned
/// directly always reports the correct last value even when truncated; the
/// only observable effect of a truncated `n` is that `BlockEnd` *under-drains*
/// the stack, leaving `(true_n - truncated_n)` stale elements buried
/// underneath. Those stale elements are invisible unless a LATER instruction's
/// own "top N" window reaches down far enough to swallow one of them instead
/// of the value that should logically be there.
///
/// So this test wraps the giant block as the *middle* element of a 3-element
/// array literal `[0, (5;5;...;5), 9]`, with the block body a single REPEATED
/// constant (not 70,000 distinct values) so that only `BlockEnd`'s `u16` is
/// exercised - distinct block values would additionally overflow
/// `const_pool`'s own `u16` index (a different truncation site), conflating
/// two bugs. Traced pre-fix (n = 70_000, truncated to 70_000 mod 65_536 =
/// 4_464): `BlockEnd` pops the true last `5` correctly but only removes
/// `4_463` of the `70_000` pushed copies, leaving the `0` canary buried more
/// than 3 slots below the top of stack. The subsequent `MakeArray(3)` then
/// takes the top 3 stack slots - which are two leftover `5`s and the `9` -
/// silently producing `[5, 5, 9]` instead of `[0, 5, 9]`. The `0` canary is
/// gone (and permanently leaked below, never popped). Post-fix, the new
/// `AstNode::Block` compiler guard makes the whole array literal decline to
/// compile (`?` propagates the `None`), so it falls back to the tree-walker,
/// which has no such limit and returns the correct `[0, 5, 9]`.
///
/// NOTE ON API CHOICE: like the two large-literal tests above, this must go
/// through `jsonata_core::_bench` (compile-to-bytecode + VM run), not bare
/// `Evaluator::evaluate` - `evaluate_internal_impl`'s `AstNode::Block` arm
/// (and `AstNode::Array`'s) is pure tree-walker code with no compiler
/// involvement, so a test built on bare `evaluate()` would pass identically
/// whether or not the guard exists, proving nothing.
#[cfg(feature = "bench")]
#[test]
fn test_large_block_does_not_silently_truncate() {
    use jsonata_core::_bench;
    let n = 70_000;
    let block_body = vec!["5"; n].join(";");
    let expr_str = format!("[0, ({block_body}), 9]");
    let ast = parse(&expr_str).unwrap();
    let data = JValue::Null;
    let result = match _bench::compile(&ast) {
        Some(prog) => _bench::run(&prog, &data).unwrap(),
        None => Evaluator::new().evaluate(&ast, &data).unwrap(),
    };
    assert_eq!(
        result,
        JValue::array(vec![
            JValue::from(0i64),
            JValue::from(5i64),
            JValue::from(9i64)
        ]),
        "large block silently corrupted a sibling array element via BlockEnd's truncated u16 operand"
    );
}

/// Two *sibling* blocks, each individually within every existing per-node
/// arity guard (`AstNode::Block`'s own `u16::MAX` element-count guard from
/// this same task, and `AstNode::Object`'s 2-pair count), can still overflow
/// `BytecodeCompiler`'s shared `const_pool` **cumulatively**: both blocks'
/// distinct literals are interned into the *same* `const_pool` by the *same`
/// `BytecodeCompiler` instance compiling the whole object literal.
///
/// NOTE ON WHY THIS SHAPE (not a single oversized construct, and not a
/// dotted field path): every "flat and wide" AST shape capable of holding
/// tens of thousands of *direct* children (`Block`, `Array`, `Object`,
/// builtin-call argument lists) is already bounded by its own per-node arity
/// guard (`AstNode::Array`/`AstNode::Object`'s guards from the prior task,
/// this task's own `AstNode::Block` guard, and the `CallBuiltin` arg-count
/// guard below) - so no *single* node can any longer overflow a pool by
/// itself. What those per-node guards cannot see is *cross-sibling*
/// accumulation: two children that are each individually small enough to
/// pass their own guard, compiled by the same `BytecodeCompiler` instance,
/// whose *combined* distinct-literal count overflows the shared pool. A
/// dotted path (`a.b.c...`) was tried first to reach a single oversized
/// `string_pool`, but the parser's own recursive-descent depth guard
/// (`MAX_PARSE_DEPTH = 1000`, unrelated to this task) rejects any path with
/// more than ~1000 steps long before it could reach 65,536 - so a
/// same-pool-different-siblings construction (which stays only 2-3 AST
/// levels deep: `Object -> Block -> literal`, well under that limit) is the
/// only reachable shape for this bug class.
///
/// Concretely: object `{"a": (0;1;...;39999), "b": (200000;200001;...;239999)}`.
/// Block "a" contributes ~40,000 distinct consts (plus its own object key),
/// block "b" another ~40,000 distinct consts (offset by 200,000 so no value
/// coincidentally dedups with block "a"'s), for roughly 80,002 total distinct
/// entries - comfortably over the `u16` pool's 65,536-slot capacity. Traced
/// pre-fix: `intern_const`'s `self.const_pool.len() as u16` wraps once the
/// pool passes 65,536 entries, and this wraparound region falls late enough
/// in block "b"'s 40,000 elements to include its own *last* (kept) literal -
/// so block "b"'s result silently aliases an earlier, wrong constant (some
/// value less than 240000) instead of its true last value `239999`. Block
/// "a" (whose own last element sits entirely before the wraparound) is
/// unaffected, so this also demonstrates the bug is real per-entry
/// corruption, not a coincidental total-count mismatch.
///
/// NOTE ON API CHOICE: same reasoning as the two tests above - bare
/// `Evaluator::evaluate` never reaches the compiled path for a top-level
/// `AstNode::Object`, so this must go through `jsonata_core::_bench`.
#[cfg(feature = "bench")]
#[test]
fn test_sibling_blocks_do_not_cumulatively_overflow_shared_const_pool() {
    use jsonata_core::_bench;
    let m = 40_000;
    let n = 40_000;
    let block_a = (0..m).map(|i| i.to_string()).collect::<Vec<_>>().join(";");
    let block_b = (0..n)
        .map(|i| (200_000 + i).to_string())
        .collect::<Vec<_>>()
        .join(";");
    let expr_str = format!("{{\"a\": ({block_a}), \"b\": ({block_b})}}");
    let ast = parse(&expr_str).unwrap();
    let data = JValue::Null;
    let result = match _bench::compile(&ast) {
        Some(prog) => _bench::run(&prog, &data).unwrap(),
        None => Evaluator::new().evaluate(&ast, &data).unwrap(),
    };
    match result {
        JValue::Object(obj) => {
            assert_eq!(obj.get("a"), Some(&JValue::from((m - 1) as i64)));
            assert_eq!(
                obj.get("b"),
                Some(&JValue::from((200_000 + n - 1) as i64)),
                "sibling block's last value was cumulatively corrupted via const_pool's \
                 wrapped u16 index"
            );
        }
        other => panic!("expected object, got {other:?}"),
    }
}

/// A `CallBuiltin` with more than `u8::MAX` (255) arguments must not silently
/// truncate via `Instr::CallBuiltin`'s `u8` `arg_count` operand. `$merge` is
/// the only compilable builtin whose `compilable_builtin_max_args` returns
/// `None` (it's variadic: `$merge(obj1, obj2, ...)` is a real, supported call
/// form - see the `"merge" => match effective_args.len() { ... }` arm in
/// `evaluator.rs`, which merges the raw argument list directly when there is
/// more than one arg), so it's the only builtin that can reach the dedicated
/// `args.len() > u8::MAX as usize` guard in `try_compile_expr_inner`'s
/// `AstNode::Function { is_builtin: true, .. }` arm - every other compilable
/// builtin already has a fixed `max` from `compilable_builtin_max_args` that
/// is well under 255.
///
/// This test calls `$merge` with 300 single-key object arguments, each with a
/// distinct key (`k0`..`k299`). Historical bug (pre-fix): `compiler.rs`'s
/// `CompiledExpr::BuiltinCall` arm emits `Instr::CallBuiltin { arg_count:
/// args.len() as u8, .. }` with no upper-bound check; `300 as u8` wraps to
/// `44`. All 300 compiled argument sub-expressions still get pushed onto the
/// VM operand stack (each is its own `compile_expr` call), but `CallBuiltin`
/// would only pop 44 of them for the merge call, so the runtime result would
/// only ever contain the last 44 arguments' keys (44 keys, not 300) - the
/// same "arguments silently go missing" corruption pattern as the sibling
/// `BlockEnd`/`MakeArray`/`MakeObject` truncation bugs above, just manifesting
/// as a short result instead of a wrong one.
///
/// Post-fix: `try_compile_expr_inner` bails out (`return None`) before ever
/// calling `BytecodeCompiler::compile`, so `_bench::compile` must return
/// `None` for this expression - asserted explicitly below - and the whole
/// expression falls back to the tree-walking `Evaluator`, which has no such
/// arg-count ceiling and sees all 300 arguments.
///
/// NOTE ON API CHOICE: same reasoning as the tests above - bare
/// `Evaluator::evaluate` never invokes `try_compile_expr`/the VM at all, so a
/// test built on it alone would not exercise the guard being fixed here.
#[cfg(feature = "bench")]
#[test]
fn test_call_with_more_than_u8_max_args_does_not_silently_truncate() {
    use jsonata_core::_bench;
    // `$zip` is genuinely variadic (`<a+>`), so a 300-argument call is valid
    // and exercises the >u8::MAX guard end to end. This used to use a
    // 300-argument `$merge`, which jsonata-js rejects with T0410 -- its
    // signature is `<a<o>:o>`, one array of objects, not varargs -- and which
    // we now reject too (#102). The guard being tested is about argument
    // *count*, so the choice of function is incidental.
    let n = 300;
    let arrays: Vec<String> = (0..n).map(|i| format!("[{i}]")).collect();
    let expr_str = format!("$zip({})", arrays.join(", "));
    let ast = parse(&expr_str).unwrap();
    let data = JValue::Null;

    // The guard must make compilation decline entirely (fall back to the
    // tree-walker) rather than succeed with a truncated arg count baked into
    // the bytecode.
    assert!(
        _bench::compile(&ast).is_none(),
        "expected the >u8::MAX-arg call to decline bytecode compilation \
         (fall back to the tree-walker), but it compiled successfully - the \
         u8::MAX arg-count guard appears to be missing or broken"
    );

    // The tree-walker has no such ceiling and must see every argument.
    let result = Evaluator::new().evaluate(&ast, &data).unwrap();
    match result {
        JValue::Array(outer) => {
            assert_eq!(outer.len(), 1, "expected one zipped tuple, got {outer:?}");
            match &outer[0] {
                JValue::Array(inner) => {
                    assert_eq!(
                        inner.len(),
                        n,
                        "zip() with 300 single-element array args silently dropped \
                         arguments - likely the u8 arg-count truncation this guard \
                         exists to prevent"
                    );
                    for i in 0..n {
                        assert_eq!(
                            inner[i],
                            JValue::from(i as i64),
                            "missing or wrong value at position {i}"
                        );
                    }
                }
                other => panic!("expected inner array, got {other:?}"),
            }
        }
        other => panic!("expected array, got {other:?}"),
    }
}

// ── Fused aggregate fast path ($sum/$max/$min/$average over `array.field`) ────
//
// `try_fused_aggregate` short-circuits `$agg(arr.field)` into a single pass.
// It must not diverge from the canonical aggregate semantics it replaces:
// a present non-numeric element is a type error (T0412), and an empty
// sequence is undefined — not a silently-skipped element or a zero.

/// Evaluate `expr` against `data`, returning the raw result.
fn fused_eval(expr: &str, data: serde_json::Value) -> Result<JValue, String> {
    let ast = parse(expr).unwrap();
    let data: JValue = data.into();
    Evaluator::new()
        .evaluate(&ast, &data)
        .map_err(|e| e.to_string())
}

#[test]
fn test_fused_aggregate_rejects_non_numeric_field() {
    // jsonata-js raises T0412 for every one of these.
    for expr in [
        "$sum(orders.p)",
        "$max(orders.p)",
        "$min(orders.p)",
        "$average(orders.p)",
    ] {
        let result = fused_eval(expr, json!({"orders": [{"p": "free"}]}));
        assert!(
            result.is_err(),
            "{expr} must reject a non-numeric field, got {result:?}"
        );
    }
}

#[test]
fn test_fused_aggregate_does_not_silently_skip_non_numeric() {
    // The dangerous shape: a valid number alongside a string. Skipping the
    // string yields a plausible-but-wrong total instead of an error.
    let result = fused_eval(
        "$sum(orders.p)",
        json!({"orders": [{"p": 1}, {"p": "free"}]}),
    );
    assert!(
        result.is_err(),
        "mixed numeric/non-numeric must error, got {result:?}"
    );
}

#[test]
fn test_fused_aggregate_empty_sequence_is_undefined() {
    // `empty.p` is an empty sequence, which is undefined — not 0.
    // (`$sum([])` on a literal empty array is still 0; that is a different case.)
    assert_eq!(
        fused_eval("$sum(empty.p)", json!({"empty": []})),
        Ok(JValue::Undefined)
    );
    assert_eq!(
        fused_eval("$max(empty.p)", json!({"empty": []})),
        Ok(JValue::Undefined)
    );
    assert_eq!(
        fused_eval("$min(empty.p)", json!({"empty": []})),
        Ok(JValue::Undefined)
    );
    // $average is the one that could plausibly divide by zero instead.
    assert_eq!(
        fused_eval("$average(empty.p)", json!({"empty": []})),
        Ok(JValue::Undefined)
    );
}

#[test]
fn test_fused_aggregate_still_aggregates_valid_input() {
    // Guard the fast path's happy cases against an over-broad fix.
    assert_eq!(
        fused_eval("$sum(orders.p)", json!({"orders": [{"p": 1}, {"p": 2}]})),
        Ok(JValue::Number(3.0))
    );
    // A missing field is undefined and drops out of the sequence.
    assert_eq!(
        fused_eval("$sum(orders.p)", json!({"orders": [{"p": 1}, {"q": 9}]})),
        Ok(JValue::Number(1.0))
    );
    // A non-object element has no fields; it drops out too (matches jsonata-js).
    assert_eq!(
        fused_eval("$sum(orders.p)", json!({"orders": [1, {"p": 2}]})),
        Ok(JValue::Number(2.0))
    );
    // Filter stages must keep working.
    assert_eq!(
        fused_eval(
            "$sum(orders[p > 1].p)",
            json!({"orders": [{"p": 1}, {"p": 5}]})
        ),
        Ok(JValue::Number(5.0))
    );
    // A literal empty array is still 0.
    assert_eq!(fused_eval("$sum([])", json!({})), Ok(JValue::Number(0.0)));
}

// ── Explicit null in path results (issue #98, root cause 1) ──────────────────
//
// An explicit JSON `null` is a value: it stays in a query-result sequence.
// Only *undefined* (an absent field) drops out. `evaluate_path`'s array-mapping
// fast path predates the null/undefined migration in #32 and skipped both,
// which silently shortened sequences and changed the answers of everything
// downstream of them.

#[test]
fn test_path_keeps_explicit_null_in_sequence() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": null}]}).into();
    let ast = parse("arr.p").unwrap();
    let result = Evaluator::new().evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::from(json!([1, null])));
}

#[test]
fn test_path_keeps_lone_explicit_null() {
    // A single null is still a value, so the singleton unwraps to null itself
    // rather than collapsing to undefined.
    let data: JValue = json!({"arr": [{"p": null}]}).into();
    let ast = parse("arr.p").unwrap();

    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::Null
    );
}

#[test]
fn test_path_still_drops_missing_fields() {
    // The other half of the distinction: an absent field is undefined and must
    // still drop out, so this fix must not turn missing into null.
    let data: JValue = json!({"arr": [{"p": 1}, {"q": 9}]}).into();
    let ast = parse("arr.p").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!(1))
    );

    // Nothing present at all is undefined, not a sequence of nulls.
    let ast = parse("arr.nope").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::Undefined
    );
}

#[test]
fn test_path_null_is_counted_and_constructed() {
    // Downstream consumers see the full sequence.
    let data: JValue = json!({"arr": [{"p": 1}, {"p": null}]}).into();

    let ast = parse("$count(arr.p)").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::Number(2.0)
    );

    let ast = parse("[arr.p]").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([1, null]))
    );
}

// ── Singleton unwrapping after a predicate step (issue #98, root cause 2) ────
//
// A predicate is an array operation, so a result sequence of one unwraps to
// that element. `arr[p = 1]` is `{"p": 1}`, not `[{"p": 1}]`. The tree-walker
// decided this from `step.stages` alone, but `arr[p = 1]` parses the predicate
// as a `Predicate` step *node* with empty stages, so the check never saw it.

#[test]
fn test_predicate_unwraps_singleton_result() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": 2}]}).into();
    let ast = parse("arr[p = 1]").unwrap();

    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!({"p": 1}))
    );
}

#[test]
fn test_predicate_keeps_multi_element_result() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": 2}]}).into();
    let ast = parse("arr[p > 0]").unwrap();

    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([{"p": 1}, {"p": 2}]))
    );
}

#[test]
fn test_predicate_singleton_that_is_itself_an_array() {
    // Unwrapping is uniform: the sequence [[5]] has one element, [5], so the
    // result is [5] -- not 5, and not [[5]].
    let data: JValue = json!({"a": [[5]]}).into();

    for expr in ["a[0]", "a[$[0] = 5]"] {
        let ast = parse(expr).unwrap();
        assert_eq!(
            Evaluator::new().evaluate(&ast, &data).unwrap(),
            JValue::from(json!([5])),
            "{expr}"
        );
    }
}

#[test]
fn test_predicate_no_match_is_undefined() {
    let data: JValue = json!({"arr": [{"p": 1}]}).into();
    let ast = parse("arr[p = 99]").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::Undefined
    );
}

#[test]
fn test_empty_predicate_still_keeps_array() {
    // `[]` must survive the unwrap rule -- it is an explicit keep-array.
    let data: JValue = json!({"arr": [{"p": 1}]}).into();
    let ast = parse("arr[].p").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([1]))
    );
}

// ── Parenthesized map stages unwrap singletons (issue #181) ──────────────────
//
// A map stage whose RHS is a parenthesized expression produces a sequence for
// the same reason a field-name step does: the array becomes the context of a
// subsequent expression (processing model, rule 3), so a one-item result
// unwraps (rule 2). The `FunctionApplication` arm set no array-mapping flag,
// so `[{'b':1}].($)` stayed `[{"b":1}]` where jsonata-js returns the object.

#[test]
fn test_parenthesized_map_stage_unwraps_singleton_object() {
    assert_eq!(
        filter_eval(r#"($r := [{'b':1}]; $r.($merge([$,{'x':1}])))"#, json!({})),
        JValue::from(json!({"b": 1, "x": 1}))
    );
}

#[test]
fn test_parenthesized_map_stage_unwraps_singleton_scalar() {
    assert_eq!(
        filter_eval(r#"($r := [{'b':1}]; $r.($v := 1; $v))"#, json!({})),
        JValue::from(1)
    );
}

#[test]
fn test_parenthesized_map_stage_singleton_array_stays_flat() {
    // The mapped value is itself an array; the singleton SEQUENCE unwraps to
    // that array (rule 2), not to a nested `[[...]]`.
    assert_eq!(
        filter_eval(r#"($r := [{'b':1}]; $r.([$]))"#, json!({})),
        JValue::from(json!([{"b": 1}]))
    );
}

#[test]
fn test_parenthesized_map_stage_multi_element_stays_array() {
    assert_eq!(
        filter_eval(
            r#"($r := [{'b':1},{'b':2}]; $r.($merge([$,{'x':1}])))"#,
            json!({})
        ),
        JValue::from(json!([{"b": 1, "x": 1}, {"b": 2, "x": 1}]))
    );
}

#[test]
fn test_parenthesized_map_stage_singleton_compares_as_scalar() {
    // Consequence of the unwrap: the block result participates in `=` as a
    // string rather than raising T2010 (array vs string).
    assert_eq!(
        filter_eval(
            r#"($r := [{'bucket':'a'}]; $r.($v := bucket; $v = 'a'))"#,
            json!({})
        ),
        JValue::from(true)
    );
}

#[test]
fn test_parenthesized_map_stage_keep_array_wins() {
    assert_eq!(
        filter_eval(r#"($r := [{'b':1}]; $r.($)[])"#, json!({})),
        JValue::from(json!([{"b": 1}]))
    );
}

// ── Map stages flatten array results one level (issue #183) ─────────────────
//
// jsonata-js's evaluateStep pushes each item's result and then flattens one
// level, with a last-step rule: a single contribution that is an explicitly
// constructed array IS the result (`[9]`), while a single contribution that
// is an inner sequence collapses as usual (`{"b":1}`). Without the flatten,
// nested map pipelines stay arrays of arrays.

#[test]
fn test_map_stage_flattens_nested_map() {
    assert_eq!(
        filter_eval(
            r#"($ids := [{'k':'a'},{'k':'b'}]; [1,2].($b := $; $ids.({'b': $b, 'k': $.k})))"#,
            json!({})
        ),
        JValue::from(json!([
            {"b": 1, "k": "a"},
            {"b": 1, "k": "b"},
            {"b": 2, "k": "a"},
            {"b": 2, "k": "b"}
        ]))
    );
}

#[test]
fn test_map_stage_flattens_constructor_arrays() {
    for (expr, expected) in [
        (r#"([1,2]).(([9,8]))"#, json!([9, 8, 9, 8])),
        (r#"([1,2]).([9,8])"#, json!([9, 8, 9, 8])),
        (r#"($x := [1,2]; $x.($v := [$]; $v))"#, json!([1, 2])),
        (r#"($x := [[1,2],[3]]; $x.($))"#, json!([1, 2, 3])),
        (r#"($x := [1,2]; $x.([5]))"#, json!([5, 5])),
    ] {
        assert_eq!(
            filter_eval(expr, json!({})),
            JValue::from(expected),
            "{expr}"
        );
    }
}

#[test]
fn test_map_stage_last_step_single_array_rules() {
    // A single constructed array result is kept as an array...
    assert_eq!(
        filter_eval(r#"($r := [{'b':1}]; $r.([$]))"#, json!({})),
        JValue::from(json!([{"b": 1}]))
    );
    assert_eq!(
        filter_eval(r#"([1]).(([9]))"#, json!({})),
        JValue::from(json!([9]))
    );
    // ...while a single inner-sequence contribution collapses like any
    // singleton sequence (issue #181).
    assert_eq!(
        filter_eval(
            r#"($ids := [{'k':'a'}]; [1].($b := $; $ids.({'b': $b})))"#,
            json!({})
        ),
        JValue::from(json!({"b": 1}))
    );
}

// ── Numeric predicates select by index (issue #98, root cause 3) ─────────────
//
// `arr[p]` does not mean "keep elements where p is truthy". When the predicate
// evaluates to a number, jsonata-js compares it against the element's *index*
// and keeps the element only on a match; negative values wrap from the end,
// and fractional values floor. Only a non-numeric result falls back to
// truthiness. All four filter implementations shared a copy of the truthiness
// rule, so all four were wrong the same way.

fn filter_eval(expr: &str, data: serde_json::Value) -> JValue {
    let ast = parse(expr).unwrap();
    let data: JValue = data.into();
    Evaluator::new().evaluate(&ast, &data).unwrap()
}

#[test]
fn test_numeric_predicate_matches_element_index() {
    // 0 == index 0 and 1 == index 1, so both survive.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": 0}, {"p": 1}]})),
        JValue::from(json!([{"p": 0}, {"p": 1}]))
    );
    // Neither value equals its own index.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": 1}, {"p": 0}]})),
        JValue::Undefined
    );
    // Only the first matches; the singleton then unwraps.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": 0}, {"p": 9}]})),
        JValue::from(json!({"p": 0}))
    );
}

#[test]
fn test_numeric_predicate_negative_wraps_and_fractional_floors() {
    // -1 wraps to index 1, which is where the second element lives.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": -1}, {"p": 1}]})),
        JValue::from(json!({"p": 1}))
    );
    // floor(0.7) == 0 and floor(1.2) == 1.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": 0.7}, {"p": 1.2}]})),
        JValue::from(json!([{"p": 0.7}, {"p": 1.2}]))
    );
}

#[test]
fn test_array_of_numbers_predicate_matches_any() {
    // An array of numbers is a set of indices; any match keeps the element.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": [0, 1]}, {"p": [5]}]})),
        JValue::from(json!({"p": [0, 1]}))
    );
    // An empty array is vacuously "all numbers" and matches no index.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": []}, {"p": []}]})),
        JValue::Undefined
    );
}

#[test]
fn test_non_numeric_predicate_still_uses_truthiness() {
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": "x"}, {"p": ""}]})),
        JValue::from(json!({"p": "x"}))
    );
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": true}, {"p": false}]})),
        JValue::from(json!({"p": true}))
    );
    // A mixed array is not an index selector, and a non-empty array is truthy.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"p": [1, "x"]}]})),
        JValue::from(json!({"p": [1, "x"]}))
    );
    // Missing and null are falsy and drop out.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": [{"q": 9}, {"p": null}]})),
        JValue::Undefined
    );
}

#[test]
fn test_comparison_predicates_unaffected() {
    // Guard: ordinary boolean filters must keep working.
    assert_eq!(
        filter_eval("a[$ = 20]", json!({"a": [10, 20, 30]})),
        JValue::from(json!(20))
    );
    assert_eq!(
        filter_eval("a[1]", json!({"a": [10, 20, 30]})),
        JValue::from(json!(20))
    );
    assert_eq!(
        filter_eval("arr[p > 1]", json!({"arr": [{"p": 1}, {"p": 5}]})),
        JValue::from(json!({"p": 5}))
    );
}

// ── Predicates on a non-array value (issue #98, root cause 4) ────────────────
//
// A non-array is a singleton sequence: index 0, length 1. The same index rule
// applies, so `arr[p]` keeps the object only when `p` is 0 (or a non-numeric
// truthy value). We previously treated a string predicate as computed property
// access, which is not a JSONata rule -- `o["a"]` returns the object because a
// non-empty string is truthy, not because "a" is looked up.

#[test]
fn test_predicate_on_object_uses_index_zero() {
    // p == 1 does not match index 0.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": {"p": 1}})),
        JValue::Undefined
    );
    // p == 0 does match index 0.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": {"p": 0}})),
        JValue::from(json!({"p": 0}))
    );
    // Non-numeric falls back to truthiness.
    assert_eq!(
        filter_eval("arr[p]", json!({"arr": {"p": true}})),
        JValue::from(json!({"p": true}))
    );
}

#[test]
fn test_index_into_object_singleton() {
    assert_eq!(
        filter_eval("arr[0]", json!({"arr": {"p": 1}})),
        JValue::from(json!({"p": 1}))
    );
    assert_eq!(
        filter_eval("arr[0].p", json!({"arr": {"p": 1}})),
        JValue::from(json!(1))
    );
    assert_eq!(
        filter_eval("arr[1]", json!({"arr": {"p": 1}})),
        JValue::Undefined
    );
    // -1 wraps to index 0 of a one-element sequence.
    assert_eq!(
        filter_eval("arr[-1]", json!({"arr": {"p": 1}})),
        JValue::from(json!({"p": 1}))
    );
}

#[test]
fn test_string_predicate_on_object_is_truthiness_not_key_access() {
    // Truthy string keeps the object; it is not a key lookup.
    assert_eq!(
        filter_eval("o[\"a\"]", json!({"o": {"a": 1}})),
        JValue::from(json!({"a": 1}))
    );
    // An empty string is falsy.
    assert_eq!(
        filter_eval("o[\"\"]", json!({"o": {"a": 1}})),
        JValue::Undefined
    );
}

// ── Comparison against undefined (issue #98, root cause 5) ───────────────────
//
// An undefined operand makes an ordered comparison undefined, it does not
// raise. `ordered_compare` was written before the null/undefined split and
// matches only on `JValue::Null`, so a real `Undefined` reached the catch-all
// and produced "T2010: Cannot compare unknown and number".

#[test]
fn test_ordered_comparison_with_undefined_is_undefined() {
    let data: JValue = json!({"arr": []}).into();
    for expr in ["arr.p < 2", "arr.p > 2", "arr.p <= 2", "arr.p >= 2"] {
        let ast = parse(expr).unwrap();
        assert_eq!(
            Evaluator::new().evaluate(&ast, &data).unwrap(),
            JValue::Undefined,
            "{expr}"
        );
    }
    // Not specific to empty arrays -- any missing path behaves the same.
    let ast = parse("nothing.x < 2").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::Undefined
    );
}

#[test]
fn test_ordered_comparison_still_errors_on_real_mismatches() {
    // Guard: an explicit null and a boolean are still type errors, and valid
    // comparisons still compare.
    let data: JValue = json!({"n": null, "b": true, "x": 5}).into();

    assert!(Evaluator::new()
        .evaluate(&parse("n < 2").unwrap(), &data)
        .is_err());
    assert!(Evaluator::new()
        .evaluate(&parse("b < 2").unwrap(), &data)
        .is_err());
    assert!(Evaluator::new()
        .evaluate(&parse("x < \"s\"").unwrap(), &data)
        .is_err());
    // null is uncomparable even opposite an undefined operand.
    assert!(Evaluator::new()
        .evaluate(&parse("missing.x < n").unwrap(), &data)
        .is_err());
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("x < 10").unwrap(), &data)
            .unwrap(),
        JValue::Bool(true)
    );
}

// ── Explicit null through a stage filter (issue #98) ─────────────────────────
//
// The tuple/stage branch of `evaluate_path` mapped a missing field to
// `JValue::Null` and then skipped every null, so an explicit null was dropped
// alongside genuinely absent fields -- the same pre-migration pattern already
// fixed in the no-stages fast path.

#[test]
fn test_stage_filter_keeps_explicit_null() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": null}]}).into();
    let ast = parse("arr.p[-1]").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([1, null]))
    );
}

#[test]
fn test_stage_filter_still_drops_missing_fields() {
    let data: JValue = json!({"arr": [{"p": 1}, {"q": 9}]}).into();
    let ast = parse("arr.p[-1]").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!(1))
    );
}

#[test]
fn test_stage_filter_indexes_within_each_group() {
    // Stage semantics: the index applies to each extracted value, not to the
    // flattened sequence.
    let data: JValue = json!({"arr": [{"p": [1, 2]}, {"p": 3}]}).into();
    let ast = parse("arr.p[-1]").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([2, 3]))
    );
}

// ── Arithmetic on a runtime null (issue #98 follow-up) ───────────────────────
//
// An explicit null operand is a type error; only *undefined* propagates. The
// operators carried compile-time `explicit_null` flags to tell those apart
// back when both were `JValue::Null`, so a null arriving at runtime -- from
// data, or as a lambda parameter -- was treated as "missing" and silently
// produced null instead of raising.

#[test]
fn test_arithmetic_on_runtime_null_is_an_error() {
    let data: JValue = json!({"n": null, "x": 5}).into();
    for expr in ["n * 2", "n + 1", "n - 1", "n / 2", "x * n", "x + n"] {
        let ast = parse(expr).unwrap();
        assert!(
            Evaluator::new().evaluate(&ast, &data).is_err(),
            "{expr} must raise on a null operand"
        );
    }
}

#[test]
fn test_arithmetic_on_null_lambda_parameter_is_an_error() {
    // The shape that surfaced this: the null reaches the operator as a lambda
    // parameter, so no compile-time flag could have marked it.
    let data: JValue = json!({"arr": [{"p": 1}, {"p": null}]}).into();
    for expr in [
        "$map([1, null], function($v) { $v * 2 })",
        "$map([1, null], function($v) { $v + 1 })",
        "$map(arr.p, function($v) { $v * 2 })",
    ] {
        let ast = parse(expr).unwrap();
        assert!(
            Evaluator::new().evaluate(&ast, &data).is_err(),
            "{expr} must raise on a null element"
        );
    }
}

#[test]
fn test_arithmetic_still_propagates_undefined() {
    // The other half: a genuinely missing operand yields undefined, not an error.
    let data: JValue = json!({"x": 5, "e": []}).into();
    for expr in [
        "missing.x * 2",
        "e.p * 2",
        "x * missing.y",
        "missing.a * missing.b",
    ] {
        let ast = parse(expr).unwrap();
        assert_eq!(
            Evaluator::new().evaluate(&ast, &data).unwrap(),
            JValue::Undefined,
            "{expr}"
        );
    }
}

#[test]
fn test_arithmetic_valid_cases_unaffected() {
    let data: JValue = json!({"x": 5, "y": 2}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("x * y").unwrap(), &data)
            .unwrap(),
        JValue::Number(10.0)
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("x - y").unwrap(), &data)
            .unwrap(),
        JValue::Number(3.0)
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(
                &parse("$map([1, 2], function($v) { $v * 2 })").unwrap(),
                &data
            )
            .unwrap(),
        JValue::from(json!([2, 4]))
    );
}

// ── Comparison inside compiled filters (issue #102, cluster A) ───────────────
//
// `compiled_ordered_cmp` is the compiled twin of `Evaluator::ordered_compare`
// and predates the null/undefined split the same way: it conflates
// `JValue::Null` with `JValue::Undefined` and leans on the vestigial
// compile-time explicit-null flags. Filters run through the compiled path, so
// `arr[p > 1]` silently dropped uncomparable elements instead of raising.

#[test]
fn test_compiled_comparison_rejects_uncomparable_operands() {
    for (payload, label) in [
        (json!({"arr": [{"p": 1}, {"p": null}]}), "null"),
        (json!({"arr": [{"p": 1}, {"p": true}]}), "boolean"),
        (json!({"arr": [{"p": 1}, {"p": {"q": 1}}]}), "object"),
    ] {
        let data: JValue = payload.into();
        for expr in ["arr[p > 1]", "arr[p > 1].p", "$sum(arr[p > 1].p)"] {
            let ast = parse(expr).unwrap();
            assert!(
                Evaluator::new().evaluate(&ast, &data).is_err(),
                "{expr} must raise on a {label} operand"
            );
        }
    }
}

#[test]
fn test_compiled_comparison_type_mismatch_still_raises() {
    // A string against a number is comparable-but-mismatched: T2009, not T2010.
    let data: JValue = json!({"arr": [{"p": 1}, {"p": "free"}]}).into();
    let ast = parse("arr[p > 1]").unwrap();
    assert!(Evaluator::new().evaluate(&ast, &data).is_err());
}

#[test]
fn test_compiled_comparison_propagates_undefined() {
    // A missing field is undefined: the comparison is undefined, so the element
    // drops out rather than raising.
    let data: JValue = json!({"arr": [{"p": 1}, {"q": 9}]}).into();
    for expr in ["arr[p > 1]", "$sum(arr[p > 1].p)"] {
        let ast = parse(expr).unwrap();
        assert_eq!(
            Evaluator::new().evaluate(&ast, &data).unwrap(),
            JValue::Undefined,
            "{expr}"
        );
    }
}

#[test]
fn test_compiled_comparison_valid_cases_unaffected() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": 5}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr[p > 1]").unwrap(), &data)
            .unwrap(),
        JValue::from(json!({"p": 5}))
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("$sum(arr[p > 1].p)").unwrap(), &data)
            .unwrap(),
        JValue::Number(5.0)
    );
}

// ── Sort comparator type checking (issue #102, cluster A) ────────────────────
//
// `merge_sort_specialized` is a Schwartzian-transform fast path for
// `function($l, $r) { $l.f > $r.f }`. It collapsed every non-numeric,
// non-string key into `SortKey::None` and treated mixed types as "maintain
// original order", so a comparator that jsonata-js rejects sorted silently.
// It now declines those inputs and lets the general comparator -- which runs
// the lambda body through `compiled_ordered_cmp` -- raise.

const SORT_BY_P: &str = "$sort(arr, function($l, $r) { $l.p > $r.p }).p";

#[test]
fn test_specialized_sort_rejects_uncomparable_keys() {
    for (payload, label) in [
        (json!({"arr": [{"p": 1}, {"p": null}]}), "null"),
        (json!({"arr": [{"p": 1}, {"p": true}]}), "boolean"),
        (json!({"arr": [{"p": {"q": 1}}, {"p": {"q": 2}}]}), "object"),
        (json!({"arr": [{"p": [1, 2]}, {"p": 3}]}), "array"),
    ] {
        let data: JValue = payload.into();
        let ast = parse(SORT_BY_P).unwrap();
        assert!(
            Evaluator::new().evaluate(&ast, &data).is_err(),
            "sort must raise on a {label} key"
        );
    }
}

#[test]
fn test_specialized_sort_rejects_mixed_key_types() {
    // Comparable individually, but not against each other: T2009.
    let data: JValue = json!({"arr": [{"p": 1}, {"p": "free"}]}).into();
    let ast = parse(SORT_BY_P).unwrap();
    assert!(Evaluator::new().evaluate(&ast, &data).is_err());
}

#[test]
fn test_specialized_sort_still_sorts_valid_keys() {
    // The fast path must keep working for what it was written for.
    let data: JValue = json!({"arr": [{"p": 3}, {"p": 1}, {"p": 2}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse(SORT_BY_P).unwrap(), &data)
            .unwrap(),
        JValue::from(json!([1, 2, 3]))
    );

    let data: JValue = json!({"arr": [{"p": "b"}, {"p": "a"}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse(SORT_BY_P).unwrap(), &data)
            .unwrap(),
        JValue::from(json!(["a", "b"]))
    );

    // A missing key is undefined, which sorts without raising.
    let data: JValue = json!({"arr": [{"p": 1}, {"q": 9}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse(SORT_BY_P).unwrap(), &data)
            .unwrap(),
        JValue::from(json!(1))
    );

    // Descending comparators keep working too.
    let data: JValue = json!({"arr": [{"p": 1}, {"p": 3}]}).into();
    let ast = parse("$sort(arr, function($l, $r) { $l.p < $r.p }).p").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([3, 1]))
    );
}

// ── Field access on a lambda parameter (issue #102, clusters B/D) ────────────
//
// The `$var.field` fast path -- used for `$l.rating`, `$v.price` in sort and
// HOF bodies -- predates the null/undefined split and mapped a missing field
// to `JValue::Null`. That is why `$map` produced `[1, null]` where jsonata-js
// drops the undefined and yields `1`, and why `$filter` raised T2010 comparing
// a "null" that was really a missing field.

#[test]
fn test_lambda_param_missing_field_is_undefined() {
    let data: JValue = json!({"arr": [{"p": 1}, {"q": 9}]}).into();

    // The undefined drops out of the sequence rather than becoming null.
    let ast = parse("$map(arr, function($v) { $v.p })").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!(1))
    );

    // And a comparison against it is undefined, not a T2010 on a null.
    let ast = parse("$filter(arr, function($v) { $v.p > 0 })").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!({"p": 1}))
    );
}

#[test]
fn test_lambda_param_explicit_null_field_is_kept() {
    // The other half: a present null is a value and must not vanish.
    let data: JValue = json!({"arr": [{"p": 1}, {"p": null}]}).into();
    let ast = parse("$map(arr, function($v) { $v.p })").unwrap();
    assert_eq!(
        Evaluator::new().evaluate(&ast, &data).unwrap(),
        JValue::from(json!([1, null]))
    );
}

// ── $map/$filter return sequences, not arrays (issue #102, cluster B) ────────

#[test]
fn test_hof_results_are_sequences() {
    // One result unwraps to that result.
    let data: JValue = json!({"arr": [{"p": "free"}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("$map(arr, function($v) { $v.p })").unwrap(), &data)
            .unwrap(),
        JValue::from(json!("free"))
    );

    // An empty result is undefined, not [].
    let data: JValue = json!({"arr": []}).into();
    for expr in [
        "$map(arr, function($v) { $v.p })",
        "$filter(arr, function($v) { $v.p > 0 })",
    ] {
        assert_eq!(
            Evaluator::new()
                .evaluate(&parse(expr).unwrap(), &data)
                .unwrap(),
            JValue::Undefined,
            "{expr}"
        );
    }

    // All results undefined is also an empty sequence.
    let data: JValue = json!({"arr": [{"q": 1}, {"q": 2}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("$map(arr, function($v) { $v.p })").unwrap(), &data)
            .unwrap(),
        JValue::Undefined
    );
}

#[test]
fn test_hof_accepts_a_non_array_as_a_singleton() {
    let data: JValue = json!({"arr": {"p": 1}}).into();
    for expr in [
        "$map(arr, function($v) { $v.p })",
        "$filter(arr, function($v) { $v.p > 0 })",
    ] {
        let result = Evaluator::new().evaluate(&parse(expr).unwrap(), &data);
        assert!(result.is_ok(), "{expr} must accept a non-array: {result:?}");
    }
}

#[test]
fn test_hof_multi_element_results_still_arrays() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": 2}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("$map(arr, function($v) { $v.p })").unwrap(), &data)
            .unwrap(),
        JValue::from(json!([1, 2]))
    );
}

// ── Equality against undefined (issue #102, cluster C) ───────────────────────
//
// jsonata-js returns false for BOTH `=` and `!=` when either operand is
// undefined -- `!=` is not the negation of `=` there. We negated, so
// `arr[p != null]` kept elements whose `p` was missing.

#[test]
fn test_inequality_with_undefined_is_false() {
    let data: JValue = json!({"n": null}).into();
    for expr in [
        "missing.x != null",
        "missing.x != 1",
        "missing.x != missing.y",
    ] {
        assert_eq!(
            Evaluator::new()
                .evaluate(&parse(expr).unwrap(), &data)
                .unwrap(),
            JValue::Bool(false),
            "{expr}"
        );
    }
}

#[test]
fn test_equality_with_undefined_is_still_false() {
    let data: JValue = json!({"n": null}).into();
    for expr in ["missing.x = null", "missing.x = 1", "missing.x = missing.y"] {
        assert_eq!(
            Evaluator::new()
                .evaluate(&parse(expr).unwrap(), &data)
                .unwrap(),
            JValue::Bool(false),
            "{expr}"
        );
    }
}

#[test]
fn test_equality_against_present_null_unaffected() {
    // A present null still compares normally; only undefined short-circuits.
    let data: JValue = json!({"n": null, "x": 1}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("n = null").unwrap(), &data)
            .unwrap(),
        JValue::Bool(true)
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("n != null").unwrap(), &data)
            .unwrap(),
        JValue::Bool(false)
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("x = 1").unwrap(), &data)
            .unwrap(),
        JValue::Bool(true)
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("x != 1").unwrap(), &data)
            .unwrap(),
        JValue::Bool(false)
    );
}

#[test]
fn test_filter_on_not_null_drops_missing_fields() {
    let data: JValue = json!({"arr": [{"p": 1}, {"q": 9}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr[p != null]").unwrap(), &data)
            .unwrap(),
        JValue::from(json!({"p": 1}))
    );

    // A non-object element has no fields, so it drops out too.
    let data: JValue = json!({"arr": [1, {"p": 2}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr[p != null]").unwrap(), &data)
            .unwrap(),
        JValue::from(json!({"p": 2}))
    );
}

// ── Object construction over a path (issue #102, cluster C) ──────────────────
//
// `arr.{"k": p}` maps the constructor over a sequence, so a single result
// unwraps to that object, and a non-array input is the singleton sequence
// containing it.

#[test]
fn test_object_construction_over_path_unwraps_singleton() {
    let data: JValue = json!({"arr": [{"p": "free"}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr.{\"k\": p}").unwrap(), &data)
            .unwrap(),
        JValue::from(json!({"k": "free"}))
    );

    // An undefined-valued key drops out, leaving an empty object -- still a
    // singleton, so it unwraps too.
    let data: JValue = json!({"arr": [{"q": 9}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr.{\"k\": p}").unwrap(), &data)
            .unwrap(),
        JValue::from(json!({}))
    );
}

#[test]
fn test_object_construction_over_non_array_input() {
    let data: JValue = json!({"arr": {"p": 1}}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr.{\"k\": p}").unwrap(), &data)
            .unwrap(),
        JValue::from(json!({"k": 1}))
    );
}

#[test]
fn test_object_construction_over_path_multi_and_empty() {
    let data: JValue = json!({"arr": [{"p": 1}, {"p": 2}]}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr.{\"k\": p}").unwrap(), &data)
            .unwrap(),
        JValue::from(json!([{"k": 1}, {"k": 2}]))
    );

    let data: JValue = json!({"arr": []}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("arr.{\"k\": p}").unwrap(), &data)
            .unwrap(),
        JValue::Undefined
    );
}

// ── Concatenation of null vs undefined (issue #102 follow-up) ────────────────
//
// `&` stringifies an explicit null as "null" -- consistent with `$string(null)`
// -- and only an *undefined* operand as the empty string. Both concat helpers
// collapsed the two, so `null & "x"` produced "x" instead of "nullx".

#[test]
fn test_concat_stringifies_explicit_null() {
    let data: JValue = json!({"n": null}).into();
    for (expr, want) in [
        ("null & \"x\"", "nullx"),
        ("n & \"x\"", "nullx"),
        ("1 & null", "1null"),
    ] {
        assert_eq!(
            Evaluator::new()
                .evaluate(&parse(expr).unwrap(), &data)
                .unwrap(),
            JValue::from(want),
            "{expr}"
        );
    }
}

#[test]
fn test_concat_treats_undefined_as_empty() {
    let data: JValue = json!({"x": 1}).into();
    for (expr, want) in [
        ("missing.y & \"x\"", "x"),
        ("\"a\" & missing.y", "a"),
        ("missing.a & missing.b", ""),
    ] {
        assert_eq!(
            Evaluator::new()
                .evaluate(&parse(expr).unwrap(), &data)
                .unwrap(),
            JValue::from(want),
            "{expr}"
        );
    }
}

#[test]
fn test_concat_ordinary_values_unaffected() {
    let data: JValue = json!({"x": 1, "s": "a"}).into();
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("s & x").unwrap(), &data)
            .unwrap(),
        JValue::from("a1")
    );
}

// ── The `in` operator (issue #102 follow-up, operator matrix) ────────────────
//
// `in` is membership, and jsonata-js compares with `===`: primitives match by
// value, composites only by identity. A non-array right side is wrapped, and
// an *undefined* operand on either side makes the result false.
//
// Two things were wrong. `evaluate_binary_op` special-cased an array left
// operand as "array filtering", so `arr in 1` evaluated as `arr[1]` and
// returned an element. And `in_operator` treated an object right side as
// key-containment and compared with deep equality.

fn in_eval(expr: &str) -> JValue {
    let data: JValue = json!({"arr": [1, 2], "nul": null, "obj": {"k": 1}}).into();
    Evaluator::new()
        .evaluate(&parse(expr).unwrap(), &data)
        .unwrap()
}

#[test]
fn test_in_with_array_left_operand_is_never_a_member() {
    for expr in ["arr in 1", "arr in 0", "arr in arr", "arr in [[1, 2]]"] {
        assert_eq!(in_eval(expr), JValue::Bool(false), "{expr}");
    }
}

#[test]
fn test_in_basic_membership() {
    assert_eq!(in_eval("1 in arr"), JValue::Bool(true));
    assert_eq!(in_eval("3 in arr"), JValue::Bool(false));
    assert_eq!(in_eval("\"a\" in [\"a\", \"b\"]"), JValue::Bool(true));
    // A non-array right side is wrapped in one.
    assert_eq!(in_eval("1 in 1"), JValue::Bool(true));
    assert_eq!(in_eval("1 in \"1\""), JValue::Bool(false));
    assert_eq!(in_eval("\"a\" in \"a\""), JValue::Bool(true));
}

#[test]
fn test_in_null_and_undefined() {
    // null is a value and matches itself; only undefined short-circuits.
    assert_eq!(in_eval("null in [null]"), JValue::Bool(true));
    assert_eq!(in_eval("nul in [null]"), JValue::Bool(true));
    assert_eq!(in_eval("nul in arr"), JValue::Bool(false));
    assert_eq!(in_eval("missing.x in arr"), JValue::Bool(false));
    assert_eq!(in_eval("arr in missing.x"), JValue::Bool(false));
    assert_eq!(in_eval("missing.x in missing.y"), JValue::Bool(false));
}

#[test]
fn test_in_composites_match_by_identity_not_structure() {
    // A structurally equal but distinct object is not a member.
    assert_eq!(in_eval("obj in [{\"k\": 1}]"), JValue::Bool(false));
    // The same object is.
    assert_eq!(in_eval("obj in [obj]"), JValue::Bool(true));
    assert_eq!(in_eval("obj in [obj, 1]"), JValue::Bool(true));
}

#[test]
fn test_in_object_right_side_is_not_key_containment() {
    // jsonata-js wraps the object and compares identity, so a key name is not
    // a member of the object it belongs to.
    assert_eq!(in_eval("\"k\" in obj"), JValue::Bool(false));
    assert_eq!(in_eval("\"z\" in obj"), JValue::Bool(false));
}

// ── Non-finite arithmetic (issue #102 follow-up, operator matrix) ────────────
//
// jsonata-js checks the *operands*, never the result: `isNumeric` throws D1001
// for an Infinity operand and reports NaN as non-numeric (T2001/T2002). So the
// operators themselves happily produce Infinity and NaN, and the error appears
// when such a value is consumed. We raised "Division by zero" at the operator
// instead, which has no counterpart in the reference.

fn num_eval(expr: &str) -> Result<JValue, String> {
    let data: JValue = json!({}).into();
    Evaluator::new()
        .evaluate(&parse(expr).unwrap(), &data)
        .map_err(|e| e.to_string())
}

#[test]
fn test_division_by_zero_yields_non_finite() {
    match num_eval("1/0") {
        Ok(JValue::Number(n)) => assert!(n.is_infinite() && n > 0.0, "got {n}"),
        other => panic!("expected +inf, got {other:?}"),
    }
    for expr in ["0/0", "1%0", "0%0"] {
        match num_eval(expr) {
            Ok(JValue::Number(n)) => assert!(n.is_nan(), "{expr} gave {n}"),
            other => panic!("{expr}: expected NaN, got {other:?}"),
        }
    }
}

#[test]
fn test_overflow_yields_infinity_but_consuming_it_raises() {
    // The multiply itself overflows to Infinity without raising...
    match num_eval("10e300 * 10e100") {
        Ok(JValue::Number(n)) => assert!(n.is_infinite(), "got {n}"),
        other => panic!("expected inf, got {other:?}"),
    }
    // ...and the error appears when that Infinity becomes an operand (D1001).
    assert!(num_eval("1/(10e300 * 10e100)").is_err());
}

#[test]
fn test_non_finite_serializes_like_javascript() {
    // JSON has no Infinity; JSON.stringify gives null and so do we.
    assert_eq!(num_eval("1/0").unwrap().to_json_string().unwrap(), "null");
}

#[test]
fn test_ordinary_division_unaffected() {
    assert_eq!(num_eval("6/3").unwrap(), JValue::Number(2.0));
    assert_eq!(num_eval("7%3").unwrap(), JValue::Number(1.0));
}

#[test]
fn test_unary_negate_null_raises_undefined_propagates() {
    // jsonata-js: undefined propagates, a number negates, anything else -- null
    // included -- is D1002. Both negate implementations treated null as
    // "missing" and returned null.
    let data: JValue = json!({"nul": null, "x": 5}).into();
    for expr in ["-(null)", "-(nul)", "-(\"s\")", "-(true)"] {
        assert!(
            Evaluator::new()
                .evaluate(&parse(expr).unwrap(), &data)
                .is_err(),
            "{expr} must raise D1002"
        );
    }
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("-(missing.x)").unwrap(), &data)
            .unwrap(),
        JValue::Undefined
    );
    assert_eq!(
        Evaluator::new()
            .evaluate(&parse("-x").unwrap(), &data)
            .unwrap(),
        JValue::Number(-5.0)
    );
}

// ── Builtins: null is a value, not a missing argument (issue #102) ───────────

#[test]
fn test_builtin_null_handling_matches_reference() {
    let data: JValue = json!({"arr": [1, 2], "nul": null, "obj": {"k": 1}}).into();
    let ev = |expr: &str| {
        Evaluator::new()
            .evaluate(&parse(expr).unwrap(), &data)
            .unwrap()
    };

    // $exists: only a *missing* value is false; an explicit null exists.
    assert_eq!(ev("$exists(nul)"), JValue::Bool(true));
    assert_eq!(ev("$exists(missing.x)"), JValue::Bool(false));

    // $append: a null is a value and gets appended; only undefined is skipped.
    assert_eq!(ev("$append(arr, null)"), JValue::from(json!([1, 2, null])));
    assert_eq!(ev("$append(arr, missing.x)"), JValue::from(json!([1, 2])));

    // $sort over a single element has nothing to compare, so the element type
    // does not matter -- the signature wraps a scalar into a singleton first.
    assert_eq!(ev("$sort(true)"), JValue::from(json!([true])));
    assert_eq!(ev("$sort(nul)"), JValue::from(json!([null])));
    assert_eq!(ev("$sort(obj)"), JValue::from(json!([{"k": 1}])));

    // Mixed comparable types across several elements is still an error.
    assert!(Evaluator::new()
        .evaluate(&parse("$sort([1, \"a\"])").unwrap(), &data)
        .is_err());
    assert_eq!(ev("$sort([3, 1, 2])"), JValue::from(json!([1, 2, 3])));
}

/// `$match` with no match yields undefined (an empty sequence in reference
/// JSONata), not `null`, so `$exists`, `$count` and array construction see
/// "nothing" rather than a single null item.
#[test]
fn test_match_no_match_is_undefined() {
    for expr in [
        r#"$match("xyz", /abc/)"#,
        r#"$match("xyz", /abc/g)"#,
        r#"$match("xyz", /abc/, 2)"#,
        r#"$match("xyz", function($s, $offset) { null })"#,
    ] {
        let ast = parse(expr).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &JValue::Null).unwrap();
        assert!(result.is_undefined(), "{expr} => {result:?}");
    }
}

#[test]
fn test_match_no_match_exists_count_and_array() {
    for (expr, expected) in [
        (r#"$exists($match("xyz", /abc/))"#, json!(false)),
        (r#"$count($match("xyz", /abc/))"#, json!(0)),
        (r#"[$match("xyz", /abc/)]"#, json!([])),
        // positive control
        (r#"$exists($match("abc", /abc/))"#, json!(true)),
    ] {
        let ast = parse(expr).unwrap();
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&ast, &JValue::Null).unwrap();
        assert_eq!(result, JValue::from(expected), "{expr}");
    }
}

// ── Group-by on a leading array constructor ──────────────────────────────────
//
// jsonata-js puts the group on the constructor node itself (parser.js `{`), so
// a path's first step evaluates once to the grouped object, which has no
// items for the next step to read: the path is undefined, as in v2.2.11.

/// Assert that `expr` is undefined (not null) with the tree-walker and with
/// the default dispatch.
fn assert_undefined_in_both_engines(expr: &str) {
    let ast = parse(expr).unwrap();
    let tree_walker = Evaluator::new().evaluate(&ast, &JValue::Null).unwrap();
    assert!(
        tree_walker.is_undefined(),
        "tree-walker: {expr} => {tree_walker:?}"
    );
    let default = jsonata_core::Expression::compile(expr)
        .unwrap()
        .evaluate(&JValue::Null)
        .unwrap();
    assert!(default.is_undefined(), "default: {expr} => {default:?}");
}

#[test]
fn test_group_on_leading_constructor_then_step_is_undefined() {
    for expr in [
        r#"[1,2]{"k": $}.k"#,
        r#"[1,2]{"k": $}.{"z": $}"#,
        r#"[1,2]{$string($): $}.[nothing]"#,
        r#"[1,2][]{"k": $}.k"#,
        r#"[1,2][$>1]{"k": $}.k"#,
        r#"[1]{"k": $}.k"#,
        r#"[[1,2],[3]]{"k": $}.k"#,
        r#"([1,2].$){"k": $}.k"#,
    ] {
        assert_undefined_in_both_engines(expr);
    }
}

#[test]
fn test_group_on_leading_constructor_unchanged_cases() {
    for (expr, expected) in [
        (r#"[1,2]{"k":$}"#, r#"Ok({"k":[1,2]})"#),
        (r#"([1,2]){"k":$}.[5]"#, "Ok([5])"),
        (r#"([1,2]){"k": $}.k"#, "Ok([1,2])"),
        (r#"[1,2]{"k": $}^(k)"#, r#"Ok({"k":[1,2]})"#),
        (r#"$count([1,2]{"k": $}.k)"#, "Ok(0)"),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
    for expr in [r#"[1,2]{"k":$}[0]"#, r#"[1,2]{"k":$}[0].k"#] {
        let err = parse(expr).unwrap_err().to_string();
        assert!(err.contains("S0209"), "{expr}: {err}");
    }
}

// ── Calling an indexed variable or field ─────────────────────────────────────
//
// jsonata-js evaluates a call's procedure expression and calls what it gives,
// so `$o[0]()` calls the first function in `$o`. A one-step path that is not a
// bare name was a parse error ("Invalid function name"). In `$o.g[0]()` the
// call is the last step (`(` binds tighter than `.`), so `g[0]` is called in
// each item of `$o`.

#[test]
fn test_call_of_indexed_function() {
    for (expr, expected) in [
        ("($o := [function(){1}]; $o[0]())", "Ok(1)"),
        ("$string[0](1)", r#"Ok("1")"#),
        (r#"$exists($o) ? $o[0]() : "none""#, r#"Ok("none")"#),
        (
            "($o := [function($x){$x+1}, function($x){$x*2}]; $o[-1](5))",
            "Ok(10)",
        ),
        ("($o := [function(){1}]; $o[0]()[0])", "Ok(1)"),
        ("($o := [function($a,$b){$a-$b}]; $o[0](?, 1)(5))", "Ok(4)"),
        (r#"($o := {"g": [function(){7}]}; $o.g[0]())"#, "Ok(7)"),
        (
            r#"($o := {"g": [function(){7}, function(){8}]}; $o.g[1]())"#,
            "Ok(8)",
        ),
        (
            r#"($o := {"g": {"h": [function(){9}]}}; $o.g.h[0]())"#,
            "Ok(9)",
        ),
        (
            r#"($o := [{"g": [function(){7}]}, {"g": [function(){8}]}]; $o.g[0]())"#,
            "Ok([7,8])",
        ),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

#[test]
fn test_call_of_indexed_non_function_raises() {
    for (expr, code) in [
        ("$o[0]()", "T1006"),
        ("($o := [1]; $o[0]())", "T1006"),
        ("($o := [function(){1}]; $o[$]())", "T1006"),
        ("($o := [function(){1}]; $o[]())", "T1006"),
        (r#"($o := {"g": [1]}; $o.g[0]())"#, "T1006"),
        ("($o := [1]; $o[0](?))", "T1008"),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
}

// ── A null sort term from a non-simple term ──────────────────────────────────
//
// A sort term that is not a plain field (`a.b`, `$.a`, a function call)
// treated a null result as undefined, so it sorted last instead of raising
// T2008. jsonata-js's comparator raises T2008 whenever it compares a null
// term with another defined one; a null beside only undefined terms, or in a
// one-item sort, is never compared and sorts before the undefined ones.

#[test]
fn test_null_from_non_simple_sort_term_raises_t2008() {
    for expr in [
        r#"[{"a":{"b":2}},{"a":{"b":null}}]^(a.b)"#,
        r#"[{"a":{"b":2}},{"a":{"b":null}}]^(>a.b)"#,
        r#"[{"a":2},{"a":null}]^($.a)"#,
        r#"[{"a":null},{"a":null}]^($.a)"#,
        r#"[{"a":true},{"a":null}]^($.a)"#,
        r#"[{"a":2},{"a":null}]^($lookup($, "a"))"#,
        r#"[{"a":null},{"a":1}]^($.a, b)"#,
        r#"[{"a":1, "b":null},{"a":1,"b":2}]^(a, $.b)"#,
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert!(
            tree_walker.contains("T2008"),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains("T2008"), "default: {expr} => {default}");
    }
}

#[test]
fn test_null_from_non_simple_sort_term_without_comparison() {
    for (expr, expected) in [
        (r#"[{},{"a":null}]^($.a)"#, r#"Ok([{"a":null},{}])"#),
        (r#"[{"a":null},{}]^($.a)"#, r#"Ok([{"a":null},{}])"#),
        (
            r#"[{"a":{"b":null}},{"a":{}},{"a":{}}]^(a.b)"#,
            r#"Ok([{"a":{"b":null}},{"a":{}},{"a":{}}])"#,
        ),
        (r#"[{"a":null}]^($.a)"#, r#"Ok({"a":null})"#),
        // The first term already orders the pair, so the null is never compared.
        (
            r#"[{"a":2,"b":null},{"a":1,"b":2}]^(a, $.b)"#,
            r#"Ok([{"a":1,"b":2},{"a":2,"b":null}])"#,
        ),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── `$eval` with an undefined focus; a function's tail builtin result ────────
//
// `$eval(expr, focus)` with an undefined focus used the undefined focus as
// its input, so it was undefined; jsonata-js keeps the current input (#247).
// A builtin called in a function's tail position returns its raw result in
// jsonata-js, never unwrapped, so an empty sequence such as `$keys({})` stays
// a value for `$map` and its siblings; and `$split` of undefined is
// undefined, not null (#250).

/// `eval_both_engines` with `input` as the input.
fn eval_both_engines_on(expr: &str, input: serde_json::Value) -> (String, String) {
    fn show(r: Result<JValue, jsonata_core::evaluator::EvaluatorError>) -> String {
        match r {
            Ok(JValue::Undefined) => "undefined".to_string(),
            Ok(v) => v.to_json_string().unwrap(),
            Err(e) => format!("Err({e})"),
        }
    }
    let input = JValue::from(input);
    let ast = parse(expr).unwrap();
    let tree_walker = show(Evaluator::new().evaluate(&ast, &input));
    let default = show(
        jsonata_core::Expression::compile(expr)
            .unwrap()
            .evaluate(&input),
    );
    (tree_walker, default)
}

#[test]
fn test_eval_with_undefined_focus_uses_the_input() {
    for (expr, input, expected) in [
        (r#"$eval("a", $v)"#, json!({"a": 5}), "5"),
        (r#"$eval("a", nothing)"#, json!({"a": 5}), "5"),
        (r#"$eval("$", $v)"#, json!([1, 2]), "[1,2]"),
        (r#"$eval("a", missing.deep)"#, json!({"a": 5}), "5"),
        (r#"($x := 3; $eval("$x + a", $v))"#, json!({"a": 5}), "8"),
        (
            r#"items.$eval("x", $u)"#,
            json!({"items": [{"x": 1}, {"x": 2}]}),
            "[1,2]",
        ),
        (r#"$eval(nothing, $v)"#, json!({"a": 5}), "undefined"),
        // A defined focus still replaces the input.
        (r#"$eval("a", {"a": 7})"#, json!({"a": 5}), "7"),
    ] {
        let (tree_walker, default) = eval_both_engines_on(expr, input);
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

#[test]
fn test_tail_builtin_result_is_not_unwrapped() {
    for (expr, expected) in [
        (r#"$map([1,2], function($v){ $keys({}) })"#, "[[],[]]"),
        (r#"$map([1,2], function($v){ ($keys({})) })"#, "[[],[]]"),
        (
            r#"$map([1,2], function($v){ $v>1 ? $keys({}) : 0 })"#,
            "[0,[]]",
        ),
        (r#"$map([1,2], function($v){ $keys(nothing) })"#, "[[],[]]"),
        (
            r#"$map([1,2], function($v){ $keys({"a":1}) })"#,
            r#"[["a"],["a"]]"#,
        ),
        (
            r#"$map([1,2], function($v){ $lookup([{}], "x") })"#,
            "[[],[]]",
        ),
        (
            r#"$map([{}, {"q":1}], function($x){ $lookup([$x], "q") })"#,
            "[[],[1]]",
        ),
        (
            r#"$map([1,2], function($v){ $match("abc", /z/) })"#,
            "[[],[]]",
        ),
        (
            r#"$each({"a":1,"b":2}, function($v){ $keys({}) })"#,
            "[[],[]]",
        ),
        (r#"$count($map([1,2], function($v){ $keys({}) }))"#, "2"),
        (
            r#"$map([1,2], function($v){ $map([1], function(){ $keys({}) }) })"#,
            "[[[]],[[]]]",
        ),
        // A direct call unwraps the result, as its call node does.
        (r#"(function(){ $keys({}) })()"#, "undefined"),
        (r#"[(function(){ $keys({}) })()]"#, "[]"),
        (r#"($f := function(){ $keys({}) }; [$f(), $f()])"#, "[]"),
        // An undefined callback result adds nothing.
        (
            r#"$map([1,2], function($v){ $split(nothing, " ") })"#,
            "undefined",
        ),
        (r#"[$split(nothing, " ")]"#, "[]"),
        (
            r#"$map([1,2], function($v){ $sift(nothing, function(){true}) })"#,
            "undefined",
        ),
        (
            r#"$map([1,2,3], function($v){ $v>1 ? $string($v) })"#,
            r#"["2","3"]"#,
        ),
        (
            r#"$map([1,2], function($v){ $v>1 ? $keys({"a":1}) })"#,
            r#"["a"]"#,
        ),
        // `$sort` tests its comparator's result as JavaScript does: an
        // array or object, even an empty one, is true.
        (r#"$sort([2,1], function($a,$b){ $keys({}) })"#, "[1,2]"),
        (r#"$sort([2,1], function($a,$b){ [] })"#, "[1,2]"),
        (r#"$sort([2,1], function($a,$b){ {} })"#, "[1,2]"),
        (r#"$sort([3,1,2], function($a,$b){ $a > $b })"#, "[1,2,3]"),
    ] {
        let (tree_walker, default) = eval_both_engines_on(expr, json!({"a": 5}));
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── The input of a builtin called in tail position ───────────────────────────
//
// jsonata-js applies a builtin called in a function's tail position from its
// trampoline, with the input `apply` was given: null when the function is a
// callback of a builtin (`$map`, ...) or applied by `~>` to a non-call, the
// call site's input for a direct call. The builtin reads it where it reads
// the context: `$eval`'s focus, or a signature's `-` parameter.

#[test]
fn test_tail_builtin_runs_with_the_apply_input() {
    for (expr, expected) in [
        (
            r#"$map(["a"], function($e){ $eval($e, $nope) })"#,
            "undefined",
        ),
        (
            r#"$map([1,2], function($v){ $eval("a", $v.nope) })"#,
            "undefined",
        ),
        (r#"$map(["a"], function($e){ $eval("a") })"#, "undefined"),
        (
            r#"$map(["a"], function($e){ $e ? $eval("a") : 0 })"#,
            "undefined",
        ),
        (r#"$filter([1], function($e){ $eval("a") })"#, "undefined"),
        (r#"$map(["a"], function($e){ $string() })"#, r#""null""#),
        (r#""x" ~> function($e){ $eval("a") }"#, "undefined"),
        (
            r#"($f := function($e){ $eval("a") }; "x" ~> $f)"#,
            "undefined",
        ),
        (
            r#"($f := function($e){ $eval("a", $nope) }; ($f ~> $string)("x"))"#,
            "undefined",
        ),
        // A direct call: the call site's input.
        (r#"($f := function($e){ $eval("a") }; $f("x"))"#, "5"),
        (r#"($f := function(){ $eval("a") }; {"a":7}.$f())"#, "7"),
        (
            r#"($f := function(){ $string() }; {"a":7}.$f())"#,
            r#""{\"a\":7}""#,
        ),
        (r#"($c := $string ~> $eval; $c("a"))"#, "5"),
        // Not in tail position: the function's own input.
        (r#"$map(["a"], function($e){ $v := $eval("a") })"#, "5"),
        (r#"$map(["a"], function($e){ $sum([$eval("a")]) })"#, "5"),
    ] {
        let (tree_walker, default) = eval_both_engines_on(expr, json!({"a": 5}));
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── Calling a step with an index or focus binding ────────────────────────────
//
// In jsonata-js `#` and `@` bind tighter than `.`, and `(` takes the bound
// step as its procedure: `$o#$i(3)` calls what `$o#$i` gives, and
// `$o.g[0]#$i(3)` calls `g[0]#$i` in each item of `$o`. These were parse
// errors (T1006), which also hid the S0215 that `$o[0]@$v` raises.

#[test]
fn test_call_of_bound_step() {
    for (expr, expected) in [
        ("($o := [function($x){$x}]; $o#$i(3))", "Ok(3)"),
        ("($o := function($x){$x}; $o#$i(3))", "Ok(3)"),
        (
            r#"($o := {"g": [function($x){$x}]}; $o.g[0]#$i(3))"#,
            "Ok(3)",
        ),
        (r#"($o := {"g": [function($x){$x}]}; $o.g#$i(3))"#, "Ok(3)"),
        (r#"$uppercase#$i("a")"#, r#"Ok("A")"#),
        (r#"$uppercase@$i("a")"#, r#"Ok("A")"#),
        ("($o := [function($x){$x}]; $o#$i(?)(4))", "Ok(4)"),
        (
            r#"($f := function($x){$x*10}; $d := [{"f":1},{"f":2}]; $d#$i.$f#$j(f+$i))"#,
            "Ok([10,30])",
        ),
        (
            r#"($f := function($x){$x*10}; $d := [{"f":1},{"f":2}]; $d@$v.$f@$w($v.f))"#,
            "Ok([10,20])",
        ),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

#[test]
fn test_call_of_bound_step_errors() {
    for (expr, code) in [
        ("($o := [function($x){$x}]; $o[0]@$v(3))", "S0215"),
        (
            r#"($o := {"g": [function($x){$x}]}; $o.g[0]@$v(3))"#,
            "S0215",
        ),
        // A focus binding keeps the context, which is not a function.
        ("($o := [function($x){$x}]; $o@$v(3))", "T1006"),
        // Two functions are an array, not a function.
        (
            "($o := [function($x){$x}, function($x){$x}]; $o#$i(3))",
            "T1006",
        ),
        ("[1,2]#$i(3)", "T1006"),
    ] {
        // S0215 is raised by the parser, the others when evaluating.
        if let Err(e) = parse(expr) {
            assert!(format!("{e:?}").contains(code), "parse: {expr} => {e:?}");
            continue;
        }
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
}

// ── T1005 for a called path that names a function ────────────────────────────
//
// jsonata-js raises T1005 ("Did you mean $name?") when a call's procedure
// path gives nothing and its first step's name is bound in the environment
// to a (JavaScript-)truthy value: a builtin, or a `$name` binding.

#[test]
fn test_call_of_path_naming_a_function_is_t1005() {
    for (expr, code) in [
        ("uppercase#$i()", "T1005"),
        ("uppercase@$i()", "T1005"),
        ("uppercase[0]()", "T1005"),
        (r#"($a := "x"; $a.uppercase#$i())"#, "T1005"),
        ("($x := 1; x#$i())", "T1005"),
        ("($uppercase := 0; uppercase#$i())", "T1006"),
        (r##""uppercase"#$i()"##, "T1006"),
        ("foo#$i()", "T1006"),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
}

// ── `~>` into a call of an expression's value ────────────────────────────────
//
// jsonata-js invokes any function call on the right of `~>` with the left
// side as its first argument, including a call whose procedure is an
// expression (`$uppercase[0]()`, `$o[1]()`, `$o#$i(4)`). Only a call of a
// named function did; the others were evaluated as calls first (T0410,
// T2006).

#[test]
fn test_chain_into_call_of_expression() {
    for (expr, expected) in [
        (r#""x" ~> $uppercase[0]()"#, r#"Ok("X")"#),
        (
            "($o := [function(){1}, function($v){$v+10}]; 1 ~> $o[1]())",
            "Ok(11)",
        ),
        ("($o := [function($a,$b){$a-$b}]; 10 ~> $o[0](3))", "Ok(7)"),
        (
            "($o := [function($a,$b){$a-$b}]; 10 ~> $o[0](?, 3))",
            "Ok(7)",
        ),
        ("($o := [function($a,$b){$a-$b}]; 10 ~> $o#$i(4))", "Ok(6)"),
        ("($o := [function($a){[$a]}]; 10 ~> $o[0]()[])", "Ok([10])"),
        (
            "($o := [function($x){$count($x)}]; nope ~> $o[0]())",
            "Ok(0)",
        ),
        ("1 ~> $string[0]()", r#"Ok("1")"#),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
    for (expr, code) in [
        ("($o := [1]; 10 ~> $o[0]())", "T1006"),
        ("10 ~> uppercase[0]()", "T1005"),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
}

// ── A string literal heading a path is a name step ───────────────────────────
//
// jsonata-js's `.` turns every string literal step of a path into a name,
// along with the predicates, group-by, sort or bindings it carries. Only a
// bare string was converted, so `"s"{k: v}.p`, `"s"[0].k` and `"s"#$i.k`
// read the string itself and gave undefined.

#[test]
fn test_string_literal_heading_path_is_name_step() {
    let data = JValue::from_json_str(
        r##"{"s":[{"k":"p","v":1},{"k":"q","v":2},{"k":"p","v":3}],"a":[{"b":"x","c":1},{"b":"y","c":2}]}"##,
    )
    .unwrap();
    let show = |r: Result<JValue, jsonata_core::evaluator::EvaluatorError>| match r {
        Ok(v) => format!("Ok({})", v.to_json_string().unwrap()),
        Err(e) => format!("Err({e})"),
    };
    let root = format!("Ok({})", data.to_json_string().unwrap());
    for (expr, expected) in [
        (r##""s"{k: v}.p"##, "Ok([1,3])"),
        (r##""s"{k: v}.$keys()"##, r##"Ok(["p","q"])"##),
        (r##""a"{b: c}.$keys()"##, r##"Ok(["x","y"])"##),
        (r##""s"[0]{k: v}.p"##, "Ok(1)"),
        (r##""s"[k="p"]{k: v}.p"##, "Ok([1,3])"),
        (r##""s"{k: v}.p[0]"##, "Ok(1)"),
        (r##""s"[0].k"##, r##"Ok("p")"##),
        (r##""s"[].k"##, r##"Ok(["p","q","p"])"##),
        (r##""s"#$i.(k & $i)"##, r##"Ok(["p0","q1","p2"])"##),
        (r##""s"^(v).k"##, r##"Ok(["p","q","p"])"##),
        (r##""s"{k: v}.%"##, root.as_str()),
        (r##""s"[0]{k: v}.%"##, root.as_str()),
        (r##""s"[0]{k: v}.p.%"##, r##"Ok({"p":1})"##),
        // Unchanged: a block is not a step to convert, and a string with no
        // `.` after it is a string.
        (r##"("s"){k: v}.p"##, "Ok(undefined)"),
        (r##""s"[0]"##, r##"Ok("s")"##),
        (r##""s"{k: v}"##, "Ok({})"),
    ] {
        let ast = parse(expr).unwrap();
        let tree_walker = show(Evaluator::new().evaluate(&ast, &data));
        let default = show(
            jsonata_core::Expression::compile(expr)
                .unwrap()
                .evaluate(&data),
        );
        let expected = if expected == "Ok(undefined)" {
            show(Ok(JValue::Undefined))
        } else {
            expected.to_string()
        };
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── Calling a quoted string step after `.` ───────────────────────────────────
//
// In jsonata-js, `(` takes `"f"[0]`, `"f"#$i` or `"f"` as the procedure
// before `.` turns the path's string steps into names, so the procedure is
// the string: calling it is T1006, never the T1005 `$`-hint a called name
// gets, and never a call of the field it would name.

#[test]
fn test_call_of_quoted_step_after_dot_is_t1006() {
    let data = JValue::from_json_str(r#"{"arr":[{"x":1}],"a":{}}"#).unwrap();
    for (expr, code) in [
        (r##"arr."uppercase"[0]()"##, "T1006"),
        (r##"a."uppercase"[0]()"##, "T1006"),
        (r##"arr."uppercase"#$i()"##, "T1006"),
        (r##"a."uppercase"#$i()"##, "T1006"),
        (r##"arr."uppercase"@$v()"##, "T1006"),
        (r##"arr."string"[0]()"##, "T1006"),
        (r##""z" ~> a."uppercase"[0]()"##, "T1006"),
        (r##"($foo := function(){1}; arr."foo"[0]())"##, "T1006"),
        (r##"($foo := 1; a."foo"@$v())"##, "T1006"),
        (r##"arr."uppercase"()"##, "T1006"),
        (r##"arr."uppercase"{"k":1}()"##, "T1006"),
        (r##"($o := {"f": $uppercase}; $o."f"("a"))"##, "T1006"),
        (r##"($o := {"f": [$uppercase]}; $o."f"#$i("a"))"##, "T1006"),
        // A backquoted or bare name is a name: T1005 suggests `$uppercase`.
        ("`uppercase`[0]()", "T1005"),
        ("arr.`uppercase`[0]()", "T1005"),
        ("arr.uppercase[0]()", "T1005"),
        ("a.uppercase#$i()", "T1005"),
    ] {
        let (tree_walker, default) =
            eval_both_engines_with(expr, EvaluatorOptions::default(), &data);
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
    // An unquoted name step calls the function its field holds.
    for expr in [
        r##"($o := {"f": $uppercase}; $o.f("a"))"##,
        r##"($o := {"f": $uppercase}; $o.`f`("a"))"##,
    ] {
        let ast = parse(expr).unwrap();
        let tree_walker = Evaluator::new().evaluate(&ast, &data).unwrap();
        let default = jsonata_core::Expression::compile(expr)
            .unwrap()
            .evaluate(&data)
            .unwrap();
        assert_eq!(tree_walker, JValue::from(json!("A")), "tree-walker: {expr}");
        assert_eq!(default, JValue::from(json!("A")), "default: {expr}");
    }
}

// ── Literal path steps (#248) ────────────────────────────────────────────────
//
// jsonata-js's `processAST` (parser.js, binary `.`) checks every step of a
// path built by `.`, the left-hand ones included: a number or `true`/`false`/
// `null` is S0213, and a string becomes a field name.

#[test]
fn test_literal_path_step_is_s0213() {
    for (expr, literal) in [
        ("null.$", "null"),
        ("true.$", "true"),
        ("false.$", "false"),
        ("2#$q.a", "2"),
        ("2.a", "2"),
        ("1.5.a", "1.5"),
        ("1e2.a", "100"),
        ("-2.a", "2"),
        ("a.-2", "-2"),
        ("a.--2", "2"),
        ("null[0].a", "null"),
        ("2[0].a", "2"),
        ("true.$[0]", "true"),
        ("null@$x.a", "null"),
        ("null[0]#$i.a", "null"),
        ("true@$v#$i.b", "true"),
        ("null^(x).a", "null"),
        (r#"2{"k":1}.a"#, "2"),
        ("null.(1)", "null"),
        ("null.[1]", "null"),
        ("null.()", "null"),
        ("a#$i.2", "2"),
        ("a.null@$v", "null"),
        ("a.b#$i.c#$j.null", "null"),
        // Already S0213: a literal right-hand step.
        ("a.2", "2"),
        ("a.null", "null"),
        ("$.2", "2"),
        // The first literal in the path is the one reported.
        ("2.a.b.c.null", "2"),
        ("null.(2.a)", "2"),
    ] {
        let err = parse(expr).unwrap_err().to_string();
        assert!(
            err.contains(&format!("S0213: The literal value {literal} cannot")),
            "{expr} => {err}"
        );
        let err = jsonata_core::Expression::compile(expr)
            .err()
            .unwrap_or_else(|| panic!("{expr} compiled"))
            .to_string();
        assert!(err.contains("S0213"), "compile {expr} => {err}");
    }
}

#[test]
fn test_literal_outside_a_dotted_path_is_evaluated() {
    for (expr, expected) in [
        ("2#$q", "Ok(2)"),
        ("$.(2)", "Ok(2)"),
        ("[0..2]", "Ok([0,1,2])"),
        // A string step is a field name, wherever it is in the path.
        (r#"({"a":[{"b":1},{"b":2}]}).("a"[0].b)"#, "Ok(1)"),
        (r#"({"a":[{"b":1},{"b":2}]}).("a".(b))"#, "Ok([1,2])"),
        (r#"({"a":[{"b":1},{"b":2}]}).("a".[b])"#, "Ok([[1],[2]])"),
        (r##"({"a":[{"b":1},{"b":2}]}).("a"#$i.b)"##, "Ok([1,2])"),
        (r##"({"a":[{"b":1},{"b":2}]}).("a"#$i[0].b)"##, "Ok(1)"),
        (r#"({"s":{"t":5}}).("s".t)"#, "Ok(5)"),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
    // A parenthesised literal is a block, not a literal step.
    for expr in ["(2).a", "-(2).a"] {
        assert_undefined_in_both_engines(expr);
    }
}

// ── Filtered array constructor inside an array constructor (#249) ────────────
//
// jsonata-js keeps a constructor's predicates and `[]` on the constructor
// node (parser.js `[`), and an array constructor pushes a member that is an
// array constructor whole instead of splicing it in (`evaluateUnary`,
// `item.value === '['`). A `#$i`/`@$v`, a sort or a following step makes a
// path, whose value is spliced.

#[test]
fn test_filtered_constructor_member_keeps_its_nesting() {
    for (expr, expected) in [
        ("[[0..9][$ % 2 = 0]]", "Ok([[0,2,4,6,8]])"),
        ("[[1, 2, [3, 4]][-1]]", "Ok([[3,4]])"),
        ("[[1,2][$>0]]", "Ok([[1,2]])"),
        ("[[1,2][true]]", "Ok([[1,2]])"),
        ("[[1,2][[0,1]]]", "Ok([[1,2]])"),
        ("[[1,2][]]", "Ok([[1,2]])"),
        ("[[1,2][$>0][]]", "Ok([[1,2]])"),
        ("[[1,2][$>1][]]", "Ok([[2]])"),
        ("[[[1,2]][0]]", "Ok([[1,2]])"),
        ("[[[1,2],[3]][0]]", "Ok([[1,2]])"),
        ("[[[1]][0]]", "Ok([[1]])"),
        ("[1,[2,3][$>0]]", "Ok([1,[2,3]])"),
        ("[[1,2][$>0], 5]", "Ok([[1,2],5])"),
        ("[[[1,2][$>0]]]", "Ok([[[1,2]]])"),
        (r#"{"x":[[1,2][$>0]]}"#, r#"Ok({"x":[[1,2]]})"#),
        (r#"[[{"a":1},{"a":2}][a>0]]"#, r#"Ok([[{"a":1},{"a":2}]])"#),
        ("$count([[1,2][$>0]])", "Ok(1)"),
        ("([[1,2][$>0]])[0]", "Ok([1,2])"),
        ("[[1,2][$>0]][0]", "Ok([1,2])"),
        ("[[1,2][$>0]][]", "Ok([[1,2]])"),
        ("$.[[1,2][$>0], 3]", "Ok([[1,2],3])"),
        // A one-item result is a single value, so these were already right.
        ("[[1,2][0]]", "Ok([1])"),
        ("[[1,2][$>1]]", "Ok([2])"),
        ("[[1,2][0][0]]", "Ok([1])"),
        ("[[1,2][$>5]]", "Ok([])"),
        // A path, not a constructor: its value is spliced.
        ("[([1,2])[0]]", "Ok([1])"),
        ("[([1,2])[$>0]]", "Ok([1,2])"),
        ("[[1,2]#$i]", "Ok([1,2])"),
        ("[[1,2][$>0]#$i]", "Ok([1,2])"),
        ("[[1,2][$>0].$]", "Ok([1,2])"),
        ("[[3,1,2]^($)]", "Ok([1,2,3])"),
        ("[[3,1,2][$>1]^($)]", "Ok([2,3])"),
        ("[[2,1][$>0]^($), 5]", "Ok([1,2,5])"),
        // `@$v` stays on the constructor in jsonata-js; `#$i` makes a path.
        ("[[1,2]@$v]", "Ok([[1,2]])"),
        // A group-by gives an object, pushed either way.
        (r#"[[1,2][$>0]{"k":$}]"#, r#"Ok([{"k":[1,2]}])"#),
    ] {
        let (tree_walker, default) = eval_both_engines(expr, EvaluatorOptions::default());
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── Functions a tail-position builtin applies ────────────────────────────────
//
// A builtin that is a function's tail call gets its function arguments
// unwrapped (jsonata-js's trampoline evaluates them directly), and applies
// each with the input it was defined with. (A function that came in as an
// argument of a direct call is wrapped in jsonata-js and gets a null input;
// that is not told apart yet, #254.)

#[test]
fn test_tail_builtin_applies_its_functions_raw() {
    for (expr, expected) in [
        (
            r#"($o := function(){ $map([1], function(){ $eval("a") }) }; $o())"#,
            "5",
        ),
        (
            r#"($o := function(){ $map([1,2], function(){ $eval("a") }) }; $o())"#,
            "[5,5]",
        ),
        (
            r#"($o := function(){ $map([1], function(){ $string() }) }; $o())"#,
            r#""{\"a\":5,\"b\":{\"a\":7}}""#,
        ),
        (
            r#"($o := function(){ $filter([1], function(){ $eval("a") }) }; $o())"#,
            "1",
        ),
        (
            r#"($o := function(){ $reduce([1,2], function($a,$b){ $eval("a") }) }; $o())"#,
            "5",
        ),
        (
            r#"($g := function($x){ $eval("a") }; $o := function(){ $map([1], $g) }; $o())"#,
            "5",
        ),
        (
            r#"b.($g := function($x){ $eval("a") }; $o := function(){ $map([1], $g) }; $o())"#,
            "7",
        ),
        (
            r#"($o := function(){ $map([1], function(){ ($eval("a")) }) }; $o())"#,
            "5",
        ),
        (
            r#"($i := function($n){ $n > 0 ? $i($n-1) : $map([1], function(){ $eval("a") }) }; $i(2))"#,
            "5",
        ),
        // Not a tail call: a null input.
        (
            r#"($g := function($x){ $eval("a") }; $map([1], $g))"#,
            "undefined",
        ),
        (
            r#"($o := function(){ [$map([1], function(){ $eval("a") })] }; $o())"#,
            "[]",
        ),
        // A function also called by its own name while it is an argument.
        (
            r#"($f := function($x){ $string() }; $c := function($g){ $f(1) }; $c($f))"#,
            r#""{\"a\":5,\"b\":{\"a\":7}}""#,
        ),
        (
            r#"($p := function($f){ $f() }; $o := function(){ $p(function(){ $map([1], function(){ $eval("a") }) }) }; $o())"#,
            "5",
        ),
    ] {
        let (tree_walker, default) = eval_both_engines_on(expr, json!({"a": 5, "b": {"a": 7}}));
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── A misplaced `@` beats a literal step ─────────────────────────────────────
//
// jsonata-js's `processAST` checks a `@` step (S0215 after a predicate,
// S0216 after a sort) while processing an operand, before the `.` around it
// checks for literal steps (S0213). `#`, `@` and `[...]` bind tighter than
// `.` there, so `1.a[0]@$v` is S0215 too, while a group-by or sort, which
// bind looser, leave the `.` first.

#[test]
fn test_focus_binding_error_precedes_literal_step() {
    for (expr, code) in [
        ("1[0]@$v.**.q", "S0215"),
        ("null[0]@$v.$append(a.c.$, n[])", "S0215"),
        ("1[0]@$v.a", "S0215"),
        ("true[0]@$v.a", "S0215"),
        ("a[0]@$v.1", "S0215"),
        ("$x[0]@$v.true", "S0215"),
        ("[1][0]@$v.2", "S0215"),
        ("1[0]#$i@$v.a", "S0215"),
        ("1^(x)@$v.a", "S0216"),
        ("a^(x)@$v.null", "S0216"),
        ("b[0]@$v + 1.a", "S0215"),
        ("1.a[0]@$v", "S0215"),
        ("a.1[0]@$v", "S0215"),
        ("a.null[0]@$v", "S0215"),
        ("1.a[0]#$i@$v", "S0215"),
        ("1.a@$u[0]@$v", "S0215"),
        ("1.(a)[0]@$v", "S0215"),
        ("1.a[0]@$v.2", "S0215"),
        // The `.` comes first.
        ("1.a + b[0]@$v", "S0213"),
        ("(1.a)[0]@$v", "S0213"),
        ("1.a.b[0]@$v", "S0213"),
        ("1.a^(x)@$v", "S0213"),
        (r#"1.a[0]{"k":1}@$v"#, "S0213"),
        ("1[]@$v.a", "S0213"),
    ] {
        let tree_walker = parse(expr).unwrap_err().to_string();
        let default = jsonata_core::Expression::compile(expr)
            .err()
            .unwrap_or_else(|| panic!("{expr} compiled"))
            .to_string();
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
}

// ── Calls of bound steps: literals, blocks, predicates, error order ──────────
//
// A literal, wildcard or constructor is no more callable under `#`/`@` than
// on its own (`"f"()`, `a.1()`): `"f"@$v()` and `a.1#$i()` are the same T1006
// parse error, ahead of any S0213 for the step or error elsewhere. A bound
// `.(expr)` step is called as its block (`a.(1)@$v()` is T1006). A filter on
// a step that gives nothing is still evaluated once, as jsonata-js's
// `evaluateFilter` does with `[undefined]`. And a `@` on a sort or group-by
// under a unary minus in a `.`'s operand comes after that `.`'s S0213, as
// both bind looser than `.` in jsonata-js.

#[test]
fn test_call_of_bound_literal_and_filter_on_nothing() {
    let data = JValue::from_json_str(
        r##"{"a":{"b":[1,2],"c":"s","f":3},"f":5,"s":"str","arr":[{"x":1,"c":"p"},{"x":2,"c":"q"}]}"##,
    )
    .unwrap();
    for (expr, code) in [
        // A bound literal step called.
        ("a.1@$v()", "T1006"),
        ("a.true#$i()", "T1006"),
        ("a.b.1#$i()", "T1006"),
        ("a(1).true@$v()", "T1006"),
        (r##"a."f"@$v()"##, "T1006"),
        (r##""f"@$v(?)"##, "T1008"),
        // A bound `.(expr)` step called.
        (r##"a.("f")@$v()"##, "T1006"),
        ("a.(1)@$v()", "T1006"),
        ("*[].(1)@$v()", "T1006"),
        ("a.(1)#$i()", "T1006"),
        // A filter on a missing step.
        (r##"x["f"@$v()]"##, "T1006"),
        ("x[[1@$v()]]", "T1006"),
        (r##"x[["f"#$i()]]"##, "T1006"),
        ("x[[$@$v()]]", "T1006"),
        (r##"x[(["f"@$v()])]"##, "T1006"),
        ("x[[a@$v()]]", "T1006"),
        ("x[[a()]]", "T1006"),
        ("x[$string(1,2,3)]", "T0410"),
        ("a.x[$string(1,2,3)]", "T0410"),
        ("x[true][$string(1,2,3)]", "T0410"),
        // In a constructor, before the errors of other members.
        (r##"[$string(1,2,3), "f"@$v()]"##, "T1006"),
        ("[$string(1,2,3), *@$v()]", "T1006"),
        (r##"{"k": $error("x"), "j": "f"@$v()}"##, "T1006"),
        ("[%^(x), 1@$v()]", "T1006"),
        // A sort under a unary minus binds looser than `.`.
        ("true.-a^(x)@$v", "S0213"),
        ("true.-2^(x)@$v", "S0213"),
        ("true[0][1].-2^(x)@$v", "S0213"),
        ("-a^(x)@$v", "S0216"),
        ("b.-a^(x)@$v + 1.c", "S0216"),
        ("true.-a[0]@$v", "S0215"),
        ("true.-$f(b^(x)@$v)", "S0216"),
    ] {
        let (tree_walker, default) =
            eval_both_engines_with(expr, EvaluatorOptions::default(), &data);
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
    // A filter that selects nothing stops there; one on a field that is
    // there is unchanged.
    for (expr, expected) in [
        ("x[false][$string(1,2,3)]", "Ok(undefined)"),
        ("x[0][$string(1,2,3)]", "Ok(undefined)"),
        ("$count(arr.x[true])", "Ok(2)"),
        ("arr.x[$ > 1]", "Ok(2)"),
        (r##"($o := [function($x){$x}]; $o#$i(3))"##, "Ok(3)"),
    ] {
        let (tree_walker, default) =
            eval_both_engines_with(expr, EvaluatorOptions::default(), &data);
        let expected = if expected == "Ok(undefined)" {
            format!("Ok({})", JValue::Undefined.to_json_string().unwrap())
        } else {
            expected.to_string()
        };
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
}

// ── No filter on nothing after a sort or a `~>` call ─────────────────────────
//
// jsonata-js's evaluatePath stops at a step that gives nothing, so a sort
// step's filters do not run when the sort's input is undefined; and `~>`
// never filters its call's result, so an undefined one is not filtered.

#[test]
fn test_no_filter_on_nothing_after_sort_or_chain() {
    let data = JValue::from_json_str(r##"{"a":{"b":[1,2]}}"##).unwrap();
    let undefined = format!("Ok({})", JValue::Undefined.to_json_string().unwrap());
    for (expr, expected) in [
        (r##"x^(a)[$error("e")]"##, undefined.as_str()),
        (r##"(x)^(a)[$error("e")]"##, undefined.as_str()),
        (r##"x^(a)^(b)[$error("e")]"##, undefined.as_str()),
        (r##"$exists(x^(c)[$error("e")])"##, "Ok(false)"),
        (r##"$lookup({},"a")^($)[$error("e")]"##, undefined.as_str()),
        (r##"a.x^(c)[$error("e")]"##, undefined.as_str()),
        (
            r##"($f:=function($x){()}; 1 ~> $f()[$error("e")])"##,
            undefined.as_str(),
        ),
        (r##"x ~> $string()[$error("e")]"##, undefined.as_str()),
        // The documented divergence: a defined call result is filtered.
        (r##""z" ~> $append([1])[0]"##, r##"Ok("z")"##),
    ] {
        let (tree_walker, default) =
            eval_both_engines_with(expr, EvaluatorOptions::default(), &data);
        assert_eq!(tree_walker, expected, "tree-walker: {expr}");
        assert_eq!(default, expected, "default: {expr}");
    }
    // A filter on a value that is not a path step still runs on nothing.
    for (expr, code) in [
        (r##"(x)[$error("e")]"##, "D3137"),
        (r##"$x[$error("e")]"##, "D3137"),
        ("a.(x)[$string(1,2,3)]", "T0410"),
    ] {
        let (tree_walker, default) =
            eval_both_engines_with(expr, EvaluatorOptions::default(), &data);
        assert!(
            tree_walker.contains(code),
            "tree-walker: {expr} => {tree_walker}"
        );
        assert!(default.contains(code), "default: {expr} => {default}");
    }
}
