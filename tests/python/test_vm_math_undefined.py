"""
Regression tests: math builtins must propagate undefined through the bytecode
VM's `call_pure_builtin` (src/evaluator.rs), not silently downgrade it to
`JValue::Null`, matching the tree-walker fix landed in PR #72
(commit c679fbb, "fix(evaluator): propagate undefined through tree-walker
math builtins") and jsonata-js's oracle behavior.

Root cause: `call_pure_builtin`'s per-arm match statements for `abs`, `ceil`,
`floor`, `round`, and `sqrt` had a combined pattern
`Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null)` --
collapsing "missing field" (Undefined) and "explicit null" (Null) into the
same Null result. `JValue::Null` and `JValue::Undefined` both surface as
Python `None` at the top level, so this divergence from the tree-walker was
invisible there. It becomes observable in object construction: object keys
with an `Undefined` value are dropped, but keys with an explicit `Null`
value are kept. So `{"x": $abs(nothing)}` (nothing = missing field) produced
`{"x": None}` on the VM (default) path but the spec-correct `{}` on the
tree-walker path (via the private `_set_force_tree_walker` toggle) -- and per the jsonata-js
oracle, `{}` is correct.

Fix: add "abs", "ceil", "floor", "round", "sqrt" to the shared
`UNDEFINED_PROPAGATING_FUNCTIONS` list (src/evaluator.rs), which the VM's
`call_pure_builtin` already consults via an early-return guard (`if
effective_args.first().is_some_and(JValue::is_undefined) &&
propagates_undefined(name) { return Ok(JValue::Undefined); }`) placed before
the per-arm match. Verified this doesn't regress the tree-walker: bare
identifiers always parse as `AstNode::Path` (never a bare top-level
`AstNode::Name`), so the tree-walker's legacy `AstNode::Name`-based
undefined-propagation pre-check never fires for real expressions like
`nothing`; and the parallel `AstNode::Path` pre-check pattern-matches on
`Ok(JValue::Null)`, which no longer matches post the null-vs-undefined path
migration (missing-field paths already evaluate to `Undefined`, not
`Null`). Also verified the arity-error case
(`$abs(nothing, 2)`) is unaffected: the compiler's max-args guard rejects
compilation for too-many-args calls before the VM ever runs, so both paths
fall back to the tree-walker and raise the identical arity error.
"""

import jsonatapy
import pytest

# Each of these references a field ("nothing") that does not exist in the
# (non-empty) input data, so the builtin's argument evaluates to Undefined,
# not Null.
MATH_FUNCTIONS = ["abs", "ceil", "floor", "round", "sqrt"]

DATA = {"a": 1}


@pytest.mark.parametrize("fn", MATH_FUNCTIONS)
def test_default_vm_path_drops_undefined_key(fn):
    """Default (VM-preferred) path: undefined-valued key must be dropped, not kept as null."""
    expr = jsonatapy.compile(f'{{"x": ${fn}(nothing)}}')
    result = expr.evaluate(DATA)
    assert result == {}, f"${fn}(nothing) in object construction should drop key 'x', got {result}"


@pytest.mark.parametrize("fn", MATH_FUNCTIONS)
def test_default_matches_forced_tree_walker(fn, force_tree_walker_toggle):
    """Default (VM) path and forced-tree-walker path must agree on object construction."""
    expr_src = f'{{"x": ${fn}(nothing)}}'
    expr = jsonatapy.compile(expr_src)

    default_result = expr.evaluate(DATA)

    force_tree_walker_toggle(True)
    tree_walker_result = expr.evaluate(DATA)
    force_tree_walker_toggle(False)

    assert default_result == tree_walker_result == {}


@pytest.mark.parametrize("fn", MATH_FUNCTIONS)
def test_top_level_undefined_result_is_none(fn):
    """Top-level call still surfaces as Python None (unaffected by this fix, sanity check)."""
    result = jsonatapy.evaluate(f"${fn}(nothing)", DATA)
    assert result is None


@pytest.mark.parametrize("fn", MATH_FUNCTIONS)
def test_explicit_null_argument_is_a_type_error(fn):
    """An explicit null is a type error, not a value these functions accept.

    This asserted that a null argument passed through as an explicit null key,
    preserving the behaviour that existed when the test was written rather than
    checking it against the reference. jsonata-js raises T0410 for all five:
    `{"x": $abs(v)}` with `v: null` is an error, while the missing-field case
    above is `{}`. Routing builtins through their signatures made us agree
    (#102) -- `n` does not match the null symbol `l`.
    """
    expr = jsonatapy.compile(f'{{"x": ${fn}(v)}}')
    data = {"v": None}

    with pytest.raises(ValueError):
        expr.evaluate(data)

    # The tree-walker still returns null here: only the compiled/VM dispatch is
    # signature-validated so far, so this assertion is deliberately one-sided
    # until that path is migrated too. Tighten it to match the line above once
    # it is.
    jsonatapy._set_force_tree_walker(True)
    try:
        assert expr.evaluate(data) == {"x": None}
    finally:
        jsonatapy._set_force_tree_walker(False)


@pytest.mark.parametrize("fn", MATH_FUNCTIONS)
def test_arity_error_unaffected_by_undefined_first_arg(fn, force_tree_walker_toggle):
    """Calling with too many args and an undefined first arg must still raise the
    arity error identically on both paths (not short-circuit to undefined).
    round() accepts an optional precision arg, so it needs 3 args to overflow;
    the other four accept exactly 1, so 2 args overflows."""
    expr_src = f"${fn}(nothing, 2, 3)" if fn == "round" else f"${fn}(nothing, 2)"
    expr = jsonatapy.compile(expr_src)

    with pytest.raises(Exception) as default_exc_info:
        expr.evaluate(DATA)

    force_tree_walker_toggle(True)
    with pytest.raises(Exception) as tw_exc_info:
        expr.evaluate(DATA)
    force_tree_walker_toggle(False)

    assert str(default_exc_info.value) == str(tw_exc_info.value)
