// Comprehensive demonstration of the JSONata evaluator
//
// This example demonstrates all the features of the evaluator:
// - Path traversal
// - Arithmetic operations
// - Comparison operations
// - Logical operations
// - String concatenation
// - Range operator
// - Function calls
// - Conditional expressions
// - Arrays and objects

use jsonata_core::value::JValue;
use jsonata_core::{evaluator::Evaluator, parser::parse};
use serde_json::json;

fn main() {
    println!("=== JSONata Evaluator Demo ===\n");

    // Example 1: Path traversal
    demo_path_traversal();

    // Example 2: Arithmetic
    demo_arithmetic();

    // Example 3: Comparisons
    demo_comparisons();

    // Example 4: Logical operations
    demo_logical();

    // Example 5: String operations
    demo_strings();

    // Example 6: Functions
    demo_functions();

    // Example 7: Conditionals
    demo_conditionals();

    // Example 8: Complex example
    demo_complex();
}

fn demo_path_traversal() {
    println!("--- Path Traversal ---");

    let data: JValue = json!({
        "user": {
            "name": "Alice",
            "address": {
                "city": "New York",
                "zip": "10001"
            }
        }
    })
    .into();

    let examples = vec!["user.name", "user.address.city", "user.address.zip"];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_arithmetic() {
    println!("--- Arithmetic Operations ---");

    let data: JValue = json!({
        "price": 100,
        "quantity": 5,
        "tax_rate": 0.1
    })
    .into();

    let examples = vec![
        "price + 10",
        "price - 10",
        "price * quantity",
        "price / 2",
        "quantity % 2",
        "price * quantity * (1 + tax_rate)",
    ];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_comparisons() {
    println!("--- Comparison Operations ---");

    let data: JValue = json!({
        "age": 25,
        "threshold": 18
    })
    .into();

    let examples = vec![
        "age > threshold",
        "age >= 25",
        "age < 30",
        "age = 25",
        "age != 30",
    ];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_logical() {
    println!("--- Logical Operations ---");

    let data: JValue = json!({
        "age": 25,
        "has_license": true
    })
    .into();

    let examples = vec![
        "age > 18 and has_license",
        "age < 16 or has_license",
        "age > 21 and age < 30",
    ];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_strings() {
    println!("--- String Operations ---");

    let data: JValue = json!({
        "first_name": "Alice",
        "last_name": "Smith"
    })
    .into();

    let examples = vec![
        r#"first_name & " " & last_name"#,
        r#""Hello, " & first_name & "!""#,
    ];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_functions() {
    println!("--- Function Calls ---");

    let data: JValue = json!({
        "name": "alice",
        "numbers": [1, 2, 3, 4, 5]
    })
    .into();

    let examples = vec![
        "uppercase(name)",
        "lowercase(name)",
        "length(name)",
        "sum(numbers)",
        "count(numbers)",
    ];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_conditionals() {
    println!("--- Conditional Expressions ---");

    let data: JValue = json!({
        "score": 85
    })
    .into();

    let examples = vec![
        r#"score >= 90 ? "A" : (score >= 80 ? "B" : "C")"#,
        r#"score > 60 ? "Pass" : "Fail""#,
    ];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}

fn demo_complex() {
    println!("--- Complex Example: E-commerce Order ---");

    let data: JValue = json!({
        "order": {
            "id": "ORD-123",
            "items": [
                {"product": "Laptop", "price": 1000, "quantity": 1},
                {"product": "Mouse", "price": 25, "quantity": 2}
            ],
            "customer": {
                "name": "Alice Smith",
                "email": "alice@example.com",
                "membership": "gold"
            },
            "shipping": {
                "method": "express",
                "cost": 15
            }
        }
    })
    .into();

    println!("Data: {}\n", serde_json::to_string_pretty(&data).unwrap());

    let examples = vec!["order.id", "order.customer.name", "order.shipping.cost"];

    for expr_str in examples {
        match parse(expr_str) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  {} => {}", expr_str, result),
                    Err(e) => println!("  {} => ERROR: {}", expr_str, e),
                }
            }
            Err(e) => println!("  Parse error for '{}': {}", expr_str, e),
        }
    }
    println!();
}
