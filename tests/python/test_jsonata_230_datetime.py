"""$fromMillis formatting changes from jsonata-js 2.3.0 beyond its reference suite's cases.

Expected values are from jsonata-js 2.3.0 from npm. Every case runs on the default path and on
the forced tree-walker.
"""

import jsonatapy
import pytest


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


def check(expr, expected):
    if isinstance(expected, str) and expected.startswith("ERR "):
        with pytest.raises(ValueError, match=expected[4:]):
            jsonatapy.compile(expr).evaluate(None)
        return
    assert jsonatapy.compile(expr).evaluate(None) == expected


@pytest.mark.parametrize(
    "expr, expected",
    [
        ('$fromMillis(0, "[H01]:[m01] [Z]", "-0330")', "20:30 -03:30"),
        ('$fromMillis(0, "[Z0001]", "-0330")', "-0330"),
        ('$fromMillis(0, "[Z01:01]", "-0030")', "-00:30"),
        ('$fromMillis(0, "[z]", "-0930")', "GMT-09:30"),
        ('$fromMillis(0, "[H01]:[m01]", "-0030")', "23:30"),
    ],
)
def test_negative_offsets_with_minutes(engine, expr, expected):
    """Both parts of a negative offset take its sign (jsonata-js #856)."""
    check(expr, expected)


@pytest.mark.parametrize(
    "expr, expected",
    [
        ('$fromMillis(30, "[f01]")', "03"),
        ('$fromMillis(30, "[f]")', "03"),
        ('$fromMillis(5, "[f1]")', "005"),
        ('$fromMillis(999, "[f01]")', "99"),
        ('$fromMillis(120, "[f,2-4]")', "12"),
        ('$fromMillis(250, "[f0,0]")', "2"),
        ('$fromMillis(0, "[fI]")', ""),
        ('$fromMillis(123, "[f#]")', "ERR D3130"),
    ],
)
def test_fractional_seconds(engine, expr, expected):
    """Fractional seconds are padded and truncated on the right (jsonata-js #855)."""
    check(expr, expected)


@pytest.mark.parametrize(
    "expr, expected",
    [
        ('$fromMillis(1e20, "[Y0001]-[M01]-[D01] [FNn] [Dwo] [P]")', "0NaN-NaN-NaN   am"),
        ('$fromMillis(1e20, "[f001]")', "NaN"),
        ('$fromMillis(1, "[Y]", "abc")', "NaN"),
        ('$fromMillis(1e19, "[Y]", "+0100")', "NaN"),
    ],
)
def test_out_of_range_dates(engine, expr, expected):
    """An out-of-range date formats as NaN digits and empty names (jsonata-js #869)."""
    check(expr, expected)
