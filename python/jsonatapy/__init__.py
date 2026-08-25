"""
jsonatapy - High-performance Python implementation of JSONata

JSONata is a lightweight query and transformation language for JSON data.

Example:
    >>> import jsonatapy
    >>> data = {"name": "World"}
    >>> result = jsonatapy.evaluate('"Hello, " & name', data)
    >>> print(result)
    "Hello, World"

    >>> # Compile once, evaluate many times
    >>> expr = jsonatapy.compile("orders[price > 100].product")
    >>> result = expr.evaluate(data)
"""

import json as _json
from collections.abc import Callable
from typing import Any

from ._jsonatapy import (
    JsonataData as _JsonataData,
)
from ._jsonatapy import (
    JsonataExpression as _JsonataExpression,
)
from ._jsonatapy import (
    __jsonata_version__,
    __version__,
)

# Private test hooks (not public API): runtime toggle forcing the
# tree-walking evaluator; seeded from JSONATAPY_FORCE_TREE_WALKER at import.
from ._jsonatapy import _get_force_tree_walker as _get_force_tree_walker
from ._jsonatapy import _set_force_tree_walker as _set_force_tree_walker
from ._jsonatapy import (
    compile as _compile,
)
from ._jsonatapy import (
    evaluate as _evaluate,
)

__all__ = [
    "JsonataData",
    "JsonataExpression",
    "__jsonata_version__",
    "__version__",
    "compile",
    "evaluate",
    "json_dumps",
    "json_loads",
]

# Use orjson when available for faster JSON serialization (3x faster than stdlib).
# These are exposed for users who want to use the evaluate_json() path with
# optimal serialization performance.
try:
    import orjson as _orjson

    def json_dumps(obj: Any) -> str:
        """Serialize to JSON string, using orjson if available."""
        return _orjson.dumps(obj).decode("utf-8")

    def json_loads(s: str | bytes) -> Any:
        """Deserialize JSON string, using orjson if available."""
        return _orjson.loads(s)

except ImportError:

    def json_dumps(obj: Any) -> str:
        """Serialize to JSON string, using stdlib json."""
        return _json.dumps(obj)

    def json_loads(s: str | bytes) -> Any:
        """Deserialize JSON string, using stdlib json."""
        return _json.loads(s)


class JsonataData:
    """
    Pre-converted data handle for efficient repeated evaluation.

    Convert Python data to an internal representation once, then reuse it
    across multiple evaluations to avoid repeated Python-to-Rust conversion overhead.

    Example:
        >>> data = JsonataData({"orders": [{"price": 150}, {"price": 50}]})
        >>> expr = compile("orders[price > 100]")
        >>> result = expr.evaluate_with_data(data)
    """

    def __init__(self, data: Any) -> None:
        """
        Create a JsonataData handle from a Python object.

        Args:
            data: The data to pre-convert (typically a dict or list)
        """
        self._data = _JsonataData(data)

    @classmethod
    def from_json(cls, json_str: str) -> "JsonataData":
        """
        Create a JsonataData handle from a JSON string (fastest path).

        Args:
            json_str: Input data as a JSON string

        Returns:
            A JsonataData handle

        Raises:
            ValueError: If the JSON string is invalid
        """
        obj = cls.__new__(cls)
        obj._data = _JsonataData.from_json(json_str)
        return obj


class JsonataExpression:
    """
    A compiled JSONata expression.

    This class wraps the Rust-implemented expression compiler and evaluator.

    Attributes:
        _expr: The underlying Rust expression object

    Example:
        >>> expr = JsonataExpression.compile("$.name")
        >>> result = expr.evaluate({"name": "Alice"})
        >>> print(result)
        "Alice"
    """

    def __init__(self, expr: _JsonataExpression) -> None:
        """
        Initialize a JsonataExpression.

        Args:
            expr: The compiled Rust expression object

        Note:
            Users should typically use the `compile()` function instead
            of instantiating this class directly.
        """
        self._expr = expr

    def evaluate(
        self,
        data: Any,
        bindings: dict[str, Any] | None = None,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> Any:
        """
        Evaluate this expression against data.

        Args:
            data: The data to query/transform (typically a dict or list)
            bindings: Optional additional variable bindings
            timeout: Maximum evaluation time in milliseconds (raises ValueError
                with a D1012 code on timeout). Overrides any default set via
                `compile(timeout=...)` for this call only.
            max_stack_depth: Maximum recursion stack depth (raises ValueError
                with a D1011 code when exceeded). Overrides any compile-time default.
            max_sequence_length: Maximum length of a query-result sequence
                (map/filter/wildcard/descendants/etc; raises ValueError with a
                D2015 code when exceeded). Overrides any compile-time default.

        Returns:
            The result of evaluating the expression

        Raises:
            ValueError: If evaluation fails, or a guardrail is exceeded

        Example:
            >>> expr = compile("orders[price > 100]")
            >>> data = {"orders": [{"price": 150}, {"price": 50}]}
            >>> result = expr.evaluate(data)
            >>> print(len(result))
            1
        """
        return self._expr.evaluate(data, bindings, timeout, max_stack_depth, max_sequence_length)

    def register(self, name: str, func: Callable[..., Any]) -> "JsonataExpression":
        """
        Register a Python function callable from the expression as ``$name(...)``.

        The function receives the (already-evaluated) arguments as positional
        Python values and must return a JSON-compatible value **synchronously**.
        This is the equivalent of jsonata-js's ``registerFunction`` — use it to
        expose enrichment/lookup, formatting, or scoring logic to an expression.

        Host functions resolve after the expression's own bindings/lambdas and
        before built-ins. A name that collides with a built-in is rejected; use
        :meth:`register_override` to replace a built-in deliberately.

        Note:
            The evaluator is synchronous, so an ``async def`` (which returns a
            coroutine) is rejected at call time. For async I/O, await it outside
            jsonata and pass the result in via ``bindings``.

        Args:
            name: The function name, called as ``$name(...)`` in the expression.
            func: A callable ``(*args) -> JSON-compatible value``.

        Returns:
            self, to allow chaining.

        Raises:
            TypeError: If ``func`` is not callable.
            ValueError: If ``name`` collides with a built-in function.

        Example:
            >>> expr = compile("$greet(name)")
            >>> expr.register("greet", lambda n: f"hello {n}")
            >>> expr.evaluate({"name": "Ada"})
            'hello Ada'
        """
        self._expr.register(name, func)
        return self

    def register_override(self, name: str, func: Callable[..., Any]) -> "JsonataExpression":
        """
        Register a Python function that deliberately replaces a built-in.

        The two legitimate uses are determinism injection for the impure
        built-ins (``$now``, ``$millis``, ``$random``) — e.g. a frozen clock for
        reproducible output — and sandboxing (disabling ``$eval``). Overriding a
        built-in that participates in the compiled fast path is rejected.

        Args:
            name: The built-in name to replace (e.g. ``"now"``).
            func: A callable ``(*args) -> JSON-compatible value``.

        Returns:
            self, to allow chaining.

        Raises:
            TypeError: If ``func`` is not callable.
            ValueError: If the built-in cannot be safely overridden.

        Example:
            >>> expr = compile("$now()")
            >>> expr.register_override("now", lambda: "2020-01-01T00:00:00.000Z")
            >>> expr.evaluate(None)
            '2020-01-01T00:00:00.000Z'
        """
        self._expr.register_override(name, func)
        return self

    def evaluate_json(
        self,
        json_str: str,
        bindings: dict[str, Any] | None = None,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> str:
        """
        Evaluate this expression with JSON string input/output (faster for large data).

        This method avoids Python↔Rust conversion overhead by accepting and returning
        JSON strings directly. This is significantly faster for large datasets (10-50x speedup).

        Args:
            json_str: Input data as a JSON string
            bindings: Optional additional variable bindings
            timeout: Maximum evaluation time in milliseconds (raises ValueError
                with a D1012 code on timeout). Overrides any default set via
                `compile(timeout=...)` for this call only.
            max_stack_depth: Maximum recursion stack depth (raises ValueError
                with a D1011 code when exceeded). Overrides any compile-time default.
            max_sequence_length: Maximum length of a query-result sequence
                (map/filter/wildcard/descendants/etc; raises ValueError with a
                D2015 code when exceeded). Overrides any compile-time default.

        Returns:
            The result as a JSON string

        Raises:
            ValueError: If JSON parsing or evaluation fails, or a guardrail is exceeded

        Example:
            >>> import json
            >>> expr = compile("items[price > 100]")
            >>> json_str = json.dumps({"items": [{"price": 150}, {"price": 50}]})
            >>> result_str = expr.evaluate_json(json_str)
            >>> result = json.loads(result_str)
            >>> print(len(result))
            1

        Note:
            For large datasets (1000+ items), this can be 10-50x faster than evaluate()
            due to avoiding the Python↔Rust object conversion overhead.
        """
        return self._expr.evaluate_json(
            json_str, bindings, timeout, max_stack_depth, max_sequence_length
        )

    def evaluate_json_or_none(
        self,
        json_str: str | None,
        bindings: dict[str, Any] | None = None,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> str | None:
        """
        Evaluate with JSON string input, distinguishing Undefined from null.

        Unlike evaluate_json(), which serializes both a JSONata Undefined
        result and an explicit JSON null result to the same text "null",
        this method returns None (the Python value) for Undefined and the
        string "null" for an explicit null result.

        Args:
            json_str: Input data as a JSON string, or None for no input
                document at all -- the top-level context (`$`) is then a
                true JSONata Undefined, distinct from passing "null" (an
                explicit JSON null context).
            bindings: Optional additional variable bindings
            timeout: Maximum evaluation time in milliseconds (raises ValueError
                with a D1012 code on timeout). Overrides any default set via
                `compile(timeout=...)` for this call only.
            max_stack_depth: Maximum recursion stack depth (raises ValueError
                with a D1011 code when exceeded). Overrides any compile-time default.
            max_sequence_length: Maximum length of a query-result sequence
                (map/filter/wildcard/descendants/etc; raises ValueError with a
                D2015 code when exceeded). Overrides any compile-time default.

        Returns:
            None if the result is JSONata Undefined, otherwise the result
            as a JSON string (e.g. "null" for an explicit null result).

        Raises:
            ValueError: If JSON parsing or evaluation fails, or a guardrail is exceeded

        Example:
            >>> expr = compile("nonexistent")
            >>> expr.evaluate_json_or_none('{"a": 1}') is None
            True
            >>> expr2 = compile("a")
            >>> expr2.evaluate_json_or_none('{"a": null}')
            'null'
        """
        return self._expr.evaluate_json_or_none(
            json_str, bindings, timeout, max_stack_depth, max_sequence_length
        )

    def evaluate_with_data(
        self,
        data: "JsonataData",
        bindings: dict[str, Any] | None = None,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> Any:
        """
        Evaluate with pre-converted data (fastest for repeated evaluation).

        Args:
            data: A JsonataData handle (pre-converted data)
            bindings: Optional additional variable bindings
            timeout: Maximum evaluation time in milliseconds (raises ValueError
                with a D1012 code on timeout). Overrides any default set via
                `compile(timeout=...)` for this call only.
            max_stack_depth: Maximum recursion stack depth (raises ValueError
                with a D1011 code when exceeded). Overrides any compile-time default.
            max_sequence_length: Maximum length of a query-result sequence
                (map/filter/wildcard/descendants/etc; raises ValueError with a
                D2015 code when exceeded). Overrides any compile-time default.

        Returns:
            The result of evaluating the expression

        Raises:
            ValueError: If evaluation fails, or a guardrail is exceeded

        Example:
            >>> data = JsonataData({"orders": [{"price": 150}, {"price": 50}]})
            >>> expr = compile("orders[price > 100]")
            >>> result = expr.evaluate_with_data(data)
        """
        return self._expr.evaluate_with_data(
            data._data, bindings, timeout, max_stack_depth, max_sequence_length
        )

    def evaluate_data_to_json(
        self,
        data: "JsonataData",
        bindings: dict[str, Any] | None = None,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> str:
        """
        Evaluate with pre-converted data, return JSON string (zero-overhead output).

        This is the fastest evaluation path: no Python-to-Rust conversion on input
        (data is pre-converted), and no Rust-to-Python conversion on output (returns
        a JSON string).

        Args:
            data: A JsonataData handle (pre-converted data)
            bindings: Optional additional variable bindings
            timeout: Maximum evaluation time in milliseconds (raises ValueError
                with a D1012 code on timeout). Overrides any default set via
                `compile(timeout=...)` for this call only.
            max_stack_depth: Maximum recursion stack depth (raises ValueError
                with a D1011 code when exceeded). Overrides any compile-time default.
            max_sequence_length: Maximum length of a query-result sequence
                (map/filter/wildcard/descendants/etc; raises ValueError with a
                D2015 code when exceeded). Overrides any compile-time default.

        Returns:
            The result as a JSON string

        Raises:
            ValueError: If evaluation fails, or a guardrail is exceeded

        Example:
            >>> import json
            >>> data = JsonataData.from_json('{"orders": [{"price": 150}, {"price": 50}]}')
            >>> expr = compile("orders[price > 100]")
            >>> result_str = expr.evaluate_data_to_json(data)
            >>> result = json.loads(result_str)
        """
        return self._expr.evaluate_data_to_json(
            data._data, bindings, timeout, max_stack_depth, max_sequence_length
        )

    @classmethod
    def compile(
        cls,
        expression: str,
        *,
        timeout: int | None = None,
        max_stack_depth: int | None = None,
        max_sequence_length: int | None = None,
    ) -> "JsonataExpression":
        """
        Compile a JSONata expression.

        This is an alternative constructor that compiles an expression string.

        Args:
            expression: A JSONata expression string
            timeout: Default max evaluation time in milliseconds for all
                evaluate*() calls on this expression (can be overridden per-call).
            max_stack_depth: Default max recursion stack depth (can be overridden per-call).
            max_sequence_length: Default max query-result sequence length
                (can be overridden per-call).

        Returns:
            A compiled JsonataExpression

        Raises:
            ValueError: If the expression cannot be parsed

        Example:
            >>> expr = JsonataExpression.compile("$.name")
            >>> expr = JsonataExpression.compile("$.name", timeout=5000)
        """
        return cls(_compile(expression, timeout, max_stack_depth, max_sequence_length))


def compile(
    expression: str,
    *,
    timeout: int | None = None,
    max_stack_depth: int | None = None,
    max_sequence_length: int | None = None,
) -> JsonataExpression:
    """
    Compile a JSONata expression into an executable form.

    Args:
        expression: A JSONata query/transformation expression string
        timeout: Default max evaluation time in milliseconds for all
            evaluate*() calls on this expression (can be overridden per-call).
        max_stack_depth: Default max recursion stack depth (can be overridden per-call).
        max_sequence_length: Default max query-result sequence length
            (can be overridden per-call).

    Returns:
        A compiled JsonataExpression that can be evaluated multiple times

    Raises:
        ValueError: If the expression cannot be parsed

    Example:
        >>> expr = compile("orders[price > 100].product")
        >>> result = expr.evaluate(data)
        >>> expr = compile("$sum(items.price)", timeout=5000)

    Note:
        Compiling an expression once and evaluating it multiple times
        is more efficient than calling `evaluate()` repeatedly with
        the same expression string.
    """
    try:
        return JsonataExpression(
            _compile(expression, timeout, max_stack_depth, max_sequence_length)
        )
    except UnicodeEncodeError as exc:
        # A Python str may hold an unpaired surrogate; a Rust String may not,
        # so such an expression cannot cross the boundary at all and PyO3
        # raises a bare UnicodeEncodeError. Re-raise as ValueError so the
        # library keeps one error type, and say why rather than leaking a
        # codec message.
        raise ValueError(
            "Expression contains an unpaired surrogate and cannot be compiled: "
            f"{exc.object[exc.start : exc.end]!r} at position {exc.start}"
        ) from exc


def evaluate(
    expression: str,
    data: Any,
    bindings: dict[str, Any] | None = None,
    *,
    timeout: int | None = None,
    max_stack_depth: int | None = None,
    max_sequence_length: int | None = None,
) -> Any:
    """
    Compile and evaluate a JSONata expression in one step.

    This is a convenience function for one-off evaluations.
    For repeated evaluations, use `compile()` instead.

    Args:
        expression: A JSONata query/transformation expression string
        data: The data to query/transform (typically a dict or list)
        bindings: Optional additional variable bindings
        timeout: Maximum evaluation time in milliseconds (raises ValueError
            with a D1012 code on timeout).
        max_stack_depth: Maximum recursion stack depth (raises ValueError
            with a D1011 code when exceeded).
        max_sequence_length: Maximum length of a query-result sequence
            (raises ValueError with a D2015 code when exceeded).

    Returns:
        The result of evaluating the expression

    Raises:
        ValueError: If parsing or evaluation fails, or a guardrail is exceeded

    Example:
        >>> data = {"name": "alice"}
        >>> result = evaluate("$uppercase(name)", data)
        >>> print(result)
        "ALICE"

        >>> # With bindings
        >>> result = evaluate("name & suffix", {"name": "Hello"}, {"suffix": "!"})
        >>> print(result)
        "Hello!"

        >>> # With a timeout guardrail (protects against non-terminating expressions)
        >>> evaluate("($inf := function(){$inf()}; $inf())", None, timeout=100)
        Traceback (most recent call last):
            ...
        ValueError: D1012: Evaluation timeout after 100 milliseconds. Check for infinite loop
    """
    return _evaluate(expression, data, bindings, timeout, max_stack_depth, max_sequence_length)
