"""Determines the expression source and input source from parsed CLI
arguments. Single source of truth for how positional arguments,
--from-file, and --null-input interact -- mirrors
src/bin/jsonata/resolve.rs::resolve() in the Rust CLI exactly, so both
implementations agree on every case in study/cli_fixtures.json.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class ExpressionInline:
    """The expression text came directly from a positional argument."""

    text: str


@dataclass(frozen=True)
class ExpressionFile:
    """The expression text should be read from this file path (--from-file)."""

    path: str


ExpressionSource = ExpressionInline | ExpressionFile


@dataclass(frozen=True)
class InputStdin:
    """Input JSON should be read from stdin."""


@dataclass(frozen=True)
class InputFile:
    """Input JSON should be read from this file path."""

    path: str


@dataclass(frozen=True)
class InputNull:
    """No input is read; the evaluation context is null (see this plan's
    Global Constraints for why this is null, not JSONata Undefined)."""


InputSource = InputStdin | InputFile | InputNull


class ResolveError(Exception):
    """Raised when CLI arguments cannot be resolved into an expression/input source."""


def resolve(
    from_file: str | None,
    positional1: str | None,
    positional2: str | None,
    null_input: bool,
) -> tuple[ExpressionSource, InputSource]:
    """Resolves the expression source and input source from parsed CLI args."""
    data_file: str | None
    expr_source: ExpressionSource

    if from_file is not None:
        if positional2 is not None:
            raise ResolveError(
                "with --from-file, only one positional argument (the input file) is allowed"
            )
        expr_source = ExpressionFile(from_file)
        data_file = positional1
    elif positional1 is not None:
        expr_source = ExpressionInline(positional1)
        data_file = positional2
    else:
        raise ResolveError("missing required argument: EXPRESSION (or use --from-file)")

    if null_input:
        if data_file is not None:
            raise ResolveError("--null-input cannot be combined with an input file argument")
        return expr_source, InputNull()

    if data_file is not None:
        return expr_source, InputFile(data_file)
    return expr_source, InputStdin()
