## Description

<!-- Provide a clear and concise description of your changes -->

## Type of Change

<!-- Check all that apply -->

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Performance improvement
- [ ] Code refactoring (no functional changes)
- [ ] Documentation update
- [ ] Test coverage improvement
- [ ] CI/CD improvement
- [ ] Dependency update

## Related Issues

<!-- Link to related issues using keywords like "Fixes #123" or "Relates to #456" -->

Fixes #
Relates to #

## Changes Made

<!-- Detailed list of changes -->

-
-
-

## Testing

<!-- Describe the tests you ran and how to reproduce them -->

### Test Environment

- Python version:
- Operating System:
- jsonatapy version:

### Test Checklist

- [ ] All existing tests pass (`cargo test` and `pytest`)
- [ ] New tests added for bug fixes/features
- [ ] Reference test suite still passes (1258/1258 tests)
- [ ] Tested manually with provided examples
- [ ] Edge cases considered and tested
- [ ] Performance impact assessed (if applicable)

### Test Cases

<!-- Provide specific test cases or examples -->

```python
# Example test case
import jsonatapy

expr = jsonatapy.compile("...")
result = expr.evaluate({...})
assert result == expected
```

## Documentation

- [ ] Code is self-documenting with clear variable names
- [ ] Added/updated docstrings for public APIs
- [ ] Added/updated inline comments for complex logic
- [ ] Updated README.md (if needed)
- [ ] Updated CHANGELOG.md
- [ ] Added/updated examples in docs/ (if applicable)
- [ ] Rustdoc comments added for Rust code
- [ ] Python type hints added/updated

## Compatibility

<!-- Important for maintaining 100% JSONata compatibility -->

- [ ] Behavior matches JavaScript JSONata reference implementation
- [ ] Tested against JSONata playground (https://try.jsonata.org/)
- [ ] No breaking changes to public API
- [ ] Backward compatible with previous versions

### JavaScript Reference Verification

<!-- If this changes JSONata behavior, verify against JS implementation -->

**JavaScript (jsonata-js) behavior:**
```javascript
// Test case with expected result
```

**jsonatapy behavior:**
```python
# Matching test case and result
```

## Breaking Changes

<!-- If this is a breaking change, describe the impact and migration path -->

- [ ] This PR introduces breaking changes

**Breaking changes:**
-

**Migration guide:**
-

## Performance Impact

<!-- Describe any performance implications -->

- [ ] Performance benchmarks run
- [ ] No significant performance regression
- [ ] Performance improvement measured and documented

**Benchmark results:**
```
# Before:
...

# After:
...
```

## Code Quality

- [ ] Code follows project style guidelines
- [ ] `cargo fmt` applied
- [ ] `cargo clippy -- -D warnings` passes with no warnings
- [ ] Python code formatted with `black`
- [ ] No new compiler warnings
- [ ] Removed debug/print statements

## Additional Notes

<!-- Any additional information that reviewers should know -->

## Reviewer Guidelines

<!-- Help reviewers focus on what matters most -->

**Focus areas for review:**
-
-

**Questions for reviewers:**
-
-

## Checklist

- [ ] I have read the [CONTRIBUTING.md](../CONTRIBUTING.md) guidelines
- [ ] My code follows the project's style guidelines
- [ ] I have performed a self-review of my own code
- [ ] I have commented my code, particularly in hard-to-understand areas
- [ ] I have made corresponding changes to the documentation
- [ ] My changes generate no new warnings
- [ ] I have added tests that prove my fix is effective or that my feature works
- [ ] New and existing unit tests pass locally with my changes
- [ ] Any dependent changes have been merged and published

---

<!--
Thank you for contributing to jsonatapy!

Your effort helps make JSONata available to the Python community with
high performance and 100% compatibility.
-->
