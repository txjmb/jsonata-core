"""Tests for the resource-guardrails feature (timeout/max_stack_depth/max_sequence_length).

Mirrors jsonata-js 2.2.1's guardrails: D1011 (stack), D1012 (timeout), D2015 (sequence).
See docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md, Phase 2.
"""

import jsonatapy
import pytest


def _force_tree_walker(expr: jsonatapy.JsonataExpression, data, **guardrails):
    """Force the tree-walker fallback by passing a non-None bindings dict
    (even empty), per the mechanism `JsonataExpression.run_eval` uses in
    `src/lib.rs`: any non-None `bindings` always routes through
    `create_evaluator()`/`Evaluator::with_options()`, bypassing the cached
    bytecode VM entirely."""
    return expr.evaluate(data, bindings={}, **guardrails)


class TestStackDepthD1011:
    def test_recursive_lambda_raises_d1011(self):
        expr = "($inf := function($n){$n+$inf($n-1)}; $inf(5))"
        with pytest.raises(ValueError, match="D1011"):
            jsonatapy.evaluate(expr, None, max_stack_depth=10)

    def test_without_option_recursive_lambda_raises_u1001_not_d1011(self):
        # No max_stack_depth set: the hardcoded native-stack backstop (U1001)
        # is what fires, not D1011.
        expr = "($inf := function($n){$n+$inf($n-1)}; $inf(5))"
        with pytest.raises(ValueError, match="U1001"):
            jsonatapy.evaluate(expr, None)

    def test_parity_vm_vs_tree_walker(self):
        # Self-recursive named lambdas can't compile to CompiledExpr (no
        # "call named lambda" IR node exists), so both the default (VM-
        # preferred) path and the bindings-forced tree-walker path fall
        # through to the same guarded evaluate_internal/evaluate_internal_impl
        # code -- this is true "for free", not because of extra plumbing.
        expr_str = "($inf := function($n){$n+$inf($n-1)}; $inf(5))"
        compiled = jsonatapy.compile(expr_str)
        with pytest.raises(ValueError, match="D1011"):
            compiled.evaluate(None, max_stack_depth=10)
        with pytest.raises(ValueError, match="D1011"):
            _force_tree_walker(compiled, None, max_stack_depth=10)


class TestTimeoutD1012:
    def test_slow_expression_raises_d1012(self):
        # NOTE: this deliberately avoids a deeply left-nested arithmetic
        # chain (e.g. `1+1+1+...`) as the brief's starting code originally
        # used. Both the parser's recursive-descent `parse_expression` and
        # (independently) the compiler's `MakeArray(u16)` bytecode instr for
        # array-literal AST nodes have real, pre-existing bugs at large N
        # that are NOT part of this guardrails feature -- see the report for
        # detail. A left-nested chain of ~4000+ terms segfaults at *parse*
        # time (native stack overflow, unguarded by `stacker::maybe_grow`
        # unlike the evaluator's recursion-depth check), and an array
        # literal `[1,1,...]` with >65536 elements silently produces a
        # wrong-length result (u16 truncation in `Instr::MakeArray`). Using
        # large *data* (a Python-supplied array, not a JSONata source-level
        # array literal) through a non-compilable lambda body sidesteps both
        # bugs while still reliably exercising the tree-walker's per-node
        # `evaluate_internal` timeout checkpoint (not the MapCall fast path,
        # which has its own dedicated test below).
        data = {"items": [{"a": i} for i in range(500_000)]}
        compiled = jsonatapy.compile("$map(items, function($x){$x.*})")
        with pytest.raises(ValueError, match="D1012"):
            _force_tree_walker(compiled, data, timeout=1)

    def test_without_option_never_times_out(self):
        data = {"items": [{"a": i} for i in range(200_000)]}
        result = jsonatapy.evaluate("items.a", data)
        assert len(result) == 200_000
        assert result[0] == 0
        assert result[-1] == 199_999

    def test_compiled_map_fast_path_raises_d1012(self):
        # Forces the eval_compiled_inner MapCall loop, which bypasses
        # evaluate_internal's per-node checkpoint entirely -- the timeout
        # check here is a deliberate extension beyond the design spec's
        # literal text (which only mentions checking at evaluate_internal).
        data = {"items": list(range(500_000))}
        with pytest.raises(ValueError, match="D1012"):
            jsonatapy.evaluate("$map(items, function($x){$x*2})", data, timeout=1)

    def test_parity_vm_vs_tree_walker_vs_compiled(self):
        data = {"items": list(range(500_000))}
        compiled = jsonatapy.compile("$map(items, function($x){$x*2})")
        with pytest.raises(ValueError, match="D1012"):
            compiled.evaluate(data, timeout=1)
        with pytest.raises(ValueError, match="D1012"):
            _force_tree_walker(compiled, data, timeout=1)

    def test_tco_trampoline_raises_d1012(self):
        # jsonata-js's own canonical timeout-guardrail example: a
        # tail-recursive lambda with no base case, evaluated through the
        # TCO trampoline (invoke_lambda_with_tco), not the plain recursive
        # evaluator call stack. Before the Task-11-followup fix, the
        # trampoline had NO timeout check at all -- only a hardcoded
        # 100,000-iteration cap producing U1001. Now check_loop_timeout is
        # called every trampoline iteration, and the 100k cap is skipped
        # entirely once a timeout is configured, so this must raise D1012,
        # not U1001.
        expr = "($inf := function(){$inf()}; $inf())"
        with pytest.raises(ValueError, match="D1012"):
            jsonatapy.evaluate(expr, None, timeout=100)


class TestSequenceLengthD2015:
    @pytest.mark.parametrize(
        "expr,data",
        [
            ("[1..1000]", None),
            ("items.name", {"items": [{"name": f"n{i}"} for i in range(1000)]}),
            ("*", {f"k{i}": i for i in range(1000)}),
            ("**", {"items": [{"v": i} for i in range(1000)]}),
            ("$keys(items)", {"items": [{f"k{i}": 1} for i in range(1000)]}),
            ("$lookup(items, 'v')", {"items": [{"v": i} for i in range(1000)]}),
            ("$append(a, b)", {"a": list(range(500)), "b": list(range(500))}),
            (
                "$spread(items)",
                {"items": [{f"k{i}": i} for i in range(1000)]},
            ),
            (
                "$each(items, function($v){$v})",
                {"items": {f"k{i}": i for i in range(1000)}},
            ),
        ],
    )
    def test_raises_d2015_generic_tree_walker_path(self, expr, data):
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate(expr, data, max_sequence_length=10)

    def test_map_raises_d2015_both_fast_and_generic_path(self):
        data = {"items": list(range(1000))}
        # Compilable lambda body -> CompiledExpr::MapCall fast path
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate(
                "$map(items, function($x){$x*2})", data, max_sequence_length=10
            )
        # Non-compilable lambda body (`$x.*` has no CompiledExpr::Wildcard arm)
        # -> generic tree-walker loop
        data2 = {"items": [{"a": i} for i in range(1000)]}
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate(
                "$map(items, function($x){$x.*})", data2, max_sequence_length=10
            )

    def test_multistep_filtered_path_raises_d2015_eval_fallback(self):
        # `items[filter].sub.v` has >1 field-path step AND a filter on the
        # first step, so `BytecodeCompiler::compile_expr` cannot inline it as
        # a simple GetDataField/GetField chain (that only handles all-simple
        # steps) or as the single-step-with-filter case -- it falls back to
        # `Instr::EvalFallback` -> `eval_compiled_inner`'s
        # `CompiledExpr::FieldPath` arm -> `compiled_eval_field_path` ->
        # `compiled_field_step`'s array-mapping branch. This regression
        # tests a genuine gap found while writing this suite: that function
        # previously had no D2015 check at all (fixed alongside `vm.rs`'s
        # `get_field_cached`, which had the same gap for the simpler
        # `items.name`-shaped case covered above).
        #
        # IMPORTANT: the filter predicate must itself be *compilable*
        # (`try_compile_expr_inner` handles simple comparisons/arithmetic/
        # literals, but has no arm for function calls like `$exists(...)`).
        # A non-compilable predicate makes `try_compile_path` return None for
        # the *whole* path expression, so the entire thing falls straight to
        # the tree-walker's `evaluate_path` (already correct from an earlier
        # task) and never reaches `compiled_eval_field_path`/
        # `compiled_field_step` at all -- silently defeating this regression
        # test (confirmed via revert-oracle: with `$exists(sub)` as the
        # filter, this test kept passing even with the `compiled_field_step`
        # fix reverted). `flag=true` is a plain equality comparison, which
        # *does* compile (`CompiledExpr::Compare`), so this correctly routes
        # through `EvalFallback`/`compiled_field_step`.
        data = {
            "items": [
                {"flag": True, "sub": [{"v": i} for i in range(1000)]}
                for _ in range(3)
            ]
        }
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate(
                "items[flag=true].sub.v", data, max_sequence_length=10
            )

    def test_filter_raises_d2015(self):
        data = {"items": list(range(1000))}
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate(
                "$filter(items, function($x){$x >= 0})",
                data,
                max_sequence_length=10,
            )

    def test_predicate_filter_raises_d2015_pure_vm_path(self):
        # `items[price > 0]` compiles fully to bytecode (GetDataField +
        # FilterByBytecode) when there's no bindings -> pure VM path, no
        # tree-walker/eval_compiled fallback involved at all.
        data = {"items": [{"price": i} for i in range(1000)]}
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate("items[price >= 0]", data, max_sequence_length=10)

    def test_predicate_filter_parity_vm_vs_tree_walker(self):
        data = {"items": [{"price": i} for i in range(1000)]}
        compiled = jsonatapy.compile("items[price >= 0]")
        with pytest.raises(ValueError, match="D2015"):
            compiled.evaluate(data, max_sequence_length=10)
        with pytest.raises(ValueError, match="D2015"):
            _force_tree_walker(compiled, data, max_sequence_length=10)

    def test_without_option_no_cap(self):
        data = {"items": list(range(1000))}
        result = jsonatapy.evaluate("items", data)
        assert len(result) == 1000

    def test_reduce_accumulator_itself_uncapped_but_append_inside_it_is_capped(self):
        # $reduce has no createSequence() call upstream (jsonata-js
        # functions.js) - its accumulator is intentionally NOT subject to
        # max_sequence_length, even if the accumulator happens to be an array.
        # This expression's accumulator grows via repeated $append calls
        # (0, 1, 2, ... elements); $append's own pre-check is
        # `arr.len() + second_len > max`, so it raises as soon as the
        # accumulator already holds `max` elements and a further single-item
        # append would exceed it -- i.e. on the 11th call (10 + 1 > 10), long
        # before $reduce's own (uncapped) loop would reach 1000 iterations.
        # This confirms the cap comes from $append's independent guard, not
        # from $reduce itself capping its accumulator.
        data = {"items": list(range(1000))}
        with pytest.raises(ValueError, match="D2015"):
            jsonatapy.evaluate(
                "$reduce(items, function($acc, $x){$append($acc, $x)}, [])",
                data,
                max_sequence_length=10,
            )


class TestCompileTimeDefaults:
    def test_compile_time_default_applies_to_evaluate_without_kwargs(self):
        expr = jsonatapy.compile(
            "($inf := function($n){$n+$inf($n-1)}; $inf(5))", max_stack_depth=10
        )
        with pytest.raises(ValueError, match="D1011"):
            expr.evaluate(None)

    def test_per_call_kwarg_overrides_compile_time_default(self):
        expr = jsonatapy.compile(
            "($inf := function($n){$n+$inf($n-1)}; $inf(5))", max_stack_depth=10
        )
        # Override with a much higher limit at call time - should now hit the
        # hardcoded U1001 ceiling instead of D1011, or succeed if the
        # recursion terminates within both limits (it won't terminate - $inf
        # has no base case - so expect U1001 here).
        with pytest.raises(ValueError, match="U1001"):
            expr.evaluate(None, max_stack_depth=100_000)
