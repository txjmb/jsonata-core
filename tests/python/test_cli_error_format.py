"""Unit tests for jsonatapy._cli.error_format."""

from __future__ import annotations

from jsonatapy._cli.error_format import format_evaluation_error


def test_coded_evaluation_error_passes_through_unwrapped() -> None:
    msg = "T2002: The left side of the + operator must evaluate to a number"
    assert format_evaluation_error(msg) == msg


def test_uncoded_evaluation_error_gets_error_prefix() -> None:
    assert format_evaluation_error("Unknown function: undefinedvar") == (
        "error: Unknown function: undefinedvar"
    )
