"""Parses --arg/--argjson CLI specs into a name -> value map for
evaluate_json_or_none() bindings."""

from __future__ import annotations

import json
import math
from typing import Any


class BindingError(Exception):
    """Raised when an --arg/--argjson spec is malformed."""


def _reject_constant(token: str) -> float:
    raise ValueError(f"{token} is not valid JSON")


def _finite_float(text: str) -> float:
    value = float(text)
    if not math.isfinite(value):
        raise ValueError(f"{text} is not valid JSON")
    return value


def parse_bindings(arg: list[str], argjson: list[str]) -> dict[str, Any]:
    """Parses --arg NAME=VALUE (string) and --argjson NAME=JSON (parsed) specs
    into a single name -> value dict. --argjson wins on name collision
    (applied second, matching src/bin/jsonata/bindings.rs's iteration order)."""
    bindings: dict[str, Any] = {}
    for spec in arg:
        name, value = _split_name_value(spec, "--arg")
        bindings[name] = value
    for spec in argjson:
        name, value = _split_name_value(spec, "--argjson")
        try:
            bindings[name] = json.loads(
                value, parse_constant=_reject_constant, parse_float=_finite_float
            )
        except ValueError as e:
            raise BindingError(f"--argjson {name}: invalid JSON value: {e}") from e
    return bindings


def _split_name_value(spec: str, flag: str) -> tuple[str, str]:
    if "=" not in spec:
        raise BindingError(f"{flag} expects NAME=VALUE, got: {spec}")
    name, _, value = spec.partition("=")
    if not name:
        raise BindingError(f"{flag} expects NAME=VALUE, got: {spec}")
    return name, value
