"""JS-parity number stringification for $string, concat, and JSON output.

jsonata-js's `$string` is `JSON.stringify` with a replacer that rounds every
NON-integer number to 15 significant digits (`Number(val.toPrecision(15))`)
and leaves integer-valued numbers at full (shortest round-trip) precision.
JavaScript's number printer then uses plain digits in [1e-6, 1e21) and
exponential notation (with `+` on positive exponents) outside it.

We diverged three ways: non-integers were truncated to 14 significant digits
(the old formatter counted the `0` of a leading `0.` as significant),
integer-valued floats above 2^53 printed their exact i64 digits instead of
the float's shortest round-trip decimal, and nested numbers in
containers went through serde with different rules again. All expected
values below were produced by the pinned jsonata-js submodule, and a
307-case randomized differential (scalars, containers, prettify, concat)
matches it exactly.
"""

import jsonatapy


def ev(src, data=None):
    return jsonatapy.compile(src).evaluate(data if data is not None else {})


def test_non_integer_rounds_to_15_significant_digits():
    assert ev("$string(1/3)") == "0.333333333333333"
    assert ev("$string(-1/3)") == "-0.333333333333333"
    assert ev("$string(0.1234567890123456789)") == "0.123456789012346"


def test_leading_zero_is_not_a_significant_digit():
    # The old formatter emitted 0.00000123456789012345 here (14 digits).
    assert ev("$string(0.00000123456789012345678)") == "0.00000123456789012346"


def test_integers_keep_shortest_roundtrip_precision():
    # Above 2^53: JS prints the float's shortest round-trip decimal, not the
    # exact i64 value (…744) the old formatter emitted.
    assert ev("$string(x)", {"x": 7.693663077734049e17}) == "769366307773404900"
    assert ev("$string(9007199254740993)") == "9007199254740992"
    assert ev("$string(1e19)") == "10000000000000000000"


def test_exponential_thresholds_match_js():
    assert ev("$string(1e21)") == "1e+21"
    assert ev("$string(1.23456789e-9)") == "1.23456789e-9"
    assert ev("$string(0.000001)") == "0.000001"
    assert ev("$string(1e-7)") == "1e-7"


def test_negative_zero_prints_as_zero():
    assert ev("$string(x)", {"x": -0.0}) == "0"


def test_nested_numbers_use_the_same_rules():
    assert ev('$string({"a": 1/3})') == '{"a":0.333333333333333}'
    assert ev("$string([1/3, 2])") == "[0.333333333333333,2]"
    assert ev("$string(x)", {"x": {"a": 7.693663077734049e17}}) == '{"a":769366307773404900}'


def test_prettify_matches_js_stringify():
    assert (
        ev('$string({"a": [1, "s"], "b": {}}, true)')
        == '{\n  "a": [\n    1,\n    "s"\n  ],\n  "b": {}\n}'
    )


def test_concat_uses_string_rules():
    assert ev('"v=" & 1/3') == "v=0.333333333333333"
