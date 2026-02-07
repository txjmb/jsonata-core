# Testing Guide

Comprehensive guide to testing in jsonatapy.

## Table of Contents

- [Test Structure](#test-structure)
- [Running Tests](#running-tests)
- [Rust Unit Tests](#rust-unit-tests)
- [Python Integration Tests](#python-integration-tests)
- [Reference Test Suite](#reference-test-suite)
- [Adding New Tests](#adding-new-tests)
- [Coverage Requirements](#coverage-requirements)
- [Test Best Practices](#test-best-practices)

## Test Structure

jsonatapy has three layers of testing:

```
tests/
├── python/                      # Python integration tests
│   ├── test_reference_suite.py  # Reference suite runner (1258 tests)
│   ├── test_basic.py            # Basic functionality
│   ├── test_functions.py        # Built-in functions
│   ├── test_lambda.py           # Lambda functions
│   ├── test_performance.py      # Performance benchmarks
│   └── ...
├── jsonata-js/                  # Reference test suite (git submodule)
│   └── test/test-suite/         # Official JSONata test cases
└── (Rust unit tests in src/*.rs with #[cfg(test)])
```

### Test Layers

1. **Rust Unit Tests** (31 tests)
   - Low-level component testing
   - Pure Rust, no Python
   - Fast execution
   - Located in `src/*.rs` modules

2. **Python Integration Tests** (~50 tests)
   - End-to-end Python API testing
   - Python bindings verification
   - Error handling validation
   - Located in `tests/python/`

3. **Reference Test Suite** (1258 tests)
   - Official JSONata compatibility tests
   - Comprehensive language feature coverage
   - Ensures 100% compatibility with jsonata-js
   - Imported from jsonata-js repository

## Running Tests

### All Tests

```bash
# Rust unit tests (fast)
cargo test

# Python integration tests
pytest tests/python/ -v

# Reference test suite (comprehensive)
uv run pytest tests/python/test_reference_suite.py

# Run everything
cargo test && pytest tests/python/ -v
```

### Specific Tests

```bash
# Single Rust test
cargo test test_evaluate_path

# Single Python test file
pytest tests/python/test_functions.py -v

# Single Python test
pytest tests/python/test_functions.py::test_sum_function -v

# Tests matching pattern
pytest tests/python/ -k "lambda" -v
```

### Test Output

```bash
# Show print statements
cargo test -- --nocapture
pytest tests/python/ -v -s

# Show detailed output
cargo test -- --show-output
pytest tests/python/ -vv

# Stop on first failure
pytest tests/python/ -x
```

### Performance

```bash
# Run tests serially (for timing)
cargo test -- --test-threads=1

# Run Python tests in parallel
pytest tests/python/ -n auto

# Skip slow tests
pytest tests/python/ -m "not slow"
```

## Rust Unit Tests

### Location

Rust unit tests are in the same file as the code, using `#[cfg(test)]`:

```rust
// src/functions.rs

pub fn sum(values: &[JValue]) -> Result<JValue, EvalError> {
    // Implementation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sum_values() {
        let values = vec![
            JValue::Number(1.0),
            JValue::Number(2.0),
            JValue::Number(3.0),
        ];
        let result = sum(&values).unwrap();
        assert_eq!(result, JValue::Number(6.0));
    }
}
```

### Running Rust Tests

```bash
# All Rust tests
cargo test

# Specific module
cargo test evaluator::tests

# Specific test
cargo test test_evaluate_path

# With output
cargo test -- --nocapture

# Release mode (faster but slower to compile)
cargo test --release
```

### Rust Test Examples

**Test successful operation:**
```rust
#[test]
fn test_filter_array() {
    let arr = JValue::array(vec![
        JValue::Number(1.0),
        JValue::Number(2.0),
        JValue::Number(3.0),
    ]);
    let predicate = |v: &JValue| matches!(v, JValue::Number(n) if *n > 1.0);
    let result = filter_array(&arr, predicate).unwrap();
    assert_eq!(result.len(), 2);
}
```

**Test error conditions:**
```rust
#[test]
fn test_sum_invalid_type() {
    let values = vec![JValue::String(Rc::from("not a number"))];
    let result = sum(&values);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), EvalError::TypeError(_)));
}
```

**Test edge cases:**
```rust
#[test]
fn test_sum_empty_array() {
    let values = vec![];
    let result = sum(&values).unwrap();
    assert_eq!(result, JValue::Undefined);
}

#[test]
fn test_sum_with_null() {
    let values = vec![JValue::Number(1.0), JValue::Null, JValue::Number(2.0)];
    let result = sum(&values).unwrap();
    assert_eq!(result, JValue::Number(3.0));
}
```

## Python Integration Tests

### Location

Python tests are in `tests/python/`:

```python
# tests/python/test_functions.py

import jsonatapy
import pytest

def test_sum_function():
    """Test $sum function with array of numbers."""
    result = jsonatapy.evaluate("$sum([1, 2, 3])", {})
    assert result == 6

def test_sum_invalid_input():
    """Test $sum with invalid input raises error."""
    with pytest.raises(ValueError, match="must be an array"):
        jsonatapy.evaluate("$sum('not an array')", {})
```

### Running Python Tests

```bash
# All Python tests
pytest tests/python/ -v

# Specific file
pytest tests/python/test_functions.py -v

# Specific test
pytest tests/python/test_functions.py::test_sum_function -v

# Pattern matching
pytest tests/python/ -k "sum or count" -v

# Show print statements
pytest tests/python/ -v -s
```

### Python Test Examples

**Basic functionality:**
```python
def test_path_expression():
    """Test simple path expression."""
    data = {"name": "Alice", "age": 30}
    result = jsonatapy.evaluate("name", data)
    assert result == "Alice"
```

**Error handling:**
```python
def test_parse_error():
    """Test invalid expression raises ValueError."""
    with pytest.raises(ValueError, match="Unexpected token"):
        jsonatapy.compile("invalid [[")
```

**Parameterized tests:**
```python
@pytest.mark.parametrize("expression,data,expected", [
    ("$sum([1, 2, 3])", {}, 6),
    ("$sum([1.5, 2.5])", {}, 4.0),
    ("$sum([10])", {}, 10),
])
def test_sum_cases(expression, data, expected):
    """Test $sum with various inputs."""
    result = jsonatapy.evaluate(expression, data)
    assert result == expected
```

**Fixtures:**
```python
@pytest.fixture
def sample_data():
    """Sample data for testing."""
    return {
        "orders": [
            {"product": "Widget", "price": 10, "quantity": 2},
            {"product": "Gadget", "price": 25, "quantity": 1},
        ]
    }

def test_with_fixture(sample_data):
    """Test using fixture data."""
    result = jsonatapy.evaluate("$sum(orders.(price * quantity))", sample_data)
    assert result == 45
```

## Reference Test Suite

### Overview

The reference test suite contains **1258 tests** from the official jsonata-js repository. These tests ensure 100% compatibility with the JSONata specification.

### Location

Tests are in the `tests/jsonata-js` git submodule:

```bash
# Initialize submodule
git submodule update --init --recursive

# Update submodule
git submodule update --remote tests/jsonata-js
```

### Running Reference Tests

```bash
# All reference tests (1258 tests)
uv run pytest tests/python/test_reference_suite.py

# Verbose output
uv run pytest tests/python/test_reference_suite.py -v

# Stop on first failure
uv run pytest tests/python/test_reference_suite.py -x

# Show which tests pass/fail
uv run pytest tests/python/test_reference_suite.py -v --tb=short
```

### Test Format

Reference tests are JSON files:

```json
{
  "description": "Test description",
  "expression": "items[price > 100]",
  "data": {
    "items": [
      {"name": "Widget", "price": 150},
      {"name": "Gadget", "price": 50}
    ]
  },
  "result": [
    {"name": "Widget", "price": 150}
  ]
}
```

### Test Categories

Reference tests cover:

- Path expressions
- Predicates and filtering
- Array operations
- Object construction
- Built-in functions (40+)
- Lambda functions
- Higher-order functions
- String operations
- Numeric operations
- Boolean logic
- Aggregations
- Edge cases

## Adding New Tests

### When to Add Tests

- **New Features**: All new features require tests
- **Bug Fixes**: Regression tests for fixed bugs
- **Edge Cases**: Unusual inputs or corner cases
- **Performance**: Benchmarks for optimized code

### Adding Rust Unit Tests

```rust
// src/functions.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_function() {
        // Arrange
        let input = JValue::array(vec![JValue::Number(1.0)]);

        // Act
        let result = new_function(&input).unwrap();

        // Assert
        assert_eq!(result, JValue::Number(1.0));
    }

    #[test]
    fn test_new_function_error() {
        let input = JValue::String(Rc::from("invalid"));
        let result = new_function(&input);
        assert!(result.is_err());
    }
}
```

### Adding Python Tests

```python
# tests/python/test_new_feature.py

import jsonatapy
import pytest

class TestNewFeature:
    """Tests for new feature."""

    def test_basic_usage(self):
        """Test basic usage of new feature."""
        result = jsonatapy.evaluate("$newFunc([1, 2, 3])", {})
        assert result == expected_value

    def test_edge_case(self):
        """Test edge case handling."""
        result = jsonatapy.evaluate("$newFunc([])", {})
        assert result is None

    def test_error_handling(self):
        """Test error is raised for invalid input."""
        with pytest.raises(ValueError, match="expected"):
            jsonatapy.evaluate("$newFunc('invalid')", {})

    @pytest.mark.parametrize("input,expected", [
        ([1, 2, 3], 6),
        ([10], 10),
        ([], None),
    ])
    def test_multiple_cases(self, input, expected):
        """Test multiple input cases."""
        result = jsonatapy.evaluate(f"$newFunc({input})", {})
        assert result == expected
```

### Test Naming

Use descriptive names:

**Good:**
```python
test_sum_empty_array()
test_sum_with_null_values()
test_sum_invalid_type_raises_error()
```

**Bad:**
```python
test_1()
test_sum()
test_error()
```

## Coverage Requirements

### Target Coverage

**Required:** 100% coverage (matching upstream jsonata-js)

### Measuring Coverage

**Rust:**
```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html
# Opens in browser: target/tarpaulin-report.html
```

**Python:**
```bash
# Run tests with coverage
pytest tests/python/ --cov=jsonatapy --cov-report=html

# View report
open htmlcov/index.html
```

### Coverage Reports

```bash
# Combined coverage report
cargo tarpaulin --out Html
pytest tests/python/ --cov=jsonatapy --cov-report=html

# CI coverage
cargo tarpaulin --out Xml
pytest tests/python/ --cov=jsonatapy --cov-report=xml
```

## Test Best Practices

### 1. Test One Thing at a Time

```python
# ✅ Good - focused test
def test_sum_returns_total():
    result = jsonatapy.evaluate("$sum([1, 2, 3])", {})
    assert result == 6

# ❌ Bad - tests multiple things
def test_sum_and_count():
    sum_result = jsonatapy.evaluate("$sum([1, 2, 3])", {})
    count_result = jsonatapy.evaluate("$count([1, 2, 3])", {})
    assert sum_result == 6
    assert count_result == 3
```

### 2. Use Descriptive Assertions

```python
# ✅ Good - clear assertion message
assert result == expected, f"Expected {expected}, got {result}"

# ✅ Good - pytest provides good messages
assert result == expected

# ❌ Bad - no context on failure
assert result
```

### 3. Test Edge Cases

```python
def test_sum_function():
    # Normal case
    assert jsonatapy.evaluate("$sum([1, 2, 3])", {}) == 6

    # Edge cases
    assert jsonatapy.evaluate("$sum([])", {}) is None
    assert jsonatapy.evaluate("$sum([1])", {}) == 1
    assert jsonatapy.evaluate("$sum([1, null, 2])", {}) == 3
```

### 4. Test Error Conditions

```python
def test_error_handling():
    # Test specific error message
    with pytest.raises(ValueError, match="must be an array"):
        jsonatapy.evaluate("$sum('not an array')", {})

    # Test error type
    with pytest.raises(ValueError):
        jsonatapy.compile("invalid [[")
```

### 5. Use Fixtures for Shared Data

```python
@pytest.fixture
def sample_orders():
    return {
        "orders": [
            {"product": "A", "price": 10},
            {"product": "B", "price": 20},
        ]
    }

def test_with_fixture(sample_orders):
    result = jsonatapy.evaluate("$sum(orders.price)", sample_orders)
    assert result == 30
```

### 6. Group Related Tests

```python
class TestSumFunction:
    """Tests for $sum function."""

    def test_sum_numbers(self):
        pass

    def test_sum_empty_array(self):
        pass

    def test_sum_with_nulls(self):
        pass
```

### 7. Document Test Purpose

```python
def test_sum_ignores_null_values():
    """
    Test that $sum correctly ignores null values in array.

    According to JSONata spec, null values should be skipped
    during summation.
    """
    result = jsonatapy.evaluate("$sum([1, null, 2, null, 3])", {})
    assert result == 6
```

## Continuous Integration

### CI Pipeline

All tests run on CI for:
- **Platforms**: Linux, macOS, Windows
- **Python versions**: 3.8, 3.9, 3.10, 3.11, 3.12
- **Architectures**: x86_64, aarch64

### CI Commands

```yaml
# .github/workflows/test.yml
- name: Run Rust tests
  run: cargo test

- name: Run Python tests
  run: |
    maturin develop --release
    pytest tests/python/ -v

- name: Run reference suite
  run: uv run pytest tests/python/test_reference_suite.py
```

## Test Maintenance

### Regular Tasks

1. **Keep reference suite updated:**
   ```bash
   git submodule update --remote tests/jsonata-js
   ```

2. **Review failing tests:**
   ```bash
   pytest tests/python/ --lf  # Run last failed
   ```

3. **Update test data:**
   - Review test coverage reports
   - Add tests for uncovered code
   - Remove obsolete tests

### Test Quality Checklist

- [ ] Tests pass locally
- [ ] Tests are deterministic (no random failures)
- [ ] Tests are fast (< 1s each for unit tests)
- [ ] Tests are independent (no shared state)
- [ ] Tests have clear names
- [ ] Tests have docstrings
- [ ] Edge cases covered
- [ ] Error cases covered
- [ ] Coverage maintained at 100%

## Next Steps

- [Learn about contributing](contributing.md)
- [Review building guide](building.md)
- [Understand architecture](architecture.md)
- [Check API reference](../api.md)
