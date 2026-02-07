# JSONata Parser Implementation

This document describes the complete implementation of the JSONata parser in Rust.

## File Location

`C:\Users\mboha\source\repos\jsonatapy\src\parser.rs` (1,242 lines)

## Quick Start

```rust
use jsonatapy::parser::parse;

// Parse an expression
let ast = parse("orders[price > 100].product")?;

// The AST can then be passed to the evaluator
```

## Features

### Lexer

The lexer converts JSONata expressions into tokens:

```rust
let mut lexer = Lexer::new("1 + 2".to_string());
let token1 = lexer.next_token()?; // Token::Number(1.0)
let token2 = lexer.next_token()?; // Token::Plus
let token3 = lexer.next_token()?; // Token::Number(2.0)
```

**Supported tokens:**
- Literals: numbers, strings, booleans, null
- Identifiers: `foo`, `bar_baz`, `` `with spaces` ``
- Variables: `$var`
- Operators: `+`, `-`, `*`, `/`, `%`, `=`, `!=`, `<`, `<=`, `>`, `>=`, `and`, `or`, `in`, `&`, `.`, `..`, `:=`
- Delimiters: `()`, `[]`, `{}`, `,`, `;`, `:`, `?`
- Comments: `/* ... */`

### Parser

The parser uses **Pratt parsing** (Top-Down Operator Precedence) to build an AST.

**Operator precedence:**
1. Function call `()`, array access `[]` - 80
2. Dot operator `.` - 75
3. Multiplication `*`, division `/`, modulo `%` - 60
4. Addition `+`, subtraction `-`, concatenation `&` - 50
5. Comparison `=`, `!=`, `<`, `>`, etc. - 40
6. Logical AND `and` - 30
7. Logical OR `or` - 25
8. Range `..`, ternary `?` - 20
9. Assignment `:=` - 10 (right-associative)

## Testing

Run the comprehensive test suite:

```bash
cargo test --lib parser
```

The test suite includes 35+ tests covering all features.

## See Also

- Full documentation: `PARSER_IMPLEMENTATION.md`
- Examples: `examples/parser_demo.rs`
- Verification script: `verify_parser.bat` or `verify_parser.sh`
