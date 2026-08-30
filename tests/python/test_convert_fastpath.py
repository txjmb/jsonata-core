"""Python→Rust conversion fast-path boundary tests.

`lazy::convert` dispatches on the type object (exact types for scalars,
`PyList_Check`/`PyDict_Check` for containers) and reads list elements as
borrowed references; scalar subclasses and duck-typed objects fall back to
the pyo3-extract chain. These tests pin the behaviors that must not differ
between the two paths, on both the lazy `evaluate(dict)` route and the
eager `JsonataData` route. The expression always *touches* the value
(`x`, not `$`), since an untouched lazy dict is passed back unconverted.
"""

import enum
import math
import sys

import jsonatapy
import pytest

X_EXPR = jsonatapy.compile("x")


class MyList(list):
    pass


class MyDict(dict):
    pass


class MyFloat(float):
    pass


class MyStr(str):
    pass


def both_paths(data):
    """Evaluate `x` over both conversion routes; assert parity; return result."""
    lazy = X_EXPR.evaluate(data)
    eager = X_EXPR.evaluate_with_data(jsonatapy.JsonataData(data))
    assert lazy == eager or (isinstance(lazy, float) and math.isnan(lazy) and math.isnan(eager))
    return lazy


def test_exact_scalars():
    assert both_paths({"x": 42}) == 42
    assert both_paths({"x": -7}) == -7
    assert both_paths({"x": 2.5}) == 2.5
    assert both_paths({"x": "héllo ✓"}) == "héllo ✓"
    assert both_paths({"x": True}) is True
    assert both_paths({"x": False}) is False
    assert both_paths({"x": None}) is None


def test_nan_and_inf_pass_through():
    assert math.isnan(both_paths({"x": float("nan")}))
    assert both_paths({"x": float("inf")}) == float("inf")


def test_exact_containers():
    assert both_paths({"x": [1, [2, "a", None, True, 3.5]]}) == [1, [2, "a", None, True, 3.5]]
    assert both_paths({"x": [{"y": 7}, {"y": 8}]}) == [{"y": 7}, {"y": 8}]
    assert jsonatapy.compile("x.y").evaluate({"x": [{"y": 7}]}) == 7


def test_empty_containers():
    assert both_paths({"x": []}) == []
    assert both_paths({"x": [[], [[]]]}) == [[], [[]]]


def test_subclasses_take_slow_path_with_same_result():
    # bool is NOT reached by the exact-int branch (distinct type object)
    assert both_paths({"x": enum.IntEnum("E", "A").A}) == 1
    assert both_paths({"x": MyFloat(1.5)}) == 1.5
    assert both_paths({"x": MyStr("hi")}) == "hi"
    assert both_paths({"x": MyList([1, 2])}) == [1, 2]
    assert both_paths({"x": MyDict(y=3)}) == {"y": 3}
    # subclass elements inside an exact list (mixed fast/slow in one loop)
    assert both_paths({"x": [MyFloat(1.5), 2, MyStr("s")]}) == [1.5, 2, "s"]


def test_int_beyond_i64_raises_on_both_paths():
    with pytest.raises((TypeError, OverflowError)):
        X_EXPR.evaluate({"x": 2**70})
    with pytest.raises((TypeError, OverflowError)):
        jsonatapy.JsonataData({"x": 2**70})


def test_lone_surrogate_string_raises_on_both_paths():
    with pytest.raises((TypeError, UnicodeEncodeError)):
        X_EXPR.evaluate({"x": "\ud800"})
    with pytest.raises((TypeError, UnicodeEncodeError)):
        jsonatapy.JsonataData({"x": "\ud800"})


def test_nested_array_indexing():
    # The "Nested Array Access" benchmark shape: pure lists all the way down.
    data = {
        "data": [[[[i, i + 1, i + 2] for i in range(0, 30, 3)] for _ in range(3)] for _ in range(3)]
    }
    assert jsonatapy.compile("data[1][1][1][1]").evaluate(data) == 4


def test_refcounts_stable_after_failed_conversion():
    # The borrowed-reference list loop must not leak or over-decref elements
    # when conversion fails partway through the list.
    item = [1, 2, 3]
    data = {"x": [item, "\ud800", item]}
    rc_before = sys.getrefcount(item)
    for _ in range(3):
        with pytest.raises((TypeError, UnicodeEncodeError)):
            jsonatapy.JsonataData(data)
        with pytest.raises((TypeError, UnicodeEncodeError)):
            X_EXPR.evaluate(data)
    assert sys.getrefcount(item) == rc_before
