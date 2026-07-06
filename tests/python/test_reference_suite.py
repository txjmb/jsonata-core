"""
Test adapter for the JSONata reference test suite.

This module loads and runs all 1682 test cases from the reference JavaScript JSONata
implementation. 1363 currently pass; the remaining 319 are xfailed pending fixes tracked
by phase in docs/superpowers/specs/2026-07-05-reference-suite-coverage-gap-design.md.
"""

import json
import re
from pathlib import Path
from typing import Any

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
    # Error format: "T2001: Unknown function: foo"
    match = re.match(r"^([TDUS]\d{4}):", str(error_msg))
    return match.group(1) if match else None


# Coverage gap remediation tracking (see
# docs/superpowers/specs/2026-07-05-reference-suite-coverage-gap-design.md).
#
# load_test_cases() used to only glob "case*.json", silently skipping 20 files / 408
# cases under other naming conventions. Phase 0 widened the glob to pick up all of
# them; of the 408, 325 fail against the current implementation for reasons owned by
# later phases of that spec. Each entry below is xfailed with a pointer to the phase
# that will fix it, so the suite stays green while the gap is tracked explicitly
# instead of silently hidden again.
_XFAIL_PHASE_BY_GROUP = {
    "function-fromMillis": "Phase 1: datetime picture-string engine ($fromMillis picture form / panics)",
    "function-tomillis": "Phase 1: datetime picture-string engine ($toMillis picture parsing / panics)",
    "function-formatInteger": "Phase 2: $formatInteger is unimplemented",
    "function-parseInteger": "Phase 2: $parseInteger is unimplemented",
    "parent-operator": "Phase 3: % parent-reference operator not parsed",
    "joins": "Phase 4: @ tuple-stream binding parse gaps / tuple-wrapper leak",
    "array-constructor": "Phase 5: untriaged straggler",
    "function-distinct": "Phase 5: untriaged straggler",
    "flattening": "Phase 5: untriaged straggler",
}

_XFAIL_TEST_IDS = {
    "array-constructor/array-sequences[2]",
    "array-constructor/array-sequences[4]",
    "flattening/sequence-of-arrays[1]",
    "function-distinct/distinct[4]",
    "function-formatInteger/formatInteger[0]",
    "function-formatInteger/formatInteger[10]",
    "function-formatInteger/formatInteger[11]",
    "function-formatInteger/formatInteger[12]",
    "function-formatInteger/formatInteger[13]",
    "function-formatInteger/formatInteger[14]",
    "function-formatInteger/formatInteger[16]",
    "function-formatInteger/formatInteger[17]",
    "function-formatInteger/formatInteger[18]",
    "function-formatInteger/formatInteger[19]",
    "function-formatInteger/formatInteger[1]",
    "function-formatInteger/formatInteger[20]",
    "function-formatInteger/formatInteger[21]",
    "function-formatInteger/formatInteger[22]",
    "function-formatInteger/formatInteger[23]",
    "function-formatInteger/formatInteger[24]",
    "function-formatInteger/formatInteger[25]",
    "function-formatInteger/formatInteger[26]",
    "function-formatInteger/formatInteger[27]",
    "function-formatInteger/formatInteger[28]",
    "function-formatInteger/formatInteger[29]",
    "function-formatInteger/formatInteger[2]",
    "function-formatInteger/formatInteger[30]",
    "function-formatInteger/formatInteger[31]",
    "function-formatInteger/formatInteger[32]",
    "function-formatInteger/formatInteger[33]",
    "function-formatInteger/formatInteger[34]",
    "function-formatInteger/formatInteger[35]",
    "function-formatInteger/formatInteger[36]",
    "function-formatInteger/formatInteger[37]",
    "function-formatInteger/formatInteger[38]",
    "function-formatInteger/formatInteger[39]",
    "function-formatInteger/formatInteger[3]",
    "function-formatInteger/formatInteger[40]",
    "function-formatInteger/formatInteger[41]",
    "function-formatInteger/formatInteger[42]",
    "function-formatInteger/formatInteger[43]",
    "function-formatInteger/formatInteger[44]",
    "function-formatInteger/formatInteger[45]",
    "function-formatInteger/formatInteger[46]",
    "function-formatInteger/formatInteger[47]",
    "function-formatInteger/formatInteger[48]",
    "function-formatInteger/formatInteger[49]",
    "function-formatInteger/formatInteger[4]",
    "function-formatInteger/formatInteger[50]",
    "function-formatInteger/formatInteger[51]",
    "function-formatInteger/formatInteger[52]",
    "function-formatInteger/formatInteger[53]",
    "function-formatInteger/formatInteger[54]",
    "function-formatInteger/formatInteger[55]",
    "function-formatInteger/formatInteger[56]",
    "function-formatInteger/formatInteger[57]",
    "function-formatInteger/formatInteger[58]",
    "function-formatInteger/formatInteger[59]",
    "function-formatInteger/formatInteger[5]",
    "function-formatInteger/formatInteger[60]",
    "function-formatInteger/formatInteger[61]",
    "function-formatInteger/formatInteger[62]",
    "function-formatInteger/formatInteger[63]",
    "function-formatInteger/formatInteger[6]",
    "function-formatInteger/formatInteger[7]",
    "function-formatInteger/formatInteger[8]",
    "function-formatInteger/formatInteger[9]",
    "function-fromMillis/formatDateTime[0]",
    "function-fromMillis/formatDateTime[10]",
    "function-fromMillis/formatDateTime[11]",
    "function-fromMillis/formatDateTime[12]",
    "function-fromMillis/formatDateTime[13]",
    "function-fromMillis/formatDateTime[14]",
    "function-fromMillis/formatDateTime[15]",
    "function-fromMillis/formatDateTime[16]",
    "function-fromMillis/formatDateTime[17]",
    "function-fromMillis/formatDateTime[18]",
    "function-fromMillis/formatDateTime[19]",
    "function-fromMillis/formatDateTime[1]",
    "function-fromMillis/formatDateTime[20]",
    "function-fromMillis/formatDateTime[21]",
    "function-fromMillis/formatDateTime[22]",
    "function-fromMillis/formatDateTime[23]",
    "function-fromMillis/formatDateTime[24]",
    "function-fromMillis/formatDateTime[25]",
    "function-fromMillis/formatDateTime[27]",
    "function-fromMillis/formatDateTime[28]",
    "function-fromMillis/formatDateTime[29]",
    "function-fromMillis/formatDateTime[2]",
    "function-fromMillis/formatDateTime[30]",
    "function-fromMillis/formatDateTime[31]",
    "function-fromMillis/formatDateTime[32]",
    "function-fromMillis/formatDateTime[33]",
    "function-fromMillis/formatDateTime[34]",
    "function-fromMillis/formatDateTime[35]",
    "function-fromMillis/formatDateTime[36]",
    "function-fromMillis/formatDateTime[37]",
    "function-fromMillis/formatDateTime[38]",
    "function-fromMillis/formatDateTime[39]",
    "function-fromMillis/formatDateTime[3]",
    "function-fromMillis/formatDateTime[40]",
    "function-fromMillis/formatDateTime[41]",
    "function-fromMillis/formatDateTime[42]",
    "function-fromMillis/formatDateTime[43]",
    "function-fromMillis/formatDateTime[44]",
    "function-fromMillis/formatDateTime[45]",
    "function-fromMillis/formatDateTime[46]",
    "function-fromMillis/formatDateTime[47]",
    "function-fromMillis/formatDateTime[48]",
    "function-fromMillis/formatDateTime[49]",
    "function-fromMillis/formatDateTime[4]",
    "function-fromMillis/formatDateTime[50]",
    "function-fromMillis/formatDateTime[51]",
    "function-fromMillis/formatDateTime[52]",
    "function-fromMillis/formatDateTime[53]",
    "function-fromMillis/formatDateTime[54]",
    "function-fromMillis/formatDateTime[55]",
    "function-fromMillis/formatDateTime[56]",
    "function-fromMillis/formatDateTime[57]",
    "function-fromMillis/formatDateTime[58]",
    "function-fromMillis/formatDateTime[59]",
    "function-fromMillis/formatDateTime[5]",
    "function-fromMillis/formatDateTime[60]",
    "function-fromMillis/formatDateTime[61]",
    "function-fromMillis/formatDateTime[62]",
    "function-fromMillis/formatDateTime[63]",
    "function-fromMillis/formatDateTime[64]",
    "function-fromMillis/formatDateTime[65]",
    "function-fromMillis/formatDateTime[66]",
    "function-fromMillis/formatDateTime[67]",
    "function-fromMillis/formatDateTime[68]",
    "function-fromMillis/formatDateTime[6]",
    "function-fromMillis/formatDateTime[7]",
    "function-fromMillis/formatDateTime[8]",
    "function-fromMillis/formatDateTime[9]",
    "function-fromMillis/isoWeekDate[0]",
    "function-fromMillis/isoWeekDate[10]",
    "function-fromMillis/isoWeekDate[11]",
    "function-fromMillis/isoWeekDate[12]",
    "function-fromMillis/isoWeekDate[13]",
    "function-fromMillis/isoWeekDate[14]",
    "function-fromMillis/isoWeekDate[15]",
    "function-fromMillis/isoWeekDate[16]",
    "function-fromMillis/isoWeekDate[17]",
    "function-fromMillis/isoWeekDate[18]",
    "function-fromMillis/isoWeekDate[1]",
    "function-fromMillis/isoWeekDate[2]",
    "function-fromMillis/isoWeekDate[3]",
    "function-fromMillis/isoWeekDate[4]",
    "function-fromMillis/isoWeekDate[5]",
    "function-fromMillis/isoWeekDate[6]",
    "function-fromMillis/isoWeekDate[7]",
    "function-fromMillis/isoWeekDate[8]",
    "function-fromMillis/isoWeekDate[9]",
    "function-parseInteger/parseInteger[0]",
    "function-parseInteger/parseInteger[10]",
    "function-parseInteger/parseInteger[11]",
    "function-parseInteger/parseInteger[12]",
    "function-parseInteger/parseInteger[13]",
    "function-parseInteger/parseInteger[14]",
    "function-parseInteger/parseInteger[15]",
    "function-parseInteger/parseInteger[16]",
    "function-parseInteger/parseInteger[17]",
    "function-parseInteger/parseInteger[18]",
    "function-parseInteger/parseInteger[19]",
    "function-parseInteger/parseInteger[1]",
    "function-parseInteger/parseInteger[20]",
    "function-parseInteger/parseInteger[21]",
    "function-parseInteger/parseInteger[22]",
    "function-parseInteger/parseInteger[23]",
    "function-parseInteger/parseInteger[24]",
    "function-parseInteger/parseInteger[25]",
    "function-parseInteger/parseInteger[26]",
    "function-parseInteger/parseInteger[27]",
    "function-parseInteger/parseInteger[28]",
    "function-parseInteger/parseInteger[29]",
    "function-parseInteger/parseInteger[2]",
    "function-parseInteger/parseInteger[30]",
    "function-parseInteger/parseInteger[31]",
    "function-parseInteger/parseInteger[32]",
    "function-parseInteger/parseInteger[33]",
    "function-parseInteger/parseInteger[34]",
    "function-parseInteger/parseInteger[35]",
    "function-parseInteger/parseInteger[36]",
    "function-parseInteger/parseInteger[37]",
    "function-parseInteger/parseInteger[38]",
    "function-parseInteger/parseInteger[39]",
    "function-parseInteger/parseInteger[3]",
    "function-parseInteger/parseInteger[40]",
    "function-parseInteger/parseInteger[41]",
    "function-parseInteger/parseInteger[42]",
    "function-parseInteger/parseInteger[43]",
    "function-parseInteger/parseInteger[44]",
    "function-parseInteger/parseInteger[45]",
    "function-parseInteger/parseInteger[46]",
    "function-parseInteger/parseInteger[47]",
    "function-parseInteger/parseInteger[48]",
    "function-parseInteger/parseInteger[49]",
    "function-parseInteger/parseInteger[4]",
    "function-parseInteger/parseInteger[50]",
    "function-parseInteger/parseInteger[51]",
    "function-parseInteger/parseInteger[52]",
    "function-parseInteger/parseInteger[53]",
    "function-parseInteger/parseInteger[54]",
    "function-parseInteger/parseInteger[55]",
    "function-parseInteger/parseInteger[56]",
    "function-parseInteger/parseInteger[57]",
    "function-parseInteger/parseInteger[58]",
    "function-parseInteger/parseInteger[59]",
    "function-parseInteger/parseInteger[5]",
    "function-parseInteger/parseInteger[6]",
    "function-parseInteger/parseInteger[7]",
    "function-parseInteger/parseInteger[8]",
    "function-parseInteger/parseInteger[9]",
    "function-tomillis/parseDateTime[10]",
    "function-tomillis/parseDateTime[11]",
    "function-tomillis/parseDateTime[12]",
    "function-tomillis/parseDateTime[13]",
    "function-tomillis/parseDateTime[14]",
    "function-tomillis/parseDateTime[15]",
    "function-tomillis/parseDateTime[16]",
    "function-tomillis/parseDateTime[17]",
    "function-tomillis/parseDateTime[18]",
    "function-tomillis/parseDateTime[19]",
    "function-tomillis/parseDateTime[1]",
    "function-tomillis/parseDateTime[20]",
    "function-tomillis/parseDateTime[21]",
    "function-tomillis/parseDateTime[22]",
    "function-tomillis/parseDateTime[23]",
    "function-tomillis/parseDateTime[24]",
    "function-tomillis/parseDateTime[25]",
    "function-tomillis/parseDateTime[26]",
    "function-tomillis/parseDateTime[27]",
    "function-tomillis/parseDateTime[28]",
    "function-tomillis/parseDateTime[29]",
    "function-tomillis/parseDateTime[2]",
    "function-tomillis/parseDateTime[30]",
    "function-tomillis/parseDateTime[31]",
    "function-tomillis/parseDateTime[32]",
    "function-tomillis/parseDateTime[33]",
    "function-tomillis/parseDateTime[34]",
    "function-tomillis/parseDateTime[3]",
    "function-tomillis/parseDateTime[41]",
    "function-tomillis/parseDateTime[42]",
    "function-tomillis/parseDateTime[43]",
    "function-tomillis/parseDateTime[44]",
    "function-tomillis/parseDateTime[45]",
    "function-tomillis/parseDateTime[46]",
    "function-tomillis/parseDateTime[4]",
    "function-tomillis/parseDateTime[5]",
    "function-tomillis/parseDateTime[6]",
    "function-tomillis/parseDateTime[7]",
    "function-tomillis/parseDateTime[8]",
    "function-tomillis/parseDateTime[9]",
    "joins/employee-map-reduce[0]",
    "joins/employee-map-reduce[10]",
    "joins/employee-map-reduce[11]",
    "joins/employee-map-reduce[1]",
    "joins/employee-map-reduce[2]",
    "joins/employee-map-reduce[3]",
    "joins/employee-map-reduce[4]",
    "joins/employee-map-reduce[5]",
    "joins/employee-map-reduce[6]",
    "joins/employee-map-reduce[7]",
    "joins/employee-map-reduce[8]",
    "joins/employee-map-reduce[9]",
    "joins/index[0]",
    "joins/index[10]",
    "joins/index[11]",
    "joins/index[12]",
    "joins/index[15]",
    "joins/index[1]",
    "joins/index[2]",
    "joins/index[3]",
    "joins/index[4]",
    "joins/index[5]",
    "joins/index[6]",
    "joins/index[7]",
    "joins/index[8]",
    "joins/index[9]",
    "joins/library-joins[0]",
    "joins/library-joins[10]",
    "joins/library-joins[1]",
    "joins/library-joins[2]",
    "joins/library-joins[3]",
    "joins/library-joins[4]",
    "joins/library-joins[5]",
    "joins/library-joins[6]",
    "joins/library-joins[7]",
    "joins/library-joins[8]",
    "joins/library-joins[9]",
    "parent-operator/parent[0]",
    "parent-operator/parent[10]",
    "parent-operator/parent[11]",
    "parent-operator/parent[12]",
    "parent-operator/parent[13]",
    "parent-operator/parent[14]",
    "parent-operator/parent[15]",
    "parent-operator/parent[16]",
    "parent-operator/parent[17]",
    "parent-operator/parent[18]",
    "parent-operator/parent[19]",
    "parent-operator/parent[1]",
    "parent-operator/parent[20]",
    "parent-operator/parent[21]",
    "parent-operator/parent[22]",
    "parent-operator/parent[23]",
    "parent-operator/parent[24]",
    "parent-operator/parent[25]",
    "parent-operator/parent[26]",
    "parent-operator/parent[27]",
    "parent-operator/parent[2]",
    "parent-operator/parent[3]",
    "parent-operator/parent[4]",
    "parent-operator/parent[5]",
    "parent-operator/parent[6]",
    "parent-operator/parent[7]",
    "parent-operator/parent[8]",
    "parent-operator/parent[9]",
}


def _build_pytest_params(
    cases: list[tuple[str, str, dict[str, Any]]],
) -> list["pytest.mark.structures.ParameterSet"]:
    params = []
    for test_id, group_name, spec in cases:
        marks = []
        if test_id in _XFAIL_TEST_IDS:
            reason = _XFAIL_PHASE_BY_GROUP.get(
                group_name, "unresolved reference-suite coverage gap"
            )
            marks.append(pytest.mark.xfail(reason=reason, strict=False))
        params.append(pytest.param(test_id, group_name, spec, marks=marks, id=test_id))
    return params


# Load all test cases
test_cases = load_test_cases()
test_params = _build_pytest_params(test_cases)

print(f"\n{'=' * 70}")
print("JSONata Reference Suite Test Loader")
print(f"{'=' * 70}")
print(f"Loaded {len(DATASETS)} datasets from {DATASET_DIR}")
print(f"Loaded {len(test_cases)} test cases ({len(_XFAIL_TEST_IDS)} xfailed pending later phases)")
print(f"{'=' * 70}\n")


@pytest.mark.reference
@pytest.mark.parametrize("test_id,group_name,spec", test_params)
def test_reference_suite(test_id: str, group_name: str, spec: dict[str, Any]):
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
            # Expected an error with specific error object
            # TODO: Validate full error object structure
            # For now, just accept that an error occurred
            pass

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
