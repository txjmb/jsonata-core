// Integration tests for Parser + Evaluator
//
// These tests verify that the parser and evaluator work together correctly
// to process complete JSONata expressions.

use jsonata_core::{
    evaluator::{Context, Evaluator},
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
