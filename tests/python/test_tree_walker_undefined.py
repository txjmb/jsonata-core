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
  2. The `JSONATAPY_FORCE_TREE_WALKER=1` env toggle (src/lib.rs), which
     bypasses the bytecode VM on every call including `evaluate()` with no
     bindings. Read per-call, so it can be flipped via monkeypatch.setenv.

Both must agree with the default (VM) path: the result is undefined, i.e.
`None` at the Python boundary.
"""

import os

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
def test_env_forced_tree_walker_propagates_undefined(expr, monkeypatch):
    """JSONATAPY_FORCE_TREE_WALKER=1 forces the tree-walker even with bindings=None."""
    monkeypatch.setenv("JSONATAPY_FORCE_TREE_WALKER", "1")
    compiled = jsonatapy.compile(expr)
    result = compiled.evaluate(None)
    assert result is None


def test_force_tree_walker_env_var_is_read_per_call(monkeypatch):
    """The toggle isn't cached at compile time -- flipping it mid-test takes effect."""
    monkeypatch.delenv("JSONATAPY_FORCE_TREE_WALKER", raising=False)
    compiled = jsonatapy.compile("$abs(nothing)")
    assert compiled.evaluate(None) is None

    monkeypatch.setenv("JSONATAPY_FORCE_TREE_WALKER", "1")
    assert compiled.evaluate(None) is None

    monkeypatch.delenv("JSONATAPY_FORCE_TREE_WALKER")
    assert os.environ.get("JSONATAPY_FORCE_TREE_WALKER") is None
    assert compiled.evaluate(None) is None
