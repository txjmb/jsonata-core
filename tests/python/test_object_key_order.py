"""Object keys come out of Python in JavaScript's own-key order.

jsonata-js builds plain JavaScript objects, which list integer-like keys
(``0``, or digits with no leading zero up to 4294967294) first in ascending
number, then every other key in insertion order. jsonata-core orders the
objects it builds the same way, so a dict returned to Python, and anything
that walks an object (``$keys``, ``$each``, ``$spread``, ``$string``), sees
the keys in that order whether the input came in as a dict or as JSON.

One known difference from jsonata-js: a dict that an expression returns
untouched (``$``, or a field of the input) is the caller's own object, not a
copy, so it keeps the caller's order, where jsonata-js would reorder it.
That is the zero-copy pass-through of a lazy dict; reordering would mean
copying every such dict on the way out. It is tested here so that a change
to it is deliberate.

Each expected value is jsonata-js 2.2.2's, for the input
``{"b": 1, "2": 2}``.
"""

import json

import jsonatapy
import pytest

DATA = {"b": 1, "2": 2}

CASES = [
    ("$keys($)", ["2", "b"]),
    ('{"b": 1, "2": 2, "1": 3, "4294967295": 4, "01": 5}', None),
    ('$merge([$, {"a": 3}])', {"2": 2, "b": 1, "a": 3}),
    ('$ ~> | $ | {"1": 4} |', {"1": 4, "2": 2, "b": 1}),
    ("$string($)", '{"2":2,"b":1}'),
    ("$sift($, function($v) { true })", {"2": 2, "b": 1}),
    ("$each($, function($v, $k) { $k })", ["2", "b"]),
    ("$spread($)", [{"2": 2}, {"b": 1}]),
]


def _keys_in_order(value):
    """The value with every dict replaced by its list of (key, value) pairs,
    so that comparing two results also compares key order."""
    if isinstance(value, dict):
        return [(k, _keys_in_order(v)) for k, v in value.items()]
    if isinstance(value, list):
        return [_keys_in_order(v) for v in value]
    return value


@pytest.mark.parametrize("expr,expected", CASES)
def test_dict_input_gives_javascript_key_order(expr, expected):
    result = jsonatapy.compile(expr).evaluate(DATA)
    if expected is None:
        assert list(result) == ["1", "2", "b", "4294967295", "01"]
    else:
        assert _keys_in_order(result) == _keys_in_order(expected)


@pytest.mark.parametrize("expr,expected", CASES)
def test_json_input_gives_the_same_order(expr, expected):
    compiled = jsonatapy.compile(expr)
    from_dict = compiled.evaluate(DATA)
    from_json = json.loads(compiled.evaluate_json(json.dumps(DATA)))
    assert _keys_in_order(from_json) == _keys_in_order(from_dict)


def test_built_object_keys_in_javascript_order():
    result = jsonatapy.evaluate('{"b": 1, "4294967294": 1, "0": 2, "-1": 3, "1.5": 4}', None)
    assert list(result) == ["0", "4294967294", "b", "-1", "1.5"]


def test_untouched_dict_is_the_callers_own():
    # Known difference from jsonata-js (see the module docstring): passed
    # straight through, the dict is not copied, so its order is the caller's.
    # A dict the expression builds is in JavaScript's order.
    assert jsonatapy.compile("$").evaluate(DATA) is DATA
    assert list(jsonatapy.compile("$sift($, function($v) { true })").evaluate(DATA)) == ["2", "b"]
