# JSONata Compatibility

jsonatapy aims for 100% compatibility with the [JSONata 2.1.0 specification](https://docs.jsonata.org/).

## Test Suite Overview

jsonatapy includes a comprehensive test adapter that runs the complete reference JSONata test suite from the official JavaScript implementation.

### Reference Test Suite

- **Total Tests**: 1258
- **Passing**: 1258 (100%)
- **Source**: Official jsonata-js repository (v2.1.0) at `tests/jsonata-js/`

## Current Compatibility Status

```bash
# Run full test suite
uv run pytest tests/python/test_reference_suite.py
```

Results show 100% compatibility with the JSONata 2.1.0 specification.

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

### Functions

**String Functions:**
- `function-string` - $string()
- `function-substring` - $substring()
- `function-uppercase` - $uppercase()
- `function-lowercase` - $lowercase()
- `function-trim` - $trim()
- `function-length` - $length()
- `function-split` - $split()
- `function-join` - $join()
- And more...

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
uv run pytest tests/python/test_reference_suite.py -v
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

## Improving Compatibility

If you find a compatibility issue:

1. **Run the specific test group** to isolate the problem:
   ```bash
   pytest tests/python/test_reference_suite.py -v -k "group_name"
   ```

2. **Check the test case** in `tests/jsonata-js/test/test-suite/groups/group_name/`

3. **File an issue** on GitHub with:
   - Test group and case number
   - Expression that fails
   - Expected vs actual result
   - Error message (if any)

4. **Submit a PR** with the fix:
   - Update Rust implementation
   - Verify test passes
   - Run full suite to check for regressions

## Resources

- **Reference Implementation**: https://github.com/jsonata-js/jsonata
- **JSONata Specification**: https://docs.jsonata.org/
- **Test Suite Source**: `tests/jsonata-js/test/test-suite/`
- **JSONata Exerciser**: https://try.jsonata.org/ (for testing expressions)
