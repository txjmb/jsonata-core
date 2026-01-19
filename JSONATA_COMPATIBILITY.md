# JSONata 2.1.0 Compatibility

This document tracks jsonatapy's compatibility with the JSONata 2.1.0 specification.

## Higher-Order Functions

| Function | Status | Notes |
|----------|--------|-------|
|  | ✅ Implemented | Supports both lambda functions and simple expressions |
|  | ✅ Implemented | Supports both lambda functions and simple expressions |
|  | ✅ Implemented | Supports lambda functions with accumulator and value parameters |
|  | ❌ Not implemented | Returns exactly one matching value or throws error |
|  | ❌ Not implemented | Filters object key/value pairs |

## Lambda Functions

| Feature | Status | Notes |
|---------|--------|-------|
| Lambda syntax  | ✅ Implemented | Full support for lambda syntax |
| Parameter binding | ✅ Implemented | Parameters correctly bound in lambda scope |
| Field access in lambdas  | ✅ Implemented | Variable path resolution works correctly |
| Multiple parameters | ✅ Implemented | E.g.,  |
| Nested lambdas | ⚠️ Untested | Should work but needs testing |

## String Functions

| Function | Status |
|----------|--------|
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |

## Numeric Functions

| Function | Status |
|----------|--------|
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |

## Array Functions

| Function | Status |
|----------|--------|
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |

## Object Functions

| Function | Status |
|----------|--------|
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |
|  | ✅ Implemented |

## Core Language Features

| Feature | Status | Notes |
|---------|--------|-------|
| Path expressions | ✅ Implemented | E.g.,  |
| Array indexing | ✅ Implemented | E.g.,  |
| Array mapping | ✅ Implemented | E.g.,  |
| Array filtering | ✅ Implemented | E.g.,  |
| Object construction | ✅ Implemented | E.g.,  |
| Conditional expressions | ✅ Implemented | E.g.,  |
| Binary operators | ✅ Implemented | Arithmetic, comparison, logical, string concatenation |
| Variable references | ✅ Implemented | E.g.,  |
| Function calls | ✅ Implemented | E.g.,  |
| $ root context | ✅ Implemented |  alone refers to root data |

## Test Results

### Higher-Order Functions Tests (test_lambda.py)
All 8 tests pass:
- ✅  with lambda function
- ✅  with simple expression
- ✅  with lambda function
- ✅  with simple expression
- ✅  with lambda function (sum)
- ✅  with lambda function (product)
- ✅ Chaining  and  with lambdas
- ✅  with lambda accessing object fields

## Known Limitations

1. **Missing Higher-Order Functions**:  and  are not yet implemented
2. **Date/Time Functions**: No date/time functions implemented yet
3. **Regular Expression Support**: Not tested/verified
4. **Async Functions**: Not implemented (requires different architecture)
5. **Parent Operator**:  parent operator not tested
6. **Wildcards**:  and  wildcards not fully tested

## Next Steps

To achieve full JSONata 2.1.0 compatibility:

1. Implement  - returns exactly one match or throws error
2. Implement  - filters object key/value pairs
3. Add comprehensive test suite from jsonata-js repository
4. Implement date/time functions (, , , etc.)
5. Test and verify regular expression support
6. Test nested lambda functions and complex scenarios
7. Verify parent operator  functionality
8. Test wildcard operators  and 

## Version Tracking

- **jsonatapy version**: 0.1.0
- **Target JSONata version**: 2.1.0
- **Status**: Core features implemented, higher-order functions working, missing some advanced features
