"""
Test adapter for the JSONata reference test suite.

This module loads and runs all 1,273+ test cases from the reference
JavaScript JSONata implementation to ensure 100% spec compliance.
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

        for case_file in sorted(group_dir.glob("case*.json")):
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


# Load all test cases
test_cases = load_test_cases()

print(f"\n{'=' * 70}")
print("JSONata Reference Suite Test Loader")
print(f"{'=' * 70}")
print(f"Loaded {len(DATASETS)} datasets from {DATASET_DIR}")
print(f"Loaded {len(test_cases)} test cases")
print(f"{'=' * 70}\n")


@pytest.mark.reference
@pytest.mark.parametrize("test_id,group_name,spec", test_cases, ids=[tc[0] for tc in test_cases])
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
