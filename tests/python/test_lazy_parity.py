"""A Python dict must behave exactly like the same data as JSON.

Dicts cross the boundary as a lazy view (`JValue::LazyPyDict`) instead of a
materialised `JValue::Object`. Every place that inspects a value has to know
about that variant -- or normalise it first -- and a missed spot is invisible
in ordinary testing because the JSON route stays correct. #98 found one:
`compiled_is_truthy` had no lazy arm and fell through to `_ => false`, making
every non-empty dict falsy on the bytecode VM.

Rather than enumerate call sites, this asserts the property directly: the two
data-entry routes must produce identical results. Any future value-inspecting
code that forgets the lazy variant breaks that equality, whichever side ends
up wrong.

Expressions are chosen to route values into as many different consumers as
possible -- truthiness, stringification, structural builtins, paths,
comparison, sorting, grouping, transforms, higher-order functions -- and are
crossed with payload shapes that make objects turn up in different positions.
Both engines run, since the bug this replaces was compiled-path only.
"""

import json

import jsonatapy
import pytest

DATASETS = {
    "plain": {
        "o": {"q": 1, "r": "s"},
        "arr": [{"p": 1}, {"p": 2}],
        "objs": [{"a": {"b": 1}}, {"a": {"b": 2}}],
        "deep": {"x": {"y": {"z": 7}}},
        "empty": {},
    },
    "nulls": {
        "o": {"q": None},
        "arr": [{"p": None}, {"p": 1}],
        "objs": [{"a": None}, {"a": {"b": 2}}],
        "deep": {"x": None},
        "empty": {},
    },
    "empties": {"o": {}, "arr": [], "objs": [{}], "deep": {"x": {}}, "empty": {}},
    "nested_arr": {
        "o": {"q": [1, 2]},
        "arr": [{"p": [1, 2]}, {"p": 3}],
        "objs": [{"a": [{"b": 1}]}],
        "deep": {"x": [{"y": 1}]},
        "empty": {},
    },
    "mixed": {
        "o": {"q": "s", "r": True},
        "arr": [{"p": "x"}, {"p": False}],
        "objs": [{"a": {"b": "z"}}],
        "deep": {"x": {"y": {"z": None}}},
        "empty": {},
    },
    "missing": {
        "o": {"q": 1},
        "arr": [{"p": 1}, {"z": 9}],
        "objs": [{"c": 1}],
        "deep": {},
        "empty": {},
    },
}

EXPRESSIONS = [
    # Truthiness consumers.
    'o ? "y" : "n"',
    "$boolean(o)",
    "$not(o)",
    "o and true",
    "o or false",
    "empty ? 1 : 2",
    # Stringification consumers.
    'o & "x"',
    "$string(o)",
    "$string(deep.x)",
    # Structural builtins.
    "$keys(o)",
    "$spread(o)",
    '$merge([o, {"z": 9}])',
    "$type(o)",
    "$exists(o)",
    "$count(o)",
    '$lookup(o, "q")',
    "$each(o, function($v, $k) { $k })",
    "$sift(o, function($v) { $v = 1 })",
    "$keys(deep)",
    # Paths and comparisons.
    "deep.x.y.z",
    "o.q",
    "o.q = 1",
    "o.q < 2",
    "objs.a.b",
    "$sum(arr.p)",
    "arr.p",
    "arr[p]",
    "arr.p[0]",
    "arr.p[-1]",
    "arr[0]",
    "deep.*",
    "**.z",
    # Sorting, grouping, transforms, higher-order functions.
    "$sort(arr, function($l, $r) { $l.p > $r.p }).p",
    'objs{"k": $string(a)}',
    'arr.{"v": p}',
    "$map(arr, function($v) { $v.p })",
    "$filter(arr, function($v) { $v.p })",
    "$reduce(arr.p, function($x, $y) { $x & $y })",
    'o ~> |$|{"q": 2}|',
    "$distinct([o, o])",
]


def _outcome(fn):
    """Return a comparable (kind, value) so raised errors compare too."""
    try:
        return ("ok", fn())
    except Exception as exc:
        return ("error", str(exc))


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


@pytest.mark.parametrize("dataset", sorted(DATASETS), ids=sorted(DATASETS))
@pytest.mark.parametrize("expr", EXPRESSIONS)
def test_dict_route_matches_json_route(expr, dataset, engine):
    data = DATASETS[dataset]
    compiled = jsonatapy.compile(expr)

    from_dict = _outcome(lambda: compiled.evaluate(data))

    def via_json():
        raw = compiled.evaluate_json_or_none(json.dumps(data))
        return None if raw is None else json.loads(raw)

    from_json = _outcome(via_json)

    assert from_dict == from_json, (
        f"{expr} on {dataset}: dict route gave {from_dict!r}, JSON route gave {from_json!r}"
    )
