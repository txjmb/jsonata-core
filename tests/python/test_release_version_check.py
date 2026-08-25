"""Guard on the release workflow's version input.

`release.yml` takes the version to publish as free text and validated only its
*shape*, so a fat-fingered `2.8.8` (for `2.2.8`) passed straight through. The
publish steps are irreversible -- PyPI and crates.io do not allow unpublishing --
so the input is checked against the manifests before anything is tagged.

The rule cannot be plain equality: the normal flow dispatches the *next* version
while the manifests still hold the current one, and `update-version` bumps them.
So the input must be a legal successor of `Cargo.toml`'s version -- the same
version (re-run, or a hand-prepared release like 2.2.8), or one step up in
exactly one component.
"""

import subprocess
from pathlib import Path

import pytest

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts" / "check-release-version.sh"


def run(version, cargo="2.2.8", pyproject=None, tmp_path=None):
    """Run the checker against a synthetic pair of manifests."""
    if pyproject is None:
        pyproject = cargo
    (tmp_path / "Cargo.toml").write_text(f'[package]\nname = "jsonata-core"\nversion = "{cargo}"\n')
    (tmp_path / "pyproject.toml").write_text(
        f'[project]\nname = "jsonatapy"\nversion = "{pyproject}"\n'
    )
    return subprocess.run(
        [str(SCRIPT), version],
        cwd=tmp_path,
        capture_output=True,
        text=True,
    )


@pytest.mark.parametrize(
    "version",
    [
        "2.2.8",  # re-run, or a release prepared by hand before dispatch
        "2.2.9",  # patch bump -- the ordinary case for this project
        "2.3.0",  # minor bump
        "3.0.0",  # major bump
    ],
)
def test_accepts_the_current_version_and_its_legal_successors(version, tmp_path):
    result = run(version, tmp_path=tmp_path)
    assert result.returncode == 0, result.stderr


@pytest.mark.parametrize(
    "version",
    [
        "2.8.8",  # the actual fat-finger: minor jumps by six
        "2.2.7",  # backwards
        "2.2.10",  # patch skips one
        "2.4.0",  # minor skips one
        "4.0.0",  # major skips one
        "2.3.1",  # minor bumped but patch not reset
        "3.1.0",  # major bumped but minor not reset
    ],
)
def test_rejects_versions_that_are_not_a_single_step_from_the_manifest(version, tmp_path):
    result = run(version, tmp_path=tmp_path)
    assert result.returncode != 0
    assert "2.2.8" in result.stderr, "the error must name the version it compared against"


@pytest.mark.parametrize("version", ["2.2", "v2.2.9", "2.2.9-rc1", "", "2.2.9 "])
def test_rejects_malformed_versions(version, tmp_path):
    assert run(version, tmp_path=tmp_path).returncode != 0


def test_rejects_manifests_that_disagree_with_each_other(tmp_path):
    result = run("2.2.9", cargo="2.2.8", pyproject="2.2.7", tmp_path=tmp_path)
    assert result.returncode != 0
    assert "pyproject.toml" in result.stderr


def test_the_repos_own_manifests_agree_and_accept_a_patch_bump():
    """The real files, not a fixture -- catches drift landing on main."""
    cargo = (REPO / "Cargo.toml").read_text()
    version = next(
        line.split('"')[1] for line in cargo.splitlines() if line.startswith("version = ")
    )
    major, minor, patch = (int(p) for p in version.split("."))
    result = subprocess.run(
        [str(SCRIPT), f"{major}.{minor}.{patch + 1}"],
        cwd=REPO,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
