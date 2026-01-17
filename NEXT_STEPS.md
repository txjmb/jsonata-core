# Next Steps - Quick Start Guide

This document provides immediate next steps to continue the jsonatapy implementation.

## 🚀 Immediate Actions (Do This First)

### 1. Verify Rust Installation
```bash
# Check if Rust is installed
rustc --version
cargo --version

# If not installed (Windows):
# Download from: https://rustup.rs/
# Or use: winget install Rustlang.Rustup
```

### 2. Install Development Tools
```bash
# Install maturin (Rust-Python bridge)
pip install maturin

# Install development dependencies
pip install pytest pytest-cov black ruff mypy
```

### 3. Test Current Build
```bash
# Try to build (will show what needs to be fixed)
cargo check

# This will likely fail because parser needs completion
# That's expected - shows what to work on next
```

## 📋 Critical Path - What to Implement Next

### Priority 1: Complete the Parser (MUST DO FIRST)

The parser skeleton exists but needs completion. Here's the order:

####  Step 1.1: Fix the Lexer
**File**: `src/parser.rs`
**Current Issue**: The `peek()` method doesn't return value

**Fix**:
```rust
fn peek(&self, offset: usize) -> Option<char> {
    self.input.get(self.position + offset).copied()  // Add `.copied()`
}
```

#### Step 1.2: Complete Tokenization
**Add these token types to lexer**:
- Comments (`/* */`)
- Backtick names (`` `name` ``)
- All multi-character operators (`:=`, `~>`, `!=`, `<=`, `>=`, `..`, `**`)
- Proper number parsing (decimals, exponentials, negatives)
- Unicode escape sequences in strings

**Reference**: Check `tests/jsonata-suite/src/parser.js` lines 65-200 for complete tokenizer

#### Step 1.3: Implement Full Parser
**Reference the JS parser** at `tests/jsonata-suite/src/parser.js`

Key features to add:
```rust
// Path expressions: foo.bar.baz
// Array predicates: foo[price > 100]
// Wildcards: foo.*
// Descendants: foo..bar
// Array constructors: [1, 2, 3]
// Object constructors: {"key": "value"}
// Conditional: condition ? then : else
```

**Test as you go**:
```bash
cargo test parser
```

### Priority 2: Wire Up the Evaluator

**File**: `src/evaluator.rs`

#### Step 2.1: Implement Binary Operations
```rust
AstNode::Binary { op, lhs, rhs } => {
    let left_val = self.evaluate(lhs, data)?;
    let right_val = self.evaluate(rhs, data)?;

    match op {
        BinaryOp::Add => // implement addition
        BinaryOp::Subtract => // implement subtraction
        // ... etc
    }
}
```

#### Step 2.2: Implement Path Traversal
This is CRITICAL for JSONata - navigating JSON structures:

```rust
AstNode::Path { steps } => {
    let mut current = data.clone();
    for step in steps {
        current = self.traverse_step(&current, step)?;
    }
    Ok(current)
}
```

**Example**: For expression `foo.bar`, this navigates from `data["foo"]` to `data["foo"]["bar"]`

#### Step 2.3: Implement Function Calls
```rust
AstNode::Function { name, args } => {
    // Evaluate arguments
    let arg_values: Vec<Value> = args.iter()
        .map(|arg| self.evaluate(arg, data))
        .collect()?;

    // Call built-in function
    call_builtin_function(name, arg_values)
}
```

### Priority 3: Implement Key Built-in Functions

Start with these essential functions:

**File**: `src/functions.rs`

```rust
// String functions (EASY - start here)
pub fn uppercase(s: &str) -> Result<Value, FunctionError> {
    Ok(Value::String(s.to_uppercase()))
}

pub fn lowercase(s: &str) -> Result<Value, FunctionError> {
    Ok(Value::String(s.to_lowercase()))
}

pub fn length(s: &str) -> Result<Value, FunctionError> {
    Ok(Value::Number(s.chars().count() as f64)) // Unicode-aware
}

// Numeric functions
pub fn sum(arr: &[Value]) -> Result<Value, FunctionError> {
    let total: f64 = arr.iter()
        .filter_map(|v| v.as_f64())
        .sum();
    Ok(Value::Number(total))
}

// Array functions
pub fn count(arr: &[Value]) -> Result<Value, FunctionError> {
    Ok(Value::Number(arr.len() as f64))
}
```

### Priority 4: Connect Python Bindings

**File**: `src/lib.rs`

Wire everything together:

```rust
use crate::parser::parse;
use crate::evaluator::Evaluator;

#[pyfunction]
fn compile(expression: &str) -> PyResult<JsonataExpression> {
    // Parse expression
    let ast = parse(expression)
        .map_err(|e| PyValueError::new_err(format!("Parse error: {}", e)))?;

    Ok(JsonataExpression {
        expression: expression.to_string(),
        ast: Some(ast), // Store AST
    })
}

#[pymethods]
impl JsonataExpression {
    fn evaluate(&self, data: PyObject, bindings: Option<PyObject>) -> PyResult<PyObject> {
        // Convert PyObject → serde_json::Value
        let json_data = python_to_json(data)?;

        // Evaluate AST
        let mut evaluator = Evaluator::new();
        let result = evaluator.evaluate(&self.ast, &json_data)
            .map_err(|e| PyValueError::new_err(format!("Eval error: {}", e)))?;

        // Convert serde_json::Value → PyObject
        json_to_python(result)
    }
}
```

### Priority 5: Test End-to-End

**Create simple Python test**:

```python
# tests/python/test_simple.py
import jsonatapy

def test_simple_literal():
    result = jsonatapy.evaluate("42", {})
    assert result == 42

def test_simple_string():
    result = jsonatapy.evaluate('"hello"', {})
    assert result == "hello"

def test_simple_path():
    data = {"name": "Alice"}
    result = jsonatapy.evaluate("name", data)
    assert result == "Alice"

def test_simple_function():
    result = jsonatapy.evaluate('$uppercase("hello")', {})
    assert result == "HELLO"
```

**Run tests**:
```bash
# Build extension
maturin develop

# Run Python tests
pytest tests/python/test_simple.py -v
```

## 🎯 Success Milestones

### Milestone 1: "Hello World" (Week 1)
- [ ] Parser compiles without errors
- [ ] Can parse simple literals: `42`, `"hello"`, `true`, `null`
- [ ] Can evaluate literals
- [ ] Python binding works for literals
- [ ] Test: `jsonatapy.evaluate("42", {})` returns `42`

### Milestone 2: "Basic Operations" (Week 2)
- [ ] Can parse arithmetic: `1 + 2`, `3 * 4`
- [ ] Can evaluate arithmetic
- [ ] Test: `jsonatapy.evaluate("1 + 2", {})` returns `3`

### Milestone 3: "JSON Navigation" (Week 3)
- [ ] Can parse paths: `foo.bar`
- [ ] Can evaluate paths
- [ ] Test: `jsonatapy.evaluate("name", {"name": "Alice"})` returns `"Alice"`

### Milestone 4: "Functions" (Week 4)
- [ ] Can call built-in functions
- [ ] Implement 5 string functions
- [ ] Implement 5 numeric functions
- [ ] Test: `jsonatapy.evaluate('$uppercase("hello")', {})` returns `"HELLO"`

## 🔍 Debugging Tips

### Parser Not Working?
```bash
# Add debug output
cargo test parser -- --nocapture

# Test specific expressions
cargo test test_parse_literals
```

### Evaluator Not Working?
```bash
# Test with simple AST nodes directly
cargo test evaluator
```

### Python Binding Not Working?
```bash
# Check if extension loaded
python -c "import jsonatapy; print(jsonatapy.__version__)"

# Rebuild extension
maturin develop --release
```

## 📚 Learning Resources

### Understanding Pratt Parsing
- Original Paper: https://tdop.github.io/
- Rust Example: https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html

### JSONata Language Reference
- Tutorial: https://docs.jsonata.org/simple
- Try it: https://try.jsonata.org/
- Reference: tests/jsonata-suite/src/parser.js

### PyO3 (Rust-Python)
- Guide: https://pyo3.rs/
- Examples: https://github.com/PyO3/pyo3/tree/main/examples

## ❓ Common Questions

**Q: Where should I start?**
A: Fix the parser lexer first (the `peek()` method), then implement full tokenization.

**Q: How do I know if my parser works?**
A: Run `cargo test parser`. Start with simple cases like `"42"`, then `"1 + 2"`, then `"foo.bar"`.

**Q: The reference JS code is confusing. What do I do?**
A: Start simple. Implement one feature at a time. Test after each feature. Don't try to match JS exactly - focus on behavior.

**Q: How long will this take?**
A: For one developer working full-time:
- Milestone 1: 3-5 days
- Milestone 2: 3-5 days
- Milestone 3: 5-7 days
- Milestone 4: 7-10 days
- **Total: 3-4 weeks to basic functionality**

**Q: Can I skip parts of the parser?**
A: Not really. The parser is the foundation. Once it works, everything else is easier.

## 🆘 Need Help?

1. Check CLAUDE.MD for architectural guidance
2. Check IMPLEMENTATION_STATUS.md for detailed TODO lists
3. Look at tests/jsonata-suite/src/parser.js for reference
4. Open an issue on GitHub
5. The test cases in tests/jsonata-suite/test/ show expected behavior

## ✅ Quick Checklist

Day 1:
- [ ] Verify Rust installed
- [ ] Run `cargo check`
- [ ] Fix `peek()` method
- [ ] Run `cargo test`

Day 2-3:
- [ ] Complete lexer tokenization
- [ ] Add tests for lexer
- [ ] Verify all tokens recognized

Day 4-5:
- [ ] Implement Pratt parser
- [ ] Parse simple expressions
- [ ] Test parsing

Week 2:
- [ ] Implement evaluator for literals
- [ ] Implement evaluator for binary ops
- [ ] Test evaluation

Week 3:
- [ ] Implement path traversal
- [ ] Wire up Python bindings
- [ ] Test end-to-end

Week 4:
- [ ] Implement built-in functions
- [ ] Comprehensive testing
- [ ] Documentation

---

**Ready to start? Begin with fixing `src/parser.rs` - that's your critical path!**
