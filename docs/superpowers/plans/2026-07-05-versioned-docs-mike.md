# Versioned Documentation via mike Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deploy documentation via `mike` instead of `mkdocs gh-deploy --force`, giving per-minor-version docs with a working version selector, and fixing a long-standing GitHub Pages build race as a side effect of no longer force-pushing the whole `gh-pages` branch on every push.

**Architecture:** `docs.yml` (every push to `main`) deploys a rolling `dev` version via `mike deploy dev --push`. `release.yml` gets a new post-release job that cuts a pinned `<major.minor>` version (e.g. `2.2` — patch releases update the same minor version, they don't mint a new one) and moves a `stable` alias to it via `mike deploy <major.minor> stable --update-aliases --push` + `mike set-default stable --push`. Both replace destructive force-pushes with `mike`'s normal fetch-then-push mechanism, which only touches its own per-version subdirectories.

**Tech Stack:** GitHub Actions (YAML), `mike` (Python package, verified locally: `deploy`/`set-default`/`list` subcommands, `-p/--push`, `-u/--update-aliases`, defaults `-b gh-pages -r origin`).

## Global Constraints

- Version by `jsonatapy`'s own minor release (`2.1`, `2.2`, ...) — not a separate jsonata-js-compatibility axis.
- New pinned version cut only at release time; every other push to `main` just updates the rolling `dev` version.
- No retroactive versions for past releases — versioning starts from the next real release forward.
- No changes to `mkdocs.yml` (`extra.version.provider: mike` is already correct) or to the repository's GitHub Pages source setting (stays branch-based — `gh-pages` is `mike`'s actual content store, not being replaced by `actions/deploy-pages`).
- `gh-pages` branch is kept, not deleted.
- The one-time legacy-content cleanup on `gh-pages` is a manual, one-off step — not part of either workflow's steady-state behavior.

---

## Task 1: One-time cleanup of legacy content on `gh-pages`

**Files:** none (a one-time repo operation, not a code change — mirrors how the `benchmark-data` branch was created in the earlier benchmark-comparison-infra project).

**Interfaces:**
- Produces: an empty `gh-pages` branch (no tracked files) for Task 2's first real `mike deploy` to populate cleanly. Confirmed locally that `mike deploy`'s only tracked output is a per-version subdirectory, `versions.json`, and `.nojekyll` (verified via a scratch repo: `git ls-tree -r --name-only` after `mike deploy dev` showed exactly `dev/**`, `versions.json`, `.nojekyll` — nothing else) — the *existing* `gh-pages` content from historical `mkdocs gh-deploy --force` runs (raw site files at the branch root: `index.html`, `css/`, `js/`, `search/`, `sitemap.xml`, etc.) is NOT in that structure and won't be touched or cleaned up by `mike` itself, so it must be cleared explicitly first.

- [ ] **Step 1: Clear the branch via an isolated worktree**

```bash
git fetch origin gh-pages
git worktree add /tmp/gh-pages-cleanup origin/gh-pages
cd /tmp/gh-pages-cleanup
git checkout -b gh-pages-cleanup-tmp
git rm -rf .
git commit -m "chore: clear legacy mkdocs gh-deploy content, migrating to mike"
git push origin gh-pages-cleanup-tmp:gh-pages
```

- [ ] **Step 2: Verify the branch is actually empty on the remote**

```bash
git ls-remote origin gh-pages
git fetch origin gh-pages
git ls-tree -r --name-only origin/gh-pages
```

Expected: the `ls-tree` output is empty (no files).

- [ ] **Step 3: Clean up the temporary worktree**

```bash
cd /mnt/c/Users/mboha/source/repos/jsonatapy
git worktree remove /tmp/gh-pages-cleanup
git worktree prune
```

No commit needed on the working branch — this task only modified `gh-pages` directly.

---

## Task 2: `docs.yml` — deploy rolling `dev` version via mike

**Files:**
- Modify: `.github/workflows/docs.yml` (full current content shown below)

**Interfaces:**
- Consumes: the cleared `gh-pages` branch from Task 1 (though this task's changes are correct regardless of `gh-pages`'s exact state — `mike` creates the branch if it doesn't exist, or adds to it if it does).
- Produces: nothing consumed by later tasks — Task 3 is a separate workflow file with its own independent `mike` invocation.

**Current file content** (`.github/workflows/docs.yml`, all 38 lines):

```yaml
name: Documentation

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: write

jobs:
  deploy:
    name: Deploy Documentation
    runs-on: ubuntu-latest

    steps:
      - name: Checkout
        uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - name: Set up Python
        uses: actions/setup-python@v6
        with:
          python-version: '3.12'

      - name: Install dependencies
        run: |
          pip install mkdocs-material pymdown-extensions

      - name: Configure git
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

      - name: Deploy to GitHub Pages
        run: mkdocs gh-deploy --force
```

- [ ] **Step 1: Replace the whole file**

```yaml
name: Documentation

on:
  push:
    branches: [main]
  workflow_dispatch:

concurrency:
  group: docs-deploy
  cancel-in-progress: true

permissions:
  contents: write

jobs:
  deploy:
    name: Deploy Documentation
    runs-on: ubuntu-latest

    steps:
      - name: Checkout
        uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - name: Set up Python
        uses: actions/setup-python@v6
        with:
          python-version: '3.12'

      - name: Install dependencies
        run: |
          pip install mkdocs-material pymdown-extensions mike

      - name: Configure git
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

      - name: Deploy dev docs
        run: mike deploy dev --push
```

(Two changes from the original: added the `concurrency` block — this workflow had none before, unlike `benchmark.yml`/`release.yml` which do, and two overlapping runs both trying to fetch-then-push `gh-pages` could still collide if pushes to `main` happen in quick succession; and replaced `mkdocs gh-deploy --force` with `mike deploy dev --push`, adding `mike` to the pip install line.)

- [ ] **Step 2: Validate the YAML**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/docs.yml'))" && echo "YAML OK"
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/docs.yml
git commit -m "$(cat <<'EOF'
feat: deploy docs via mike instead of mkdocs gh-deploy --force

mkdocs gh-deploy --force rewrites the entire gh-pages branch history
on every push to main. GitHub's legacy Pages builder (this repo's
Pages source is branch-based) auto-triggers a build on every push to
that branch, and because --force rewrites rather than fast-forwards,
rapid successive pushes race: a build against a ref that gets
overwritten moments later by the next force-push fails, immediately
followed by a build against the final ref that succeeds. Confirmed via
the Pages builds API this dates back to March, predates all recent
work, and the site itself was unaffected - but the mechanism is
fragile and the failed builds are confusing.

mike (already configured in mkdocs.yml's extra.version.provider but
never actually invoked) does a normal fetch-then-push, touching only
its own per-version subdirectory and a small versions.json index -
never rewriting the whole branch. This removes the race's root cause,
and is also the actual tool needed for the real goal: preserving
per-minor-version docs (see the new release.yml job in the next
commit) instead of only ever serving the latest build.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `release.yml` — cut pinned version + `stable` alias at release time

**Files:**
- Modify: `.github/workflows/release.yml` (append a new job after the existing `benchmark-and-record` job, which currently ends the file at line 700)

**Interfaces:**
- Consumes: `needs.validate-version.outputs.version` (already defined at `release.yml:83-84`, a plain semver string like `2.1.5`), `needs.create-github-release.result` (an existing job in the same workflow).
- Produces: nothing consumed by later tasks — this is the last code-change task.

- [ ] **Step 1: Append the new job at the end of `release.yml`**

Append after the existing `benchmark-and-record` job (i.e. at the very end of the file):

```yaml

  deploy-release-docs:
    name: Deploy Release Documentation
    runs-on: ubuntu-latest
    needs: [validate-version, create-github-release]
    if: needs.create-github-release.result == 'success' && github.event.inputs.dry_run != 'true'
    timeout-minutes: 10

    permissions:
      contents: write

    steps:
      - name: Checkout release tag
        uses: actions/checkout@v6
        with:
          ref: refs/tags/v${{ needs.validate-version.outputs.version }}
          fetch-depth: 0

      - name: Set up Python
        uses: actions/setup-python@v6
        with:
          python-version: '3.12'

      - name: Install dependencies
        run: |
          pip install mkdocs-material pymdown-extensions mike

      - name: Configure git
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

      - name: Deploy versioned docs
        run: |
          VERSION="${{ needs.validate-version.outputs.version }}"
          MAJOR_MINOR="${VERSION%.*}"
          mike deploy "$MAJOR_MINOR" stable --update-aliases --push
          mike set-default stable --push
```

(`${VERSION%.*}` is bash parameter expansion stripping the shortest match of `.*` from the end — for `2.2.1` this gives `2.2`, so a patch release like `2.2.1` → `2.2.2` updates the *same* `2.2` docs rather than minting a new version each patch, matching the "version by minor" global constraint. This job runs after `create-github-release`, the same informational-only position as the `benchmark-and-record` job — publish has already happened by this point, so it cannot block or be blocked by the release. No `submodules: true` on the checkout — unlike the benchmark job, doc deployment doesn't need the jsonata-js reference-suite submodule.)

- [ ] **Step 2: Validate the YAML**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML OK"
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "$(cat <<'EOF'
feat: cut a pinned doc version + stable alias at release time

New post-release job (same informational-only position as
benchmark-and-record - runs after create-github-release, so it can't
block or be blocked by the actual release). Deploys the release as a
mike version named after its major.minor (patch releases update the
same minor version's docs, not a new version per patch), aliases it
as "stable", and sets "stable" as the site's default so the root URL
always shows the latest real release rather than the bleeding-edge
dev build from docs.yml.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Fix stale `gh-pages` references in `.github/workflows/README.md`

**Files:**
- Modify: `.github/workflows/README.md:77` and `.github/workflows/README.md:260`

**Interfaces:** none — pure documentation fix, no code interfaces.

- [ ] **Step 1: Fix the first stale reference**

Find (line 77, in the benchmark workflow's "Outputs:" list):
```markdown
- Historical data on gh-pages branch
```

Replace with:
```markdown
- Historical data on the `benchmark-data` branch (one `results/v<version>.json` per release, plus `results/latest.json`)
```

- [ ] **Step 2: Fix the second stale reference**

Find (line 260, under "### Weekly Tasks"):
```markdown
- Check benchmark trends on gh-pages
```

Replace with:
```markdown
- Check benchmark trends on the `benchmark-data` branch
```

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/README.md
git commit -m "$(cat <<'EOF'
docs: fix stale gh-pages references to benchmark data

Benchmark history moved to the benchmark-data branch in an earlier
project (gh-pages was being destroyed by every mkdocs gh-deploy
--force, before this project replaced that mechanism with mike). This
README still described the old location.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: End-to-end verification

**Files:** none — this task only runs checks.

**Interfaces:** none.

- [ ] **Step 1: Push the branch and open a PR**

```bash
git push -u origin <branch-name>
gh pr create --title "Versioned docs via mike, fixing the gh-pages deploy race" --body "$(cat <<'EOF'
## Summary
- `docs.yml` now deploys via `mike deploy dev --push` instead of `mkdocs gh-deploy --force`, which was rewriting the entire `gh-pages` branch history on every push.
- `release.yml` gets a new post-release job that cuts a pinned `<major.minor>` doc version and moves a `stable` alias to it, so past releases' docs stay browsable instead of being overwritten by the latest build.
- This also fixes a long-standing GitHub Pages builder race (confirmed via the Pages builds API, dating back to March, unrelated to any recent work) caused by the force-push mechanism - `mike` only ever does a normal fetch-then-push.
- One-time cleanup: cleared `gh-pages`'s legacy non-mike content so it doesn't sit alongside the new versioned structure as orphaned cruft.
- Fixed two stale `.github/workflows/README.md` mentions of benchmark data living on `gh-pages` (it moved to `benchmark-data` in an earlier project).

## Test plan
- [ ] Manually trigger `docs.yml` against this PR's branch (via `workflow_dispatch`, since this workflow has no `pull_request` trigger) to verify the `mike deploy dev --push` step works before merging
- [ ] Confirm `mike list` shows `dev` after that run, and the site renders with a working version-selector
- [ ] Confirm no new "Page build failed" entries appear in the Pages builds history for that run
- [ ] The `release.yml` job (cutting a pinned version + `stable` alias) cannot be tested without a real release - the next real release is the true first end-to-end test of that path

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 2: Manually trigger `docs.yml` against this branch**

```bash
gh workflow run docs.yml --ref <branch-name>
```

(`docs.yml` has no `pull_request` trigger — only `push: branches: [main]` and `workflow_dispatch` — so this is the only way to exercise the new `mike deploy dev --push` step before merging, rather than merging blind and hoping.)

- [ ] **Step 3: Check the triggered run**

```bash
gh run list --workflow=docs.yml --limit 3
gh run view <run-id> --log
```

Expected: the "Deploy dev docs" step succeeds. If it fails, read the actual error — do not assume it's the same class of issue as the legacy race; `mike` failures are usually straightforward (e.g. a bad flag or missing git identity), not the git-history race this project is fixing.

- [ ] **Step 4: Confirm `gh-pages` and the Pages build history**

```bash
git fetch origin gh-pages
git ls-tree -r --name-only origin/gh-pages
gh api repos/txjmb/jsonata-core/pages/builds --jq '.[0:3] | .[] | {status, created_at, error: .error.message}'
```

Expected: the tree includes `dev/`, `versions.json`, `.nojekyll` (and nothing from the pre-cleanup legacy content, confirming Task 1 took effect). The most recent Pages build entry should show `"status": "built"` with no `"error"` — confirming the race is actually gone for this push, not just less frequent.

- [ ] **Step 5: Document what this task cannot verify**

The `deploy-release-docs` job in `release.yml` (Task 3) has no automated test here — it can only be exercised by an actual release (same limitation already documented for the `benchmark-and-record` job in the benchmark-comparison-infra project). Record this explicitly: the next real release cut after this merges is the true end-to-end test of whether `mike deploy <major.minor> stable --update-aliases --push` and `mike set-default stable --push` work correctly against production, including confirming `mike list` shows both `dev` and the new pinned version afterward.
