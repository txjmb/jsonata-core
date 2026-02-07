"""
pytest configuration for jsonatapy tests.

This file configures pytest markers and hooks for the test suite,
including the reference JSONata test suite integration.
"""

import pytest
import json
from pathlib import Path


def pytest_configure(config):
    """
    Register custom markers for test organization.

    Markers:
        reference: Tests from the reference JSONata suite
        group(name): Tests from a specific test group
        slow: Performance and slow-running tests
        compatibility: JavaScript compatibility tests
    """
    config.addinivalue_line("markers", "reference: marks tests from the reference JSONata suite")
    config.addinivalue_line("markers", "group(name): marks tests from specific test group")
    config.addinivalue_line(
        "markers", "slow: marks tests as slow (deselect with '-m \"not slow\"')"
    )
    config.addinivalue_line("markers", "compatibility: marks JavaScript compatibility tests")


def pytest_collection_modifyitems(config, items):
    """
    Modify test items after collection to add markers dynamically.

    This adds the 'reference' marker to all tests from test_reference_suite.py
    and adds group-specific markers based on test parameters.
    """
    for item in items:
        # Add reference marker to all reference suite tests
        if "test_reference_suite" in item.nodeid:
            item.add_marker(pytest.mark.reference)

            # Extract group name from test parameters
            if hasattr(item, "callspec"):
                group_name = item.callspec.params.get("group_name")
                if group_name:
                    # Add a dynamic marker for the group
                    item.add_marker(pytest.mark.group(group_name))


class ReferenceSuiteReporter:
    """
    Pytest plugin to track reference suite compatibility.

    This plugin collects statistics about test results from the
    reference JSONata suite and generates a compatibility report.
    """

    def __init__(self):
        self.total = 0
        self.passed = 0
        self.failed = 0
        self.skipped = 0
        self.by_group = {}
        self.failed_tests = []

    @pytest.hookimpl(tryfirst=True, hookwrapper=True)
    def pytest_runtest_makereport(self, item, call):
        """Hook to capture test results."""
        outcome = yield
        report = outcome.get_result()

        # Only process reference suite tests
        if "test_reference_suite" not in item.nodeid:
            return

        if report.when == "call":
            self.total += 1

            # Extract group name from parameters
            group = None
            if hasattr(item, "callspec"):
                group = item.callspec.params.get("group_name")

            # Update statistics
            if report.passed:
                self.passed += 1
                if group:
                    self.by_group.setdefault(group, {"passed": 0, "failed": 0, "skipped": 0})
                    self.by_group[group]["passed"] += 1

            elif report.failed:
                self.failed += 1
                if group:
                    self.by_group.setdefault(group, {"passed": 0, "failed": 0, "skipped": 0})
                    self.by_group[group]["failed"] += 1

                # Track failed test details
                test_id = item.callspec.params.get("test_id", "unknown")
                expr = item.callspec.params.get("spec", {}).get("expr", "unknown")
                self.failed_tests.append(
                    {
                        "test_id": test_id,
                        "group": group,
                        "expr": expr,
                        "error": str(report.longrepr)
                        if hasattr(report, "longrepr")
                        else "Unknown error",
                    }
                )

            elif report.skipped:
                self.skipped += 1
                if group:
                    self.by_group.setdefault(group, {"passed": 0, "failed": 0, "skipped": 0})
                    self.by_group[group]["skipped"] += 1

    def pytest_sessionfinish(self, session):
        """Print summary report at the end of the test session."""
        if self.total == 0:
            return

        pct = (self.passed / self.total * 100) if self.total > 0 else 0

        # Print console report
        print("\n" + "=" * 70)
        print("JSONata Reference Suite Compatibility Report")
        print("=" * 70)
        print(f"Total Tests:  {self.total}")
        print(f"Passed:       {self.passed} ({self.passed / self.total * 100:.1f}%)")
        print(f"Failed:       {self.failed} ({self.failed / self.total * 100:.1f}%)")
        if self.skipped > 0:
            print(f"Skipped:      {self.skipped} ({self.skipped / self.total * 100:.1f}%)")
        print(f"\nCompatibility: {pct:.1f}%")

        # Print by-group summary
        if self.by_group:
            print("\nResults by Group:")
            print(f"{'Group':<40} {'Pass':>6} {'Fail':>6} {'Skip':>6} {'Total':>6} {'%':>6}")
            print("-" * 70)

            for group, stats in sorted(self.by_group.items()):
                total_group = stats["passed"] + stats["failed"] + stats["skipped"]
                pct_group = stats["passed"] / total_group * 100 if total_group > 0 else 0
                status_icon = "✓" if stats["failed"] == 0 else "✗"

                print(
                    f"{group:<40} {stats['passed']:>6} {stats['failed']:>6} "
                    f"{stats['skipped']:>6} {total_group:>6} {pct_group:>5.1f}% {status_icon}"
                )

        # Write JSON report
        report_path = Path("test-suite-report.json")
        report_data = {
            "total": self.total,
            "passed": self.passed,
            "failed": self.failed,
            "skipped": self.skipped,
            "compatibility_pct": pct,
            "by_group": self.by_group,
            "failed_tests": self.failed_tests[:50],  # Limit to first 50 failures
        }

        with open(report_path, "w", encoding="utf-8") as f:
            json.dump(report_data, f, indent=2)

        print(f"\n{'=' * 70}")
        print(f"Detailed report written to: {report_path}")
        print(f"{'=' * 70}")


# Register the reporter plugin
def pytest_configure(config):
    """Register the reference suite reporter plugin."""
    # Only register if we're running reference suite tests
    if config.option.collectonly:
        return

    reporter = ReferenceSuiteReporter()
    config.pluginmanager.register(reporter, "reference_suite_reporter")


@pytest.fixture(scope="session")
def jsonatapy():
    """
    Session-scoped fixture to import jsonatapy once.

    This ensures the extension is loaded once and reused across all tests.
    """
    import jsonatapy

    return jsonatapy
