"""
Triage registry for the jsonata-js checks that live outside the JSON reference suite.

`test_reference_suite.py` runs jsonata-js's `test/test-suite/groups` cases. jsonata-js also has
checks written as JavaScript, which no adapter can run: `implementation-tests.js`,
`parser-recovery.js`, `async-function.js` and `parser-pluggable-regex.js`. Each `it()` in those
files gets one entry in `REGISTRY` below, saying what we did with it:

- `Ported("tests/python/test_x.py::test_name")` -- the behavior is checked by that test here.
- `NotApplicable("reason")` -- the check tests something this implementation does not have
  (a JavaScript runtime detail, a Promise API, parser recovery mode).
- `TODO` / `TODO_NEW` -- not triaged yet. `TODO_NEW` marks the checks added in jsonata-js 2.3.0,
  which are triaged first.

The registry has to name exactly the checks in the pinned submodule, so moving the submodule to
a release that adds, removes or renames a check fails `test_registry_matches_reference_files`
until the registry is updated. Keys are `<file> > <describe> > ... > <it>`; a title that repeats
under the same path gets ` #2`, ` #3` after its first occurrence.

`UNTRIAGED_CEILING` pins how many entries are still TODO, in the same style as
`UNVERIFIED_ERROR_CEILING` in `test_reference_suite.py`: it fails if the count grows and fails
asking to be lowered if it shrinks. The 2.3.0 release brings it to zero.
"""

from __future__ import annotations

import ast
import re
from dataclasses import dataclass
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parent.parent.parent
JS_TEST_DIR = ROOT / "tests/jsonata-js/test"
JS_FILES = (
    "implementation-tests.js",
    "parser-recovery.js",
    "async-function.js",
    "parser-pluggable-regex.js",
)


@dataclass(frozen=True)
class Ported:
    """Covered here by `test`, a `path::function` node id relative to the repo root."""

    test: str


@dataclass(frozen=True)
class NotApplicable:
    """Not portable, and why."""

    reason: str


@dataclass(frozen=True)
class Untriaged:
    """Not triaged yet. `new` marks checks added in jsonata-js 2.3.0."""

    new: bool


TODO = Untriaged(new=False)
TODO_NEW = Untriaged(new=True)

Entry = Ported | NotApplicable | Untriaged

# Reasons shared by several entries.
RECOVERY = NotApplicable("parser recovery mode (`recover: true`, `errors()`) is not implemented")
PROMISE = NotApplicable("a Promise in the input: this implementation has no async API")
PROMISE_FUNCTIONS = NotApplicable(
    "a binding whose functions return objects holding more functions, behind Promises: "
    "this implementation has no async API, and a host function returns only JSON values"
)
HOST_FUNCTION_VALUES = NotApplicable(
    "a host function that takes or returns a JSONata function (a custom matcher, a callback, "
    "a closure): the Python host API passes only JSON values across the boundary"
)
GENERATOR = NotApplicable(
    "a JavaScript generator function as a host function, which jsonata-js runs to its first "
    "yield; a Python host function returns its value directly"
)
HOST_CLONE = NotApplicable(
    "a host-registered `clone`, which jsonata-js's transform calls to copy its input (for "
    "Node-RED): a transform here copies the input itself, and `$clone` is only the builtin"
)
INHERITED = NotApplicable(
    "the `inherited` option, which reads what a JavaScript object inherits (class getters and "
    "methods, URL objects): input here is JSON-like data with no prototype chain"
)
PROTOTYPE = NotApplicable(
    "a JavaScript prototype chain or getter in the input, or a host identity `clone`: input "
    "here is JSON-like data, copied before a transform changes it"
)
D1013 = NotApplicable(
    "jsonata-js reserves the keys `_jsonata_lambda` and `_jsonata_function` (D1013, 'a "
    "restriction of this implementation rather than JSONata itself') because its functions "
    "are objects carrying those flags; a function here is its own kind of value"
)
THREADS = NotApplicable(
    "concurrent evaluations of one expression awaiting async host functions: there is no "
    "async API, and a compiled expression belongs to the thread that compiled it"
)
REGEX_ENGINE = NotApplicable(
    "the `RegexEngine` option for a custom regex implementation is not implemented; regexes "
    "here run on the Rust regex crate, which matches in linear time, so ReDoS is not possible"
)

# fmt: off
REGISTRY: dict[str, Entry] = {
    # --- implementation-tests.js ---
    "implementation-tests.js > Functions with side-effects > Evaluator - function: millis > $millis() returns milliseconds since the epoch > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_millis_returns_milliseconds_since_the_epoch"),
    "implementation-tests.js > Functions with side-effects > Evaluator - function: millis > $millis() always returns same value within an expression > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_timestamp_is_the_same_within_an_expression"),
    "implementation-tests.js > Functions with side-effects > Evaluator - function: millis > $millis() returns different timestamp for subsequent evaluate() calls > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_timestamp_differs_between_evaluations"),
    "implementation-tests.js > Functions with side-effects > $now() returns timestamp > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_now_formats"),
    "implementation-tests.js > Functions with side-effects > $now() returns timestamp with defined format > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_now_formats"),
    "implementation-tests.js > Functions with side-effects > $now() returns timestamp with defined format and timezone > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_now_formats"),
    "implementation-tests.js > Functions with side-effects > $now() always returns same value within an expression > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_timestamp_is_the_same_within_an_expression"),
    "implementation-tests.js > Functions with side-effects > $now() returns different timestamp for subsequent evaluate() calls > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_timestamp_differs_between_evaluations"),
    "implementation-tests.js > Functions with side-effects > $millis() and $now() are not affected by an overlapping evaluate() call > should return the same value within each evaluation": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_overlapping_evaluations_keep_their_own_timestamp"),
    "implementation-tests.js > Functions with side-effects > $millis() returns milliseconds since the epoch > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_millis_returns_milliseconds_since_the_epoch"),
    "implementation-tests.js > Functions with side-effects > Evaluator - functions: random > random number\") > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_random"),
    "implementation-tests.js > Functions with side-effects > Evaluator - functions: random > consequetive random numbers should be different\") > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_random"),
    "implementation-tests.js > Tests that rely on JavaScript-style object traversal > foo.*[0] > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_object_traversal_order"),
    "implementation-tests.js > Tests that rely on JavaScript-style object traversal > **[2] > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_object_traversal_order"),
    "implementation-tests.js > Tests that use the $clone() function > clone undefined > should return undefined": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_clone"),
    "implementation-tests.js > Tests that use the $clone() function > clone empty object > should return empty object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_clone"),
    "implementation-tests.js > Tests that use the $clone() function > clone object > should return same object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_clone"),
    "implementation-tests.js > Tests that use the $clone() function > transform expression with overridden $clone function > should return result object": HOST_CLONE,
    "implementation-tests.js > Tests that use the $clone() function > transform expression with overridden $clone value > should use the built-in $clone and succeed (expression-level override is ignored for security)": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_ignores_a_clone_bound_in_the_expression"),
    "implementation-tests.js > Tests that use the $clone() function > transform expression with overridden $clone value > should throw T2013 if the host overrides $clone with a non-function": HOST_CLONE,
    "implementation-tests.js > Tests that bind Javascript functions > Override implementation of $now() > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_override_now"),
    "implementation-tests.js > Tests that bind Javascript functions > function throws a `DOMException` with a read-only `message` property > rethrows correctly": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_exception_message_is_kept"),
    "implementation-tests.js > Tests that bind Javascript functions > map a user-defined Javascript function with signature > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_as_a_callback"),
    "implementation-tests.js > Tests that bind Javascript functions > map a user-defined Javascript function with undefined signature > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_as_a_callback"),
    "implementation-tests.js > Tests that bind Javascript functions > map a user-defined Javascript function > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_as_a_callback"),
    "implementation-tests.js > Tests that bind Javascript functions > $filter with a user-defined Javascript function > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_as_a_callback"),
    "implementation-tests.js > Tests that bind Javascript functions > $sift with a user-defined Javascript function > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_as_a_callback"),
    "implementation-tests.js > Tests that bind Javascript functions > $each with a user-defined Javascript function > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_as_a_callback"),
    "implementation-tests.js > Tests that bind Javascript functions > Partially apply user-defined Javascript function > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_partially_apply_host_function"),
    "implementation-tests.js > Tests that bind Javascript functions > User defined matchers > should match using a custom matcher": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined matchers > should split using a custom matcher": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined matchers > should replace using a custom matcher": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined matchers > should test inclusion using a custom matcher": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined higher-order functions > should be able to invoke a built-in function passed as an argument": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined higher-order functions > should be able to invoke a lambda function passed as an argument": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined higher-order functions > should be able to invoke a user-defined function passed as an argument": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined higher-order functions > should be able to return a function from a user-defined function": HOST_FUNCTION_VALUES,
    "implementation-tests.js > Tests that bind Javascript functions > User defined generator function > should be able to invoke a generator function returning a simple value": GENERATOR,
    "implementation-tests.js > Tests that bind Javascript functions > User defined generator function > should be able to invoke a generator function and map over its return array value": GENERATOR,
    "implementation-tests.js > Tests that bind Javascript functions > User defined generator function > should be able to invoke a generator function and map over its return object value": GENERATOR,
    "implementation-tests.js > Tests that bind Javascript functions > User defined higher-order generator functions > a higher-order generator function will not work": GENERATOR,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /ab/ (\"ab\") > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /ab/ () > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /ab+/ (\"ababbabbcc\") > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /a(b+)/ (\"ababbabbcc\") > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /a(b+)/ (\"ababbabbcc\").next() > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /a(b+)/ (\"ababbabbcc\").next().next() > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /a(b+)/ (\"ababbabbcc\").next().next().next() > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > /a(b+)/i (\"Ababbabbcc\") > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_regex_called_as_a_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > empty regex > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_empty_regex"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > empty regex > should throw error #2": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_empty_regex"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > empty regex: Escaped termination > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_empty_regex"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > empty regex: Escaped termination > should throw error #2": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_empty_regex"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: syntax error inside braces > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: unclosed function call inside braces > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: syntax error inside nested template > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: syntax error in second interpolation > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: unterminated, starting mid-expression > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: illegal escape > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: number formatting matches & > should return result object": Ported("tests/python/test_jsonata_230_template_strings.py::test_number_formatting_matches_concat"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string with 1000 interpolations > should return the same as the & chain": Ported("tests/python/test_jsonata_230_template_strings.py::test_1000_interpolations_match_the_concat_chain"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string with 20000 interpolations > should not grow the stack with the number of parts": Ported("tests/python/test_jsonata_230_template_strings.py::test_20000_interpolations_do_not_nest"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: evaluation error reports the embedded expression, not an & operator > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_evaluation_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: evaluation error reports the embedded expression that failed > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_evaluation_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > template string: unescaped } in the text > should throw error": Ported("tests/python/test_jsonata_230_template_strings.py::test_syntax_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"test escape \\\\\", /\\\\/) > should find \\": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"ababbabbcc\",/ab/) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"ababbabbcc\",/a(b+)/) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"ababbabbcc\",/a(b+)/, 1) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"ababbabbcc\",/a(b+)/, 0) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(nothing,/a(xb+)/) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"ababbabbcc\",/a(xb+)/) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"a, b, c, d\", /ab/, -3) > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"a, b, c, d\", /ab/, null) > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"a, b, c, d\", /ab/, \"2\") > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"a, b, c, d\", \"ab\") > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(\"a, b, c, d\", true) > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(12345, 3) > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Functions - $match > $match(12345) > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_errors"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that attempt to access the object prototype > should ignore __proto__": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_names_are_ordinary_keys"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that attempt to access the object prototype > should throw an error trying to invoke toString()": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_methods_are_not_functions"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that attempt to pollute the object prototype > should ignore __proto__": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_names_are_ordinary_keys"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that attempt to pollute the object prototype > should throw an error with __lookupGetter__": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_methods_are_not_functions"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that attempt to pollute the object prototype > should ignore constructor": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_names_are_ordinary_keys"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that attempt to pollute the object prototype > should not allow code execution via $clone override and __proto__ pollution (issue #859)": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_methods_are_not_functions"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should not look up inherited properties by default": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should look up the properties of a URL": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should look up a getter and invoke a method of a class": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should look up inherited properties with a partially applied $lookup": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should only include an object's own properties when listing or serialising it": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should pass on an error thrown by a getter": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should still look up an object's own properties that have blocked names": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_names_are_ordinary_keys"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should not look up the prototype or constructor of an object": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should not be able to create a function from a string": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_methods_are_not_functions"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should ignore __proto__ in a transform": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_names_are_ordinary_keys"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should throw an error trying to change an inherited function in a transform": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Expressions that look up the properties that objects inherit, with the inherited option > should throw an error trying to change an object returned by a getter in a transform": INHERITED,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match the prototype of the object being transformed > should throw an error, rather than change the prototype": PROTOTYPE,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match the prototype of the object being transformed > should throw an error when the prototype is also a property of the object": PROTOTYPE,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change a built-in function": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_matches_only_the_copy"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change a lambda": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_matches_only_the_copy"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change a function provided by the host": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_cannot_match_a_host_function"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change an object provided by the host": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_matches_only_the_copy"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change the prototype of a class": PROTOTYPE,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change Object.prototype when the input has no prototype": PROTOTYPE,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should throw an error, rather than change the input through $$": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_matches_only_the_copy"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should not change anything if any of the matches aren't in the copy": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_matches_only_the_copy"),
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should not run any getters when checking the matches": PROTOTYPE,
    "implementation-tests.js > Tests that are specific to a Javascript runtime > Transforms that match values that aren't in the copy being transformed > should allow values in the copy that aren't objects to be matched": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_matches_values_in_the_copy_that_are_not_objects"),
    "implementation-tests.js > Test that yield platform specific results > $sqrt(10) * $sqrt(10) > should return result object": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_sqrt_rounding"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatInteger(12345678901234567000, '0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatInteger(1e21, '#,##0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatInteger(-1e21, '0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatInteger(123456789012345678901234, '0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatNumber(1e21, '#,##0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatNumber(-1e21, '#,##0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatNumber(1.5e21, '#,##0.00') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatNumber(123456789012345678901234, '0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatNumber(1e19, '0%') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers >= 1e21 > $formatNumber(1e20, '#,##0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_numbers_from_1e21"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber(1.1, '0.00000000000000000000') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber(0.1 + 0.2, '0.##################') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber(531135999999999.94, '0.00') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber(9007199254740994, '0.00') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber(12345678901234567000, '0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber(123456789012345678901234, '0.000') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber with more than 100 decimal places > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting numbers beyond the precision of a double > $formatNumber with 309 integer digits and an exponent > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_beyond_double_precision"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber(NaN, '£#,##0.00') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber(Infinity, '£#,##0.00') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber(-Infinity, '£#,##0.00') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber(-Infinity, '#,##0.00;(#,##0.00)') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber(Infinity, '0.0e0') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber(1e308, '0%') > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Formatting NaN and infinity > $formatNumber with the NaN and infinity properties set > should return result object": Ported("tests/python/test_jsonata_230_number_format.py::test_nan_and_infinity"),
    "implementation-tests.js > Sorting large arrays > $sort() of 100,000 numbers > should return the sorted array": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_sort_large_arrays"),
    "implementation-tests.js > Sorting large arrays > $sort() of 100,000 objects with a comparator function > should return the sorted array": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_sort_large_arrays"),
    "implementation-tests.js > Sorting large arrays > order-by of 100,000 objects > should return the sorted array": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_sort_large_arrays"),
    "implementation-tests.js > Tests that include infinite recursion > stack overflow - infinite recursive function - non-tail call > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > stack overflow - infinite recursive function - tail call > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > stack overflow - infinite recursive function - tail call (no stack guardrail) > should throw error": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > guardrails on Ackermann function > should complete for small parameters": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_ackermann_completes_for_small_parameters"),
    "implementation-tests.js > Tests that include infinite recursion > guardrails on Ackermann function > larger inputs cause stack overflow": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > guardrails on Ackermann function > larger inputs cause stack overflow #2": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > guardrails on sequence length > prevents large ranges": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > guardrails on sequence length > prevents large intermediate sequences": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that include infinite recursion > guardrails on sequence length > prevents appending large sequences": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_guardrails"),
    "implementation-tests.js > Tests that the descendant operator is consistent with the single wildcard for functions > should not include a lambda in the descendants": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_descendants_skip_functions"),
    "implementation-tests.js > Tests that the descendant operator is consistent with the single wildcard for functions > should not include a built-in function in the descendants": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_descendants_skip_functions"),
    "implementation-tests.js > Tests that the descendant operator is consistent with the single wildcard for functions > should still descend into the other values of an object that holds a function": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_descendants_skip_functions"),
    "implementation-tests.js > Tests that $each and $sift only read an object's own properties > $each should skip a key that a callback deletes, rather than read an inherited property": PROTOTYPE,
    "implementation-tests.js > Tests that $each and $sift only read an object's own properties > $sift should skip a key that a callback deletes, rather than read an inherited property": PROTOTYPE,
    "implementation-tests.js > Tests that interpreter internals are not reachable from an expression > a transform rejects a lambda as the update value": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_rejects_a_function_as_the_update"),
    "implementation-tests.js > Tests that interpreter internals are not reachable from an expression > a transform rejects a built-in function as the update value": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_transform_rejects_a_function_as_the_update"),
    "implementation-tests.js > Tests that a transform cannot reparent an object through a __proto__ key > sets __proto__ as an own data property without changing the prototype": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_prototype_names_are_ordinary_keys"),
    "implementation-tests.js > Tests invalid object creation > prevents creating an object mimicking a lambda": D1013,
    "implementation-tests.js > Tests invalid object creation > prevents creating an object mimicking a function": D1013,
    "implementation-tests.js > Tests that use internal frame push callbacks > frame push callback bound to expression > calls callback when new frame created": NotApplicable("an internal jsonata-js hook (`Symbol.for('jsonata.__createFrame_push')`) with no counterpart here"),
    "implementation-tests.js > Concurrency tests > invokes a single compiled expression multiple times concurrently": THREADS,
    # --- parser-recovery.js ---
    "parser-recovery.js > Invoke parser with valid expression > Account.Order[0] > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > Account. > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > Account[ > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > Account.Order[;0].Product > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > Account.Order[0;].Product > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > Account.Order[0].Product; > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > $inputSource[0].UnstructuredAnswers^()[0].Text > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > «a {x;} b» > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > «abc > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > «Hello {name} > should return ast": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > «a {x;} b > should record the error inside the braces without throwing": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > <<a {«b > should report the unterminated inner template and the missing }": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > «a {+} b» > should recover from an invalid expression inside a closed template": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > «s3://{bucket;}/x» > should tokenize the rest of the template as text, not as a regex": RECOVERY,
    "parser-recovery.js > Invoke parser with incomplete expression > An expression with syntax error should not be executable > Account. > should return ast": RECOVERY,
    # --- async-function.js ---
    "async-function.js > Invoke JSONata with callback > Make HTTP request > should return promise to results": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_result_is_queried"),
    "async-function.js > Invoke JSONata with callback - errors > type error > should throw": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_error_is_raised"),
    "async-function.js > Invoke JSONata with callback - errors > Make HTTP request with dodgy url > should throw": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_host_function_error_is_raised"),
    "async-function.js > Invoke JSONata with callback - return values > should handle an undefined value": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_missing_value_is_none"),
    "async-function.js > Invoke JSONata with callback - return values > should handle a null value": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_return_values"),
    "async-function.js > Invoke JSONata with callback - return values > should handle a value": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_return_values"),
    "async-function.js > Invoke JSONata with callback - return values > should handle a promise": PROMISE,
    "async-function.js > Evaluate concurrent expressions with callbacks > should process two expressions concurrently": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_one_expression_evaluated_twice"),
    "async-function.js > Handle chained functions that end in promises > basic async function": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function, but invokes another function": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function, but invokes another function several times": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function and part of a numeric expression": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function, but invokes another function several times and part of a numeric expression": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function, but invokes another function - nested": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function, then invokes a built-in function": PROMISE_FUNCTIONS,
    "async-function.js > Handle chained functions that end in promises > basic async function inside a template string": NotApplicable("a binding whose functions return Promises: this implementation has no async API"),
    "async-function.js > Handle chained functions that end in promises > registered async function inside a template string": Ported("tests/python/test_jsonata_230_template_strings.py::test_registered_function_inside_a_template"),
    "async-function.js > Handle chained functions that end in promises > basic async function, but invokes a non-existent function": PROMISE_FUNCTIONS,
    "async-function.js > Array with Promise items > should await each item": PROMISE,
    # --- parser-pluggable-regex.js ---
    "parser-pluggable-regex.js > Invoke parser with custom RegexEngine param > should call RegexEngine param constructure during evaluation": REGEX_ENGINE,
    "parser-pluggable-regex.js > Detect and repel ReDoS attack > should successfully process a safe regex": REGEX_ENGINE,
    "parser-pluggable-regex.js > Detect and repel ReDoS attack > should behave like the build-in regex processor (copied from regex test case)": Ported("tests/python/test_jsonata_js_implementation_tests.py::test_match_with_the_built_in_regex"),
    "parser-pluggable-regex.js > Detect and repel ReDoS attack > should reject a potential ReDoS attack": REGEX_ENGINE,
}
# fmt: on

UNTRIAGED_CEILING = 0


# A mocha `describe("title", ...)` or `it("title", ...)` call, at the start of a token.
_CALL = re.compile(r"(?<![\w$.])(describe|it)(?:\.(?:skip|only))?\s*\(\s*")


def _skip_string(src: str, i: int) -> int:
    """Index just past the JS string or template literal that opens at `src[i]`."""
    quote = src[i]
    j = i + 1
    while src[j] != quote:
        j += 2 if src[j] == "\\" else 1
    return j + 1


def _string_value(literal: str) -> str:
    if literal[0] == "`":
        return literal[1:-1]
    return str(ast.literal_eval(literal))


def extract(src: str) -> list[str]:
    """Each `it()` title in a mocha file, prefixed with its enclosing `describe()` titles.

    A small scanner, not a JS parser: it tracks brace depth, skipping strings, template
    literals and comments, which is all the jsonata-js test files need. Checked against
    mocha's own enumeration for jsonata-js 2.2.2 and 2.3.0.
    """
    found: list[str] = []
    stack: list[tuple[str, int]] = []  # (describe title, brace depth of its body)
    depth = 0
    i = 0
    while i < len(src):
        c = src[i]
        if c in "'\"`":
            i = _skip_string(src, i)
            continue
        if src.startswith("//", i):
            newline = src.find("\n", i)
            i = len(src) if newline == -1 else newline
            continue
        if src.startswith("/*", i):
            i = src.index("*/", i) + 2
            continue
        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            while stack and stack[-1][1] > depth:
                stack.pop()
        else:
            m = _CALL.match(src, i)
            if m and src[m.end()] in "'\"`":
                end = _skip_string(src, m.end())
                title = _string_value(src[m.end() : end])
                if m.group(1) == "it":
                    found.append(" > ".join([t for t, _ in stack] + [title]))
                else:
                    stack.append((title, depth + 1))
                i = end
                continue
        i += 1
    return found


def reference_check_keys() -> list[str]:
    """Every `it()` in the JS test files, keyed as `REGISTRY` is."""
    keys: list[str] = []
    for name in JS_FILES:
        src = (JS_TEST_DIR / name).read_text(encoding="utf-8")
        seen: dict[str, int] = {}
        for path in extract(src):
            seen[path] = seen.get(path, 0) + 1
            suffix = f" #{seen[path]}" if seen[path] > 1 else ""
            keys.append(f"{name} > {path}{suffix}")
    return keys


requires_submodule = pytest.mark.skipif(
    not JS_TEST_DIR.exists(), reason="tests/jsonata-js submodule not checked out"
)


@requires_submodule
def test_registry_matches_reference_files():
    """The registry names exactly the checks in the pinned jsonata-js, in file order."""
    actual = reference_check_keys()
    missing = [k for k in actual if k not in REGISTRY]
    stale = [k for k in REGISTRY if k not in set(actual)]
    assert not missing and not stale, (
        "REGISTRY is out of sync with tests/jsonata-js/test/*.js.\n"
        + "".join(f"  missing: {k}\n" for k in missing)
        + "".join(f"  stale:   {k}\n" for k in stale)
    )
    assert list(REGISTRY) == actual, "REGISTRY entries are not in file order"


def test_ported_entries_name_existing_tests():
    """A `Ported` entry must point at a test function that exists."""
    broken = []
    for key, entry in REGISTRY.items():
        if not isinstance(entry, Ported):
            continue
        file, _, func = entry.test.partition("::")
        path = ROOT / file
        func_name = func.split("::")[-1].split("[")[0]
        if not path.exists() or not re.search(
            rf"^\s*(?:async )?def {re.escape(func_name)}\(", path.read_text(encoding="utf-8"), re.M
        ):
            broken.append(f"  {key} -> {entry.test}")
    assert not broken, "Ported entries name tests that do not exist:\n" + "\n".join(broken)


def test_not_applicable_entries_give_a_reason():
    empty = [
        k for k, e in REGISTRY.items() if isinstance(e, NotApplicable) and not e.reason.strip()
    ]
    assert not empty, "NotApplicable entries without a reason:\n" + "\n".join(empty)


def test_untriaged_count_does_not_grow():
    """Pin how many reference checks are still untriaged."""
    untriaged = [k for k, e in REGISTRY.items() if isinstance(e, Untriaged)]
    assert len(untriaged) <= UNTRIAGED_CEILING, (
        f"{len(untriaged)} checks are untriaged, up from {UNTRIAGED_CEILING}."
    )
    if len(untriaged) < UNTRIAGED_CEILING:
        pytest.fail(
            f"Only {len(untriaged)} checks are untriaged now, down from {UNTRIAGED_CEILING}. "
            f"Lower UNTRIAGED_CEILING to {len(untriaged)}."
        )
