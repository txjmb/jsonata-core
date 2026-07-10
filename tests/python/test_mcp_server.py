"""In-memory tests for the jsonatapy MCP server -- uses FastMCP's own
in-process Client, no subprocess or network involved. Requires the `mcp`
extra (fastmcp) to be installed; skipped entirely if it isn't.
"""

from __future__ import annotations

import pytest

fastmcp = pytest.importorskip("fastmcp")

from fastmcp import Client  # noqa: E402
from jsonatapy._cli.mcp_server import create_server  # noqa: E402


async def test_evaluate_tool_returns_json_result() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate", {"expression": "a + b", "data": '{"a": 1, "b": 2}'}
        )
        assert result.data == "3"


async def test_evaluate_tool_with_bindings() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate",
            {"expression": "$x * 2", "data": "{}", "bindings": {"x": 5}},
        )
        assert result.data == "10"


async def test_evaluate_tool_undefined_result_returns_empty_string() -> None:
    """MCP tool return types must be JSON-representable, so the
    Undefined-vs-null distinction evaluate_json_or_none() provides in
    Python (None vs "null") is re-flattened to an empty string here (None
    isn't a valid MCP tool string return) -- an empty string is
    distinguishable from the text "null" for a caller checking the result."""
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("evaluate", {"expression": "nonexistent", "data": "{}"})
        assert result.data == ""


async def test_evaluate_tool_raises_tool_error_on_evaluation_failure() -> None:
    from fastmcp.exceptions import ToolError

    server = create_server()
    async with Client(server) as client:
        with pytest.raises(ToolError):
            await client.call_tool("evaluate", {"expression": "null + 1", "data": "{}"})


async def test_validate_tool_reports_ok_for_valid_expression() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("validate", {"expression": "a.b.c"})
        assert result.data == {"ok": True}


async def test_validate_tool_reports_error_for_invalid_expression() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("validate", {"expression": "a["})
        assert result.data["ok"] is False
        assert "Parse error" in result.data["error"]


async def test_evaluate_batch_runs_multiple_expressions() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate_batch",
            {"expressions": ["a", "b", "a + b"], "data": '{"a": 1, "b": 2}'},
        )
        assert result.data == ["1", "2", "3"]


async def test_evaluate_batch_reports_per_expression_errors_without_failing_the_batch() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate_batch",
            {"expressions": ["a", "null + 1"], "data": '{"a": 1}'},
        )
        assert result.data[0] == "1"
        assert "T2002:" in result.data[1]


async def test_evaluate_batch_compile_error_is_not_double_prefixed() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool(
            "evaluate_batch",
            {"expressions": ["a["], "data": "{}"},
        )
        assert result.data[0] == "Parse error: Unexpected token: Eof"


async def test_explain_with_no_topic_returns_function_index() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("explain", {"topic": None})
        assert "$sum" in result.data
        assert "$filter" in result.data


async def test_explain_with_specific_topic_returns_that_section() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("explain", {"topic": "string"})
        assert "$uppercase" in result.data
        assert "$substring" in result.data


async def test_explain_with_unknown_topic_lists_available_topics() -> None:
    server = create_server()
    async with Client(server) as client:
        result = await client.call_tool("explain", {"topic": "not-a-real-topic"})
        assert "unknown topic" in result.data.lower()
