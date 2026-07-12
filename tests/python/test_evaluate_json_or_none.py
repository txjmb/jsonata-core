"""Tests for JsonataExpression.evaluate_json_or_none() -- the one new
public API method this project adds in Phase 2, so the Python bindings can
distinguish a JSONata Undefined result from an explicit JSON null result
(both currently collapse to the same value via evaluate()/evaluate_json()).
"""

from __future__ import annotations

import jsonatapy


def test_undefined_result_returns_none() -> None:
    expr = jsonatapy.compile("nonexistent_field")
    assert expr.evaluate_json_or_none('{"a": 1}') is None


def test_explicit_null_result_returns_the_string_null() -> None:
    expr = jsonatapy.compile("nullField")
    assert expr.evaluate_json_or_none('{"nullField": null}') == "null"


def test_normal_result_returns_serialized_json() -> None:
    expr = jsonatapy.compile("a + b")
    assert expr.evaluate_json_or_none('{"a": 1, "b": 2}') == "3"


def test_bindings_are_applied() -> None:
    expr = jsonatapy.compile("$x * 2")
    assert expr.evaluate_json_or_none("{}", {"x": 5}) == "10"


def test_invalid_json_input_raises_value_error() -> None:
    expr = jsonatapy.compile("a")
    try:
        expr.evaluate_json_or_none("not json")
    except ValueError:
        pass
    else:
        raise AssertionError("expected ValueError for invalid JSON input")


def test_evaluation_error_raises_value_error_with_coded_message() -> None:
    expr = jsonatapy.compile("null + 1")
    try:
        expr.evaluate_json_or_none("{}")
    except ValueError as e:
        assert str(e).startswith("T2002:")
    else:
        raise AssertionError("expected ValueError for null + 1")


def test_none_json_str_binds_context_to_true_undefined() -> None:
    """None (no input at all) must produce a true JSONata Undefined context,
    distinct from an explicit JSON null context. The bare context reference
    `$` distinguishes them: with Undefined context, `$` evaluates to
    Undefined (this method returns None); with a null context, `$` evaluates
    to the null value itself (this method returns the string "null")."""
    expr = jsonatapy.compile("$")
    assert expr.evaluate_json_or_none(None) is None
    assert expr.evaluate_json_or_none("null") == "null"
