"""Regression tests for inputs that used to panic or exhaust memory (2.2.17).

Each case either panicked in Rust (`Option::unwrap`, divide-by-zero, an
`expect`) or tried to build a string of hundreds of millions of characters
(or ~1e12 roman `M`s), taking the process down. The fixes follow jsonata-js
2.3.0, which added hard limits for the same inputs: D2016 for `$pad`, D3142
for date/time width modifiers, a decimal fallback for roman numerals above
1e7, and a size check before matching a timestamp. Expected values were
checked against jsonata-js 2.3.0.

Every case runs both on the default (VM-preferred) path and on the
tree-walker, which a non-None ``bindings`` dict forces.
"""

import jsonatapy
import pytest

PATHS = [
    pytest.param(None, id="default"),
    pytest.param({}, id="tree-walker"),
]


def _eval(expr, data=None, bindings=None):
    return jsonatapy.compile(expr).evaluate(data, bindings)


# ── $pad: padding capped at 1e7 characters (D2016, jsonata-js #832) ─────────


@pytest.mark.parametrize("bindings", PATHS)
@pytest.mark.parametrize(
    "expr",
    [
        '$pad("x", 20000000)',
        '$pad("x", -20000000)',
        '$pad("x", 10000002)',  # padding of 10000001
        '$pad("", 1e300)',  # used to saturate to i32::MAX and allocate 2 GB
        '$pad("x", 1/0)',
    ],
)
def test_pad_over_limit_raises_d2016(expr, bindings):
    with pytest.raises(ValueError, match="D2016"):
        _eval(expr, bindings=bindings)


@pytest.mark.parametrize("bindings", PATHS)
def test_pad_at_limit_still_pads(bindings):
    # A padding of exactly 1e7 characters is allowed.
    assert len(_eval('$pad("x", 10000001)', bindings=bindings)) == 10000001
    assert len(_eval('$pad("x", -10000001, "ab")', bindings=bindings)) == 10000001


@pytest.mark.parametrize("bindings", PATHS)
def test_pad_unchanged_behaviour(bindings):
    assert _eval('$pad("foo", 5)', bindings=bindings) == "foo  "
    assert _eval('$pad("foo", -5, "#")', bindings=bindings) == "##foo"
    assert _eval('$pad("foo", 5.9)', bindings=bindings) == "foo  "
    assert _eval('$pad("foo", 2)', bindings=bindings) == "foo"


# ── Roman numerals above 1e7 fall back to decimal ───────────────────────────


@pytest.mark.parametrize("bindings", PATHS)
@pytest.mark.parametrize(
    "expr, expected",
    [
        ("$formatInteger(-1e15, 'i')", "-1000000000000000"),
        ("$formatInteger(1e8, 'I')", "100000000"),
        ("$formatInteger(10000001, 'I')", "10000001"),
        ("$formatInteger(1999, 'I')", "MCMXCIX"),
        ("$formatInteger(1e20, 'I')", "100000000000000000000"),
        ("$formatInteger(1e7, 'i')", "m" * 10000),
    ],
)
def test_roman_numeral_limit(expr, expected, bindings):
    assert _eval(expr, bindings=bindings) == expected


# ── Date/time width modifiers above 100 raise D3142 ─────────────────────────


@pytest.mark.parametrize("bindings", PATHS)
@pytest.mark.parametrize(
    "picture",
    [
        "[Y,600000000]",  # used to divide by zero
        "[f,150000000]",  # used to build a 150M-character string
        "[Y,101]",
        "[Y,2-101]",
        "[D,99999999999999999999999]",  # too large for usize: still caught
    ],
)
def test_width_modifier_over_100_raises_d3142(picture, bindings):
    with pytest.raises(ValueError, match="D3142"):
        _eval(f"$fromMillis(0, '{picture}')", bindings=bindings)


@pytest.mark.parametrize("bindings", PATHS)
def test_width_modifier_of_100_is_allowed(bindings):
    # [Y,100] used to panic too: 10^100 overflowed i64 in the year modulus.
    assert _eval("$fromMillis(0, '[Y,100]')", bindings=bindings) == "0" * 96 + "1970"
    assert _eval("$fromMillis(0, '[Y,2]')", bindings=bindings) == "70"


@pytest.mark.parametrize("bindings", PATHS)
def test_width_modifier_over_100_raises_d3142_when_parsing(bindings):
    with pytest.raises(ValueError, match="D3142"):
        _eval("$toMillis('1970', '[Y,101]')", bindings=bindings)


# ── Other decimal-digit families ────────────────────────────────────────────


@pytest.mark.parametrize("bindings", PATHS)
def test_parse_integer_ascii_digits_with_other_family_picture(bindings):
    # Used to panic: '1' - U+0661 underflowed to an invalid code point.
    assert _eval("$parseInteger('12', '\u0661')", bindings=bindings) == 12


@pytest.mark.parametrize("bindings", PATHS)
def test_other_digit_families_still_work(bindings):
    assert _eval("$parseInteger('\u0661\u0662', '\u0661')", bindings=bindings) == 12
    assert (
        _eval("$formatInteger(12, '\u0660\u0661')", bindings=bindings) == "\u0661\u0662"
    )
    assert (
        _eval("$fromMillis(0, '[Y\u0661]')", bindings=bindings)
        == "\u0661\u0669\u0667\u0660"
    )


# ── Fractional seconds as a name ────────────────────────────────────────────


@pytest.mark.parametrize("bindings", PATHS)
def test_fractional_seconds_as_name_raises_d3133(bindings):
    # Used to panic on an `expect`.
    with pytest.raises(ValueError, match="D3133"):
        _eval("$fromMillis(0, '[fN]')", bindings=bindings)


# ── Timestamps that cannot match the picture ────────────────────────────────


@pytest.mark.parametrize("bindings", PATHS)
@pytest.mark.parametrize(
    "expr",
    [
        # 100,000 markers against a 100,000-character timestamp: used to try
        # to allocate ~140 GB for the regex.
        "$toMillis($pad('', 99999, '1') & 'x', $pad('', 100000, '[Y]1'))",
        # A literal longer than the timestamp: used to compile a regex of
        # ten million characters.
        "$toMillis('x', $pad('', 10000000, 'a'))",
        # Fewer characters than markers.
        "$toMillis('1', '[Y][M]')",
    ],
)
def test_unmatchable_timestamp_is_undefined(expr, bindings):
    assert _eval(expr, bindings=bindings) is None


@pytest.mark.parametrize("bindings", PATHS)
def test_invalid_picture_still_raises_before_size_check(bindings):
    # The picture is analysed first, so an invalid one is an error even when
    # the timestamp is too short to match.
    with pytest.raises(ValueError, match="D3142"):
        _eval("$toMillis('', '[Y,101]')", bindings=bindings)


@pytest.mark.parametrize("bindings", PATHS)
def test_ordinary_parsing_unchanged(bindings):
    assert (
        _eval("$toMillis('2024-01-02', '[Y]-[M]-[D]')", bindings=bindings)
        == 1704153600000
    )
