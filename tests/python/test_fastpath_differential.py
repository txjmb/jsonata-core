"""Differential tests aimed at the evaluator's optimisation fast paths.

The reference suite checks *semantics*; it does not check that an
optimisation produces the same answer as the code it replaces. Those are
different properties, and the second is where issue #97 hid: the fused
aggregate fast path returned a plausible wrong number for
``$sum(array.field)``, and the suite could not catch it because exactly one
of its ~1686 cases is shaped that way -- and that one declines the fast path.

An engine toggle alone would not have caught it either. The default engine
*is* the fused path, so any suite case that diverged would already be
failing. What was missing is inputs: expressions shaped to trigger each fast
path, run against payloads that break the assumptions those fast paths make.

The corpus has two halves. The first targets the optimisation fast paths as
described above. The second is an *operator matrix*: every binary operator
crossed with every value kind on both sides, added because the first half puts
only *sequences* around an operator -- that is what path expressions produce --
and every bug found in the resulting gap belonged to one family, an explicit
null being treated as undefined. ``null & "x"`` returned ``"x"`` instead of
``"nullx"`` and survived a corpus reporting zero divergences.

Expectations come from the pinned jsonata-js in ``tests/jsonata-js`` via
``scripts/gen_fastpath_corpus.js``. Every case runs twice -- once through the
default engine (bytecode VM where available) and once with the tree-walker
forced -- so an optimisation that diverges in only one of them still shows
up.

Cases that diverge today are listed in ``fastpath_known_divergences.json``
and marked xfail(strict). They are pre-existing conformance gaps, mostly in
shapes the reference suite does not cover, and they are *not* specific to the
fast paths -- each one reproduces in both engines. The list is a baseline to
shrink, not a specification: fix a divergence and its xfail turns into an
XPASS that tells you to delete the entry.

Error *codes* are not asserted. jsonata-core's messages do not carry codes
uniformly yet, so requiring them would bury the value-vs-error divergences
this harness exists to catch under message-format noise.
"""

import json
import math
import pathlib
import re

import jsonatapy
import pytest

_FIXTURES = pathlib.Path(__file__).parent.parent / "fixtures"
_corpus = json.loads((_FIXTURES / "fastpath_differential.json").read_text())
# The builtin matrix lives in its own fixture: one combined file crosses the
# repo's 500KB per-file CI limit.
_builtins = json.loads((_FIXTURES / "builtin_differential.json").read_text())
_known = json.loads((_FIXTURES / "fastpath_known_divergences.json").read_text())

DATASETS = {**_corpus["datasets"], **_builtins["datasets"]}
CASES = _corpus["cases"] + _builtins["cases"]
KNOWN_DIVERGENCES = {tuple(k.split("|", 3)): v for k, v in _known["divergences"].items()}

ENGINES = {False: "vm_preferred", True: "forced_tree_walker"}

# How the payload crosses the Python boundary. This matters as much as the
# engine: a dict arrives as a lazy Python view, and several fast paths -- the
# fused aggregate among them -- match on JValue::Object and decline outright,
# so a dict-only harness never reaches the code that carried issue #97.
ENTRIES = ("dict", "json")


# Marker distinguishing "raised" from any legitimate return value.
#
# It carries the message because the corpus records the *code* jsonata-js
# raised, and an error that merely raises is not conformance -- `$replace(1)`
# raising "Argument count mismatch" where the reference raises T0410 used to
# read as a match. `Raised` instances are compared with isinstance, never with
# `is`; there is no singleton.
class Raised:
    """An evaluation that raised, plus the code its message carried."""

    _CODE = re.compile(r"\b([TDSQ]\d{4})\b")

    def __init__(self, exc):
        self.message = str(exc)
        found = self._CODE.search(self.message)
        self.code = found.group(1) if found else None

    def __repr__(self):
        return f"raised {self.code or '(no code)'}: {self.message!r}"


# Sentinel distinguishing "undefined" from "null" on the json route only.
# evaluate_json_or_none returns Python None for an explicit JSON null and the
# raw None (no text at all) for undefined -- that is the one place this
# harness can tell the two apart, since the dict route hands values back
# through PyO3's own null/undefined collapse and genuinely cannot. Using this
# sentinel here (instead of collapsing to None like normalize() does) is what
# makes issues #109/#110/#112 visible: builtins that returned an explicit
# null where jsonata-js returns undefined.
UNDEFINED = object()


def case_key(case, engine, entry):
    """Baseline key. Includes the engine and the data-entry route: a case that
    diverges in only some of them is the interesting kind -- it means an
    optimisation disagrees with the path it replaces, rather than both sharing
    a conformance gap."""
    return (case["fastpath"], case["expr"], case["dataset"], f"{ENGINES[engine]}/{entry}")


def case_id(case):
    return f"{case['fastpath']}::{case['expr']}::{case['dataset']}"


def normalize(value):
    """Make jsonata-js and jsonatapy results comparable.

    Numbers compare as floats (JS has a single number type), and ``undefined``
    and ``null`` both arrive as ``None`` through the Python binding, so this
    does not distinguish them.
    """
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        return float(value)
    if isinstance(value, list):
        return [normalize(v) for v in value]
    if isinstance(value, dict):
        return {k: normalize(v) for k, v in value.items()}
    return value


def evaluate(case, entry):
    """Return the jsonatapy result for a case, or a `Raised` if it raised."""
    data = DATASETS[case["dataset"]]
    expr = jsonatapy.compile(case["expr"])
    try:
        if entry == "json":
            # evaluate_json_or_none returns raw=None for Undefined and the
            # text "null" for an explicit null -- surface that distinction
            # via UNDEFINED rather than collapsing it, see UNDEFINED above.
            raw = expr.evaluate_json_or_none(json.dumps(data))
            return UNDEFINED if raw is None else json.loads(raw)
        return expr.evaluate(data)
    except Exception as exc:  # any failure at all is a divergence signal
        return Raised(exc)


def diverges(case, entry="dict"):
    """Return a description of how the case diverges, or None if it matches."""
    got = evaluate(case, entry)
    expected = case["expected"]

    if expected["kind"] == "error":
        if not isinstance(got, Raised):
            return f"jsonata-js raised {expected['code'] or 'an error'}, jsonatapy returned {got!r}"
        # Both raised. When the reference names a code, ours must be the same
        # one: a JSONata error code is part of the contract, and callers
        # branch on it. When it does not -- a raw JavaScript TypeError
        # escaping the reference rather than a JSONata error -- there is
        # nothing to compare, and raising at all is the whole expectation.
        want = expected.get("code")
        if want is None or got.code == want:
            return None
        return (
            f"jsonata-js raised {want}, jsonatapy raised {got.code or 'no code'} ({got.message!r})"
        )

    if expected["kind"] == "nonfinite":
        # Infinity/NaN cannot round-trip through JSON, so the corpus records the
        # kind rather than a value.
        #
        # On the JSON route the result is serialised before we see it, and JSON
        # has no way to spell Infinity -- our serialiser emits null, exactly as
        # JavaScript's JSON.stringify does for the same value. So null is the
        # correct observation there, not a divergence; the dict route is what
        # actually checks the number.
        if entry == "json" and got is None:
            return None
        want = {"inf": float("inf"), "-inf": float("-inf"), "nan": float("nan")}[expected["value"]]
        if isinstance(got, Raised):
            return f"jsonata-js returned {want}, jsonatapy raised"
        if isinstance(got, float) and (got == want or (math.isnan(got) and math.isnan(want))):
            return None
        return f"jsonata-js returned {want}, jsonatapy {got!r}"

    # Only the json route can tell undefined and explicit null apart (see the
    # UNDEFINED sentinel comment above), so only it gets the sharper check.
    # The dict route keeps the historical collapsed comparison verbatim.
    if expected["kind"] == "undefined":
        if entry == "json":
            if got is UNDEFINED:
                return None
            if isinstance(got, Raised):
                return "jsonata-js returned undefined, jsonatapy raised"
            return f"jsonata-js undefined, jsonatapy returned {got!r}"
        want = None
    else:
        want = expected["value"]
        if entry == "json" and got is UNDEFINED:
            return f"jsonata-js returned {want!r}, jsonatapy returned undefined"

    if isinstance(got, Raised):
        return f"jsonata-js returned {want!r}, jsonatapy raised"
    if normalize(got) != normalize(want):
        return f"jsonata-js {want!r}, jsonatapy {got!r}"
    return None


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    """Run a case through each evaluation path."""
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


@pytest.fixture(params=ENTRIES, ids=ENTRIES)
def entry(request):
    """Run a case through each data-entry route."""
    return request.param


@pytest.mark.parametrize("case", CASES, ids=case_id)
def test_fastpath_matches_reference(case, engine, entry, request):
    key = case_key(case, engine, entry)
    if key in KNOWN_DIVERGENCES:
        request.node.add_marker(pytest.mark.xfail(strict=True, reason=KNOWN_DIVERGENCES[key]))
    detail = diverges(case, entry)
    assert detail is None, f"{case['expr']} on {case['dataset']} ({entry}): {detail}"
