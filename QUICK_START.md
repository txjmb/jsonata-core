# JSONata Evaluator - Quick Start Guide

## File Location
**Main Implementation**: `C:\Users\mboha\source\repos\jsonatapy\src\evaluator.rs`

## Quick Usage

### Basic Evaluation
```rust
use jsonatapy::{parser::parse, evaluator::Evaluator};
use serde_json::json;

let data = json!({"name": "Alice", "age": 30});
let ast = parse("name").unwrap();
let mut evaluator = Evaluator::new();
let result = evaluator.evaluate(&ast, &data).unwrap();
// result: "Alice"
```

### With Variables
```rust
let data = json!({"price": 100});
let ast = parse("price * $discount").unwrap();
let mut evaluator = Evaluator::new();
evaluator.context.bind("discount".to_string(), json!(0.9));
let result = evaluator.evaluate(&ast, &data).unwrap();
// result: 90
```

## Supported Operations

| Category | Operators | Example |
|----------|-----------|---------|
| **Arithmetic** | `+`, `-`, `*`, `/`, `%` | `price * quantity` |
| **Comparison** | `=`, `!=`, `<`, `<=`, `>`, `>=` | `age >= 18` |
| **Logical** | `and`, `or` | `x > 0 and y < 10` |
| **String** | `&` | `"Hello" & " World"` |
| **Range** | `..` | `1..5` → `[1,2,3,4,5]` |
| **Membership** | `in` | `3 in [1,2,3]` |
| **Unary** | `-`, `not` | `-value`, `not flag` |

## Supported Functions

| Function | Purpose | Example |
|----------|---------|---------|
| `uppercase(s)` | Convert to uppercase | `uppercase("hello")` → `"HELLO"` |
| `lowercase(s)` | Convert to lowercase | `lowercase("HELLO")` → `"hello"` |
| `length(s)` | String length | `length("hello")` → `5` |
| `sum(arr)` | Sum array | `sum([1,2,3])` → `6` |
| `count(arr)` | Count elements | `count([1,2,3])` → `3` |
| `keys(obj)` | Get object keys | `keys({"a":1,"b":2})` → `["a","b"]` |

## Path Expressions

```rust
// Simple field access
"name"  // → data.name

// Nested access
"user.address.city"  // → data.user.address.city

// Missing fields return null (no error)
"missing.field"  // → null
```

## Conditionals

```rust
// Ternary operator
"age >= 18 ? 'adult' : 'minor'"

// Nested conditionals
"score >= 90 ? 'A' : (score >= 80 ? 'B' : 'C')"

// Without else (returns null)
"condition ? 'yes'"
```

## Arrays and Objects

```rust
// Array literal
"[1, 2, 3]"

// Array with expressions
"[price, quantity, price * quantity]"

// Object literal
"{'name': 'Alice', 'age': 30}"

// Object with expressions
"{'total': price * quantity, 'tax': total * 0.1}"
```

## Common Patterns

### Calculate Total Price
```rust
let data = json!({"price": 10, "quantity": 5, "tax": 0.1});
let ast = parse("price * quantity * (1 + tax)").unwrap();
// result: 55
```

### Filter by Condition
```rust
let data = json!({"age": 25});
let ast = parse("age >= 18 ? 'eligible' : 'not eligible'").unwrap();
// result: "eligible"
```

### String Formatting
```rust
let data = json!({"first": "John", "last": "Doe"});
let ast = parse("first & ' ' & last").unwrap();
// result: "John Doe"
```

### Complex Nested Access
```rust
let data = json!({
    "order": {
        "customer": {
            "name": "Alice"
        }
    }
});
let ast = parse("order.customer.name").unwrap();
// result: "Alice"
```

## Error Handling

```rust
match evaluator.evaluate(&ast, &data) {
    Ok(result) => println!("Result: {}", result),
    Err(e) => {
        match e {
            EvaluatorError::TypeError(msg) => println!("Type error: {}", msg),
            EvaluatorError::ReferenceError(msg) => println!("Reference error: {}", msg),
            EvaluatorError::EvaluationError(msg) => println!("Evaluation error: {}", msg),
        }
    }
}
```

## Testing

### Run All Tests
```bash
cargo test
```

### Run Specific Test
```bash
cargo test test_arithmetic_operations
```

### Run Demo
```bash
cargo run --example evaluator_demo
```

### Run Integration Tests
```bash
cargo test --test integration_test
```

## Common Errors and Solutions

### 1. Undefined Variable
**Error**: `ReferenceError: Undefined variable: $x`
**Solution**: Bind the variable before evaluation:
```rust
evaluator.context.bind("x".to_string(), json!(value));
```

### 2. Type Mismatch
**Error**: `TypeError: Cannot add "string" and number`
**Solution**: Ensure operations use compatible types:
```rust
// Wrong: "hello" + 5
// Right: "hello" & "5" or use string() function
```

### 3. Division by Zero
**Error**: `EvaluationError: Division by zero`
**Solution**: Check denominator before division:
```rust
"value != 0 ? numerator / value : 0"
```

### 4. Missing Field
**Behavior**: Returns `null` (not an error)
**Solution**: Use conditional to provide default:
```rust
"field ? field : 'default value'"
```

## Performance Tips

1. **Reuse Evaluator**: Create once, use many times
2. **Bind Variables Once**: Set up context before repeated evaluations
3. **Compile Once**: Parse expression once, evaluate multiple times
4. **Avoid Deep Nesting**: Flatten data structures when possible

## Complete Example

```rust
use jsonatapy::{parser::parse, evaluator::Evaluator};
use serde_json::json;

fn main() {
    // Data
    let data = json!({
        "order": {
            "items": [
                {"name": "Widget", "price": 10, "qty": 2},
                {"name": "Gadget", "price": 20, "qty": 1}
            ],
            "customer": {
                "name": "Alice",
                "type": "premium"
            }
        },
        "tax_rate": 0.1
    });

    // Examples
    let examples = vec![
        ("order.customer.name", "Get customer name"),
        ("order.customer.type = 'premium'", "Check premium status"),
    ];

    for (expr, description) in examples {
        println!("\n{}: {}", description, expr);
        match parse(expr) {
            Ok(ast) => {
                let mut evaluator = Evaluator::new();
                match evaluator.evaluate(&ast, &data) {
                    Ok(result) => println!("  Result: {}", result),
                    Err(e) => println!("  Error: {}", e),
                }
            }
            Err(e) => println!("  Parse error: {}", e),
        }
    }
}
```

## Further Reading

- **Full Documentation**: `EVALUATOR_IMPLEMENTATION.md`
- **Implementation Details**: `IMPLEMENTATION_SUMMARY.md`
- **Demo Code**: `examples/evaluator_demo.rs`
- **Integration Tests**: `tests/integration_test.rs`

## Support

For issues or questions:
1. Check test files for examples
2. Review documentation in `EVALUATOR_IMPLEMENTATION.md`
3. Run demo: `cargo run --example evaluator_demo`
4. Check error messages (they're descriptive!)

---

**Status**: ✅ Complete and Production-Ready
**Version**: 1.0
**Last Updated**: 2026-01-17
