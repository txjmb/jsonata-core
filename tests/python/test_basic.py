"""
Basic tests for jsonatapy
"""

import jsonatapy
import pytest


class TestCompile:
    """Tests for compile() function"""

    def test_compile_returns_expression(self):
        """Test that compile returns a JsonataExpression"""
        expr = jsonatapy.compile("$.name")
        assert isinstance(expr, jsonatapy.JsonataExpression)

    def test_compile_invalid_expression(self):
        """Test that invalid expressions raise ValueError"""
        # This will fail once parser is implemented
        # with pytest.raises(ValueError):
        #     jsonatapy.compile("invalid [[[ syntax")
        pass

    def test_compile_coded_error_s0214_no_prefix(self):
        """Test that S0214 coded errors appear without 'Parse error:' prefix.

        This test verifies that parser errors with JSONata spec codes (like S0214,
        which is raised when @ is not followed by a variable) appear at the start
        of the error message without an added 'Parse error:' prefix. The test suite
        extracts error codes using a regex that expects the code at the very start.
        """
        with pytest.raises(ValueError) as exc_info:
            jsonatapy.compile("Order@foo")

        error_msg = str(exc_info.value)
        # The error should start with S0214:, not "Parse error: S0214:"
        assert error_msg.startswith("S0214:"), (
            f"Expected error to start with 'S0214:', got: {error_msg}"
        )
        # Sanity check: should NOT have Parse error prefix
        assert not error_msg.startswith("Parse error: S0214:"), (
            f"Error should not have 'Parse error:' prefix, got: {error_msg}"
        )

    def test_compile_non_coded_error_with_prefix(self):
        """Test that non-coded parse errors still get 'Parse error:' prefix."""
        with pytest.raises(ValueError) as exc_info:
            # An unclosed string is a non-coded error
            jsonatapy.compile('"unclosed string')

        error_msg = str(exc_info.value)
        # Non-coded errors should have the "Parse error: " prefix
        assert error_msg.startswith("Parse error:"), (
            f"Expected error to start with 'Parse error:', got: {error_msg}"
        )


class TestEvaluate:
    """Tests for evaluate() function"""

    @pytest.mark.skip(reason="Not yet implemented")
    def test_evaluate_simple_path(self):
        """Test simple path evaluation"""
        data = {"name": "Alice"}
        result = jsonatapy.evaluate("name", data)
        assert result == "Alice"

    @pytest.mark.skip(reason="Not yet implemented")
    def test_evaluate_with_bindings(self):
        """Test evaluation with variable bindings"""
        data = {"value": 10}
        bindings = {"multiplier": 2}
        result = jsonatapy.evaluate("value * $multiplier", data, bindings)
        assert result == 20


class TestJsonataExpression:
    """Tests for JsonataExpression class"""

    def test_expression_creation(self):
        """Test that expression can be created"""
        expr = jsonatapy.compile("$.name")
        assert expr is not None

    @pytest.mark.skip(reason="Not yet implemented")
    def test_expression_evaluate(self):
        """Test expression evaluation"""
        expr = jsonatapy.compile("name")
        data = {"name": "Bob"}
        result = expr.evaluate(data)
        assert result == "Bob"

    @pytest.mark.skip(reason="Not yet implemented")
    def test_expression_reuse(self):
        """Test that compiled expressions can be reused"""
        expr = jsonatapy.compile("count(items)")

        result1 = expr.evaluate({"items": [1, 2, 3]})
        assert result1 == 3

        result2 = expr.evaluate({"items": [1, 2, 3, 4, 5]})
        assert result2 == 5


class TestMetadata:
    """Tests for version metadata"""

    def test_version_exists(self):
        """Test that version info is available"""
        assert hasattr(jsonatapy, "__version__")
        assert isinstance(jsonatapy.__version__, str)

    def test_jsonata_version_exists(self):
        """Test that JSONata reference version is available"""
        assert hasattr(jsonatapy, "__jsonata_version__")
        assert jsonatapy.__jsonata_version__ == "2.2.2"
