"""Unit tests for jsonatapy._cli.resolve -- the single source of truth for
how CLI positional arguments, --from-file, and --null-input interact.
Mirrors resolve.rs's own test suite in the Rust CLI exactly, case for case.
"""

from __future__ import annotations

import pytest
from jsonatapy._cli.resolve import (
    ExpressionFile,
    ExpressionInline,
    InputFile,
    InputNull,
    InputStdin,
    ResolveError,
    resolve,
)


def test_plain_expression_and_stdin() -> None:
    expr, inp = resolve(from_file=None, positional1="name", positional2=None, null_input=False)
    assert expr == ExpressionInline("name")
    assert inp == InputStdin()


def test_plain_expression_and_file() -> None:
    expr, inp = resolve(
        from_file=None, positional1="name", positional2="data.json", null_input=False
    )
    assert expr == ExpressionInline("name")
    assert inp == InputFile("data.json")


def test_missing_expression_is_an_error() -> None:
    with pytest.raises(ResolveError):
        resolve(from_file=None, positional1=None, positional2=None, null_input=False)


def test_null_input_with_no_data_file_is_null_source() -> None:
    expr, inp = resolve(from_file=None, positional1="$now()", positional2=None, null_input=True)
    assert expr == ExpressionInline("$now()")
    assert inp == InputNull()


def test_null_input_with_data_file_is_an_error() -> None:
    with pytest.raises(ResolveError):
        resolve(from_file=None, positional1="name", positional2="data.json", null_input=True)


def test_from_file_shifts_positional1_to_the_data_file() -> None:
    expr, inp = resolve(
        from_file="expr.jsonata", positional1="data.json", positional2=None, null_input=False
    )
    assert expr == ExpressionFile("expr.jsonata")
    assert inp == InputFile("data.json")


def test_from_file_with_no_positionals_reads_stdin() -> None:
    expr, inp = resolve(
        from_file="expr.jsonata", positional1=None, positional2=None, null_input=False
    )
    assert expr == ExpressionFile("expr.jsonata")
    assert inp == InputStdin()


def test_from_file_with_two_positionals_is_an_error() -> None:
    with pytest.raises(ResolveError):
        resolve(
            from_file="expr.jsonata",
            positional1="extra1",
            positional2="extra2",
            null_input=False,
        )
