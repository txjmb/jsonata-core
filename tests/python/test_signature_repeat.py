"""Tests for the jsonata-js 2.2.1 signature '+' (repeat) and '-' (context) modifiers.

These exercise src/signature.rs's regex-based matching engine directly through
JSONata expressions, covering cases the reference suite's 6 new
function-signatures cases (035-040) don't fully exercise on their own (see
docs/superpowers/plans/2026-07-04-jsonata-2.2.1-phase1-signatures.md).
"""

import jsonatapy


def test_repeat_param_basic():
    result = jsonatapy.evaluate(
        'λ($arg1, $arg2)<n+n:o>{{"a": $arg1, "b": $arg2}}(1, 2, 3)', None
    )
    assert result == {"a": 1, "b": 2}


def test_repeat_param_with_array_subtype():
    result = jsonatapy.evaluate(
        'λ($arg1, $arg2)<a<n>+:o>{{"a": $arg1, "b": $arg2}}([1, 2], [3, 4], [5, 6])',
        None,
    )
    assert result == {"a": [1, 2], "b": [3, 4]}


def test_context_fallback_bare_call_uses_context():
    # Bare (non-dot-chained) lambda call: this is the only shape that
    # actually exercises signature.rs's own '-' context-substitution path
    # (dot-chained calls have a separate, unrelated implicit-arg mechanism).
    result = jsonatapy.evaluate(
        'λ($arg1, $arg2, $arg3)<n+s-:a<n>>{[$arg1, $arg2, $arg3]}(1, 2)', "b"
    )
    assert result == [1, 2, "b"]


def test_context_fallback_not_used_when_arg_supplied():
    result = jsonatapy.evaluate(
        'λ($arg1, $arg2, $arg3)<n+s-:a<n>>{[$arg1, $arg2, $arg3]}(1, 2, "a")', "b"
    )
    assert result == [1, 2, "a"]


def test_repeat_param_rejects_wrong_type():
    import pytest

    with pytest.raises(ValueError, match="T0410"):
        jsonatapy.evaluate(
            'λ($arg1, $arg2)<n+n:o>{{"a": $arg1, "b": $arg2}}(1, "x")', None
        )
