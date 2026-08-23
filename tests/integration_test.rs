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

/// jsonata-js's own canonical guardrails-documentation example: an infinite
/// tail-recursive call. `invoke_lambda_with_tco`'s trampoline loop previously
/// had no timeout check at all -- only the hardcoded `MAX_TCO_ITERATIONS`
/// iteration cap -- so this expression ran up to 100,000 iterations
/// completely ignoring `timeout_ms` and failed with U1001 instead of D1012.
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
/// (not 100,000 iterations) to keep the test suite fast; the pre-existing
/// U1001-at-100k-iterations behavior is unchanged and not the target of this
/// regression guard.
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

/// Without a configured timeout, the `MAX_TCO_ITERATIONS` hardcoded backstop
/// must still apply to genuinely infinite tail recursion -- the timeout gate
/// added to `invoke_lambda_with_tco` (`timeout_ms.is_none() && iterations >
/// MAX_TCO_ITERATIONS`) must not disable the cap when there is no timeout to
/// take over as the exit condition, or this would hang forever. Takes on the
/// order of tens of milliseconds (100,000 trivial trampoline iterations); not
/// a bounded/fast variant, since the whole point is exercising the cap itself.
#[test]
fn test_tco_trampoline_no_timeout_configured_still_hits_iteration_cap() {
    let data = JValue::Null;
    let expr_str = "($inf := function(){$inf()}; $inf())";
    let ast = parse(expr_str).unwrap();
    let mut evaluator = Evaluator::new();
    let err = evaluator
        .evaluate(&ast, &data)
        .expect_err("expected U1001 at the iteration cap");
    assert!(
        err.to_string().contains("U1001"),
        "expected U1001, got: {err}"
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
/// arg-count ceiling and merges all 300 objects correctly.
///
/// NOTE ON API CHOICE: same reasoning as the tests above - bare
/// `Evaluator::evaluate` never invokes `try_compile_expr`/the VM at all, so a
/// test built on it alone would not exercise the guard being fixed here.
#[cfg(feature = "bench")]
#[test]
fn test_merge_call_with_more_than_u8_max_args_does_not_silently_truncate() {
    use jsonata_core::_bench;
    let n = 300;
    let objects: Vec<String> = (0..n).map(|i| format!("{{\"k{i}\": {i}}}")).collect();
    let expr_str = format!("$merge({})", objects.join(", "));
    let ast = parse(&expr_str).unwrap();
    let data = JValue::Null;

    // The guard must make compilation decline entirely (fall back to the
    // tree-walker) rather than succeed with a truncated arg count baked into
    // the bytecode.
    assert!(
        _bench::compile(&ast).is_none(),
        "expected the >u8::MAX-arg $merge call to decline bytecode compilation \
         (fall back to the tree-walker), but it compiled successfully - the \
         u8::MAX arg-count guard appears to be missing or broken"
    );

    let result = Evaluator::new().evaluate(&ast, &data).unwrap();
    match result {
        JValue::Object(obj) => {
            assert_eq!(
                obj.len(),
                n,
                "merge() with 300 single-key object args silently dropped keys - \
                 likely the u8 arg-count truncation this guard exists to prevent"
            );
            for i in 0..n {
                assert_eq!(
                    obj.get(&format!("k{i}")),
                    Some(&JValue::from(i as i64)),
                    "missing or wrong value for k{i} in merged result"
                );
            }
        }
        other => panic!("expected object, got {other:?}"),
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
