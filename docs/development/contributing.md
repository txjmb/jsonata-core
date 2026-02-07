# Contributing to jsonatapy

Thank you for your interest in contributing to jsonatapy!

## Table of Contents

- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Code Style](#code-style)
- [Testing Requirements](#testing-requirements)
- [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)

## Getting Started

### Ways to Contribute

- **Bug Reports**: Report issues with clear reproduction steps
- **Feature Requests**: Suggest new features or improvements
- **Code Contributions**: Submit bug fixes or new features
- **Documentation**: Improve docs, add examples, fix typos
- **Testing**: Add test cases, improve test coverage
- **Performance**: Profile and optimize hot paths

### Before You Start

1. Check existing issues and PRs to avoid duplicates
2. For major changes, open an issue first to discuss
3. Read [CLAUDE.md](../../CLAUDE.md) for architecture overview
4. Review the [building guide](building.md) for development setup

## Development Setup

### Prerequisites

- **Rust** (latest stable, 1.70+)
- **Python** (3.8+)
- **maturin** (Python-Rust build tool)

### Quick Setup

```bash
# Clone repository
git clone https://github.com/yourorg/jsonatapy.git
cd jsonatapy

# Create virtual environment
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# or .venv\Scripts\activate  # Windows

# Install development tools
pip install maturin pytest pytest-cov black ruff mypy

# Install Rust tools
cargo install cargo-watch

# Build in development mode
maturin develop --release

# Verify installation
python -c "import jsonatapy; print(jsonatapy.__version__)"
```

See [Building Guide](building.md) for detailed setup instructions.

## Code Style

### Rust Code Style

**Required Standards:**
- Follow Rust 2021 edition conventions
- Pass `cargo fmt` (formatting)
- Pass `cargo clippy -- -D warnings` (zero warnings)
- Document all public APIs with rustdoc (`///`)

**Format Code:**
```bash
cargo fmt
```

**Run Linter:**
```bash
cargo clippy -- -D warnings
```

**Example:**
```rust
/// Calculate the sum of an array of numbers.
///
/// # Arguments
/// * `values` - Array of numeric values
///
/// # Returns
/// Sum as f64, or error if input is invalid
pub fn sum_values(values: &[JValue]) -> Result<f64, EvalError> {
    let mut sum = 0.0;
    for value in values {
        match value {
            JValue::Number(n) => sum += n,
            _ => return Err(EvalError::TypeError("Expected number".into())),
        }
    }
    Ok(sum)
}
```

### Python Code Style

**Required Standards:**
- PEP 8 compliance (enforced by black and ruff)
- Type hints for all public APIs (PEP 484)
- Docstrings following NumPy/Sphinx style

**Format Code:**
```bash
black python/ tests/
```

**Run Linter:**
```bash
ruff check python/ tests/
```

**Type Check:**
```bash
mypy python/
```

**Example:**
```python
def evaluate_expression(
    expression: str,
    data: Any,
    bindings: Optional[Dict[str, Any]] = None
) -> Any:
    """
    Evaluate a JSONata expression against data.

    Parameters
    ----------
    expression : str
        JSONata expression string
    data : Any
        Input data (typically dict or list)
    bindings : Optional[Dict[str, Any]]
        Variable bindings (default: None)

    Returns
    -------
    Any
        Result of evaluation

    Raises
    ------
    ValueError
        If expression syntax is invalid or evaluation fails

    Examples
    --------
    >>> evaluate_expression("name", {"name": "Alice"})
    "Alice"
    """
    return _evaluate(expression, data, bindings)
```

### Naming Conventions

**Rust:**
- Functions/variables: `snake_case`
- Types/traits: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Modules: `snake_case`

**Python:**
- Functions/variables: `snake_case`
- Classes: `PascalCase`
- Constants: `SCREAMING_SNAKE_CASE`
- Private members: `_leading_underscore`

### Code Organization

**Mirror JavaScript Reference:**
- Module structure should map 1:1 with JavaScript source
- Core algorithms should follow same logical flow
- Keep synchronization with upstream as priority

**Example Structure:**
```
src/
├── lib.rs          # Python bindings (PyO3)
├── parser.rs       # Mirrors parser.js
├── evaluator.rs    # Mirrors jsonata.js
├── functions.rs    # Mirrors functions.js
├── datetime.rs     # Mirrors datetime.js
├── signature.rs    # Mirrors signature.js
└── value.rs        # JValue type system
```

## Testing Requirements

### Test Coverage

**Required:** All contributions must include tests.

**Coverage Target:** 100% (matching upstream jsonata-js)

### Test Types

**1. Rust Unit Tests**
```rust
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
        assert_eq!(sum_values(&values).unwrap(), 6.0);
    }

    #[test]
    fn test_sum_invalid_type() {
        let values = vec![JValue::String(Rc::from("not a number"))];
        assert!(sum_values(&values).is_err());
    }
}
```

**2. Python Integration Tests**
```python
import jsonatapy
import pytest

def test_sum_function():
    """Test $sum function."""
    result = jsonatapy.evaluate("$sum([1, 2, 3])", {})
    assert result == 6

def test_sum_invalid_input():
    """Test $sum with invalid input."""
    with pytest.raises(ValueError, match="must be an array"):
        jsonatapy.evaluate("$sum('not an array')", {})
```

**3. Reference Test Suite**

All contributions must pass the reference test suite:

```bash
# Run full reference suite (1258 tests)
uv run pytest tests/python/test_reference_suite.py
```

### Running Tests

```bash
# Rust unit tests (31 tests)
cargo test

# Python integration tests
pytest tests/python/ -v

# Reference suite (1258 tests)
uv run pytest tests/python/test_reference_suite.py

# With coverage
pytest --cov=jsonatapy --cov-report=html tests/python/
cargo tarpaulin --out Html

# Specific test
pytest tests/python/test_functions.py::test_sum -v
```

### Test Guidelines

- **Test edge cases**: Empty arrays, null values, undefined, etc.
- **Test error conditions**: Invalid inputs, type mismatches
- **Test performance**: Add benchmarks for critical paths
- **Add descriptive names**: `test_sum_empty_array` not `test_1`
- **Document behavior**: Add docstrings explaining what's tested

## Documentation

### Documentation Requirements

All contributions must include documentation:

1. **Code Documentation**
   - Rustdoc comments for public Rust APIs
   - Python docstrings for public Python APIs

2. **User Documentation**
   - Update relevant docs in `docs/`
   - Add examples for new features
   - Update API reference if needed

3. **Changelog**
   - Add entry to `CHANGELOG.md`
   - Follow [Keep a Changelog](https://keepachangelog.com/) format

### Documentation Style

**Concise and Clear:**
```markdown
## Function Name

Brief description in one sentence.

### Parameters
- `param1` (type): Description

### Returns
- Description of return value

### Example
\`\`\`python
result = function(param1)
\`\`\`
```

### Building Documentation

```bash
# Rust docs
cargo doc --open

# Python docs (if using Sphinx)
cd docs/
make html
```

## Pull Request Process

### Before Submitting

1. **Run all checks:**
   ```bash
   # Format
   cargo fmt
   black python/ tests/

   # Lint
   cargo clippy -- -D warnings
   ruff check python/ tests/

   # Test
   cargo test
   pytest tests/python/ -v
   ```

2. **Update documentation:**
   - Add/update docstrings
   - Update `docs/` if needed
   - Add entry to `CHANGELOG.md`

3. **Create descriptive commit messages:**
   ```
   feat: Add $newFunction for array manipulation

   - Implement $newFunction following jsonata-js behavior
   - Add unit tests and reference suite compatibility
   - Update documentation and examples

   Fixes #123
   ```

### Commit Message Format

Follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation changes
- `test:` Test additions or fixes
- `refactor:` Code refactoring
- `perf:` Performance improvements
- `chore:` Maintenance tasks

**Examples:**
```
feat: Implement $filter function with lambda support
fix: Correct null handling in path expressions
docs: Add migration guide from jsonata-python
test: Add edge cases for $sum function
refactor: Optimize predicate evaluation
perf: Use Rc for zero-copy string handling
```

### PR Checklist

- [ ] Code follows style guidelines (cargo fmt, black)
- [ ] All tests pass (cargo test, pytest)
- [ ] No clippy warnings (cargo clippy -- -D warnings)
- [ ] Reference test suite passes (1258/1258)
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
- [ ] Commit messages follow format
- [ ] PR description explains changes
- [ ] Related issue referenced

### PR Template

```markdown
## Description
Brief description of changes

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Documentation update
- [ ] Performance improvement
- [ ] Refactoring

## Related Issue
Fixes #(issue number)

## Testing
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Reference suite still passes (1258/1258)
- [ ] Manual testing performed

## Checklist
- [ ] Code follows style guidelines
- [ ] All tests pass
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
```

## Issue Guidelines

### Reporting Bugs

**Good Bug Report:**
```markdown
## Description
Brief description of the bug

## Steps to Reproduce
1. Install jsonatapy
2. Run: `jsonatapy.evaluate("expression", data)`
3. Observe error

## Expected Behavior
What should happen

## Actual Behavior
What actually happens

## Environment
- jsonatapy version: 0.1.0
- Python version: 3.11
- OS: Ubuntu 22.04

## Additional Context
- Error messages
- Stack traces
- Sample data (if applicable)
```

### Feature Requests

**Good Feature Request:**
```markdown
## Feature Description
Clear description of proposed feature

## Use Case
Why is this feature needed?

## Proposed Solution
How should it work?

## Alternatives Considered
Other approaches you've thought about

## Additional Context
Links to related features, examples from other libraries
```

### Questions

For usage questions:
1. Check [documentation](../README.md)
2. Search existing issues
3. Ask in discussions (if enabled)

## Development Workflow

### Typical Workflow

1. **Fork and clone:**
   ```bash
   git clone https://github.com/your-username/jsonatapy.git
   cd jsonatapy
   ```

2. **Create branch:**
   ```bash
   git checkout -b feat/my-feature
   ```

3. **Make changes:**
   - Write code
   - Add tests
   - Update docs

4. **Test locally:**
   ```bash
   cargo fmt && cargo clippy -- -D warnings && cargo test
   black python/ tests/ && ruff check python/ tests/
   pytest tests/python/ -v
   ```

5. **Commit:**
   ```bash
   git add .
   git commit -m "feat: Add new feature"
   ```

6. **Push and create PR:**
   ```bash
   git push origin feat/my-feature
   # Create PR on GitHub
   ```

### Continuous Integration

All PRs run through CI:
- **Format check** (cargo fmt, black)
- **Lint** (clippy, ruff)
- **Tests** (cargo test, pytest)
- **Reference suite** (1258 tests)
- **Multi-platform** (Linux, macOS, Windows)
- **Multi-version** (Python 3.8-3.12)

## Code Review Process

### What to Expect

1. **Initial Review**: Maintainers review within 3-5 days
2. **Feedback**: Address review comments
3. **Approval**: At least one maintainer approval required
4. **Merge**: Maintainer merges approved PR

### Review Criteria

- ✅ Code quality and style
- ✅ Test coverage
- ✅ Documentation completeness
- ✅ Performance implications
- ✅ Compatibility with upstream jsonata-js
- ✅ Breaking changes (require discussion)

## Getting Help

- **Documentation**: Check [docs/](../README.md)
- **Architecture**: Read [CLAUDE.md](../../CLAUDE.md)
- **Issues**: Search existing issues
- **Questions**: Open a discussion or issue

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

## Acknowledgments

Contributors are credited in:
- `CONTRIBUTORS.md` (if applicable)
- Release notes
- Git history

Thank you for contributing to jsonatapy!
