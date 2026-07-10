"""Core evaluate-mode CLI logic: argument parsing, evaluation, output.

Mirrors src/bin/jsonata/main.rs in the Rust CLI. See study/cli_spec.md for
the full flag/exit-code contract both implementations must satisfy.
"""

from __future__ import annotations

import argparse

import jsonatapy


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
        "-n", "--null-input", action="store_true", help="Don't read input; $ is null"
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


def run(argv: list[str]) -> int:
    """Parses argv and returns the process exit code. Full logic added starting Task 4."""
    build_parser().parse_args(argv)
    return 0
