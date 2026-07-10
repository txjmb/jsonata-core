"""CLI-only error-message presentation for evaluate_json_or_none() failures.

jsonatapy.compile()'s ValueError messages are already fully formatted
(matching Rust's ParserError::display_message()) and need no processing --
callers print str(exc) directly. JsonataExpression.evaluate_json_or_none()'s
ValueError messages are the raw unwrapped text (matching Rust's
EvaluatorError::message()): already spec-coded ("T2002: ...") when
applicable, otherwise plain. This module adds the CLI's "error: " prefix
only when it's not already coded -- mirrors src/bin/jsonata/error_format.rs
exactly.
"""

from __future__ import annotations

import re

_CODE_PREFIX_RE = re.compile(r"^[TDUS]\d{4}:")


def format_evaluation_error(message: str) -> str:
    """Formats an evaluate_json_or_none()-raised ValueError's message for
    CLI stderr output."""
    if _CODE_PREFIX_RE.match(message):
        return message
    return f"error: {message}"
