"""Unit tests for jsonatapy._cli.bindings."""

from __future__ import annotations

import pytest
from jsonatapy._cli.bindings import BindingError, parse_bindings


def test_arg_binds_a_string() -> None:
    b = parse_bindings(["region=us"], [])
    assert b == {"region": "us"}


def test_argjson_binds_a_parsed_value() -> None:
    b = parse_bindings([], ["limit=42"])
    assert b == {"limit": 42}


def test_arg_without_equals_is_an_error() -> None:
    with pytest.raises(BindingError):
        parse_bindings(["justaname"], [])


def test_argjson_with_invalid_json_is_an_error() -> None:
    with pytest.raises(BindingError):
        parse_bindings([], ["x=not json"])


def test_arg_value_may_contain_equals_signs() -> None:
    b = parse_bindings(["eq=a=b"], [])
    assert b == {"eq": "a=b"}
