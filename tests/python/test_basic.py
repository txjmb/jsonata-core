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
        with pytest.raises(ValueError):
            jsonatapy.compile("invalid [[[ syntax")

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

    @pytest.mark.parametrize(
        "expression, expected",
        [
            ("($o := [function(){1}]; $o[0]())", 1),
            ("$string[0](1)", "1"),
            ('$exists($o) ? $o[0]() : "none"', "none"),
            ('($o := {"g": [function(){7}]}; $o.g[0]())', 7),
        ],
    )
    def test_compile_call_of_indexed_function(self, expression, expected):
        """Calling an indexed variable or field compiles and calls what the index gives.

        It was a parse error ("Invalid function name"); jsonata-js calls the
        function the procedure expression evaluates to.
        """
        assert jsonatapy.compile(expression).evaluate(None) == expected

    def test_call_of_indexed_non_function_raises_t1006(self):
        """An indexed value that is not a function raises T1006 when called."""
        with pytest.raises(ValueError, match="T1006"):
            jsonatapy.compile("($o := [1]; $o[0]())").evaluate(None)

    @pytest.mark.parametrize(
        "expression",
        [
            "($o := [function($x){$x}]; $o#$i(3))",
            '($o := {"g": [function($x){$x}]}; $o.g[0]#$i(3))',
        ],
    )
    def test_call_of_bound_step(self, expression):
        """Calling a step with an index binding calls what the step gives.

        It was a parse error (T1006); jsonata-js's `(` takes the bound step
        as its procedure.
        """
        assert jsonatapy.compile(expression).evaluate(None) == 3

    def test_call_of_focus_bound_predicated_step_raises_s0215(self):
        """A focus binding after a predicate is S0215, called or not."""
        with pytest.raises(ValueError, match="S0215"):
            jsonatapy.compile("($o := [function($x){$x}]; $o[0]@$v(3))").evaluate(None)

    def test_call_of_bound_builtin_name_raises_t1005(self):
        """A called path naming a builtin without its `$` is T1005, as in jsonata-js."""
        with pytest.raises(ValueError, match="T1005"):
            jsonatapy.compile("uppercase#$i()").evaluate(None)

    @pytest.mark.parametrize(
        "expression, code",
        [
            ("a.1@$v()", "T1006"),
            ('a.("f")@$v()', "T1006"),
            ('x["f"@$v()]', "T1006"),
            ("x[$string(1,2,3)]", "T0410"),
            # Members run in order here; jsonata-js runs them concurrently.
            ('[$string(1,2,3), "f"@$v()]', "T0410"),
            ("true.-a^(x)@$v", "S0213"),
        ],
    )
    def test_call_of_bound_literal_and_filter_on_nothing(self, expression, code):
        """A bound literal call is T1006 when it runs, and a filter on nothing still runs."""
        with pytest.raises(ValueError, match=code):
            jsonatapy.compile(expression).evaluate({"a": {"b": [1, 2]}})

    @pytest.mark.parametrize(
        "expression, code",
        [("1[0]@$v.**.q", "S0215"), ("1.a[0]@$v", "S0215"), ("1.a.b[0]@$v", "S0213")],
    )
    def test_focus_binding_error_precedes_literal_step(self, expression, code):
        """A misplaced `@` in an operand of `.` is raised before its literal step."""
        with pytest.raises(ValueError, match=code):
            jsonatapy.compile(expression)

    @pytest.mark.parametrize(
        "expression",
        ['arr."uppercase"[0]()', 'a."uppercase"#$i()', '($o := {"f": $uppercase}; $o."f"("a"))'],
    )
    def test_call_of_quoted_step_after_dot_raises_t1006(self, expression):
        """A called quoted string step is the string, as in jsonata-js: T1006, not T1005."""
        with pytest.raises(ValueError, match="T1006"):
            jsonatapy.compile(expression).evaluate({"arr": [{"x": 1}], "a": {}})

    @pytest.mark.parametrize(
        "expression, expected",
        [
            ('"x" ~> $uppercase[0]()', "X"),
            ("($o := [function(){1}, function($v){$v+10}]; 1 ~> $o[1]())", 11),
        ],
    )
    def test_chain_into_call_of_indexed_function(self, expression, expected):
        """`~>` passes its left side as the first argument of any call on its right."""
        assert jsonatapy.compile(expression).evaluate(None) == expected

    @pytest.mark.parametrize(
        "expression, expected",
        [
            ('"s"{k: v}.p', [1, 3]),
            ('"s"[0].k', "p"),
            ('"s"#$i.k', ["p", "q", "p"]),
        ],
    )
    def test_string_literal_heading_path_is_name_step(self, expression, expected):
        """A string heading a path reads that field, under its predicates and group-by."""
        data = {"s": [{"k": "p", "v": 1}, {"k": "q", "v": 2}, {"k": "p", "v": 3}]}
        assert jsonatapy.compile(expression).evaluate(data) == expected


class TestJsonataExpression:
    """Tests for JsonataExpression class"""

    def test_expression_creation(self):
        """Test that expression can be created"""
        expr = jsonatapy.compile("$.name")
        assert expr is not None


class TestMetadata:
    """Tests for version metadata"""

    def test_version_exists(self):
        """Test that version info is available"""
        assert hasattr(jsonatapy, "__version__")
        assert isinstance(jsonatapy.__version__, str)

    def test_jsonata_version_exists(self):
        """Test that JSONata reference version is available"""
        assert hasattr(jsonatapy, "__jsonata_version__")
        assert jsonatapy.__jsonata_version__ == "2.3.0"
