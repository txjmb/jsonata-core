"""
Regression tests: math/length builtins must propagate undefined through the
tree-walking evaluator, not raise a TypeError.

Root cause: `evaluate_function_call`'s per-function dispatch arms for `abs`,
`ceil`, `floor`, `round`, `sqrt`, and `length` checked argument count and then
matched on the argument's concrete type, but had no explicit
`is_undefined()` short-circuit -- unlike their neighboring arms
(`uppercase`/`lowercase`/`number`) which already had one. A missing path
(e.g. `$abs(nothing)` where `nothing` doesn't exist) evaluates to
`JValue::Undefined`, which fell through to each arm's catch-all `_ =>` branch
and raised "... requires a number argument" instead of returning undefined.

The bytecode VM (`call_pure_builtin`, src/evaluator.rs) never had this bug --
it already special-cased `Null | Undefined` inline for these functions.

Two ways to force the tree-walker are exercised here:
  1. Passing a non-empty `bindings` dict to `evaluate()` (works on any build).
  2. The tree-walker toggle (src/lib.rs), which bypasses the bytecode VM on
     every call including `evaluate()` with no bindings. Seeded from the
     JSONATAPY_FORCE_TREE_WALKER env var at import time and flipped at
     runtime via the private `jsonatapy._set_force_tree_walker` hook
     (a relaxed atomic, so it costs nothing per evaluation -- issue #74).

Both must agree with the default (VM) path: the result is undefined, i.e.
`None` at the Python boundary.
"""

import os
import subprocess
import sys

import jsonatapy
import pytest

# (expression, description) -- each references a field that does not exist
# in the (empty) input data, so the builtin's argument evaluates to undefined.
UNDEFINED_INPUT_CASES = [
    "$abs(nothing)",
    "$ceil(nothing)",
    "$floor(nothing)",
    "$length(missing)",
    "$round(unknown)",
    "$sqrt(nothing)",
]


@pytest.mark.parametrize("expr", UNDEFINED_INPUT_CASES)
def test_default_path_propagates_undefined(expr):
    """Sanity baseline: the default (VM-preferred) path already does this correctly."""
    result = jsonatapy.evaluate(expr, None)
    assert result is None


@pytest.mark.parametrize("expr", UNDEFINED_INPUT_CASES)
def test_bindings_forced_tree_walker_propagates_undefined(expr):
    """Non-empty `bindings` routes evaluate() through the tree-walker on any build."""
    compiled = jsonatapy.compile(expr)
    result = compiled.evaluate(None, bindings={"__unused": 1})
    assert result is None


@pytest.mark.parametrize("expr", UNDEFINED_INPUT_CASES)
def test_toggle_forced_tree_walker_propagates_undefined(expr, force_tree_walker_toggle):
    """The runtime toggle forces the tree-walker even with bindings=None."""
    force_tree_walker_toggle(True)
    compiled = jsonatapy.compile(expr)
    result = compiled.evaluate(None)
    assert result is None


def test_force_toggle_takes_effect_without_recompiling(force_tree_walker_toggle):
    """The toggle isn't cached per expression -- flipping it mid-test affects an
    already-compiled expression's subsequent evaluations (the property every
    compare-both-paths-on-one-expression test in this suite relies on)."""
    compiled = jsonatapy.compile("$abs(nothing)")
    assert jsonatapy._get_force_tree_walker() is False
    assert compiled.evaluate(None) is None

    force_tree_walker_toggle(True)
    assert jsonatapy._get_force_tree_walker() is True
    assert compiled.evaluate(None) is None


def test_env_var_seeds_toggle_at_import():
    """JSONATAPY_FORCE_TREE_WALKER=1 in the process environment turns the
    toggle on from import time -- how the CI tree-walker suite job forces the
    whole process. (Runs in a subprocess: the toggle is seeded once at module
    import, so it can't be tested by mutating os.environ in this process.)"""
    code = "import jsonatapy; print(jsonatapy._get_force_tree_walker())"
    for env_val, expected in (("1", "True"), ("", "False"), ("0", "False")):
        env = dict(os.environ, JSONATAPY_FORCE_TREE_WALKER=env_val)
        out = subprocess.run([sys.executable, "-c", code], env=env, capture_output=True, text=True)
        assert out.stdout.strip() == expected, (env_val, out.stdout, out.stderr)
