"""Tests for host-callable custom functions (register / register_override).

These exercise the Python (PyO3) binding of the Rust host-function feature:
a Python callable registered on a compiled expression is callable from the
expression as ``$name(...)``. See docs/superpowers/specs/
2026-07-19-host-callable-functions-design.md.
"""

import jsonatapy
import pytest


class TestRegister:
    def test_direct_call(self):
        expr = jsonatapy.compile("$greet(name)")
        expr.register("greet", lambda n: f"hello {n}")
        assert expr.evaluate({"name": "Ada"}) == "hello Ada"

    def test_multiple_args(self):
        expr = jsonatapy.compile("$convert(amount, currency)")
        expr.register("convert", lambda amt, cur: amt * (1.1 if cur == "EUR" else 1.0))
        assert expr.evaluate({"amount": 10, "currency": "EUR"}) == pytest.approx(11.0)

    def test_zero_args(self):
        expr = jsonatapy.compile("$token()")
        expr.register("token", lambda: "abc-123")
        assert expr.evaluate(None) == "abc-123"

    def test_maps_over_sequence(self):
        expr = jsonatapy.compile("items.$double(qty)")
        expr.register("double", lambda q: q * 2)
        assert expr.evaluate({"items": [{"qty": 2}, {"qty": 5}]}) == [4, 10]

    def test_returns_structured_value(self):
        expr = jsonatapy.compile("$wrap(x)")
        expr.register("wrap", lambda x: {"value": x, "doubled": x * 2})
        assert expr.evaluate({"x": 21}) == {"value": 21, "doubled": 42}

    def test_works_via_evaluate_json(self):
        expr = jsonatapy.compile("$double(n)")
        expr.register("double", lambda n: n * 2)
        assert expr.evaluate_json('{"n": 21}') == "42"

    def test_register_returns_self_for_chaining(self):
        expr = jsonatapy.compile("$a() & $b()")
        result = expr.register("a", lambda: "x").register("b", lambda: "y")
        assert result is expr
        assert expr.evaluate(None) == "xy"

    def test_in_expression_function_shadows_host_fn(self):
        expr = jsonatapy.compile("($greet := function($n){ 'local ' & $n }; $greet('x'))")
        expr.register("greet", lambda n: "HOST")
        assert expr.evaluate(None) == "local x"


class TestRegisterErrors:
    def test_collision_with_builtin_rejected(self):
        expr = jsonatapy.compile("$sum(x)")
        with pytest.raises(ValueError, match="shadows a built-in"):
            expr.register("sum", lambda x: 0)

    def test_non_callable_rejected(self):
        expr = jsonatapy.compile("$x()")
        with pytest.raises(TypeError, match="must be callable"):
            expr.register("x", 42)

    def test_host_exception_propagates(self):
        expr = jsonatapy.compile("$boom()")

        def boom():
            raise KeyError("nope")

        expr.register("boom", boom)
        with pytest.raises(ValueError, match="nope"):
            expr.evaluate(None)

    def test_async_function_rejected(self):
        async def afn():
            return 1

        expr = jsonatapy.compile("$a()")
        expr.register("a", afn)
        with pytest.raises(ValueError, match="coroutine"):
            expr.evaluate(None)


class TestOverride:
    def test_override_now_for_determinism(self):
        expr = jsonatapy.compile("$now()")
        expr.register_override("now", lambda: "2020-01-01T00:00:00.000Z")
        assert expr.evaluate(None) == "2020-01-01T00:00:00.000Z"

    def test_override_eval_for_sandboxing(self):
        expr = jsonatapy.compile("$eval('1+1')")

        def blocked(*_args):
            raise ValueError("$eval is disabled")

        expr.register_override("eval", blocked)
        with pytest.raises(ValueError, match="disabled"):
            expr.evaluate(None)

    def test_override_compilable_builtin_rejected(self):
        expr = jsonatapy.compile("$round(x)")
        with pytest.raises(ValueError, match="compiled fast path"):
            expr.register_override("round", lambda x: 0)


class TestNoImpact:
    def test_no_host_fns_unchanged(self):
        # Sanity: an expression with no host fns is unaffected.
        assert jsonatapy.evaluate("$sum([1,2,3])", None) == 6

    def test_host_fn_alongside_bindings(self):
        expr = jsonatapy.compile("$scale(base * $factor)")
        expr.register("scale", lambda v: v + 1)
        assert expr.evaluate({"base": 10}, {"factor": 2}) == 21
