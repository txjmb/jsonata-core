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

from typing import Any, Dict, Optional

from ._jsonatapy import (
    JsonataExpression as _JsonataExpression,
    compile as _compile,
    evaluate as _evaluate,
    __version__,
    __jsonata_version__,
)

__all__ = [
    "compile",
    "evaluate",
    "JsonataExpression",
    "__version__",
    "__jsonata_version__",
]


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

    def evaluate(self, data: Any, bindings: Optional[Dict[str, Any]] = None) -> Any:
        """
        Evaluate this expression against data.

        Args:
            data: The data to query/transform (typically a dict or list)
            bindings: Optional additional variable bindings

        Returns:
            The result of evaluating the expression

        Raises:
            ValueError: If evaluation fails

        Example:
            >>> expr = compile("orders[price > 100]")
            >>> data = {"orders": [{"price": 150}, {"price": 50}]}
            >>> result = expr.evaluate(data)
            >>> print(len(result))
            1
        """
        return self._expr.evaluate(data, bindings)

    @classmethod
    def compile(cls, expression: str) -> "JsonataExpression":
        """
        Compile a JSONata expression.

        This is an alternative constructor that compiles an expression string.

        Args:
            expression: A JSONata expression string

        Returns:
            A compiled JsonataExpression

        Raises:
            ValueError: If the expression cannot be parsed

        Example:
            >>> expr = JsonataExpression.compile("$.name")
        """
        return cls(_compile(expression))


def compile(expression: str) -> JsonataExpression:
    """
    Compile a JSONata expression into an executable form.

    Args:
        expression: A JSONata query/transformation expression string

    Returns:
        A compiled JsonataExpression that can be evaluated multiple times

    Raises:
        ValueError: If the expression cannot be parsed

    Example:
        >>> expr = compile("orders[price > 100].product")
        >>> result = expr.evaluate(data)

    Note:
        Compiling an expression once and evaluating it multiple times
        is more efficient than calling `evaluate()` repeatedly with
        the same expression string.
    """
    return JsonataExpression(_compile(expression))


def evaluate(expression: str, data: Any, bindings: Optional[Dict[str, Any]] = None) -> Any:
    """
    Compile and evaluate a JSONata expression in one step.

    This is a convenience function for one-off evaluations.
    For repeated evaluations, use `compile()` instead.

    Args:
        expression: A JSONata query/transformation expression string
        data: The data to query/transform (typically a dict or list)
        bindings: Optional additional variable bindings

    Returns:
        The result of evaluating the expression

    Raises:
        ValueError: If parsing or evaluation fails

    Example:
        >>> data = {"name": "alice"}
        >>> result = evaluate("$uppercase(name)", data)
        >>> print(result)
        "ALICE"

        >>> # With bindings
        >>> result = evaluate("name & suffix", {"name": "Hello"}, {"suffix": "!"})
        >>> print(result)
        "Hello!"
    """
    return _evaluate(expression, data, bindings)
