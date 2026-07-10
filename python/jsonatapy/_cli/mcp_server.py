"""FastMCP server exposing JSONata evaluation as MCP tools for agentic use.

Four tools: evaluate, validate, explain (Task 10), evaluate_batch. See the
design spec's Phase 2 section for the tool contract. This module imports
`fastmcp` at module level -- callers (jsonatapy.__main__) must catch
ImportError around importing THIS MODULE, not around individual calls,
since fastmcp is an optional dependency (the `mcp` extra).
"""

from __future__ import annotations

from typing import Any

from fastmcp import FastMCP
from fastmcp.exceptions import ToolError

import jsonatapy

from .error_format import format_evaluation_error


def create_server() -> FastMCP[Any]:
    mcp: FastMCP[Any] = FastMCP(name="jsonatapy")

    @mcp.tool
    def evaluate(expression: str, data: str, bindings: dict[str, Any] | None = None) -> str:
        """Evaluate a JSONata expression against a JSON document.

        Args:
            expression: A JSONata expression string.
            data: The input document as a JSON string.
            bindings: Optional variable bindings (name -> JSON-compatible value).

        Returns:
            The result as a JSON string. Empty string means the JSONata
            result was Undefined (no match) -- distinct from the text
            "null", which means an explicit null result.
        """
        try:
            expr = jsonatapy.compile(expression)
        except ValueError as e:
            raise ToolError(str(e)) from e
        try:
            result = expr.evaluate_json_or_none(data, bindings)
        except ValueError as e:
            raise ToolError(format_evaluation_error(str(e))) from e
        return result if result is not None else ""

    @mcp.tool
    def validate(expression: str) -> dict[str, Any]:
        """Check whether a JSONata expression parses without evaluating it.

        Args:
            expression: A JSONata expression string.

        Returns:
            {"ok": True} if the expression parses, or
            {"ok": False, "error": "<message>"} if it doesn't. The error
            message has no structured position field -- see this plan's
            Global Constraints for why.
        """
        try:
            jsonatapy.compile(expression)
        except ValueError as e:
            return {"ok": False, "error": str(e)}
        return {"ok": True}

    @mcp.tool
    def evaluate_batch(expressions: list[str], data: str) -> list[str]:
        """Evaluate multiple JSONata expressions against the same document
        in one call, avoiding N round-trips.

        Args:
            expressions: A list of JSONata expression strings.
            data: The input document as a JSON string, shared by all expressions.

        Returns:
            One result per expression, in order (empty string for an
            Undefined result). A failed expression's entry is its
            formatted error message (same format as `evaluate`'s ToolError
            text) rather than aborting the whole batch -- callers
            distinguish a result from an error by attempting to
            json.loads() it, or by cross-referencing against `validate` first.
        """
        results: list[str] = []
        for expression in expressions:
            try:
                expr = jsonatapy.compile(expression)
                result = expr.evaluate_json_or_none(data)
                results.append(result if result is not None else "")
            except ValueError as e:
                results.append(format_evaluation_error(str(e)))
        return results

    return mcp


def serve(http: bool, port: int | None) -> None:
    """Runs the MCP server. stdio transport by default; HTTP if http=True."""
    server = create_server()
    if http:
        server.run(transport="http", host="127.0.0.1", port=port or 8000)
    else:
        server.run()
