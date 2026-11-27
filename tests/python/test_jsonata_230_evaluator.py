"""Evaluator fixes from jsonata-js 2.3.0 that its reference suite does not fully cover.

Each expected value or error code was produced by jsonata-js 2.3.0 from npm, with the bindings
in `BINDINGS`. Every case runs on the default path and on the forced tree-walker.
"""

import re

import jsonatapy
import pytest

BINDINGS = {"arr": [[1, 2], [4, 5]], "i": 1, "j": 1}
UNDEFINED = object()


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


def check(expr, data, expected):
    if isinstance(expected, str) and re.fullmatch(r"[TDUS]\d{4}", expected):
        with pytest.raises(ValueError, match=rf"\b{expected}\b"):
            jsonatapy.compile(expr).evaluate(data, BINDINGS)
        return
    result = jsonatapy.compile(expr).evaluate(data, BINDINGS)
    if expected is UNDEFINED:
        assert result is None
    else:
        assert result == expected


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        ('$split("Hello World", nothing)', None, UNDEFINED),
        ('$replace("Hello World", nothing, "Earth")', None, UNDEFINED),
        ('["a b", "c d"].$split($, nothing)', None, UNDEFINED),
        ('$split("a b", " ")', None, ["a", "b"]),
    ],
)
def test_split_replace(engine, expr, data, expected):
    """$split/$replace with an undefined pattern give undefined (jsonata-js #831)."""
    check(expr, data, expected)


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        ("($f := function(){1}; [[$f].**])", None, []),
        ("[[$sum].**]", None, []),
    ],
)
def test_descendants(engine, expr, data, expected):
    """** never returns a function, like * (jsonata-js #873)."""
    check(expr, data, expected)


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        ("(1e308 * 10) and true", None, "D1001"),
        ("true and (1e308 * 10)", None, "D1001"),
        ("false or (1e308 * 10)", None, "D1001"),
        ("[1e308 * 10] or false", None, "D1001"),
        ("false and (1e308 * 10)", None, False),
        ("true or (1e308 * 10)", None, True),
        ("(1e308 * 10) ? 1 : 2", None, "D1001"),
        ("$boolean(1e308 * 10)", None, "D1001"),
        ("$not(1e308 * 10)", None, "D1001"),
        ("$boolean([0, 1e308 * 10])", None, "D1001"),
        ("(0/0) and true", None, False),
        ('$boolean({"a": 1e308 * 10})', None, True),
        (
            "( $f := function($n, $a){ $n > 3 ? ($a ? 1 : 0) : $f($n+1, $a) }; $f(0, 1e308*10) )",
            None,
            "D1001",
        ),
        ("(1e308*10) ?: 5", None, "D1001"),
        ("0 ?: 5", None, 5),
        ("[1,2][1e308*10]", None, "D1001"),
        ("[1,2][[0, 1e308*10]]", None, "D1001"),
        ('[1,2][["a", 1e308*10]]', None, "D1001"),
        ("$filter([1e308*10], function($v){$v})", None, "D1001"),
        ("$single([1e308*10], function($v){$v})", None, "D1001"),
        ('$sift({"a":1e308*10}, function($v){$v})', None, "D1001"),
    ],
)
def test_boolean_cast(engine, expr, data, expected):
    """A boolean cast raises D1001 for an infinite number it reads (jsonata-js #857): and/or, conditions, ?:, $boolean, $not, predicates and the predicate functions. NaN is false."""
    check(expr, data, expected)


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        ("$arr[$i][0]", None, 4),
        ("$arr[$i][$j]", None, 5),
        ("$arr[$i]", None, [4, 5]),
        ("$arr[$i][]", None, [[4, 5]]),
        ("$arr[$i][0][0]", None, 4),
        ("$arr[[$i]][0]", None, [4, 5]),
        ("x[$i][$j]", {"x": [[1, 2], [3, 4]]}, 4),
        ("x[$ > 0][0]", {"x": [[1, 2], [3, 4]]}, "T2010"),
        ("$arr[$i][][0]", None, [4]),
        ("x[$i][][0]", {"x": [[1, 2], [4, 5]]}, [4]),
    ],
)
def test_chained_predicates(engine, expr, data, expected):
    """A computed index that selects one array returns it when another predicate follows (jsonata-js #861)."""
    check(expr, data, expected)


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        (
            "orders#$i.[items.sku]",
            {
                "orders": [
                    {"items": [{"sku": "p"}, {"sku": "q"}]},
                    {"items": []},
                    {"items": [{"sku": "r"}]},
                ]
            },
            [["p", "q"], [], ["r"]],
        ),
        (
            "orders@$o.[$o.items.sku]",
            {"orders": [{"items": [{"sku": "p"}, {"sku": "q"}]}, {"items": [{"sku": "r"}]}]},
            [["p", "q"], ["r"]],
        ),
        ("foo#$i.[$][$ > 1]", {"foo": [1, 2, 3]}, [2, 3]),
        ("foo#$i.[$, $i]", {"foo": [1, 2]}, [[1, 0], [2, 1]]),
        ("foo#$i.[$, 9][]", {"foo": [1, 2]}, [[1, 9], [2, 9]]),
        ("foo#$i.[$, 9]#$j", {"foo": [1, 2]}, [[1, 9], [2, 9]]),
        ("foo.[$, 9]#$k.$k", {"foo": [1, 2]}, [0, 0]),
        ("foo.[$, 9]#$k.$", {"foo": [1, 2]}, [[1, 9], [2, 9]]),
        ("foo.[$, 9]@$v.$v", {"foo": [1, 2]}, [[1, 9], [2, 9]]),
    ],
)
def test_tuple_arrays(engine, expr, data, expected):
    """A constructed array is one value of a tuple stream (jsonata-js #841)."""
    check(expr, data, expected)


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        ("$ ~> |a|null|", {"a": {"b": 1}}, "T2011"),
        ('$ ~> |a.b|"s"|', {"a": {"b": 1}}, "T2011"),
        ("$ ~> |a.b|nothing|", {"a": {"b": 1}}, {"a": {"b": 1}}),
        ("$ ~> |a.b|{}, 5|", {"a": {"b": 1}}, "T2012"),
        ('$ ~> |a.b|nothing, "z"|', {"a": {"b": 1}}, {"a": {"b": 1}}),
        ("$ ~> |g|5|", {"g": [[{"h": 1}]]}, "T2011"),
        ('$ ~> |g|{"y": 1}|', {"g": [[{"h": 1}]]}, {"g": [[{"h": 1}]]}),
        ("$ ~> |null|5|", {"a": 1}, "T2011"),
        ("$ ~> |null|{}|", {"a": 1}, {"a": 1}),
        ("$ ~> |[]|5|", {"a": 1}, {"a": 1}),
        ('$ ~> |a|{"c": 2}|', {"a": {"b": 1}}, {"a": {"b": 1, "c": 2}}),
    ],
)
def test_transform(engine, expr, data, expected):
    """The transform's update and delete clauses run, and are type-checked, for every match, and a null update is T2011."""
    check(expr, data, expected)


GROUPED = {
    "o": [{"b": 1, "c": "x"}, {"b": 2, "c": "y"}],
    "n": {"m": [{"k": "a", "v": 1}, {"k": "b", "v": 2}, {"k": "a", "v": 3}]},
}


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        ("o{c: b}.x", GROUPED, 1),
        ("o{c: b}.$keys()", GROUPED, ["x", "y"]),
        ("o{c: b}.($.x + $.y)", GROUPED, 3),
        ("n.m{k: v}.a", GROUPED, [1, 3]),
        ("n.m{k: $sum(v)}.$each(function($v,$k){$k & $v})", GROUPED, ["a4", "b2"]),
        ('o.b{"k":$}[].k', GROUPED, [1, 2]),
        ("n.m{k: v}.nope", GROUPED, UNDEFINED),
        ("o{c: b}.x.$string()", GROUPED, "1"),
        ("(o{c: b}).x", GROUPED, 1),
    ],
)
def test_grouped_path_steps(engine, expr, data, expected):
    """A step after a group-by reads the grouped object as one value (jsonata-js #838)."""
    check(expr, data, expected)
