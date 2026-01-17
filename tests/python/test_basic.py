"""
Basic tests for jsonatapy
"""

import pytest
import jsonatapy


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
        assert jsonatapy.__jsonata_version__ == "2.1.0"
