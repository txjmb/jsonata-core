#!/bin/bash
# Cross-check the version a release run was dispatched with against the manifests.
#
# release.yml takes the version as free-text workflow_dispatch input and validated
# only its shape, so a fat-fingered "2.8.8" (for 2.2.8) sailed through. The publish
# steps that follow are irreversible: neither PyPI nor crates.io allows unpublishing.
#
# Plain equality is not the rule -- the ordinary flow dispatches the NEXT version
# while the manifests still hold the current one, and update-version bumps them. So
# the input must be a legal successor of Cargo.toml's version: the same version
# (a re-run, or a release prepared by hand before dispatch), or one step up in
# exactly one component with the components below it reset.
set -euo pipefail

VERSION="${1:?Usage: check-release-version.sh <version> (e.g. 2.2.9)}"

if ! echo "$VERSION" | grep -qE '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "::error::Invalid version format '$VERSION'. Expected X.Y.Z (e.g. 2.2.9)." >&2
    exit 1
fi

read_version() {
    local file="$1"
    if [ ! -f "$file" ]; then
        echo "::error::$file not found (run this from the repository root)" >&2
        exit 1
    fi
    local found
    found="$(grep -m1 -E '^version = "' "$file" | cut -d'"' -f2 || true)"
    if [ -z "$found" ]; then
        echo "::error::no 'version = \"...\"' line in $file" >&2
        exit 1
    fi
    echo "$found"
}

CARGO="$(read_version Cargo.toml)"
PYPROJECT="$(read_version pyproject.toml)"

if [ "$CARGO" != "$PYPROJECT" ]; then
    echo "::error::Cargo.toml says $CARGO but pyproject.toml says $PYPROJECT." >&2
    echo "::error::The manifests must agree before a release. Fix the drift, then re-dispatch." >&2
    exit 1
fi

IFS='.' read -r MAJOR MINOR PATCH <<< "$CARGO"

SAME="$MAJOR.$MINOR.$PATCH"
NEXT_PATCH="$MAJOR.$MINOR.$((PATCH + 1))"
NEXT_MINOR="$MAJOR.$((MINOR + 1)).0"
NEXT_MAJOR="$((MAJOR + 1)).0.0"

case "$VERSION" in
    "$SAME"|"$NEXT_PATCH"|"$NEXT_MINOR"|"$NEXT_MAJOR") ;;
    *)
        echo "::error::Release version '$VERSION' is not a legal successor of $CARGO (the version in Cargo.toml and pyproject.toml)." >&2
        echo "::error::Allowed: $SAME (re-run, or a release prepared by hand), $NEXT_PATCH (patch), $NEXT_MINOR (minor), $NEXT_MAJOR (major)." >&2
        echo "::error::If '$VERSION' is a typo, re-dispatch with the intended version. If it is deliberate, bump the manifests on the branch first." >&2
        exit 1
        ;;
esac

if [ "$VERSION" = "$SAME" ]; then
    echo "Release version $VERSION matches the manifests (re-run, or prepared by hand)."
else
    echo "Release version $VERSION is a legal bump from $CARGO."
fi
