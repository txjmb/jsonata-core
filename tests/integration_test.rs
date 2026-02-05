// Integration tests for Parser + Evaluator
//
// These tests verify that the parser and evaluator work together correctly
// to process complete JSONata expressions.

use jsonatapy::{parser::parse, evaluator::{Evaluator, Context}};
use serde_json::{json, Value};

#[test]
fn test_simple_field_access() {
    let data = json!({
        "name": "Alice",
        "age": 30
    });

    let ast = parse("name").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("Alice"));
}

#[test]
fn test_nested_field_access() {
    let data = json!({
        "user": {
            "profile": {
                "name": "Bob"
            }
        }
    });

    let ast = parse("user.profile.name").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("Bob"));
}

#[test]
fn test_arithmetic_expression() {
    let data = json!({
        "price": 100,
        "quantity": 5
    });

    // Test basic multiplication - arithmetic produces f64 results
    let ast = parse("price * quantity").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, json!(500.0));

    // Test complex arithmetic
    let ast = parse("(price + 10) * quantity").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, json!(550.0));
}

#[test]
fn test_comparison_expression() {
    let data = json!({
        "age": 25,
        "threshold": 18
    });

    let ast = parse("age > threshold").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_logical_expression() {
    let data = json!({
        "age": 25,
        "has_license": true
    });

    let ast = parse("age >= 18 and has_license").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_string_concatenation() {
    let data = json!({
        "first": "Hello",
        "second": "World"
    });

    let ast = parse(r#"first & " " & second"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("Hello World"));
}

#[test]
fn test_function_call() {
    let data = json!({
        "name": "alice"
    });

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$uppercase(name)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("ALICE"));
}

#[test]
fn test_nested_function_calls() {
    let data = json!({
        "text": "HELLO"
    });

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$length($lowercase(text))").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!(5));
}

#[test]
fn test_conditional_expression() {
    let data = json!({
        "score": 85
    });

    let ast = parse(r#"score >= 80 ? "Pass" : "Fail""#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("Pass"));
}

#[test]
fn test_array_literal() {
    let data = json!({
        "a": 1,
        "b": 2,
        "c": 3
    });

    let ast = parse("[a, b, c]").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!([1, 2, 3]));
}

#[test]
fn test_object_literal() {
    let data = json!({
        "x": 10,
        "y": 20
    });

    let ast = parse(r#"{"sum": x + y, "product": x * y}"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    // Arithmetic operations produce f64 results
    assert_eq!(result, json!({"sum": 30.0, "product": 200.0}));
}

#[test]
fn test_range_operator() {
    let data = Value::Null;

    let ast = parse("1..5").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!([1, 2, 3, 4, 5]));
}

#[test]
fn test_in_operator() {
    let data = json!({
        "value": 3,
        "list": [1, 2, 3, 4, 5]
    });

    let ast = parse("value in list").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_complex_real_world_example() {
    let data = json!({
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
    });

    // Access nested fields
    let ast = parse("order.customer.name").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, json!("Alice Smith"));

    // Check customer type
    let ast = parse(r#"order.customer.type = "premium""#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_missing_field_returns_null() {
    let data = json!({
        "name": "Alice"
    });

    let ast = parse("missing_field").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, Value::Null);
}

#[test]
fn test_deep_nesting() {
    let data = json!({
        "a": {
            "b": {
                "c": {
                    "d": {
                        "e": "deep value"
                    }
                }
            }
        }
    });

    let ast = parse("a.b.c.d.e").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("deep value"));
}

#[test]
fn test_multiple_operations() {
    let data = json!({
        "x": 10,
        "y": 20,
        "z": 30
    });

    let ast = parse("(x + y) * z / 2").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!(450.0));
}

#[test]
fn test_sum_function() {
    let data = json!({
        "numbers": [1, 2, 3, 4, 5]
    });

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$sum(numbers)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!(15.0));
}

#[test]
fn test_count_function() {
    let data = json!({
        "items": [1, 2, 3, 4, 5]
    });

    // Built-in functions require the $ prefix in JSONata
    let ast = parse("$count(items)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!(5));
}

#[test]
fn test_nested_conditionals() {
    let data = json!({
        "score": 75
    });

    let ast = parse(r#"score >= 90 ? "A" : (score >= 80 ? "B" : (score >= 70 ? "C" : "F"))"#).unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!("C"));
}

#[test]
fn test_block_expression() {
    let data = Value::Null;

    let ast = parse("(1; 2; 3)").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    // Block should return the last expression
    assert_eq!(result, json!(3));
}

#[test]
fn test_unary_negation() {
    let data = json!({
        "value": 42
    });

    let ast = parse("-value").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!(-42.0));
}

#[test]
fn test_modulo_operator() {
    let data = json!({
        "dividend": 17,
        "divisor": 5
    });

    let ast = parse("dividend % divisor").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!(2.0));
}

#[test]
fn test_comparison_operators() {
    let data = json!({
        "a": 10,
        "b": 20
    });

    // Less than
    let ast = parse("a < b").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), Value::Bool(true));

    // Less than or equal
    let ast = parse("a <= b").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), Value::Bool(true));

    // Greater than
    let ast = parse("b > a").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), Value::Bool(true));

    // Greater than or equal
    let ast = parse("b >= a").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), Value::Bool(true));

    // Equal
    let ast = parse("a = 10").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), Value::Bool(true));

    // Not equal
    let ast = parse("a != b").unwrap();
    let mut evaluator = Evaluator::new();
    assert_eq!(evaluator.evaluate(&ast, &data).unwrap(), Value::Bool(true));
}

#[test]
fn test_string_comparison() {
    let data = json!({
        "name1": "Alice",
        "name2": "Bob"
    });

    let ast = parse("name1 < name2").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, Value::Bool(true));
}

#[test]
fn test_empty_array() {
    let data = Value::Null;

    let ast = parse("[]").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!([]));
}

#[test]
fn test_empty_object() {
    let data = Value::Null;

    let ast = parse("{}").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, json!({}));
}

#[test]
fn test_error_undefined_variable() {
    let data = Value::Null;

    // Undefined variables return null in JSONata (not an error)
    let ast = parse("$undefined").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data).unwrap();

    assert_eq!(result, Value::Null);
}

#[test]
fn test_error_type_mismatch() {
    let data = json!({
        "text": "hello",
        "number": 42
    });

    let ast = parse("text + number").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);

    assert!(result.is_err());
}

#[test]
fn test_error_division_by_zero() {
    let data = json!({
        "value": 10
    });

    let ast = parse("value / 0").unwrap();
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);

    assert!(result.is_err());
}

#[test]
fn test_with_variables() {
    let data = json!({
        "price": 100
    });

    let ast = parse("price * $discount").unwrap();

    // Create context with discount variable
    let mut context = Context::new();
    context.bind("discount".to_string(), json!(0.9));
    let mut evaluator = Evaluator::with_context(context);

    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, json!(90.0));
}
