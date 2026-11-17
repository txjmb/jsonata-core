# JSONata Compatibility

jsonatapy aims for 100% compatibility with the [JSONata specification](https://docs.jsonata.org/).

## Test Suite Overview

jsonatapy includes a comprehensive test adapter that runs the complete reference JSONata test suite from the official JavaScript implementation.

### Reference Test Suite

- **Total Tests**: 1686
- **Passing**: 1686 (100%)
- **Known divergences**: none in the suite (see [Known differences outside the reference suite](#known-differences-outside-the-reference-suite) for closure shapes it does not cover). `KNOWN_DIVERGENCES` in `tests/python/test_reference_suite.py` is the registry; entries there are strict `xfail`, so one that starts passing fails the suite until it is removed
- **Source**: Official jsonata-js repository (v2.2.2) at `tests/jsonata-js/`

### Known differences outside the reference suite

These shapes appear in neither the reference suite nor the differential corpora, and are kept
on purpose. Each is pinned in `tests/lambda_closure_suite.rs::divergent_from_reference`.

A function captures the variables it uses when it is defined, by value. jsonata-js closures
hold their defining frame by reference, so they see a variable rebound after they were made:

| Expression | jsonatapy | jsonata-js |
|---|---|---|
| `$x := 1; $g := function(){$x}; $x := 2; $g()` | `1` | `2` |
| `$esc := ( $f := function(){$g()}; $g := function(){"m"}; $f ); $esc()` | T1006 | `"m"` |
| `$f := A; $g := function(){$f()}; $f := B; $g()` | `"A"` | `"B"` |
| `$f := A; $g := function(){$f()}; $f := 42; $g()` | `"A"` | error (calls 42) |
| a recursive `$f` aliased, then `$f` rebound, then the alias called | recurses into itself | recurses into the rebound `$f` |

Live frames would make every named recursive function an `Rc` reference cycle, and so a memory
leak in a long-lived Python process. See
[#159](https://github.com/txjmb/jsonata-core/issues/159) for the designs considered.

Some path and evaluation-order shapes differ too:

| Expression | jsonatapy | jsonata-js | Why |
|---|---|---|---|
| `x[]#$i#$j.$i` | `[0,1]` | undefined | `#` after `[]` adds an index stage rather than replacing the step's index |
| `acct.o[0]#$i[].id` | `[1]` | `1` | a `[]` after `#` keeps a singleton array |
| `[$string(1,2,3), "f"@$v()]` | T0410 | T1006 | constructor members run in order, so the first member's error is reported; jsonata-js runs them concurrently |
| `a ~> $map(function($v){$v.b})[1]` | `"y"` | `["x","y"]` | a filter on the call after `~>` is applied; jsonata-js ignores it ([#286](https://github.com/txjmb/jsonata-core/issues/286)) |
| `nope[**[$error("E")]]` | undefined | D3137 | a `**` on a missing value runs no filters; jsonata-js runs them on nothing ([#290](https://github.com/txjmb/jsonata-core/issues/290)) |
| `(%.b)[%.x]{"k":1}.k` | undefined | `1` | a block whose path starts with a `%` nothing can bind groups as tuples ([#285](https://github.com/txjmb/jsonata-core/issues/285)) |

(Data for these rows: `{"x":[1,2],"acct":{"o":[{"id":1}]},"a":[{"b":"x"},{"b":"y"}]}`.)

## Current Compatibility Status

```bash
# Run full test suite
uv run pytest tests/python/test_reference_suite.py
```

Results show 1686/1686 (100%) of the suite passing against the reference JSONata implementation.

Two of those 1686 pass only because *something* was raised, not because the error code was
compared — our message for them carries no code for the suite to check. `UNVERIFIED_ERROR_CEILING`
in `tests/python/test_reference_suite.py` pins that count at 2 so it cannot quietly grow.

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
