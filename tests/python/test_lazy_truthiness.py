"""A dict crossing the Python boundary must be as truthy as the same JSON.

Python dicts arrive as a lazy view (`JValue::LazyPyDict`) rather than a
materialised object. The tree-walker's `is_truthy` handles that variant; the
compiled path's `compiled_is_truthy` did not, and fell through to its
catch-all `_ => false`, making every non-empty dict falsy on the bytecode VM.

The two data-entry routes must agree: evaluating against a dict and against
the equivalent JSON string is the same question asked twice.
"""

import json

import jsonatapy
import pytest

# Expressions whose result depends on the truthiness of an object value.
TRUTHINESS_EXPRS = [
    'o ? "yes" : "no"',
    "$boolean(o)",
    "o and true",
    "o or false",
    "$not(o)",
    "arr[p]",
]

DATA = {
    "o": {"q": 1},
    "arr": [{"p": {"q": 1}}, {"p": {"q": 2}}],
}


@pytest.mark.parametrize("expr", TRUTHINESS_EXPRS)
def test_dict_and_json_routes_agree(expr):
    compiled = jsonatapy.compile(expr)
    from_dict = compiled.evaluate(DATA)
    raw = compiled.evaluate_json_or_none(json.dumps(DATA))
    from_json = None if raw is None else json.loads(raw)

    assert from_dict == from_json, (
        f"{expr}: dict route gave {from_dict!r}, JSON route gave {from_json!r}"
    )


def test_non_empty_dict_is_truthy():
    assert jsonatapy.compile('o ? "yes" : "no"').evaluate({"o": {"q": 1}}) == "yes"
    assert jsonatapy.compile("$boolean(o)").evaluate({"o": {"q": 1}}) is True


def test_empty_dict_is_falsy():
    assert jsonatapy.compile('o ? "yes" : "no"').evaluate({"o": {}}) == "no"
    assert jsonatapy.compile("$boolean(o)").evaluate({"o": {}}) is False
