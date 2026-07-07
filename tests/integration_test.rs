// Integration tests for Parser + Evaluator
//
// These tests verify that the parser and evaluator work together correctly
// to process complete JSONata expressions.

use jsonata_core::{
    evaluator::{Context, Evaluator, EvaluatorOptions},
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

    // Undefined variables return null in JSONata (not an error)
    let ast = parse("$undefined").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, JValue::Null);
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

    let ast = parse("value / 0").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);

    assert!(result.is_err());
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

/// Deep non-tail recursion must hit the soft recursion-depth guard and return
/// a graceful U1001 error - not overrun the *real* OS stack and crash the
/// process. Windows' default thread stack (~1MB) is much smaller than
/// Linux's (~8MB), so this is run on a thread with an explicitly small stack
/// to make the platform-independent of the test host: reference test suite
/// case `tail-recursion/case005` crashed the whole process on windows-latest
/// CI runners before the fix (see GitHub issue #34).
#[test]
fn test_deep_recursion_does_not_overflow_native_stack() {
    // JValue/EvaluatorError are Rc-based (deliberately !Send for speed), so the
    // evaluation has to stay inside the spawned closure - only a plain String
    // summary crosses the join boundary.
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024) // 1MB, matching Windows' default thread stack
        .spawn(|| {
            let data = JValue::Null;
            let ast = parse("($inf := function($n){$n+$inf($n-1)}; $inf(5))").unwrap();
            let mut evaluator = Evaluator::new();
            match evaluator.evaluate(&ast, &data) {
                Ok(v) => format!("Ok({v:?})"),
                Err(e) => format!("Err({e})"),
            }
        })
        .unwrap();

    let outcome = handle
        .join()
        .expect("evaluation overflowed the native stack instead of returning a graceful error");

    assert!(
        outcome.contains("U1001"),
        "expected a U1001 stack-overflow error, got: {outcome}"
    );
}

/// A user-configured `max_stack_depth` tighter than the hardcoded native-stack
/// ceiling (302) must trip D1011, not U1001 — the tree-walker's own guardrail,
/// not the Rust-specific native-stack safety net.
#[test]
fn test_max_stack_depth_raises_d1011_not_u1001() {
    let data = JValue::Null;
    let ast = parse("($inf := function($n){$n+$inf($n-1)}; $inf(5))").unwrap();
    let context = Context::new();
    let options = jsonata_core::evaluator::EvaluatorOptions {
        max_stack_depth: Some(10),
        ..Default::default()
    };
    let mut evaluator = Evaluator::with_options(context, options);
    let result = evaluator.evaluate(&ast, &data);
    let err = result.expect_err("expected a D1011 stack-overflow error");
    assert!(
        err.to_string().contains("D1011"),
        "expected D1011, got: {err}"
    );
}

/// A `max_stack_depth` at or above the hardcoded ceiling changes nothing — the
/// hardcoded 302 ceiling (and its U1001 error) remains the effective, always-on
/// backstop (GitHub issue #34).
#[test]
fn test_max_stack_depth_above_hardcoded_ceiling_still_raises_u1001() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let data = JValue::Null;
            let ast = parse("($inf := function($n){$n+$inf($n-1)}; $inf(5))").unwrap();
            let options = jsonata_core::evaluator::EvaluatorOptions {
                max_stack_depth: Some(100_000),
                ..Default::default()
            };
            let mut evaluator = Evaluator::with_options(Context::new(), options);
            match evaluator.evaluate(&ast, &data) {
                Ok(v) => format!("Ok({v:?})"),
                Err(e) => format!("Err({e})"),
            }
        })
        .unwrap();
    let outcome = handle.join().unwrap();
    assert!(outcome.contains("U1001"), "expected U1001, got: {outcome}");
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
// so the fragile runtime pieces (create_tuple_stream's deferred incoming-unbind
// for `%.%` chains, the step's own-label bind before apply_stages, and routing
// a `%` path-step over a tuple stream) fail loudly if broken. Expected values
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
