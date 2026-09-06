#!/usr/bin/env python3
"""Cross-check every doc/memory claim about reference-suite numbers against reality.

The jsonata-js reference-suite total (1686 cases), the passing count (total minus
however many divergences are tracked as strict xfail -- currently none, so 1686),
and the jsonata-js version (2.2.2) are
each restated in prose across README.md, docs/, .github/, tests/, and
.serena/memories/ instead of being computed once. Those restatements silently went
stale for months (the docs said "1258/1258" and "v2.1.0" long after the submodule
had moved to 1686 cases and v2.2.2) because nothing checked them against the
submodule or the KNOWN_DIVERGENCES registry in test_reference_suite.py.

This script computes the ground truth directly (by counting cases in
tests/jsonata-js/test/test-suite/groups, reading tests/jsonata-js/package.json's
version, and counting KNOWN_DIVERGENCES entries) and checks a registry of known
"claim sites" against it. It is intentionally NOT a generic text scanner: each
claim site is an explicit (file, regex) pair anchored on stable surrounding text,
the same style as KNOWN_DIVERGENCES itself. If a claim's surrounding wording
changes enough that its anchor regex stops matching, this script fails loudly
telling you to update the registry below -- that is a feature, not a bug: an
un-registered claim is exactly the kind of drift this script exists to catch.

Usage: python scripts/check_reference_stats.py
Exit code 0 if every registered claim matches ground truth, 1 otherwise.
"""

from __future__ import annotations

import ast
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


@dataclass
class GroundTruth:
    total: int
    passing: int
    xfail: int
    version: str


def compute_ground_truth() -> GroundTruth:
    groups_dir = ROOT / "tests/jsonata-js/test/test-suite/groups"
    total = 0
    for group_dir in sorted(groups_dir.iterdir()):
        if not group_dir.is_dir():
            continue
        for case_file in sorted(group_dir.glob("*.json")):
            spec = json.loads(case_file.read_text(encoding="utf-8"))
            total += len(spec) if isinstance(spec, list) else 1

    package_json = json.loads((ROOT / "tests/jsonata-js/package.json").read_text(encoding="utf-8"))
    version = package_json["version"]

    suite_src = (ROOT / "tests/python/test_reference_suite.py").read_text(encoding="utf-8")
    tree = ast.parse(suite_src)
    xfail = None
    for node in ast.walk(tree):
        targets = None
        if isinstance(node, ast.Assign):
            targets = node.targets
        elif isinstance(node, ast.AnnAssign) and node.target is not None:
            targets = [node.target]
        if not targets:
            continue
        for target in targets:
            if isinstance(target, ast.Name) and target.id == "KNOWN_DIVERGENCES":
                if not isinstance(node.value, ast.Dict):
                    raise SystemExit("KNOWN_DIVERGENCES is not a dict literal; can't count it")
                xfail = len(node.value.keys)
    if xfail is None:
        raise SystemExit("Could not find KNOWN_DIVERGENCES in tests/python/test_reference_suite.py")

    return GroundTruth(total=total, passing=total - xfail, xfail=xfail, version=version)


@dataclass
class Claim:
    file: str
    pattern: str
    # Maps regex named group -> ground-truth field ('total', 'passing', 'xfail', 'version')
    fields: dict[str, str]
    flags: int = re.MULTILINE


CLAIMS: list[Claim] = [
    Claim(
        "README.md",
        r"JSONata%20conformance-(?P<passing>\d+)%2F(?P<total>\d+)-brightgreen",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "README.md",
        r"1600\+ \((?P<passing>\d+)/(?P<total>\d+) passing for last build",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "README.md",
        r"jsonata-core` passes \*\*(?P<passing>\d+)/(?P<total>\d+)\*\* JSONata reference tests",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "README.md",
        r"\*\*(?P<passing>\d+)/(?P<total>\d+) JSONata reference tests passing\*\*",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/index.md",
        r"\*\*(?P<passing>\d+)/(?P<total>\d+)\*\* JSONata reference tests passing",
        {"passing": "passing", "total": "total"},
    ),
    Claim("docs/compatibility.md", r"\*\*Total Tests\*\*: (?P<total>\d+)", {"total": "total"}),
    Claim("docs/compatibility.md", r"\*\*Passing\*\*: (?P<passing>\d+) \(", {"passing": "passing"}),
    Claim(
        "docs/compatibility.md",
        r"Official jsonata-js repository \(v(?P<version>[\d.]+)\)",
        {"version": "version"},
    ),
    Claim(
        "docs/compatibility.md",
        r"Results show (?P<passing>\d+)/(?P<total>\d+) \(",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/credits.md",
        r"passes (?P<passing>\d+) of the (?P<total>\d+) tests from the jsonata-js",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/migration-from-jsonata-python.md",
        r"Passes (?P<passing>\d+)/(?P<total>\d+) reference test suite tests",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/migration-from-jsonata-python.md",
        r"implements the JSONata (?P<version>[\d.]+) specification \((?P<passing>\d+)/(?P<total>\d+)",
        {"version": "version", "passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/migration-from-jsonata-python.md",
        r"passes \*\*(?P<passing>\d+)/(?P<total>\d+)\*\* \([\d.]+%\) of the official JSONata reference test suite",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/development/testing.md",
        r"test_reference_suite\.py  # Reference suite runner \((?P<total>\d+) tests\)",
        {"total": "total"},
    ),
    Claim(
        "docs/development/testing.md",
        r"\*\*Reference Test Suite\*\* \((?P<total>\d+) tests\)",
        {"total": "total"},
    ),
    Claim(
        "docs/development/testing.md",
        r"(?P<passing>\d+)/(?P<total>\d+) passing \(no known divergences",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/development/testing.md",
        r"contains (?P<total>\d+) tests from the official jsonata-js repository \(v(?P<version>[\d.]+)\)\. "
        r"(?P<passing>\d+) pass",
        {"total": "total", "version": "version", "passing": "passing"},
    ),
    Claim(
        "docs/development/testing.md",
        r"# All reference tests \((?P<total>\d+) tests\)",
        {"total": "total"},
    ),
    Claim(
        "docs/development/architecture.md",
        r"\*\*Reference Test Suite\*\* \((?P<total>\d+) tests\)",
        {"total": "total"},
    ),
    Claim(
        "docs/development/architecture.md",
        r"(?P<passing>\d+)/(?P<total>\d+) pass rate",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "docs/rust-crate.md",
        r"Passes (?P<passing>\d+)/(?P<total>\d+) JSONata (?P<version>[\d.]+) reference tests",
        {"passing": "passing", "total": "total", "version": "version"},
    ),
    Claim(
        "docs/jsonata-language.md",
        r"\*\*(?P<passing>\d+)/(?P<total>\d+) tests passing\*\*",
        {"passing": "passing", "total": "total"},
    ),
    Claim("docs/README.md", r"Full JSONata (?P<version>[\d.]+) support", {"version": "version"}),
    Claim(
        "docs/README.md",
        r"(?P<passing>\d+)/(?P<total>\d+) test suite compatibility",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        "bindings/c/README.md",
        r"\((?P<passing>\d+)/(?P<total>\d+) reference tests passing, v(?P<version>[\d.]+)",
        {"passing": "passing", "total": "total", "version": "version"},
    ),
    Claim(
        ".github/PULL_REQUEST_TEMPLATE.md",
        r"Reference test suite still passes \((?P<passing>\d+)/(?P<total>\d+) tests",
        {"passing": "passing", "total": "total"},
    ),
    Claim(
        ".github/workflows/README.md",
        r"Reference JSONata test suite \((?P<total>\d+) tests, (?P<passing>\d+) passing",
        {"total": "total", "passing": "passing"},
    ),
    Claim(
        "tests/python/test_reference_suite.py",
        r"runs all (?P<total>\d+) test cases from the reference JavaScript JSONata\n"
        r"implementation \(jsonata-js v(?P<version>[\d.]+)\)\. (?P<passing>\d+) pass",
        {"total": "total", "version": "version", "passing": "passing"},
        re.MULTILINE | re.DOTALL,
    ),
    Claim(
        "tests/datetime_picture_suite.rs", r"the full (?P<total>\d+)-case suite", {"total": "total"}
    ),
    Claim(
        "tests/python/test_fastpath_differential.py", r"~(?P<total>\d+) cases", {"total": "total"}
    ),
    Claim(
        ".serena/memories/conventions.md",
        r"(?P<total>\d+)-case suite, not just",
        {"total": "total"},
    ),
    Claim(
        ".serena/memories/suggested_commands.md",
        r"# (?P<total>\d+) JSONata-js reference cases \(primary compat gate; (?P<passing>\d+) pass",
        {"total": "total", "passing": "passing"},
    ),
    Claim(
        ".serena/memories/task_completion.md",
        r"# full (?P<total>\d+)-case JS compat suite \((?P<passing>\d+) pass",
        {"total": "total", "passing": "passing"},
    ),
    Claim(
        ".serena/memories/core.md",
        r"jsonata-js` v(?P<version>[\d.]+) test suite \((?P<passing>\d+)/(?P<total>\d+) passing",
        {"version": "version", "passing": "passing", "total": "total"},
    ),
    Claim(".serena/memories/core.md", r"source of the (?P<total>\d+) cases", {"total": "total"}),
    Claim(
        "docs/performance.md",
        r"\*\*jsonata-js\*\* \| JavaScript \| (?P<version>[\d.]+) \|",
        {"version": "version"},
    ),
    Claim(
        "benchmarks/python/generate_performance_doc.py",
        r"\*\*jsonata-js\*\* \| JavaScript \| (?P<version>[\d.]+) \|",
        {"version": "version"},
    ),
    Claim(
        "docs/api.md",
        r'print\(jsonatapy\.__jsonata_version__\)\s*#\s*"(?P<version>[\d.]+)"',
        {"version": "version"},
    ),
    Claim(
        "tests/python/test_basic.py",
        r'assert jsonatapy\.__jsonata_version__ == "(?P<version>[\d.]+)"',
        {"version": "version"},
    ),
    Claim(
        "src/lib.rs",
        r'const JSONATA_REFERENCE_VERSION: &str = "(?P<version>[\d.]+)";',
        {"version": "version"},
    ),
    Claim(
        "docs/changelog.md",
        r"\*\*Current target version:\*\* v(?P<version>[\d.]+)",
        {"version": "version"},
    ),
]


def main() -> int:
    truth = compute_ground_truth()
    print(
        f"Ground truth: {truth.passing}/{truth.total} passing, "
        f"{truth.xfail} known xfail, jsonata-js v{truth.version}"
    )

    errors: list[str] = []
    file_cache: dict[str, str] = {}

    for claim in CLAIMS:
        if claim.file not in file_cache:
            path = ROOT / claim.file
            if not path.exists():
                errors.append(f"{claim.file}: file not found")
                continue
            file_cache[claim.file] = path.read_text(encoding="utf-8")
        content = file_cache.get(claim.file)
        if content is None:
            continue

        match = re.search(claim.pattern, content, claim.flags)
        if match is None:
            errors.append(
                f"{claim.file}: anchor pattern not found ({claim.pattern!r}). "
                "Either the claim was reworded (update the registry in "
                "scripts/check_reference_stats.py) or it was deleted (remove the entry)."
            )
            continue

        for group_name, field in claim.fields.items():
            actual = match.group(group_name)
            expected = str(getattr(truth, field))
            if actual != expected:
                errors.append(
                    f"{claim.file}: claims {field}={actual}, but ground truth is {field}={expected} "
                    f"(matched text: {match.group(0)!r})"
                )

    if errors:
        print("\n::error::Reference-suite stats have drifted out of sync:", file=sys.stderr)
        for error in errors:
            print(f"::error::  {error}", file=sys.stderr)
        print(
            "\nGround truth comes from tests/jsonata-js (submodule) and "
            "KNOWN_DIVERGENCES in tests/python/test_reference_suite.py. "
            "Fix the stale claim(s) above, or update the registry in "
            "scripts/check_reference_stats.py if a claim's wording changed.",
            file=sys.stderr,
        )
        return 1

    print(f"All {len(CLAIMS)} registered reference-suite claims match ground truth.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
