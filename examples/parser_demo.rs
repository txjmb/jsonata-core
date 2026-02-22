// Parser demonstration
// This example shows how to use the JSONata parser

use jsonata_core::parser;

fn main() {
    println!("JSONata Parser Demo\n");
    println!("===================\n");

    // Example 1: Simple arithmetic
    println!("Example 1: Simple arithmetic");
    let expr = "1 + 2 * 3";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 2: Path expression
    println!("Example 2: Path expression");
    let expr = "user.name";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 3: Function call
    println!("Example 3: Function call");
    let expr = "sum(1, 2, 3)";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 4: Array literal
    println!("Example 4: Array literal");
    let expr = "[1, 2, 3, 4, 5]";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 5: Object literal
    println!("Example 5: Object literal");
    let expr = r#"{"name": "Alice", "age": 30}"#;
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 6: Conditional expression
    println!("Example 6: Conditional expression");
    let expr = "x > 0 ? 'positive' : 'negative'";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 7: Complex nested expression
    println!("Example 7: Complex nested expression");
    let expr = "(a + b) * c.d";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 8: String concatenation
    println!("Example 8: String concatenation");
    let expr = r#""Hello" & " " & "World""#;
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 9: Variable reference
    println!("Example 9: Variable reference");
    let expr = "$myVar + 10";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }

    // Example 10: Block expression
    println!("Example 10: Block expression");
    let expr = "(x := 5; y := 10; x + y)";
    match parser::parse(expr) {
        Ok(ast) => println!("  '{}' => {:?}\n", expr, ast),
        Err(e) => println!("  Error: {}\n", e),
    }
}
