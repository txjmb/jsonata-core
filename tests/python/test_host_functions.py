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

    @pytest.mark.parametrize(
        "source",
        [
            "$tick() ~> $string()",
            "$tick() ~> $string",
            '$tick() ~> $formatNumber(?, "#")',
        ],
    )
    def test_chain_left_side_evaluated_once(self, source):
        # `~>` used to evaluate its left side twice: once to check for
        # undefined and again as the function's argument (#193).
        calls = []
        expr = jsonatapy.compile(source)
        expr.register("tick", lambda: calls.append(1) or len(calls))
        assert expr.evaluate(None) == "1"
        assert len(calls) == 1

    @pytest.mark.parametrize(
        ("source", "expected"),
        [
            ("5 ~> $dbl", 10),
            ("5 ~> $dbl()", 10),
            ("($f := $dbl; 5 ~> $f)", 10),
            ("5 ~> ($dbl ~> $string)", "10"),
            ("$map([1, 2], $dbl)", [2, 4]),
            ("$type($dbl)", "function"),
            ("$exists($dbl)", True),
            ("(function($dbl){ $exists($dbl) })()", False),
        ],
    )
    def test_host_function_as_value(self, source, expected):
        # A host function named without a call is a function value, as a
        # builtin is, so `~>` can apply it (#193).
        expr = jsonatapy.compile(source)
        # A host function gets every argument a higher-order function has.
        expr.register("dbl", lambda n, *_: n * 2)
        assert expr.evaluate(None) == expected


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

    def test_override_gets_its_own_hof_arity_not_the_builtins(self):
        # `now` has table arity 2 (picture, timezone), but jsonata-js truncates
        # a by-reference HOF callback to the *override's own*
        # implementation.length. A three-parameter override receives $map's
        # full (item, index, array) triple, not the shadowed builtin's two.
        calls = []

        def spy(item, index, array):
            calls.append((item, index, array))
            return "x"

        expr = jsonatapy.compile('$map(["a", "b"], $now)')
        expr.register_override("now", spy)
        assert expr.evaluate(None) == ["x", "x"]
        assert calls == [("a", 0, ["a", "b"]), ("b", 1, ["a", "b"])]

    def test_star_args_override_gets_one_hof_arg(self):
        # A JavaScript `function(...args)` has length 0 and gets only the value;
        # a Python `*args` callable counts the same way.
        calls = []

        def spy(*args):
            calls.append(args)
            return "x"

        expr = jsonatapy.compile('$map(["a", "b"], $now)')
        expr.register_override("now", spy)
        assert expr.evaluate(None) == ["x", "x"]
        assert calls == [("a",), ("b",)]

    def test_uninspectable_callable_gets_every_hof_arg(self):
        # A callable inspect.signature can't read keeps the old behavior:
        # every argument the higher-order function has.
        calls = []

        class Opaque:
            def __call__(self, *args):
                calls.append(args)
                return "x"

            @property
            def __signature__(self):
                raise ValueError("no signature")

        expr = jsonatapy.compile('$map(["a"], $f)')
        expr.register("f", Opaque())
        assert expr.evaluate(None) == "x"
        assert calls == [("a", 0, ["a"])]


class TestNoImpact:
    def test_no_host_fns_unchanged(self):
        # Sanity: an expression with no host fns is unaffected.
        assert jsonatapy.evaluate("$sum([1,2,3])", None) == 6

    def test_host_fn_alongside_bindings(self):
        expr = jsonatapy.compile("$scale(base * $factor)")
        expr.register("scale", lambda v: v + 1)
        assert expr.evaluate({"base": 10}, {"factor": 2}) == 21


class TestCalleeEvaluatedOnce:
    def test_tail_call_callee_runs_once(self):
        # The callee of a call in a lambda's tail position is evaluated once,
        # so a host function inside it runs once per call.
        calls = []

        def tick():
            calls.append(1)
            return 1

        expr = jsonatapy.compile("(function() { ($tick(); $string)(1) })()")
        expr.register("tick", tick)
        assert expr.evaluate(None) == "1"
        assert len(calls) == 1


class TestFunctionValues:
    """Functions cross the boundary both ways, as between jsonata-js and JavaScript."""

    def test_function_argument_is_callable(self):
        expr = jsonatapy.compile("$apply(function($x, $y){ $x * $y }, 6, 7)")
        expr.register("apply", lambda fn, a, b: fn(a, b))
        assert expr.evaluate(None) == 42

    def test_functions_nested_in_arguments(self):
        expr = jsonatapy.compile('$call({"f": $uppercase, "fs": [$lowercase]}, "Ab")')
        expr.register("call", lambda obj, s: [obj["f"](s), obj["fs"][0](s)])
        assert expr.evaluate(None) == ["AB", "ab"]

    def test_function_argument_comes_back_as_itself(self):
        expr = jsonatapy.compile("$identity($sum)([1, 2])")
        expr.register("identity", lambda fn: fn)
        assert expr.evaluate(None) == 3

    def test_function_argument_repr(self):
        expr = jsonatapy.compile("[$show($sum), $show(function(){1})]")
        expr.register("show", repr)
        assert expr.evaluate(None) == ["<jsonata function $sum>", "<jsonata function>"]

    def test_function_kept_past_its_evaluation_raises(self):
        kept = []
        expr = jsonatapy.compile("$keep($sum)")
        expr.register("keep", lambda fn: kept.append(fn) or 1)
        assert expr.evaluate(None) == 1
        with pytest.raises(ValueError, match="can only be called while"):
            kept[0]([1])

    def test_function_kept_for_another_evaluation_raises(self):
        kept = []
        expr = jsonatapy.compile("$keep($sum)")
        expr.register("keep", lambda fn: kept.append(fn) or 1)
        expr.evaluate(None)
        other = jsonatapy.compile("$call()")
        other.register("call", lambda: kept[0]([1]))
        with pytest.raises(ValueError, match="can only be called while"):
            other.evaluate(None)

    def test_error_in_a_function_argument_keeps_its_code(self):
        expr = jsonatapy.compile("$apply(function(){ $error('boom') })")
        expr.register("apply", lambda fn: fn())
        with pytest.raises(ValueError, match=r"^D3137: boom"):
            expr.evaluate(None)

    def test_host_can_catch_an_error_in_a_function_argument(self):
        def attempt(fn):
            try:
                return fn()
            except ValueError as e:
                return str(e)[:5]

        expr = jsonatapy.compile("$attempt(function(){ $error('boom') })")
        expr.register("attempt", attempt)
        assert expr.evaluate(None) == "D3137"

    def test_returned_callable_reports_its_arity(self):
        expr = jsonatapy.compile("$map([1, 2], $make())")
        expr.register("make", lambda: lambda value, index: [value, index])
        assert expr.evaluate(None) == [[1, 0], [2, 1]]

    def test_callable_bindings(self):
        assert jsonatapy.evaluate("$f(2)", None, {"f": lambda x: x * 10}) == 20
        assert jsonatapy.evaluate("$type($f)", None, {"f": len}) == "function"
        assert jsonatapy.evaluate("$o.m(3)", None, {"o": {"m": lambda x: -x}}) == -3

    def test_function_result_is_none_in_python(self):
        assert jsonatapy.evaluate("$f", None, {"f": len}) is None
