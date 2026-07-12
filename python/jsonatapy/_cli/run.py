"""Core evaluate-mode CLI logic: argument parsing, evaluation, output.

Mirrors src/bin/jsonata/main.rs in the Rust CLI. See study/cli_spec.md for
the full flag/exit-code contract both implementations must satisfy.
"""

from __future__ import annotations

import argparse
import json
import math
import sys

import jsonatapy

from .bindings import BindingError, parse_bindings
from .error_format import format_evaluation_error
from .resolve import (
    ExpressionFile,
    ExpressionInline,
    InputFile,
    InputNull,
    InputStdin,
    ResolveError,
    resolve,
)


def build_parser() -> argparse.ArgumentParser:
    """Builds the argparse parser for evaluate-mode (`jsonatapy [OPTIONS] [EXPR] [FILE]`).

    argparse's default error handling (unknown/malformed flags) already
    prints a usage message to stderr and calls sys.exit(2) -- matching this
    CLI's exit-code-2 usage-error convention with no extra code needed.
    """
    parser = argparse.ArgumentParser(
        prog="jsonatapy",
        description="Evaluate JSONata expressions against JSON data",
    )
    parser.add_argument("-c", "--compact", action="store_true", help="Compact JSON output")
    parser.add_argument(
        "-r", "--raw-output", action="store_true", help="Print string results without quotes"
    )
    parser.add_argument(
        "-n", "--null-input", action="store_true", help="Don't read input; $ is Undefined"
    )
    parser.add_argument(
        "-f",
        "--from-file",
        metavar="FILE",
        default=None,
        help="Read the expression from FILE instead of the first positional argument",
    )
    parser.add_argument(
        "--arg",
        action="append",
        default=[],
        metavar="NAME=VALUE",
        help="Bind $NAME to a string value (repeatable)",
    )
    parser.add_argument(
        "--argjson",
        action="append",
        default=[],
        metavar="NAME=JSON",
        help="Bind $NAME to a parsed JSON value (repeatable)",
    )
    parser.add_argument(
        "-V",
        "--version",
        action="version",
        version=f"jsonatapy {jsonatapy.__version__}",
    )
    parser.add_argument(
        "positional1",
        nargs="?",
        default=None,
        metavar="EXPRESSION_OR_FILE",
        help="The JSONata expression (or, with --from-file, the input data file)",
    )
    parser.add_argument(
        "positional2",
        nargs="?",
        default=None,
        metavar="FILE",
        help="The input data file (used only when --from-file supplies the expression)",
    )
    return parser


def _read_expression(expr_source: ExpressionInline | ExpressionFile) -> str | int:
    """Returns the expression text, or an int exit code on failure."""
    if isinstance(expr_source, ExpressionInline):
        return expr_source.text
    try:
        with open(expr_source.path, encoding="utf-8") as f:
            return f.read()
    except OSError as e:
        print(
            f"error: could not read expression file {expr_source.path}: {e}",
            file=sys.stderr,
        )
        return 2


def _reject_constant(token: str) -> float:
    raise ValueError(f"{token} is not valid JSON")


def _finite_float(text: str) -> float:
    value = float(text)
    if not math.isfinite(value):
        raise ValueError(f"{text} is not valid JSON")
    return value


def _read_input_json(input_source: InputStdin | InputFile | InputNull) -> str | int | None:
    """Returns the raw input JSON text, None for InputNull (no input document
    at all -- binds $ to a true Undefined via evaluate_json_or_none), or an
    int exit code on failure. Does NOT parse the JSON itself -- only
    validates it, since evaluate_json_or_none() takes the raw text directly."""
    if isinstance(input_source, InputNull):
        return None
    if isinstance(input_source, InputStdin):
        raw = sys.stdin.read()
    else:  # InputFile
        try:
            with open(input_source.path, encoding="utf-8") as f:
                raw = f.read()
        except OSError as e:
            print(
                f"error: could not read input file {input_source.path}: {e}",
                file=sys.stderr,
            )
            return 2
    try:
        json.loads(raw, parse_constant=_reject_constant, parse_float=_finite_float)  # validate only
    except ValueError as e:
        print(f"error: invalid JSON input: {e}", file=sys.stderr)
        return 3
    return raw


def run(argv: list[str]) -> int:
    args = build_parser().parse_args(argv)

    try:
        expr_source, input_source = resolve(
            args.from_file, args.positional1, args.positional2, args.null_input
        )
    except ResolveError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    try:
        var_bindings = parse_bindings(args.arg, args.argjson)
    except BindingError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    expression = _read_expression(expr_source)
    if isinstance(expression, int):
        return expression

    input_json = _read_input_json(input_source)
    if isinstance(input_json, int):
        return input_json

    try:
        expr = jsonatapy.compile(expression)
    except ValueError as e:
        print(str(e), file=sys.stderr)
        return 1

    try:
        result_json = expr.evaluate_json_or_none(input_json, var_bindings or None)
    except ValueError as e:
        print(format_evaluation_error(str(e)), file=sys.stderr)
        return 1

    if result_json is None:
        return 0  # Undefined result: print nothing

    print_result(result_json, args.compact, args.raw_output)
    return 0


def print_result(result_json: str, compact: bool, raw_output: bool) -> None:
    """Prints a non-undefined evaluate_json_or_none() result per -c/-r flags.

    `result_json` is already valid JSON text -- this function only handles
    presentation (raw string unquoting, pretty vs compact), it never
    re-evaluates or re-validates the JSON.
    """
    value = json.loads(result_json)

    if raw_output and isinstance(value, str):
        print(value)
        return

    if compact:
        print(json.dumps(value, separators=(",", ":")))
    else:
        print(json.dumps(value, indent=2))
