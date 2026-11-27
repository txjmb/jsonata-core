"""$formatNumber and $formatInteger checks from jsonata-js 2.3.0's implementation-tests.js.

jsonata-js keeps these out of its shared JSON suite because they depend on how numbers are held:
numbers of 1e21 and above (#864), digits beyond a double's precision (#868), and NaN and
infinity, which JSON can't hold but a binding can (#868). Expected values are copied from
those tests. Every case runs on the default path and on the forced tree-walker.
"""

import jsonatapy
import pytest


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


def evaluate(expr, bindings=None):
    return jsonatapy.compile(expr).evaluate(None, bindings)


@pytest.mark.parametrize(
    "expr, expected",
    [
        ("$formatInteger(12345678901234567000, '0')", "12345678901234567000"),
        ("$formatInteger(1e21, '#,##0')", "1,000,000,000,000,000,000,000"),
        ("$formatInteger(-1e21, '0')", "-1000000000000000000000"),
        ("$formatInteger(123456789012345678901234, '0')", "123456789012345690000000"),
        ("$formatNumber(1e21, '#,##0')", "1,000,000,000,000,000,000,000"),
        ("$formatNumber(-1e21, '#,##0')", "-1,000,000,000,000,000,000,000"),
        ("$formatNumber(1.5e21, '#,##0.00')", "1,500,000,000,000,000,000,000.00"),
        ("$formatNumber(123456789012345678901234, '0')", "123456789012345690000000"),
        ("$formatNumber(1e19, '0%')", "1000000000000000000000%"),
        ("$formatNumber(1e20, '#,##0')", "100,000,000,000,000,000,000"),
    ],
)
def test_numbers_from_1e21(engine, expr, expected):
    """Formatting numbers >= 1e21 (jsonata-js #864)."""
    assert evaluate(expr) == expected


@pytest.mark.parametrize(
    "expr, expected",
    [
        ("$formatNumber(1.1, '0.00000000000000000000')", "1.10000000000000000000"),
        ("$formatNumber(0.1 + 0.2, '0.##################')", "0.30000000000000004"),
        ("$formatNumber(531135999999999.94, '0.00')", "531135999999999.94"),
        ("$formatNumber(9007199254740994, '0.00')", "9007199254740994.00"),
        ("$formatNumber(12345678901234567000, '0')", "12345678901234567000"),
        ("$formatNumber(123456789012345678901234, '0.000')", "123456789012345690000000.000"),
        (
            "$formatNumber(1.5, '0.' & $join([1..101].('0'))) = '1.5' & $join([1..100].('0'))",
            True,
        ),
        ("$formatNumber(1, $pad('', 309, '0') & 'e0') = $pad('1', 309, '0') & 'e-308'", True),
    ],
)
def test_beyond_double_precision(engine, expr, expected):
    """Formatting from the shortest round-trip decimal digits (jsonata-js #868)."""
    assert evaluate(expr) == expected


@pytest.mark.parametrize(
    "expr, bindings, expected",
    [
        ("$formatNumber($v, '£#,##0.00')", {"v": float("nan")}, "NaN"),
        ("$formatNumber($v, '£#,##0.00')", {"v": float("inf")}, "£Infinity"),
        ("$formatNumber($v, '£#,##0.00')", {"v": float("-inf")}, "-£Infinity"),
        ("$formatNumber($v, '#,##0.00;(#,##0.00)')", {"v": float("-inf")}, "(Infinity)"),
        ("$formatNumber($v, '0.0e0')", {"v": float("inf")}, "Infinity"),
        ("$formatNumber(1e308, '0%')", None, "Infinity%"),
        (
            "[$formatNumber($n, '0.00', $o), $formatNumber($i, '0.00', $o)]",
            {
                "n": float("nan"),
                "i": float("inf"),
                "o": {"NaN": "not a number", "infinity": "∞"},
            },
            ["not a number", "∞"],
        ),
    ],
)
def test_nan_and_infinity(engine, expr, bindings, expected):
    """NaN and infinity use the decimal format's NaN and infinity properties (jsonata-js #868)."""
    assert evaluate(expr, bindings) == expected
