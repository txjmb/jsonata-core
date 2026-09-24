#!/bin/bash
# Print the release notes for one version: that version's section of CHANGELOG.md,
# without its "## [X.Y.Z] - DATE" heading and without any "### ..." subheadings that
# have no entries (bump-changelog.sh's template leaves Deprecated/Removed/... empty).
#
# release.yml uses this as the GitHub Release body. Before it did, the body was a raw
# list of every commit subject since the previous tag: merge commits, superseded
# intermediate steps, and unrelated housekeeping (a post-release README version fix-up
# showed up as "Update jsonata-core version to 2.2.9" in the 2.2.10 notes). The curated
# CHANGELOG.md entry is what readers should see.
#
# Fails loudly if the section is missing or has no bullet entries, so a release never
# publishes an empty body. It runs before anything is published.
set -euo pipefail

VERSION="${1:?Usage: release-notes.sh <version> [changelog-path] (e.g. 2.2.10)}"
CHANGELOG="${2:-CHANGELOG.md}"

if [ ! -f "$CHANGELOG" ]; then
    echo "::error::$CHANGELOG not found" >&2
    exit 1
fi

# Literal prefix match (index, not a regex) so the dots in the version are not
# wildcards, and so a named release heading ("## [2.2.8] \"Conform-ata\" - ...")
# matches too. The closing "]" keeps [2.2.1] from matching [2.2.10].
NOTES="$(awk -v ver="$VERSION" '
    function flush() {
        sub(/\n+$/, "\n", buf)
        if (hdr != "" && buf ~ /[^[:space:]]/) printf "%s\n\n%s\n", hdr, buf
        hdr = ""; buf = ""
    }
    index($0, "## [" ver "]") == 1 { found = 1; next }
    !found { next }
    /^## \[/ { exit }
    /^### / { flush(); hdr = $0; next }
    hdr == "" { print; next }
    buf == "" && /^[[:space:]]*$/ { next }
    { buf = buf $0 "\n" }
    END { flush() }
' "$CHANGELOG" | sed '/./,$!d')"

if ! printf '%s\n' "$NOTES" | grep -q '^- '; then
    echo "::error::$CHANGELOG has no entries for [$VERSION] (no '## [$VERSION]' section, or no '- ...' bullet lines in it)." >&2
    exit 1
fi

printf '%s\n' "$NOTES"
