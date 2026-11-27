"""The transform operator changes the objects its location matches, by identity.

jsonata-js copies the input and changes each matched object of the copy in
place, so an object that merely *equals* a matched one is left alone. Before
2.2.17, jsonata-core matched by value and changed every equal object too.
It now also raises D1014 (jsonata-js 2.3.0) when the location matches an
object that is not part of the copy, such as one it built.

Expected values were checked against jsonata-js 2.3.0.
"""

import jsonatapy
import pytest

PATHS = [
    pytest.param(None, id="default"),
    pytest.param({}, id="tree-walker"),
]


@pytest.mark.parametrize("bindings", PATHS)
@pytest.mark.parametrize(
    "expr, expected",
    [
        # Equal siblings: only the matched one changes.
        (
            '{"b":{"a":1},"c":{"a":1}} ~> | b | {"x":2} |',
            {"b": {"a": 1, "x": 2}, "c": {"a": 1}},
        ),
        ('[{"a":1},{"a":1}] ~> | $[0] | {"x":2} |', [{"a": 1, "x": 2}, {"a": 1}]),
        (
            '{"a":[{"b":1},{"b":1}]} ~> | a[0] | {"x":1} |',
            {"a": [{"b": 1, "x": 1}, {"b": 1}]},
        ),
        (
            '{"a":[[{"b":1}],[{"b":1}]]} ~> | a[0] | {"x":1} |',
            {"a": [[{"b": 1, "x": 1}], [{"b": 1}]]},
        ),
        (
            '{"x":[{"a":1},{"a":1}]} ~> | x[1] | {"y":2}, "a" |',
            {"x": [{"a": 1}, {"y": 2}]},
        ),
        # One object bound to a variable and used twice: the copy gives each
        # place its own object.
        (
            '($o := {"a":1}; {"b":$o,"c":$o} ~> | b | {"x":2} |)',
            {"b": {"a": 1, "x": 2}, "c": {"a": 1}},
        ),
        # Every match still changes, equal or not.
        (
            '{"a":[{"b":1},{"b":1}]} ~> | a[b=1] | {"x":1} |',
            {"a": [{"b": 1, "x": 1}, {"b": 1, "x": 1}]},
        ),
        # Matches apply in order, each to the object as earlier ones left it.
        (
            '{"a":{"b":{"c":1}}} ~> | [a.b, a] | {"z": $.b.z ? $.b.z + 1 : 1} |',
            {"a": {"b": {"c": 1, "z": 1}, "z": 2}},
        ),
        (
            '{"a":{"b":{"c":1}}} ~> | [a, a.b] | {"b":{"q":1}} |',
            {"a": {"b": {"q": 1}}},
        ),
        (
            '{"a":{"b":{"c":1}}} ~> | [a, a] | {"n": $count($keys($))} |',
            {"a": {"b": {"c": 1}, "n": 2}},
        ),
        # The delete clause sees the object after its update.
        ('{"a":{"b":1}} ~> | a | {"x":1}, $keys($) |', {"a": {}}),
        ('{"a":{"b":{"c":1}}} ~> | $ | {"x":1} |', {"a": {"b": {"c": 1}}, "x": 1}),
    ],
)
def test_transform_matches_by_identity(expr, expected, bindings):
    assert jsonatapy.compile(expr).evaluate(None, bindings) == expected


@pytest.mark.parametrize("bindings", PATHS)
def test_transform_on_python_input(bindings):
    data = {"b": {"a": 1}, "c": {"a": 1}, "l": [{"a": 1}, {"a": 1}]}
    expr = jsonatapy.compile('$ ~> | b | {"x":2} |')
    assert expr.evaluate(data, bindings) == {
        "b": {"a": 1, "x": 2},
        "c": {"a": 1},
        "l": [{"a": 1}, {"a": 1}],
    }
    expr = jsonatapy.compile('$ ~> | l[1] | {"x":2} |')
    assert expr.evaluate(data, bindings)["l"] == [{"a": 1}, {"a": 1, "x": 2}]
    # The input itself is never changed.
    assert data == {"b": {"a": 1}, "c": {"a": 1}, "l": [{"a": 1}, {"a": 1}]}


@pytest.mark.parametrize("bindings", PATHS)
def test_bound_transform_applies_per_item(bindings):
    expr = '($t := | a | {"x":1} |; [{"a":{}},{"a":{}}].$t($))'
    assert jsonatapy.compile(expr).evaluate(None, bindings) == [
        {"a": {"x": 1}},
        {"a": {"x": 1}},
    ]


@pytest.mark.parametrize("bindings", PATHS)
@pytest.mark.parametrize(
    "expr",
    [
        '{"a":{"b":1}} ~> | {"z":1} | {"x":1} |',
        '{"a":{"b":1}} ~> | $merge([a]) | {"x":1} |',
    ],
)
def test_location_outside_the_copy_raises_d1014(expr, bindings):
    with pytest.raises(ValueError, match="D1014"):
        jsonatapy.compile(expr).evaluate(None, bindings)
