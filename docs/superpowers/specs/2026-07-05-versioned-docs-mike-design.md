# Versioned documentation via mike, fixing the gh-pages deploy race

## Context

`docs.yml` deploys documentation on every push to `main` via `mkdocs gh-deploy --force`,
which force-pushes the entire rendered site to the `gh-pages` branch, replacing its full
history every time. GitHub's legacy Pages builder (this repo's Pages source is
branch-based: `source: {branch: gh-pages, path: /}`) auto-triggers a build on every push to
that branch — and because `--force` rewrites history rather than fast-forwarding, rapid
successive pushes race: a build kicked off against a ref that gets overwritten moments
later by the next force-push fails with a generic "Page build failed" error, immediately
followed by a build against the final ref that succeeds. Confirmed via
`gh api repos/.../pages/builds`: this pattern goes back to March 2026, predates all of
today's work, and the site itself is unaffected (`gh api repos/.../pages` reports
`"status": "built"`) — but the errored entries in the build history are confusing and the
underlying mechanism is fragile.

Separately, `mkdocs.yml` already has `extra.version.provider: mike` configured — intended
to enable mkdocs-material's version-selector dropdown — but nothing in either workflow
actually invokes `mike`, so this has been inert. During this project's brainstorming, a
real desire surfaced: preserve documentation for past minor releases (so someone using an
older `jsonatapy` version compatible with an older `jsonata-js` release can view docs that
match what they're running), not just serve the latest build. `mike` is purpose-built for
exactly this — it manages multiple version subdirectories on `gh-pages` and updates them
via a normal fetch-then-push, never a destructive force-push of the whole branch.

Adopting `mike` for the intended versioning goal also resolves the race as a side effect:
removing the force-push removes the mechanism that was causing GitHub's Pages builder to
trip over itself.

## Scope

- Version documentation by `jsonatapy`'s own minor release (`2.1`, `2.2`, ...) — the
  jsonata-js compatibility level each version targets is documentation *content* (the
  existing `compatibility.md` page), not a separate versioning axis.
- `docs.yml` (every push to `main`) deploys a rolling `dev` version — day-to-day doc edits
  show up immediately without minting a new permanent version.
- `release.yml` cuts a new pinned `<major.minor>` version only at actual release time
  (patch releases like `2.2.1` → `2.2.2` update the *same* `2.2` docs, not a new version
  each patch), and moves a `stable` alias to point at it. The site root defaults to
  `stable` via `mike set-default`, so casual visitors land on the latest real release's
  docs, not the bleeding-edge `dev` build.
- Fix the stale mention in `.github/workflows/README.md` that still says benchmark
  history lives on `gh-pages` (it moved to the `benchmark-data` branch earlier today).
- One-time cleanup: the existing un-versioned content sitting at the root of `gh-pages`
  (from all historical `mkdocs gh-deploy --force` runs) isn't in the structure `mike`
  expects (per-version subdirectories + a `versions.json` index + a redirect at the root).
  The first real `mike` deploy needs this old root-level content cleared first, so it
  doesn't sit alongside the new versioned structure as orphaned cruft.

**Non-goals:**
- No retroactive versions for past releases (2.1, etc.) — versioning starts from the next
  real release forward, not backfilled.
- No changes to `mkdocs.yml` — `extra.version.provider: mike` is already correct.
- No change to the repository's GitHub Pages source setting — it stays branch-based
  (`gh-pages`), since that's what `mike` needs; this project does NOT migrate to the
  Actions-native `actions/deploy-pages` mechanism (that mechanism replaces the whole site
  per deploy, incompatible with preserving multiple coexisting versions).
- `gh-pages` branch is kept, not deleted — it's mike's actual content store going forward,
  not a vestigial artifact.

## Design

### 1. `docs.yml` (rolling `dev` version, every push to `main`)

- Add `mike` to the "Install dependencies" step, alongside the existing
  `mkdocs-material`/`pymdown-extensions`.
- Replace `mkdocs gh-deploy --force` with `mike deploy dev --push`.
- Add a `concurrency` group (this workflow currently has none, unlike `benchmark.yml`/
  `release.yml` which do) with `cancel-in-progress: true` — two overlapping `docs.yml`
  runs both trying to fetch-then-push `gh-pages` could still collide if pushes to `main`
  happen in quick succession; only the newest push's `dev` content actually matters, so an
  in-flight older deploy can safely be superseded rather than fought over.
- `on:` triggers (push to `main`, `workflow_dispatch`) stay unchanged.
- Git identity setup (`github-actions[bot]`) stays as-is.

### 2. `release.yml` (pinned `<major.minor>` version + `stable` alias, at release time)

- New step in the existing informational, post-`create-github-release` part of the
  workflow (the same spot as the `benchmark-and-record` job added earlier today — after
  publish, so this cannot block or be blocked by the actual release).
- Needs `mkdocs`, `mkdocs-material`, `pymdown-extensions`, and `mike` installed, and a
  checkout of the release tag (same checkout pattern as the benchmark job:
  `ref: refs/tags/v${{ needs.validate-version.outputs.version }}`).
- Extract major.minor from `needs.validate-version.outputs.version` (e.g. `2.2.1` → `2.2`)
  with a shell parameter expansion or `cut`, not a new dependency.
- Run `mike deploy <major.minor> stable --update-aliases --push`, then
  `mike set-default stable --push`.

### 3. One-time `gh-pages` cleanup

Before (or as part of) the first real `mike deploy`, clear the existing root-level files
on `gh-pages` that predate `mike` (the flat site output from historical
`mkdocs gh-deploy --force` runs), so they don't linger as orphaned content alongside the
new per-version subdirectory structure. This is a one-time, manually-run step (not part of
either workflow's steady-state behavior going forward).

### 4. Documentation fix

Update `.github/workflows/README.md`'s two stale mentions of benchmark data living on
`gh-pages` to reflect that it now lives on the `benchmark-data` branch (established
earlier today in the benchmark-comparison-infra project).

## Testing

- Verify `mike`'s actual CLI behavior locally first (deploy against a scratch/throwaway
  branch) before wiring it into CI, rather than assuming exact flag behavior from memory.
- Validate YAML for both `docs.yml` and `release.yml` after editing.
- Live test: push a real commit, confirm `docs.yml` deploys the `dev` version correctly
  and the site renders with a working version-selector, and confirm no new "Page build
  failed" entries appear in `gh api repos/.../pages/builds` for that push (the race is
  actually gone, not just less frequent).
- The `release.yml` path (cutting a pinned version + `stable` alias) cannot be fully
  tested without a real release, the same limitation already documented for the
  `benchmark-and-record` job added earlier today. The next real release is the true first
  end-to-end test of this path.

## Definition of done

- `docs.yml` deploys a `dev` version via `mike` on every push to `main`, with no more
  force-pushes and no more Pages-builder race errors for those pushes.
- `release.yml` has a step that cuts a pinned `<major.minor>` version and moves the
  `stable` alias, verified against the next real release.
- The site's version-selector works and the root URL defaults to the latest `stable`
  release.
- `gh-pages`'s pre-mike root-level content has been cleared once; no new orphaned content
  accumulates going forward (mike's normal deploy behavior only touches its own
  version subdirectories and index).
- `.github/workflows/README.md` no longer references `gh-pages` for benchmark data.
