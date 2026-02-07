"""
Integration tests for jsonatapy - testing the full Python → Rust → Python flow
These tests will work once the extension is built with maturin.
"""


import pytest

# These tests require the compiled extension
pytest_plugins = []

try:
    import jsonatapy

    EXTENSION_AVAILABLE = True
except ImportError:
    EXTENSION_AVAILABLE = False

pytestmark = pytest.mark.skipif(
    not EXTENSION_AVAILABLE, reason="Extension not built. Run: maturin develop"
)


class TestLiterals:
    """Test literal value evaluation"""

    def test_number_literal(self):
        result = jsonatapy.evaluate("42", {})
        assert result == 42

    def test_string_literal(self):
        result = jsonatapy.evaluate('"hello"', {})
        assert result == "hello"

    def test_boolean_true(self):
        result = jsonatapy.evaluate("true", {})
        assert result is True

    def test_boolean_false(self):
        result = jsonatapy.evaluate("false", {})
        assert result is False

    def test_null_literal(self):
        result = jsonatapy.evaluate("null", {})
        assert result is None


class TestArithmetic:
    """Test arithmetic operations"""

    def test_addition(self):
        result = jsonatapy.evaluate("1 + 2", {})
        assert result == 3

    def test_subtraction(self):
        result = jsonatapy.evaluate("10 - 3", {})
        assert result == 7

    def test_multiplication(self):
        result = jsonatapy.evaluate("4 * 5", {})
        assert result == 20

    def test_division(self):
        result = jsonatapy.evaluate("15 / 3", {})
        assert result == 5

    def test_modulo(self):
        result = jsonatapy.evaluate("10 % 3", {})
        assert result == 1

    def test_complex_expression(self):
        result = jsonatapy.evaluate("(2 + 3) * 4", {})
        assert result == 20


class TestComparison:
    """Test comparison operations"""

    def test_equality(self):
        result = jsonatapy.evaluate("5 = 5", {})
        assert result is True

    def test_inequality(self):
        result = jsonatapy.evaluate("5 != 3", {})
        assert result is True

    def test_less_than(self):
        result = jsonatapy.evaluate("3 < 5", {})
        assert result is True

    def test_less_than_or_equal(self):
        result = jsonatapy.evaluate("5 <= 5", {})
        assert result is True

    def test_greater_than(self):
        result = jsonatapy.evaluate("7 > 3", {})
        assert result is True

    def test_greater_than_or_equal(self):
        result = jsonatapy.evaluate("5 >= 5", {})
        assert result is True


class TestLogical:
    """Test logical operations"""

    def test_and_true(self):
        result = jsonatapy.evaluate("true and true", {})
        assert result is True

    def test_and_false(self):
        result = jsonatapy.evaluate("true and false", {})
        assert result is False

    def test_or_true(self):
        result = jsonatapy.evaluate("false or true", {})
        assert result is True

    def test_or_false(self):
        result = jsonatapy.evaluate("false or false", {})
        assert result is False


class TestPathTraversal:
    """Test JSON path navigation"""

    def test_simple_property(self):
        data = {"name": "Alice"}
        result = jsonatapy.evaluate("name", data)
        assert result == "Alice"

    def test_nested_property(self):
        data = {"user": {"name": "Bob"}}
        result = jsonatapy.evaluate("user.name", data)
        assert result == "Bob"

    def test_deep_nesting(self):
        data = {"a": {"b": {"c": {"d": 42}}}}
        result = jsonatapy.evaluate("a.b.c.d", data)
        assert result == 42

    def test_array_access(self):
        data = {"items": [1, 2, 3]}
        result = jsonatapy.evaluate("items[0]", data)
        assert result == 1


class TestStringFunctions:
    """Test built-in string functions"""

    def test_uppercase(self):
        result = jsonatapy.evaluate('$uppercase("hello")', {})
        assert result == "HELLO"

    def test_lowercase(self):
        result = jsonatapy.evaluate('$lowercase("WORLD")', {})
        assert result == "world"

    def test_length(self):
        result = jsonatapy.evaluate('$length("hello")', {})
        assert result == 5

    def test_substring(self):
        result = jsonatapy.evaluate('$substring("hello", 1, 3)', {})
        assert result == "el"

    def test_substring_before(self):
        result = jsonatapy.evaluate('$substringBefore("hello-world", "-")', {})
        assert result == "hello"

    def test_substring_after(self):
        result = jsonatapy.evaluate('$substringAfter("hello-world", "-")', {})
        assert result == "world"

    def test_trim(self):
        result = jsonatapy.evaluate('$trim("  hello  ")', {})
        assert result == "hello"

    def test_contains(self):
        result = jsonatapy.evaluate('$contains("hello world", "world")', {})
        assert result is True

    def test_split(self):
        result = jsonatapy.evaluate('$split("a,b,c", ",")', {})
        assert result == ["a", "b", "c"]

    def test_join(self):
        data = {"items": ["a", "b", "c"]}
        result = jsonatapy.evaluate('$join(items, ",")', data)
        assert result == "a,b,c"

    def test_replace(self):
        result = jsonatapy.evaluate('$replace("hello", "l", "L")', {})
        assert result == "heLLo"


class TestNumericFunctions:
    """Test built-in numeric functions"""

    def test_sum(self):
        data = {"numbers": [1, 2, 3, 4, 5]}
        result = jsonatapy.evaluate("$sum(numbers)", data)
        assert result == 15

    def test_max(self):
        data = {"numbers": [3, 1, 4, 1, 5]}
        result = jsonatapy.evaluate("$max(numbers)", data)
        assert result == 5

    def test_min(self):
        data = {"numbers": [3, 1, 4, 1, 5]}
        result = jsonatapy.evaluate("$min(numbers)", data)
        assert result == 1

    def test_average(self):
        data = {"numbers": [2, 4, 6, 8]}
        result = jsonatapy.evaluate("$average(numbers)", data)
        assert result == 5

    def test_abs(self):
        result = jsonatapy.evaluate("$abs(-42)", {})
        assert result == 42

    def test_floor(self):
        result = jsonatapy.evaluate("$floor(3.7)", {})
        assert result == 3

    def test_ceil(self):
        result = jsonatapy.evaluate("$ceil(3.2)", {})
        assert result == 4

    def test_round(self):
        result = jsonatapy.evaluate("$round(3.6)", {})
        assert result == 4

    def test_sqrt(self):
        result = jsonatapy.evaluate("$sqrt(16)", {})
        assert result == 4

    def test_power(self):
        result = jsonatapy.evaluate("$power(2, 3)", {})
        assert result == 8


class TestArrayFunctions:
    """Test built-in array functions"""

    def test_count(self):
        data = {"items": [1, 2, 3, 4, 5]}
        result = jsonatapy.evaluate("$count(items)", data)
        assert result == 5

    def test_append(self):
        data = {"items": [1, 2, 3]}
        result = jsonatapy.evaluate("$append(items, 4)", data)
        assert result == [1, 2, 3, 4]

    def test_reverse(self):
        data = {"items": [1, 2, 3]}
        result = jsonatapy.evaluate("$reverse(items)", data)
        assert result == [3, 2, 1]

    def test_sort(self):
        data = {"items": [3, 1, 4, 1, 5]}
        result = jsonatapy.evaluate("$sort(items)", data)
        assert result == [1, 1, 3, 4, 5]

    def test_distinct(self):
        data = {"items": [1, 2, 2, 3, 3, 3]}
        result = jsonatapy.evaluate("$distinct(items)", data)
        assert result == [1, 2, 3]

    def test_exists(self):
        data = {"name": "Alice"}
        result = jsonatapy.evaluate("$exists(name)", data)
        assert result is True


class TestObjectFunctions:
    """Test built-in object functions"""

    def test_keys(self):
        data = {"a": 1, "b": 2, "c": 3}
        result = jsonatapy.evaluate("$keys($)", data)
        assert sorted(result) == ["a", "b", "c"]

    def test_lookup(self):
        data = {"users": {"alice": {"age": 30}, "bob": {"age": 25}}}
        result = jsonatapy.evaluate('$lookup(users, "alice")', data)
        assert result == {"age": 30}

    def test_spread(self):
        data = {"a": 1, "b": 2}
        result = jsonatapy.evaluate("$spread($)", data)
        # Result should be array of key-value pairs
        assert isinstance(result, list)

    def test_merge(self):
        data = {"obj1": {"a": 1}, "obj2": {"b": 2}}
        result = jsonatapy.evaluate("$merge([obj1, obj2])", data)
        assert result == {"a": 1, "b": 2}


class TestComplexExpressions:
    """Test complex real-world JSONata expressions"""

    def test_filter_and_transform(self):
        """Filter orders by price and extract product names"""
        # This would require predicate support
        # result = jsonatapy.evaluate("orders[price > 100].product", data)
        # assert result == ["A", "C"]

    def test_nested_function_calls(self):
        """Test nested function calls"""
        result = jsonatapy.evaluate('$uppercase($substring("hello", 0, 2))', {})
        assert result == "HE"

    def test_arithmetic_with_paths(self):
        """Test arithmetic with JSON paths"""
        data = {"price": 100, "tax": 10}
        result = jsonatapy.evaluate("price + tax", data)
        assert result == 110

    def test_conditional_expression(self):
        """Test conditional (ternary) expressions"""
        data = {"age": 25}
        result = jsonatapy.evaluate('age >= 18 ? "adult" : "minor"', data)
        assert result == "adult"


class TestDataConversion:
    """Test Python ↔ Rust data conversion"""

    def test_none_to_null(self):
        """Test that Python None becomes JSON null"""
        data = {"value": None}
        result = jsonatapy.evaluate("value", data)
        assert result is None

    def test_bool_conversion(self):
        """Test Python bool conversion"""
        data = {"flag": True}
        result = jsonatapy.evaluate("flag", data)
        assert result is True

    def test_int_conversion(self):
        """Test Python int conversion"""
        data = {"count": 42}
        result = jsonatapy.evaluate("count", data)
        assert result == 42

    def test_float_conversion(self):
        """Test Python float conversion"""
        data = {"value": 3.14}
        result = jsonatapy.evaluate("value", data)
        assert result == 3.14

    def test_string_conversion(self):
        """Test Python string conversion"""
        data = {"text": "hello"}
        result = jsonatapy.evaluate("text", data)
        assert result == "hello"

    def test_list_conversion(self):
        """Test Python list conversion"""
        data = {"items": [1, 2, 3]}
        result = jsonatapy.evaluate("items", data)
        assert result == [1, 2, 3]

    def test_dict_conversion(self):
        """Test Python dict conversion"""
        data = {"obj": {"a": 1, "b": 2}}
        result = jsonatapy.evaluate("obj", data)
        assert result == {"a": 1, "b": 2}

    def test_nested_structures(self):
        """Test deeply nested structure conversion"""
        data = {
            "users": [
                {"name": "Alice", "scores": [90, 85, 88]},
                {"name": "Bob", "scores": [75, 80, 82]},
            ]
        }
        result = jsonatapy.evaluate("users", data)
        assert result == data["users"]


class TestVariableBindings:
    """Test variable bindings in evaluation context"""

    def test_simple_binding(self):
        """Test simple variable binding"""
        data = {"value": 10}
        bindings = {"multiplier": 2}
        result = jsonatapy.evaluate("value * $multiplier", data, bindings)
        assert result == 20

    def test_multiple_bindings(self):
        """Test multiple variable bindings"""
        data = {}
        bindings = {"x": 5, "y": 3}
        result = jsonatapy.evaluate("$x + $y", data, bindings)
        assert result == 8

    def test_binding_overrides_data(self):
        """Test that bindings can override data properties"""
        data = {"value": 10}
        bindings = {"value": 20}
        result = jsonatapy.evaluate("$value", data, bindings)
        assert result == 20


class TestErrorHandling:
    """Test error handling and exceptions"""

    def test_parse_error(self):
        """Test that invalid syntax raises ValueError"""
        with pytest.raises(ValueError, match="Parse error"):
            jsonatapy.compile("invalid [[[ syntax")

    def test_undefined_variable(self):
        """Test that undefined variables raise appropriate error"""
        # This depends on how the evaluator handles undefined variables
        # It might return None or raise an error
        try:
            result = jsonatapy.evaluate("$undefined", {})
            # If it returns None, that's acceptable
            assert result is None or result == {}
        except ValueError:
            # If it raises an error, that's also acceptable
            pass

    def test_type_error(self):
        """Test that type errors are properly handled"""
        # Trying to add a string and a number should raise an error
        with pytest.raises((TypeError, ValueError)):
            jsonatapy.evaluate('"hello" + 42', {})

    def test_division_by_zero(self):
        """Test division by zero handling"""
        with pytest.raises((ValueError, ZeroDivisionError)):
            jsonatapy.evaluate("10 / 0", {})


class TestExpressionReuse:
    """Test that compiled expressions can be reused efficiently"""

    def test_compile_once_evaluate_many(self):
        """Test compiling once and evaluating multiple times"""
        expr = jsonatapy.compile("name")

        result1 = expr.evaluate({"name": "Alice"})
        assert result1 == "Alice"

        result2 = expr.evaluate({"name": "Bob"})
        assert result2 == "Bob"

        result3 = expr.evaluate({"name": "Charlie"})
        assert result3 == "Charlie"

    def test_complex_expression_reuse(self):
        """Test reusing complex expressions"""
        expr = jsonatapy.compile("$sum(prices)")

        result1 = expr.evaluate({"prices": [10, 20, 30]})
        assert result1 == 60

        result2 = expr.evaluate({"prices": [5, 15, 25, 35]})
        assert result2 == 80


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
