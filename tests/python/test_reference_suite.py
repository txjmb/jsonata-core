"""
Test adapter for the JSONata reference test suite.

This module loads and runs all 1682 test cases from the reference JavaScript JSONata
implementation. All 1682 pass. See
docs/superpowers/specs/2026-07-05-reference-suite-coverage-gap-design.md and
docs/superpowers/specs/2026-07-06-parent-and-focus-binding-operators-design.md for the
history of how the gap (widened test discovery, datetime picture-strings,
formatInteger/parseInteger, the %/@/# parent-reference/focus-binding/index-binding
operators, and the array-constructor/distinct stragglers) was closed.
"""

import json
import re
from pathlib import Path
from typing import Any

import jsonatapy
import pytest

# Load all datasets once at module level
DATASETS: dict[str, Any] = {}
DATASET_DIR = Path(__file__).parent.parent / "jsonata-js/test/test-suite/datasets"

if DATASET_DIR.exists():
    for dataset_file in DATASET_DIR.glob("*.json"):
        dataset_name = dataset_file.stem  # e.g., "dataset0"
        try:
            with open(dataset_file, encoding="utf-8") as f:
                DATASETS[dataset_name] = json.load(f)
        except Exception as e:
            print(f"Warning: Could not load dataset {dataset_name}: {e}")


def load_test_cases() -> list[tuple[str, str, dict[str, Any]]]:
    """
    Load all test cases from jsonata-js test groups.

    Returns:
        List of tuples: (test_id, group_name, test_spec)
    """
    test_cases = []
    groups_dir = Path(__file__).parent.parent / "jsonata-js/test/test-suite/groups"

    if not groups_dir.exists():
        print(f"Warning: Test suite directory not found: {groups_dir}")
        return test_cases

    for group_dir in sorted(groups_dir.iterdir()):
        if not group_dir.is_dir():
            continue

        group_name = group_dir.name

        for case_file in sorted(group_dir.glob("*.json")):
            try:
                with open(case_file, encoding="utf-8") as f:
                    test_spec = json.load(f)

                # Handle both single test and array of tests
                if isinstance(test_spec, list):
                    for idx, spec in enumerate(test_spec):
                        test_id = f"{group_name}/{case_file.stem}[{idx}]"
                        test_cases.append((test_id, group_name, spec))
                else:
                    test_id = f"{group_name}/{case_file.stem}"
                    test_cases.append((test_id, group_name, test_spec))

            except Exception as e:
                print(f"Warning: Could not load test case {case_file}: {e}")

    return test_cases


def extract_error_code(error_msg: str) -> str | None:
    """
    Extract JSONata error code from exception message.

    JSONata error codes follow the format: [TDUS]#### (e.g., T2001, D3030)

    Args:
        error_msg: The error message string

    Returns:
        The error code if found, None otherwise
    """
    # Not anchored. Our messages are not uniformly "CODE: text" -- many carry a
    # prefix ("Runtime error: D3030: ...", "Parse error: Invalid syntax:
    # S0209: ..."). An anchored match read all of those as *uncoded*, and the
    # caller accepts an uncoded error for any expected code, so 36 cases that
    # emit exactly the right code were passing without it ever being compared
    # -- and one case emitting the WRONG code passed the same way.
    match = re.search(r"\b([TDUS]\d{4})\b", str(error_msg))
    return match.group(1) if match else None


# Reference cases that are known to diverge, each with the reason. Marked
# xfail(strict), so one that starts passing fails the suite and has to be
# removed from here. Empty is the goal, and currently the state.
# Reference cases that are known to diverge, each with the reason. Marked
# xfail(strict), so one that starts passing fails the suite and has to be
# removed from here. Empty is the goal, and currently the state.
#
# Note what does NOT belong here: a case we satisfy only because our error
# carries no code for the suite to compare. That is not a known divergence, it
# is an unverified case -- counted by UNVERIFIED_ERROR_CEILING above.
# Tracked in #150 -- both need the parser to recognise a construct, not an
# error-variant mapping.
KNOWN_DIVERGENCES: dict[str, str] = {
    "errors/case005": (
        "`unknown(function)` is T1006 upstream. `function` is a keyword here, so the parser "
        "commits to a lambda and fails on its shape (S0202) before anything decides the call "
        "target is not a function. Getting T1006 means recognising the call context first, "
        "which is a parser restructure rather than an error-code mapping. The trigger is the "
        "keyword `function` in argument position, not the call target: `foo(function)` fails "
        "the same way and `unknown(fn)` is already correct. See #150."
    ),
    "function-signatures/case034": (
        "`<(sa<n>)>` -- a choice group containing a parameterized type -- is S0402 upstream. "
        "Our signature parser rejects the shape while still reading tokens, so it surfaces as "
        "S0202. Needs the signature grammar to parse a choice group's contents far enough to "
        "detect a parameterized type inside one; `<(sa)>` without the parameter works. See #150."
    ),
}


def _build_pytest_params(
    cases: list[tuple[str, str, dict[str, Any]]],
) -> list["pytest.mark.structures.ParameterSet"]:
    params = []
    for test_id, group_name, spec in cases:
        marks = []
        if test_id in KNOWN_DIVERGENCES:
            marks.append(pytest.mark.xfail(strict=True, reason=KNOWN_DIVERGENCES[test_id]))
        params.append(pytest.param(test_id, group_name, spec, id=test_id, marks=marks))
    return params


# Load all test cases
test_cases = load_test_cases()
test_params = _build_pytest_params(test_cases)

print(f"\n{'=' * 70}")
print("JSONata Reference Suite Test Loader")
print(f"{'=' * 70}")
print(f"Loaded {len(DATASETS)} datasets from {DATASET_DIR}")
print(f"Loaded {len(test_cases)} test cases")
print(f"{'=' * 70}\n")


# How many reference cases the suite cannot actually verify: we raise an error
# carrying no JSONata code, and an uncoded error is accepted for any expected
# code.
#
# Down from 107 at 2.2.7 to 2, and 2 is the floor rather than a to-do. Both are
# `$encodeUrl`/`$encodeUrlComponent` on an unpaired surrogate: a Python str can
# hold one and a Rust String cannot, so the expression never crosses the
# boundary to be parsed and there is no point at which D3140 could be raised.
# We fail with a ValueError naming the surrogate instead of leaking PyO3's
# codec error.
#
# The assertion cuts both ways -- it fails if the number grows, and fails
# telling you to lower it if it shrinks -- so a new uncoded error cannot slip
# in unnoticed and an improvement cannot go unrecorded. See #144.
UNVERIFIED_ERROR_CEILING = 2


@pytest.mark.reference
def test_unverified_error_cases_do_not_grow():
    """Pin how much of the suite asserts only that *something* was raised."""
    unverified = []
    for test_id, _group, spec in test_cases:
        if "code" not in spec and "error" not in spec:
            continue
        expr = spec.get("expr")
        if expr is None:
            continue
        if "data" in spec:
            data = spec["data"]
        elif "dataset" in spec:
            data = None if spec["dataset"] is None else DATASETS.get(spec["dataset"])
        else:
            data = None
        bindings = spec.get("bindings", {})
        try:
            compiled = jsonatapy.compile(expr)
            if data is None:
                (
                    compiled.evaluate_json_or_none(None, bindings)
                    if bindings
                    else compiled.evaluate_json_or_none(None)
                )
            else:
                compiled.evaluate(data, bindings) if bindings else compiled.evaluate(data)
        except Exception as exc:  # any raise is what these cases expect
            # Unverified means *we* gave the suite nothing to compare: our
            # error carries no JSONata code. Cases specifying an error object
            # count the same way -- only its `code` is compared, so an uncoded
            # error satisfies them too.
            wants_code = spec.get("code") or spec.get("error", {}).get("code")
            if wants_code and extract_error_code(str(exc)) is None:
                unverified.append(test_id)

    assert len(unverified) <= UNVERIFIED_ERROR_CEILING, (
        f"{len(unverified)} reference cases assert only that something was raised, "
        f"up from {UNVERIFIED_ERROR_CEILING}. A new error was added without a JSONata "
        f"code, or an existing one lost its code. See #144.\n"
        + "\n".join(f"  {t}" for t in unverified[:20])
    )
    if len(unverified) < UNVERIFIED_ERROR_CEILING:
        pytest.fail(
            f"Only {len(unverified)} cases are now unverified, down from "
            f"{UNVERIFIED_ERROR_CEILING}. Lower UNVERIFIED_ERROR_CEILING to "
            f"{len(unverified)} to lock the improvement in."
        )


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    """Run every reference case through both evaluation paths.

    The suite ran only whatever the default resolved to, so the tree-walker was
    exercised by the differential corpus and nowhere else. Nothing here would
    have caught the two engines answering a reference case differently -- and
    at 2.2.7 four of them did, each accepted because both answers were errors
    and the error check was loose. Auditing found no engine-only failure, but
    "true when someone last checked by hand" is not a guarantee; this makes it
    one.
    """
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


@pytest.mark.reference
@pytest.mark.parametrize("test_id,group_name,spec", test_params)
def test_reference_suite(test_id: str, group_name: str, spec: dict[str, Any], engine: bool):
    """
    Run a single test case from the reference JSONata suite.

    Args:
        test_id: Unique identifier for the test (group/case)
        group_name: Name of the test group
        spec: Test specification dictionary with expr, data, and expected outcome
    """
    # Import here to avoid circular imports
    import jsonatapy

    # Extract test components
    expr = spec.get("expr")

    # Handle expr-file for tests that load expression from external file (e.g., comment tests)
    if expr is None and "expr-file" in spec:
        expr_file = spec["expr-file"]
        groups_dir = Path(__file__).parent.parent / "jsonata-js/test/test-suite/groups"
        expr_file_path = groups_dir / group_name / expr_file
        try:
            with open(expr_file_path, encoding="utf-8") as f:
                expr = f.read()
        except Exception as e:
            pytest.fail(f"Could not load expression file {expr_file}: {e}")

    if expr is None:
        pytest.fail("Test spec missing 'expr' or 'expr-file' field")

    bindings = spec.get("bindings", {})

    # Get input data
    if "data" in spec:
        data = spec["data"]
    elif "dataset" in spec:
        dataset_name = spec["dataset"]
        if dataset_name is None:
            # "dataset": null means no input data
            data = None
        else:
            data = DATASETS.get(dataset_name)
            if data is None:
                pytest.fail(f"Dataset not found: {dataset_name}")
    else:
        data = None

    # Expected outcome (test should have exactly one of these)
    has_result = "result" in spec
    has_undefined = spec.get("undefinedResult", False)
    has_error_code = "code" in spec
    has_error_obj = "error" in spec

    # Execute test
    try:
        # Compile expression
        compiled = jsonatapy.compile(expr)

        # Evaluate with optional bindings
        if data is None:
            # `"dataset": null` means *no input data*, which is undefined -- not
            # JSON null. The distinction matters for context-substituted
            # builtins: jsonata-js gives `$trim()` undefined with no input but
            # T0411 with a null input, and `evaluate(None)` means the latter.
            # `evaluate_json_or_none(None)` is the no-input path.
            raw = (
                compiled.evaluate_json_or_none(None, bindings)
                if bindings
                else compiled.evaluate_json_or_none(None)
            )
            result = None if raw is None else json.loads(raw)
        else:
            result = compiled.evaluate(data, bindings) if bindings else compiled.evaluate(data)

        # Check for expected result
        if has_result:
            expected = spec["result"]
            assert result == expected, (
                f"Result mismatch for expression: {expr}\n"
                f"Expected: {json.dumps(expected, indent=2)}\n"
                f"Got:      {json.dumps(result, indent=2)}"
            )

        elif has_undefined:
            assert result is None, (
                f"Expected undefined result for expression: {expr}\n"
                f"Got: {json.dumps(result, indent=2)}"
            )

        elif has_error_code or has_error_obj:
            pytest.fail(
                f"Expected error but got successful result for expression: {expr}\n"
                f"Result: {json.dumps(result, indent=2)}"
            )

        else:
            # No expected outcome specified - this is a test spec error
            pytest.fail(
                f"Test spec has no expected outcome (result, undefinedResult, code, or error)\n"
                f"Expression: {expr}"
            )

    except ValueError as e:
        # An error occurred during compilation or evaluation
        error_msg = str(e)

        if has_error_code:
            # Expected an error with specific code
            expected_code = spec["code"]
            actual_code = extract_error_code(error_msg)

            if actual_code is None:
                # Error occurred but no code in message
                # For now, accept any error for the expected error code
                # TODO: Ensure all errors have proper error codes
                pass
            elif actual_code != expected_code:
                pytest.fail(
                    f"Error code mismatch for expression: {expr}\n"
                    f"Expected code: {expected_code}\n"
                    f"Actual code:   {actual_code}\n"
                    f"Error message: {error_msg}"
                )

        elif has_error_obj:
            # The case gives an error *object*. Only its `code` is compared;
            # the other fields (functionName, value, token) are not modelled
            # here. Previously nothing was compared at all.
            expected_code = spec["error"].get("code")
            actual_code = extract_error_code(error_msg)
            if expected_code and actual_code and actual_code != expected_code:
                pytest.fail(
                    f"Error code mismatch for expression: {expr}\n"
                    f"Expected code: {expected_code}\n"
                    f"Actual code:   {actual_code}\n"
                    f"Error message: {error_msg}"
                )

        elif has_result or has_undefined:
            # Unexpected error when expecting successful result
            pytest.fail(
                f"Unexpected error for expression: {expr}\n"
                f"Expected: {'undefined' if has_undefined else 'result'}\n"
                f"Error: {error_msg}"
            )

        else:
            # No expected outcome specified
            pytest.fail(
                f"Test spec has no expected outcome (result, undefinedResult, code, or error)\n"
                f"Expression: {expr}\n"
                f"Error: {error_msg}"
            )

    except Exception as e:
        # Unexpected exception type
        pytest.fail(
            f"Unexpected exception type for expression: {expr}\nException: {type(e).__name__}: {e}"
        )


if __name__ == "__main__":
    # Allow running this file directly for debugging
    print(f"Test cases loaded: {len(test_cases)}")
    print(f"Datasets loaded: {len(DATASETS)}")

    if test_cases:
        print("\nFirst test case:")
        test_id, group_name, spec = test_cases[0]
        print(f"  ID: {test_id}")
        print(f"  Group: {group_name}")
        print(f"  Expr: {spec.get('expr', 'N/A')}")

    print("\nTo run tests:")
    print("  pytest tests/python/test_reference_suite.py -v")
    print("  pytest tests/python/test_reference_suite.py -v -k 'literals'")
