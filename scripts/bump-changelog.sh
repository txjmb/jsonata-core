#!/bin/bash
# Bump CHANGELOG.md for a release: rename the accumulated "[Unreleased]" section to
# "[$VERSION] - $DATE" and insert a fresh, empty "[Unreleased]" template above it.
#
# Fails loudly (does not silently proceed) if [Unreleased] has no actual bullet
# entries -- an empty release note is almost always a sign the changelog was never
# updated during development, which is exactly how this file went stale for years
# before this script existed. If a release genuinely has nothing user-facing to
# note, edit CHANGELOG.md by hand instead of running this script unmodified.
set -euo pipefail

VERSION="${1:?Usage: bump-changelog.sh <version> [changelog-path] (e.g. 2.1.7)}"
CHANGELOG="${2:-CHANGELOG.md}"
DATE="$(date -u +%Y-%m-%d)"

if [ ! -f "$CHANGELOG" ]; then
    echo "::error::$CHANGELOG not found" >&2
    exit 1
fi

if ! grep -q '^## \[Unreleased\]$' "$CHANGELOG"; then
    echo "::error::$CHANGELOG has no '## [Unreleased]' section to bump" >&2
    exit 1
fi

UNRELEASED_BODY="$(awk '
    /^## \[Unreleased\]$/ { found=1; next }
    found && /^## \[/ { exit }
    found { print }
' "$CHANGELOG")"

if ! echo "$UNRELEASED_BODY" | grep -q '^- '; then
    echo "::error::CHANGELOG.md's [Unreleased] section has no entries (no '- ...' bullet lines)." >&2
    echo "::error::Add changelog entries before releasing, or edit CHANGELOG.md by hand if this release genuinely has nothing user-facing to note." >&2
    exit 1
fi

# Optional release name (e.g. RELEASE_NAME='Conform-ata' produced
# '## [2.2.8] "Conform-ata" - 2026-08-25'). Empty keeps the plain heading.
NAME_SEGMENT=""
if [ -n "${RELEASE_NAME:-}" ]; then
    NAME_SEGMENT=" \"$RELEASE_NAME\""
fi

TMP="$(mktemp)"
awk -v version="$VERSION" -v name_segment="$NAME_SEGMENT" -v date="$DATE" '
    /^## \[Unreleased\]$/ {
        print "## [Unreleased]"
        print ""
        print "### Added"
        print ""
        print "### Changed"
        print ""
        print "### Deprecated"
        print ""
        print "### Removed"
        print ""
        print "### Fixed"
        print ""
        print "### Security"
        print ""
        print "## [" version "]" name_segment " - " date
        next
    }
    { print }
' "$CHANGELOG" > "$TMP"

mv "$TMP" "$CHANGELOG"
echo "Bumped $CHANGELOG: [Unreleased] -> [$VERSION]$NAME_SEGMENT - $DATE (fresh [Unreleased] template inserted above it)"
