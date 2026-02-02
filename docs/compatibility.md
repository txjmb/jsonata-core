# JSONata Compatibility

jsonatapy aims for 100% compatibility with the [JSONata 2.1.0 specification](https://docs.jsonata.org/).

## Test Suite Overview

The jsonatapy project includes a comprehensive test adapter that runs the complete reference JSONata test suite from the official JavaScript implementation. This ensures full spec compliance and compatibility.

### Reference Test Suite

- **Total Test Cases**: 1,273+
- **Test Groups**: 102
- **Datasets**: 28 shared input data files
- **Source**: Official jsonata-js repository (v2.1.0)

## Current Compatibility Status

**Note:** Run the full test suite to generate current statistics:

```bash
pytest tests/python/test_reference_suite.py -v
```

Results will be written to `test-suite-report.json` with detailed statistics by test group.

## Test Groups

The reference suite is organized into 102 test groups covering all aspects of JSONata:

### Core Functionality
- `literals` - Literal values (numbers, strings, booleans, null)
- `fields` - Field access and navigation
- `context` - Context variable ($)
- `variables` - Variable bindings
- `wildcards` - Wildcard selectors

### Operators
- `comparison-operators` - Equality, inequality, less than, greater than
- `numeric-operators` - Addition, subtraction, multiplication, division, modulo
- `boolean-expressions` - AND, OR, NOT operations
- `string-concat` - String concatenation
- `conditionals` - Ternary operator and if-then-else

### Arrays
- `array-constructor` - Array construction syntax
- `simple-array-selectors` - Basic array indexing and slicing
- `multiple-array-selectors` - Complex array selections
- `predicates` - Array filtering with predicates
- `transforms` - Array transformation operations

### Functions (55+ function groups)

**String Functions:**
- `function-string` - $string()
- `function-substring` - $substring()
- `function-uppercase` - $uppercase()
- `function-lowercase` - $lowercase()
- `function-trim` - $trim()
- `function-length` - $length()
- `function-split` - $split()
- `function-join` - $join()
- And many more...

**Numeric Functions:**
- `function-number` - $number()
- `function-abs` - $abs()
- `function-floor` - $floor()
- `function-ceil` - $ceil()
- `function-round` - $round()
- `function-sqrt` - $sqrt()
- `function-power` - $power()
- And more...

**Array Functions:**
- `function-count` - $count()
- `function-sum` - $sum()
- `function-max` - $max()
- `function-min` - $min()
- `function-average` - $average()
- `function-append` - $append()
- `function-reverse` - $reverse()
- `function-sort` - $sort()
- `function-distinct` - $distinct()
- And more...

**Object Functions:**
- `function-keys` - $keys()
- `function-lookup` - $lookup()
- `function-spread` - $spread()
- `function-merge` - $merge()
- `function-exists` - $exists()
- And more...

**Higher-Order Functions:**
- `function-map` - $map()
- `function-filter` - $filter()
- `function-reduce` - $reduce()
- `function-single` - $single()
- `function-sift` - $sift()

### Advanced Features
- `lambdas` - Lambda function syntax
- `closures` - Closure semantics
- `higher-order-functions` - Passing functions as arguments
- `partial-function-application` - Partial application
- `tail-recursion` - Tail-recursive functions
- `regex` - Regular expression support
- `encoding` - Character encoding functions

### Error Handling
- `errors` - Error conditions and messages
- `missing-paths` - Handling undefined paths
- `null` - Null value handling
- `parser-recovery` - Parser error recovery

## Running Compatibility Tests

### Run All Tests

```bash
pytest tests/python/test_reference_suite.py -v
```

### Run Specific Group

```bash
# Run only literal tests
pytest tests/python/test_reference_suite.py -v -k "literals"

# Run only string function tests
pytest tests/python/test_reference_suite.py -v -k "function-string"

# Run only lambda tests
pytest tests/python/test_reference_suite.py -v -k "lambdas"
```

### Run with Detailed Output

```bash
# Short traceback format
pytest tests/python/test_reference_suite.py -v --tb=short

# Show only first 10 failures
pytest tests/python/test_reference_suite.py -v --maxfail=10

# Show full diff for failures
pytest tests/python/test_reference_suite.py -v --tb=long
```

### Generate Compatibility Report

```bash
# Run tests and generate report
pytest tests/python/test_reference_suite.py -v --tb=short

# View report
cat test-suite-report.json | python -m json.tool
```

The report includes:
- Total tests, passed, failed, skipped
- Overall compatibility percentage
- Statistics by test group
- List of failed tests with details

## Known Limitations

This section will be updated as compatibility testing progresses. Current known limitations:

### Not Yet Implemented
- TBD after initial test run

### Partial Implementation
- TBD after initial test run

### Intentional Differences
- None currently planned

## Improving Compatibility

If you find a compatibility issue:

1. **Run the specific test group** to isolate the problem:
   ```bash
   pytest tests/python/test_reference_suite.py -v -k "group_name"
   ```

2. **Check the test case** in `tests/jsonata-suite/test/test-suite/groups/group_name/`

3. **File an issue** on GitHub with:
   - Test group and case number
   - Expression that fails
   - Expected vs actual result
   - Error message (if any)

4. **Submit a PR** with the fix:
   - Update Rust implementation
   - Verify test passes
   - Run full suite to check for regressions

## Compatibility Goals

### Version 0.1.0
- **Target**: 70-80% compatibility
- **Focus**: Core functionality, basic functions
- **Priority**: Literals, operators, path expressions, common functions

### Version 0.2.0
- **Target**: 90-95% compatibility
- **Focus**: Advanced functions, error handling
- **Priority**: All built-in functions, proper error codes

### Version 1.0.0
- **Target**: 100% compatibility
- **Focus**: Edge cases, full spec compliance
- **Priority**: All features, all edge cases, perfect error messages

## Test Suite Structure

The reference test suite uses JSON-based test specifications:

```json
{
  "expr": "JSONata expression",
  "dataset": "dataset0",
  "bindings": {"var": "value"},
  "result": <expected result>
}
```

Each test can specify:
- `result`: Expected successful result
- `undefinedResult`: Result should be undefined
- `code`: Expected error code (e.g., "T2001")
- `error`: Expected error object
- `timelimit`: Timeout in milliseconds
- `depth`: Maximum recursion depth

## Resources

- **Reference Implementation**: https://github.com/jsonata-js/jsonata
- **JSONata Specification**: https://docs.jsonata.org/
- **Test Suite Source**: `tests/jsonata-suite/test/test-suite/`
- **JSONata Exerciser**: https://try.jsonata.org/ (for testing expressions)

---

**Last Updated**: 2026-01-24
**JSONata Version**: 2.1.0
**Test Suite Commit**: ff36f0bd0f1aa4307662ffcc9f68abbba2f20915
