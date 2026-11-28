"""$fromMillis formatting changes from jsonata-js 2.3.0 beyond its reference suite's cases.

Expected values are from jsonata-js 2.3.0 from npm. Every case runs on the default path and on
the forced tree-walker.
"""

import math

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
        # A timezone that isn't a number is a NaN offset in the timezone markers too.
        ('$fromMillis(0, "[Z01:01t]", "abc")', "+0N:aN"),
        (
            '$fromMillis(0, "[H01]:[m01] [Z01:01] [z] [Z0001] [Z0]", "abc")',
            "NaN:NaN +0N:aN GMT+0N:aN +0NaN +NaN:NaN",
        ),
        # A digit family moves every character, those of NaN included.
        ('$fromMillis(8.64e15+1, "[H\u0660\u0661]")', "\u067e\u0691\u067e"),
        ('$fromMillis(8.64e15+1, "[f\u0660\u0660\u0661]")', "\u067e\u0691\u067e"),
    ],
)
def test_out_of_range_dates(engine, expr, expected):
    """An out-of-range date formats as NaN digits and empty names (jsonata-js #869)."""
    check(expr, expected)


@pytest.mark.parametrize(
    "expr, expected",
    [
        ("$toMillis('2020-01-01 00:00 -03:30', '[Y]-[M]-[D] [H]:[m] [Z]')", 1577849400000),
        ("$toMillis('2020-01-01 00:00 -0030', '[Y]-[M]-[D] [H]:[m] [Z0001]')", 1577838600000),
        ("$toMillis('2020-01-01 00:00 GMT-09:30', '[Y]-[M]-[D] [H]:[m] [z]')", 1577871000000),
        # A width modifier is read with parseInt (PR #302 item 12).
        ("$toMillis('20200101', '[Y,5x][M01][D01]')", 575306841600000),
        ("$toMillis('2020x0101', '[Y]x[M,2x][D01]')", 1577836800000),
        # Names are matched case-insensitively but looked up as written.
        ("$toMillis('January 5 2020', '[MNn] [D] [Y]')", 1578182400000),
        ("$toMillis('january 5 2020', '[MNn] [D] [Y]')", "ERR D3136"),
        ("$toMillis('2020 twelfth', '[Y] [Mwo]')", 1606780800000),
        ("$toMillis('2018-01-01T12:00:00.57', '[Y]-[M]-[D]T[H]:[m]:[s].[f]')", 1514808000570),
        ("$toMillis('2020-1-1 12pm', '[Y]-[M]-[D] [h][P]')", 1577880000000),
        # Every grouping separator is removed, not only a comma.
        ("$parseInteger('1.234.567', '#.##0')", 1234567),
        ("$parseInteger('1 234 567', '# ##0')", 1234567),
        # Long pictures parse in linear time.
        ("$type($toMillis('1', '[Y' & $pad('', 600000, '0.') & ']'))", "number"),
    ],
)
def test_to_millis_matcher(engine, expr, expected):
    """$toMillis's linear-time picture matcher (jsonata-js #869, and #856 for offsets)."""
    check(expr, expected)


def test_unknown_number_word_is_nan(engine):
    """An unknown word gives NaN, as jsonata-js's wordsToNumber does."""
    assert math.isnan(jsonatapy.compile("$parseInteger('foo', 'w')").evaluate(None))
