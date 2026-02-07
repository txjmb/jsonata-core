# jsonatapy Test Suite

This directory contains the complete test suite for jsonatapy, including integration with the reference JSONata test suite.

## Test Structure

### Manual Tests (14 files)
- `test_basic.py` - Basic API and compilation tests
- `test_integration.py` - Comprehensive feature tests (480 lines)
- `test_lambda.py` - Lambda function tests (118 lines)
- `test_higher_order.py` - Higher-order function tests
- `test_object.py` - Object construction tests
- `test_filter_obj.py` - Filter + object tests
- `test_simple_mapping.py` - Array mapping tests
- `test_complex.py` - Complex expression tests
- `test_e2e.py` - End-to-end validation (85 lines)
- `test_perf.py` - Performance benchmarks
- `test_json_api.py` - JSON string API benchmarks
- `test_json_vs_js.py` - JS compatibility benchmarks
- `test_manual.py` - Manual debugging tests
- `test_trace.py` - Diagnostic tests

### Reference Test Suite (1,258+ test cases)
- `test_reference_suite.py` - Adapter for JSONata reference tests
- `conftest.py` - Pytest configuration and reporting
- Tests load from `../jsonata-js/test/test-suite/`

## Running Tests

### Prerequisites

1. **Build the extension**:
   ```bash
   # Install Rust if needed
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # Build extension
   pip install maturin
   maturin develop --release
   ```

2. **Install test dependencies**:
   ```bash
   pip install pytest pytest-cov
   ```

3. **Ensure git submodule is loaded**:
   ```bash
   git submodule update --init --recursive
   ```

### Run All Manual Tests

```bash
pytest tests/python/ -v -m "not reference"
```

### Run Reference Suite Tests

```bash
# Run all reference tests (1,258 tests - takes time!)
pytest tests/python/test_reference_suite.py -v

# Run with progress bar instead of verbose
pytest tests/python/test_reference_suite.py -o console_output_style=progress

# Stop after first 10 failures
pytest tests/python/test_reference_suite.py --maxfail=10
```

### Run Specific Test Groups

```bash
# Run only literal tests
pytest tests/python/test_reference_suite.py -v -k "literals"

# Run only string function tests
pytest tests/python/test_reference_suite.py -v -k "function-string"

# Run only lambda tests
pytest tests/python/test_reference_suite.py -v -k "lambdas"

# Run only array tests
pytest tests/python/test_reference_suite.py -v -k "array-constructor"
```

### View Compatibility Report

After running reference suite tests, a report is generated:

```bash
# View JSON report
cat test-suite-report.json | python -m json.tool

# Or just view the summary (printed at end of test run)
pytest tests/python/test_reference_suite.py --tb=short
```

The report includes:
- Total tests, passed, failed
- Compatibility percentage
- Results by test group
- Failed test details

## Test Output Examples

### Successful Test Run

```
tests/python/test_reference_suite.py::test_reference_suite[literals/case000] PASSED
tests/python/test_reference_suite.py::test_reference_suite[literals/case001] PASSED
...

======================================================================
JSONata Reference Suite Compatibility Report
======================================================================
Total Tests:  1258
Passed:       945 (75.1%)
Failed:       313 (24.9%)

Compatibility: 75.1%

Results by Group:
Group                                    Pass   Fail   Skip  Total      %
----------------------------------------------------------------------
literals                                  45      0      0     45  100.0% ✓
fields                                    38      2      0     40   95.0% ✗
...
```

### Compatibility Report (test-suite-report.json)

```json
{
  "total": 1258,
  "passed": 945,
  "failed": 313,
  "compatibility_pct": 75.1,
  "by_group": {
    "literals": {"passed": 45, "failed": 0, "skipped": 0},
    "fields": {"passed": 38, "failed": 2, "skipped": 0},
    ...
  },
  "failed_tests": [
    {
      "test_id": "fields/case015",
      "group": "fields",
      "expr": "foo.bar.baz",
      "error": "..."
    }
  ]
}
```

## Pytest Markers

Tests are organized with markers:

- `@pytest.mark.reference` - Reference suite tests
- `@pytest.mark.group(name)` - Tests from specific group
- `@pytest.mark.slow` - Slow-running tests
- `@pytest.mark.compatibility` - JS compatibility tests

### Using Markers

```bash
# Run only reference tests
pytest tests/python/ -v -m reference

# Skip slow tests
pytest tests/python/ -v -m "not slow"

# Run specific group
pytest tests/python/ -v -m "group and literals"
```

## Debugging Failed Tests

### 1. Run specific failing test

```bash
pytest tests/python/test_reference_suite.py::test_reference_suite[group/case000] -vv
```

### 2. Check test specification

Look at the JSON file:
```bash
cat tests/jsonata-js/test/test-suite/groups/group/case000.json
```

### 3. Test expression in Python

```python
import jsonatapy
import json

# Load dataset
with open('tests/jsonata-js/test/test-suite/datasets/dataset0.json') as f:
    data = json.load(f)

# Test expression
expr = jsonatapy.compile("your.expression.here")
result = expr.evaluate(data)
print(result)
```

### 4. Compare with JavaScript reference

```bash
cd tests/jsonata-js
npm install
node -e "
const jsonata = require('jsonata');
const data = require('./test/test-suite/datasets/dataset0.json');
const result = jsonata('your.expression.here').evaluate(data);
console.log(JSON.stringify(result, null, 2));
"
```

## Test Coverage

```bash
# Run with coverage
pytest tests/python/ -v --cov=jsonatapy --cov-report=html

# View coverage report
open htmlcov/index.html  # or start htmlcov/index.html on Windows
```

## Continuous Integration

Tests run automatically on GitHub Actions:
- All manual tests run on every commit
- Reference suite tests run on every commit
- Compatibility report generated and uploaded as artifact
- CI fails if compatibility drops below 70%

See `.github/workflows/test.yml` for configuration.

## Performance Testing

```bash
# Run performance benchmarks
pytest tests/python/test_perf.py -v

# Run JS comparison benchmarks
pytest tests/python/test_json_vs_js.py -v

# Run JSON API benchmarks
pytest tests/python/test_json_api.py -v
```

## Test Development

### Adding Manual Tests

1. Create test file: `test_feature.py`
2. Write pytest functions: `def test_something():`
3. Run: `pytest tests/python/test_feature.py -v`

### Testing Against Reference Suite

The reference suite is automatically loaded and run. To update:

```bash
# Update submodule to latest
cd tests/jsonata-js
git pull origin master
cd ../..
git add tests/jsonata-js
git commit -m "Update reference test suite"
```

## Troubleshooting

### No tests collected

**Problem**: `collected 0 items`

**Solution**:
```bash
# Check submodule is loaded
git submodule status
git submodule update --init --recursive

# Verify test files exist
ls tests/jsonata-js/test/test-suite/groups/
```

### Import error

**Problem**: `ModuleNotFoundError: No module named 'jsonatapy'`

**Solution**:
```bash
# Build extension
maturin develop --release

# Verify installation
python -c "import jsonatapy; print(jsonatapy.__version__)"
```

### Tests timing out

**Problem**: Tests take too long

**Solution**:
```bash
# Run subset
pytest tests/python/test_reference_suite.py -k "literals" -v

# Use progress bar
pytest tests/python/test_reference_suite.py -o console_output_style=progress

# Stop early
pytest tests/python/test_reference_suite.py --maxfail=10
```

## Resources

- [JSONata Documentation](https://docs.jsonata.org/)
- [Reference Implementation](https://github.com/jsonata-js/jsonata)
- [Compatibility Report](../../docs/compatibility.md)
- [pytest Documentation](https://docs.pytest.org/)
