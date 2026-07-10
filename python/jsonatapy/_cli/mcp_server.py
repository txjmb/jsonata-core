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

# Curated JSONata function reference, grouped by category. Every function
# name here is confirmed implemented in this crate (grep src/evaluator.rs's
# dispatch match arms for the quoted name to re-verify if this list is ever
# extended). Kept concise deliberately -- this content is reused as-is for
# the Phase 3 agentic study's "jsonata+docs" condition cheatsheet, where
# token cost directly affects the study's own measurements.
_REFERENCE: dict[str, str] = {
    "string": (
        "String functions: $string(v) convert to string, $length(s), "
        "$substring(s,start,len?), $substringBefore(s,chars), "
        "$substringAfter(s,chars), $uppercase(s), $lowercase(s), $trim(s), "
        "$pad(s,width,char?), $contains(s,pattern), $split(s,sep,limit?), "
        "$join(arr,sep?), $match(s,pattern,limit?), $replace(s,pattern,repl,limit?)."
    ),
    "numeric": (
        "Numeric functions: $number(v), $abs(n), $floor(n), $ceil(n), "
        "$round(n,precision?), $power(base,exp), $sqrt(n), "
        "$formatNumber(n,picture,options?), $formatBase(n,radix?), "
        "$formatInteger(n,picture), $parseInteger(s,picture)."
    ),
    "aggregation": (
        "Aggregation functions (operate on arrays): $sum(arr), $max(arr), "
        "$min(arr), $average(arr), $count(arr)."
    ),
    "array": (
        "Array functions: $append(arr1,arr2), $count(arr), $distinct(arr), "
        "$reverse(arr), $shuffle(arr), $sort(arr,comparator?), $zip(arr1,arr2,...)."
    ),
    "object": (
        "Object functions: $keys(obj), $lookup(obj,key), $merge(arr_of_objs), "
        "$spread(obj), $sift(obj,predicate), $each(obj,function)."
    ),
    "higher-order": (
        "Higher-order functions: $map(arr,function), $filter(arr,predicate), "
        "$reduce(arr,function,init?), $single(arr,predicate), $sift(obj,predicate), "
        "$each(obj,function)."
    ),
    "boolean": "Boolean functions: $boolean(v), $not(v), $exists(v).",
    "datetime": (
        "Date/time functions: $now(picture?,timezone?), $millis(), "
        "$fromMillis(n,picture?,timezone?), $toMillis(s,picture?)."
    ),
    "encoding": (
        "Encoding functions: $base64encode(s), $base64decode(s), "
        "$encodeUrl(s), $encodeUrlComponent(s), $decodeUrl(s), $decodeUrlComponent(s)."
    ),
    "misc": (
        "Other functions: $type(v) returns the JSONata type name, "
        "$error(msg) raises a custom error, $assert(cond,msg), "
        "$eval(expr_str,context?) evaluates a JSONata expression given as a string."
    ),
}


def _explain(topic: str | None) -> str:
    if topic is None:
        lines = ["JSONata function reference. Call explain(topic=<name>) for details."]
        for category, summary in _REFERENCE.items():
            lines.append(f"- {category}: {summary}")
        return "\n".join(lines)

    normalized = topic.strip().lower()
    if normalized in _REFERENCE:
        return _REFERENCE[normalized]

    available = ", ".join(_REFERENCE.keys())
    return f"unknown topic {topic!r}. Available topics: {available}"


def create_server() -> FastMCP[Any]:
    mcp: FastMCP[Any] = FastMCP(name="jsonatapy")

    @mcp.tool(run_in_thread=False)
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

    @mcp.tool(run_in_thread=False)
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

    @mcp.tool(run_in_thread=False)
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

    @mcp.tool(run_in_thread=False)
    def explain(topic: str | None = None) -> str:
        """Get curated JSONata function reference material.

        Args:
            topic: A category name (e.g. "string", "numeric", "array",
                "object", "higher-order", "boolean", "datetime", "encoding",
                "misc"). Omit to get the full category index.

        Returns:
            Reference text for the requested topic, the full index if no
            topic given, or a list of available topics if the given topic
            isn't recognized.
        """
        return _explain(topic)

    return mcp


def serve(http: bool, port: int | None) -> None:
    """Runs the MCP server. stdio transport by default; HTTP if http=True."""
    server = create_server()
    if http:
        server.run(transport="http", host="127.0.0.1", port=port or 8000)
    else:
        server.run()
