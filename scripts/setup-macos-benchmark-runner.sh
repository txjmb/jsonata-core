#!/bin/bash
# One-time setup for the self-hosted Mac Mini runner to run the release
# benchmark job. Run this DIRECTLY ON THE MAC MINI (not in CI) as the same
# user account the actions-runner LaunchAgent runs under.
#
# Everything else the benchmark job needs (uv, Python via `uv python
# install`, the Rust toolchain via dtolnay/rust-toolchain) is installed
# fresh by the workflow's own GitHub Actions on every run - this script only
# covers Node.js, which has no such self-installing step in the workflow
# (actions/setup-node is untested on this runner).
#
# Installs Node.js as a plain user-space binary, NOT via Homebrew: this
# machine didn't have Homebrew installed, and Homebrew's installer needs
# sudo (a password prompt this script can't answer non-interactively).
# Same reasoning this project already applies to Python on this runner -
# see release.yml's comments on why actions/setup-python is avoided here in
# favor of `uv python install`, which is also sudo-free.
set -euo pipefail

RUNNER_DIR="$HOME/source/jsonatapy/actions-runner"
NODE_DIR="$HOME/.local/node"
NODE_VERSION_LINE=$(curl -fsSL https://nodejs.org/dist/latest-v20.x/ | grep -o 'node-v20[^"]*darwin-arm64\.tar\.gz' | head -1)

if [ -z "$NODE_VERSION_LINE" ]; then
    echo "Could not determine latest Node 20.x darwin-arm64 build from nodejs.org - aborting." >&2
    exit 1
fi

echo "=== Installing Node.js ($NODE_VERSION_LINE) to $NODE_DIR/current ==="
mkdir -p "$NODE_DIR"
TMP_TARBALL="$(mktemp -t node-XXXXXX).tar.gz"
curl -fsSL -o "$TMP_TARBALL" "https://nodejs.org/dist/latest-v20.x/$NODE_VERSION_LINE"
TMP_EXTRACT_DIR="$(mktemp -d -t node-extract-XXXXXX)"
tar -xzf "$TMP_TARBALL" -C "$TMP_EXTRACT_DIR"
rm -rf "$NODE_DIR/current"
mv "$TMP_EXTRACT_DIR"/node-v*-darwin-arm64 "$NODE_DIR/current"
rm -f "$TMP_TARBALL"
rmdir "$TMP_EXTRACT_DIR" 2>/dev/null || true

echo ""
echo "=== Verification ==="
"$NODE_DIR/current/bin/node" --version
PATH="$NODE_DIR/current/bin:$PATH" "$NODE_DIR/current/bin/npm" --version

echo ""
echo "=== Extending the actions-runner's job PATH ==="
if [ ! -f "$RUNNER_DIR/.path" ]; then
    echo "No .path file found at $RUNNER_DIR/.path - is RUNNER_DIR correct? Skipping PATH update." >&2
    echo "You'll need to add $NODE_DIR/current/bin to the runner's PATH some other way." >&2
    exit 1
fi

if grep -q "$NODE_DIR/current/bin" "$RUNNER_DIR/.path"; then
    echo "$RUNNER_DIR/.path already includes the Node bin directory - nothing to do."
else
    cp "$RUNNER_DIR/.path" "$RUNNER_DIR/.path.bak-$(date +%Y%m%d%H%M%S)"
    printf '%s' "$(cat "$RUNNER_DIR/.path"):$NODE_DIR/current/bin" > "$RUNNER_DIR/.path"
    echo "Updated $RUNNER_DIR/.path (backup saved alongside it)."
fi

echo ""
echo "=== Restarting the runner service to pick up the new PATH ==="
(cd "$RUNNER_DIR" && ./svc.sh stop && ./svc.sh start)

echo ""
echo "Done. Node.js is installed and the actions-runner service has been"
echo "restarted so job PATHs include it. This persists across job runs -"
echo "no need to re-run this script unless Node.js is later removed or the"
echo "runner is reinstalled from scratch."
