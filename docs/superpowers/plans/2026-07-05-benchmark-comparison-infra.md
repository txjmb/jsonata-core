# Durable Cross-Implementation Benchmark Comparison Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make cross-implementation benchmark comparison actually work: a durable history branch that nothing can silently wipe, all 4 comparison targets (jsonatapy, jsonata-js, jsonata-rs, jsonata-python) producing real numbers in every *release-triggered* run, and a fast PR/push job for day-to-day feedback.

**Scope revision found during Task 5 end-to-end verification (not knowable when this plan was written):** `jsonata-python` (rayokota) is genuinely ~18-45ms/iteration — 2,300-69,000x slower than JS per this project's own historical benchmark notes. It was never actually installed in the PR/push job before this project (a real gap this plan fixed), but once it started actually running instead of failing instantly, the PR job's runtime jumped from ~2 minutes to ~40 minutes. Per user decision: `jsonata-python` is installed and benchmarked **only** in the release-triggered job (`release.yml`), not the PR/push job (`benchmark.yml`) — the PR job keeps jsonata-rs (fast, ~0.01-0.3ms/iteration, no runtime impact) but omits jsonata-python entirely, preserving fast PR feedback. The release job still gets the full 4-way comparison, matching the original goal of "always find performance regressions" at the point where a long runtime doesn't cost anything (releases are infrequent and this job is already informational-only).

**Architecture:** A new orphan branch (`benchmark-data`) holds one JSON file per release plus a `latest.json` pointer, written only by a new post-release job in `release.yml`. The existing PR/push job in `benchmark.yml` gets two independent fixes — building/installing the two previously-dead comparison targets, and reading `benchmark-data` instead of the `gh-pages` path that gets destroyed by every `mkdocs gh-deploy --force`. The duplicated comparison logic between the two jobs is extracted to a standalone script (`benchmarks/python/compare.py`) rather than existing twice as inline heredocs.

**Tech Stack:** GitHub Actions (YAML), Python (compare.py, existing benchmark.py), Cargo (jsonata-rs bench harness), npm (jsonata-js).

## Global Constraints

- No changes to the >10% regression threshold in `compare.py`'s logic.
- No new benchmark test cases/expressions — this is infrastructure only, not new coverage.
- No retroactive backfill of `benchmark-data` — it starts empty and accumulates from the next release forward.
- Do not touch `benchmark.yml`'s `schedule`/`workflow_dispatch` triggers, or `docs.yml` at all.
- Do not change this project's own dependency pins (`Cargo.lock`, `uv.lock` at repo root) — the always-latest requirement applies only to the 3 external comparison targets' install steps inside the benchmark harness (`benchmarks/rust/Cargo.lock` IS in scope — it pins `jsonata-rs`, one of the comparison targets, not our own dependency).
- Release job in `release.yml` is informational-only: it runs after `create-github-release`, so it cannot block a release that has already published to PyPI/crates.io.
- One fresh GitHub issue per release when a regression is detected (not an accumulating/reopened issue).

---

## Task 1: Create the `benchmark-data` branch

**Files:** none (a one-time repo setup action, not a code change).

**Interfaces:**
- Produces: an empty orphan branch `benchmark-data` on `origin`, containing only `results/.gitkeep`. Every later task assumes this branch already exists.

- [ ] **Step 1: Create and push the orphan branch**

Run from the current worktree (any clean checkout works — this only pushes a new branch, it doesn't affect the current branch's own history):

```bash
git checkout --orphan benchmark-data
git rm -rf --cached . > /dev/null
git clean -fdx -- . ':!docs/superpowers'
mkdir -p results
touch results/.gitkeep
git add results/.gitkeep
git commit -m "chore: initialize benchmark-data branch for historical benchmark results"
git push origin benchmark-data
```

The `git clean -fdx -- . ':!docs/superpowers'` step removes the now-untracked working tree files left over from the previous branch's checkout (everything except this plan's own `docs/superpowers/` directory, so you don't delete the very plan you're executing) — the orphan branch's tree should contain nothing but `results/.gitkeep`.

- [ ] **Step 2: Verify the branch is correct on the remote**

```bash
git ls-tree origin/benchmark-data
```

Expected: exactly one entry, `results/.gitkeep` (as a blob inside the `results` tree — `git ls-tree` shows the top-level `results` tree object; run `git ls-tree -r origin/benchmark-data` to see the full path if you want to confirm the leaf file directly).

- [ ] **Step 3: Return to the working branch**

```bash
git checkout feat/benchmark-comparison-infra
git status --short
```

Expected: clean status, back on `feat/benchmark-comparison-infra`, no orphan-branch artifacts left in the working tree.

No commit needed for this task — nothing in the working branch's own tree changed.

---

## Task 2: Extract `compare.py` to a standalone script (and fix a real output-writing bug found while extracting it)

**Files:**
- Create: `benchmarks/python/compare.py`
- Modify: `.github/workflows/benchmark.yml:138-212` (the "Compare with baseline" step)

**Interfaces:**
- Produces: `benchmarks/python/compare.py`, invoked as `python python/compare.py <baseline.json> <results.json>` from a `cd benchmarks` working directory. Writes `comparison.json` (relative to cwd) with `{"regressions": [...], "improvements": [...]}`. Sets `regression_detected=true` on `$GITHUB_OUTPUT` when any regression is found (only if the `GITHUB_OUTPUT` env var is actually set — safe to run standalone locally too).
- Consumes: two JSON files matching the existing `benchmark.py` results shape (`{"results": [{"name": ..., "jsonatapy_ms": ...}, ...]}`).

**Why this task exists, not just "move the code":** the current inline heredoc (`.github/workflows/benchmark.yml:145-210`) has a real bug — it writes to a file literally named `$GITHUB_OUTPUT` (a dollar-sign-prefixed literal filename), not the actual GitHub Actions output file. This is because the heredoc delimiter is quoted (`<< 'EOF'`), which disables all shell variable expansion inside the heredoc body, so the Python string `'$GITHUB_OUTPUT'` never gets substituted to the real path. This means `regression_detected` has never actually been set to `true` on any run, regardless of whether real regressions existed — the `Fail on regression` step (`benchmark.yml:346-351`) has been dead code. Fix this while extracting.

- [ ] **Step 1: Write the failing test**

Create `tests/python/test_benchmark_compare.py`:

```python
"""Tests for benchmarks/python/compare.py's regression-detection logic."""

import json
import os
import subprocess
import sys
from pathlib import Path

COMPARE_SCRIPT = Path(__file__).parent.parent.parent / "benchmarks" / "python" / "compare.py"


def _write_results(path, entries):
    path.write_text(json.dumps({"results": entries}))


def test_detects_regression_over_10_percent(tmp_path):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 12.0}])  # 20% slower

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, result.stderr
    assert "Found 1 regressions" in result.stdout

    comparison = json.loads((tmp_path / "comparison.json").read_text())
    assert len(comparison["regressions"]) == 1
    assert comparison["regressions"][0]["name"] == "Array Sum"
    assert comparison["improvements"] == []


def test_detects_improvement_over_10_percent(tmp_path):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 8.0}])  # 20% faster

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, result.stderr
    comparison = json.loads((tmp_path / "comparison.json").read_text())
    assert comparison["regressions"] == []
    assert len(comparison["improvements"]) == 1


def test_within_threshold_is_neither(tmp_path):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 10.5}])  # 5% slower

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
    )

    assert result.returncode == 0, result.stderr
    comparison = json.loads((tmp_path / "comparison.json").read_text())
    assert comparison["regressions"] == []
    assert comparison["improvements"] == []


def test_writes_regression_detected_to_github_output(tmp_path, monkeypatch):
    baseline = tmp_path / "baseline.json"
    current = tmp_path / "current.json"
    output_file = tmp_path / "github_output.txt"
    output_file.write_text("")
    _write_results(baseline, [{"name": "Array Sum", "jsonatapy_ms": 10.0}])
    _write_results(current, [{"name": "Array Sum", "jsonatapy_ms": 12.0}])

    env = dict(os.environ)
    env["GITHUB_OUTPUT"] = str(output_file)

    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT), str(baseline), str(current)],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        env=env,
    )

    assert result.returncode == 0, result.stderr
    assert "regression_detected=true" in output_file.read_text()


def test_missing_args_exits_nonzero():
    result = subprocess.run(
        [sys.executable, str(COMPARE_SCRIPT)],
        capture_output=True,
        text=True,
    )
    assert result.returncode != 0
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
uv run pytest tests/python/test_benchmark_compare.py -v
```

Expected: FAIL — `benchmarks/python/compare.py` doesn't exist yet (collection error or `FileNotFoundError`/non-zero exit from the subprocess calls).

- [ ] **Step 3: Create `benchmarks/python/compare.py`**

```python
#!/usr/bin/env python3
"""Compare current benchmark results against a baseline, detecting regressions.

A test is a regression if jsonatapy_ms is more than 10% slower than the
baseline; an improvement if more than 10% faster. Writes comparison.json
(in the current working directory) with both lists, and — if the
GITHUB_OUTPUT environment variable is set — appends
"regression_detected=true" to it when any regression is found.

Usage: compare.py <baseline.json> <results.json>
"""

import json
import os
import sys

REGRESSION_THRESHOLD_PCT = 10
IMPROVEMENT_THRESHOLD_PCT = -10


def main() -> int:
    if len(sys.argv) != 3:
        print("Usage: compare.py <baseline.json> <results.json>", file=sys.stderr)
        return 2

    baseline_path, results_path = sys.argv[1], sys.argv[2]

    with open(baseline_path) as f:
        baseline = json.load(f)
    with open(results_path) as f:
        current = json.load(f)

    regressions = []
    improvements = []

    for curr_result in current["results"]:
        name = curr_result["name"]
        baseline_result = next((r for r in baseline["results"] if r["name"] == name), None)

        if not baseline_result:
            continue
        if not curr_result.get("jsonatapy_ms") or not baseline_result.get("jsonatapy_ms"):
            continue

        curr_time = curr_result["jsonatapy_ms"]
        base_time = baseline_result["jsonatapy_ms"]
        change_pct = ((curr_time - base_time) / base_time) * 100

        entry = {
            "name": name,
            "baseline_ms": base_time,
            "current_ms": curr_time,
            "change_pct": change_pct,
        }

        if change_pct > REGRESSION_THRESHOLD_PCT:
            regressions.append(entry)
        elif change_pct < IMPROVEMENT_THRESHOLD_PCT:
            improvements.append(entry)

    print(f"Found {len(regressions)} regressions and {len(improvements)} improvements")

    if regressions:
        print("\n⚠️ Performance Regressions Detected:")
        for r in regressions:
            print(f"  - {r['name']}: {r['baseline_ms']:.2f}ms → {r['current_ms']:.2f}ms ({r['change_pct']:+.1f}%)")

    if improvements:
        print("\n✅ Performance Improvements:")
        for i in improvements:
            print(f"  - {i['name']}: {i['baseline_ms']:.2f}ms → {i['current_ms']:.2f}ms ({i['change_pct']:+.1f}%)")

    with open("comparison.json", "w") as f:
        json.dump({"regressions": regressions, "improvements": improvements}, f, indent=2)

    if regressions:
        github_output = os.environ.get("GITHUB_OUTPUT")
        if github_output:
            with open(github_output, "a") as f:
                f.write("regression_detected=true\n")

    return 0


if __name__ == "__main__":
    sys.exit(main())
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
uv run pytest tests/python/test_benchmark_compare.py -v
```

Expected: 5 passed.

- [ ] **Step 5: Update `benchmark.yml`'s "Compare with baseline" step to call the script**

In `.github/workflows/benchmark.yml`, replace the entire step (currently lines 138-212, from `- name: Compare with baseline` through the closing `python compare.py "${{ steps.run_benchmark.outputs.results_file }}"` line) with:

```yaml
      - name: Compare with baseline
        id: compare
        if: github.event_name == 'pull_request' && hashFiles('benchmarks/baseline/baseline.json') != ''
        run: |
          cd benchmarks
          python python/compare.py baseline/baseline.json "${{ steps.run_benchmark.outputs.results_file }}"
```

- [ ] **Step 6: Validate the YAML and run the full local test suite**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/benchmark.yml'))" && echo "YAML OK"
uv run pytest tests/python/test_benchmark_compare.py -v
uv run ruff check benchmarks/python/compare.py tests/python/test_benchmark_compare.py
uv run ruff format --check benchmarks/python/compare.py tests/python/test_benchmark_compare.py
```

Expected: YAML OK, 5 passed, ruff clean on both new files (run `uv run ruff format benchmarks/python/compare.py tests/python/test_benchmark_compare.py` first if formatting isn't clean, then re-check).

- [ ] **Step 7: Commit**

```bash
git add benchmarks/python/compare.py tests/python/test_benchmark_compare.py .github/workflows/benchmark.yml
git commit -m "$(cat <<'EOF'
refactor: extract compare.py from inline heredoc, fix dead regression output

The inline heredoc in benchmark.yml wrote to a file literally named
'$GITHUB_OUTPUT' (a quoted heredoc delimiter disables shell expansion,
so the Python string never got substituted to the real output path).
regression_detected has therefore never actually been set to true on
any run, making the "Fail on regression" step dead code regardless of
whether real regressions existed. Extracting to a real, testable
script fixes this and lets the upcoming release job reuse the same
comparison logic instead of duplicating a second heredoc.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Fix the PR/push job in `benchmark.yml`

**Files:**
- Modify: `.github/workflows/benchmark.yml` (multiple steps within the `benchmark` job)

**Interfaces:**
- Consumes: `benchmarks/python/compare.py` from Task 2.
- Produces: nothing new for later tasks — Task 4 (release.yml) mirrors this job's install-step patterns but is a separate file with its own steps.

- [ ] **Step 1: Update "Install Python dependencies" to always fetch jsonata-python's latest release**

Find (around line 78-80):
```yaml
      - name: Install Python dependencies
        run: |
          uv pip install --system maturin rich matplotlib pandas
```

Replace with:
```yaml
      - name: Install Python dependencies
        run: |
          uv pip install --system --upgrade maturin rich matplotlib pandas jsonata
```

(`jsonata` is rayokota's PyPI package, imported as `jsonata_python` — already listed in `pyproject.toml`'s `bench` extras and already fully wired into `benchmark.py`'s `JSONATA_PYTHON_AVAILABLE` gating; it was just never installed here. `--upgrade` ensures it's always the latest published release, not a cached older wheel.)

- [ ] **Step 2: Update "Install JavaScript dependencies" to always fetch jsonata-js's latest release**

Find (around line 82-85):
```yaml
      - name: Install JavaScript dependencies
        run: |
          cd benchmarks/javascript
          npm install
```

Replace with:
```yaml
      - name: Install JavaScript dependencies
        run: |
          cd benchmarks/javascript
          npm install jsonata@latest
```

(`package.json` pins `"jsonata": "^2.1.0"`, which is capped below a hypothetical 3.0.0 release. Installing `jsonata@latest` explicitly bypasses that range for the benchmark run specifically, without changing the committed `package.json` pin.)

- [ ] **Step 3: Add a new step to build the jsonata-rs comparison binary**

Insert this new step immediately after "Install JavaScript dependencies" (i.e. before the existing "Build jsonatapy (optimized)" step, around what is currently line 86):

```yaml
      - name: Build jsonata-rs comparison binary
        run: |
          cd benchmarks/rust
          cargo update -p jsonata-rs
          cargo build --release
```

(`benchmarks/rust/` is a real, already-working subproject — `cargo build --release` there produces `target/release/jsonata-rs-bench`, which `benchmark.py`'s `_run_jsonata_rs_benchmark` already looks for and calls; it was simply never built in CI. `cargo update -p jsonata-rs` is scoped to that one dependency — it re-resolves `jsonata-rs` to its latest release matching the `"0.3"` range in `benchmarks/rust/Cargo.toml` without touching any other locked dependency in that Cargo.lock.)

- [ ] **Step 4: Update the "Verify installation" step to also confirm jsonata-rs and jsonata-python**

Find (around line 92-95):
```yaml
      - name: Verify installation
        run: |
          python -c "import jsonatapy; print(f'jsonatapy version: {jsonatapy.__version__}')"
          cd benchmarks/javascript && node -e "const jsonata = require('jsonata'); console.log('jsonata installed');"
```

Replace with:
```yaml
      - name: Verify installation
        run: |
          python -c "import jsonatapy; print(f'jsonatapy version: {jsonatapy.__version__}')"
          python -c "import jsonata; print('jsonata-python installed')"
          cd benchmarks/javascript && node -e "const jsonata = require('jsonata'); console.log('jsonata installed');"
          test -x ../rust/target/release/jsonata-rs-bench && echo "jsonata-rs-bench built"
```

- [ ] **Step 5: Point the baseline-download step at `benchmark-data` instead of `gh-pages`**

Find the entire "Download baseline (main branch)" step (around line 121-136):
```yaml
      - name: Download baseline (main branch)
        if: github.event_name == 'pull_request'
        continue-on-error: true
        run: |
          mkdir -p benchmarks/baseline

          # Try to download from gh-pages branch
          git fetch origin gh-pages:gh-pages 2>/dev/null || true

          if git show gh-pages:benchmark-results/latest.json > benchmarks/baseline/baseline.json.tmp 2>/dev/null; then
            mv benchmarks/baseline/baseline.json.tmp benchmarks/baseline/baseline.json
            echo "Baseline results downloaded from gh-pages"
          else
            rm -f benchmarks/baseline/baseline.json.tmp
            echo "No baseline results found"
          fi
```

Replace with:
```yaml
      - name: Download baseline (previous release)
        if: github.event_name == 'pull_request'
        continue-on-error: true
        run: |
          mkdir -p benchmarks/baseline

          # Try to download from the benchmark-data branch
          git fetch origin benchmark-data 2>/dev/null || true

          if git show origin/benchmark-data:results/latest.json > benchmarks/baseline/baseline.json.tmp 2>/dev/null; then
            mv benchmarks/baseline/baseline.json.tmp benchmarks/baseline/baseline.json
            echo "Baseline results downloaded from benchmark-data"
          else
            rm -f benchmarks/baseline/baseline.json.tmp
            echo "No baseline results found"
          fi
```

(`gh-pages` is destroyed on every push by `docs.yml`'s `mkdocs gh-deploy --force` — this is the root cause investigated before this plan existed. `benchmark-data`, created in Task 1, is never touched by any other workflow.)

- [ ] **Step 6: Validate the YAML**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/benchmark.yml'))" && echo "YAML OK"
```

- [ ] **Step 7: Commit**

```bash
git add .github/workflows/benchmark.yml
git commit -m "$(cat <<'EOF'
fix: make jsonata-rs and jsonata-python comparisons actually run in CI

Both had full calling code already in benchmark.py
(_run_jsonata_rs_benchmark, JSONATA_PYTHON_AVAILABLE gating) but were
always "NOT AVAILABLE": jsonata-rs's bench binary was never built
(benchmarks/rust/ is a real, working subproject — confirmed buildable
directly), and jsonata-python was never installed despite being listed
in pyproject.toml's bench extras. Also points baseline lookup at the
new benchmark-data branch (Task 1) instead of gh-pages, which
docs.yml's mkdocs gh-deploy --force destroys on every push. All three
external comparison targets (jsonata-js, jsonata-rs, jsonata-python)
now install/build against their latest published release on every
run, not a pinned/locked version.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Add the release-triggered `benchmark-and-record` job to `release.yml`

**Files:**
- Modify: `.github/workflows/release.yml` (add a new job; add `issues: write` — scoped to just this job, not the workflow-level permissions block, which stays as-is)

**Interfaces:**
- Consumes: `needs.validate-version.outputs.version` (already defined in `release.yml:83-84`, a plain semver string like `2.1.5`), `needs.create-github-release.result` (already an implicit job output), `benchmarks/python/compare.py` from Task 2, `origin/benchmark-data` branch from Task 1.
- Produces: nothing consumed elsewhere in this plan — this is the terminal task.

- [ ] **Step 1: Add the new job at the end of `release.yml`**

Append this job after the existing `dry-run-summary` job (i.e. at the end of the file, after line 546):

```yaml

  benchmark-and-record:
    name: Benchmark and Record Release Performance
    runs-on: ubuntu-latest
    needs: [validate-version, create-github-release]
    if: needs.create-github-release.result == 'success' && github.event.inputs.dry_run != 'true'
    timeout-minutes: 45

    permissions:
      contents: write
      issues: write

    steps:
      - name: Checkout release tag
        uses: actions/checkout@v6
        with:
          ref: refs/tags/v${{ needs.validate-version.outputs.version }}
          submodules: true
          fetch-depth: 0

      - name: Set up Python
        uses: actions/setup-python@v6
        with:
          python-version: '3.11'

      - name: Set up Node.js
        uses: actions/setup-node@v6
        with:
          node-version: '20'
          cache: 'npm'
          cache-dependency-path: benchmarks/javascript/package.json

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache Rust dependencies
        uses: Swatinem/rust-cache@v2

      - name: Install UV
        uses: astral-sh/setup-uv@v7

      - name: Install Python dependencies
        run: |
          uv pip install --system --upgrade maturin rich matplotlib pandas jsonata

      - name: Install JavaScript dependencies
        run: |
          cd benchmarks/javascript
          npm install jsonata@latest

      - name: Build jsonata-rs comparison binary
        run: |
          cd benchmarks/rust
          cargo update -p jsonata-rs
          cargo build --release

      - name: Build jsonatapy (optimized)
        run: |
          maturin build --release --out dist
          uv pip install --system --find-links dist jsonatapy

      - name: Verify installation
        run: |
          python -c "import jsonatapy; print(f'jsonatapy version: {jsonatapy.__version__}')"
          python -c "import jsonata; print('jsonata-python installed')"
          cd benchmarks/javascript && node -e "const jsonata = require('jsonata'); console.log('jsonata installed');"
          test -x ../rust/target/release/jsonata-rs-bench && echo "jsonata-rs-bench built"

      - name: Run benchmark suite
        id: run_benchmark
        run: |
          cd benchmarks
          python python/benchmark.py --iterations 5000
          RESULTS_FILE=$(ls -t results/benchmark_results_*.json | head -1)
          echo "results_file=$RESULTS_FILE" >> $GITHUB_OUTPUT
          echo "Benchmark results saved to: $RESULTS_FILE"

      - name: Download previous release baseline
        id: baseline
        continue-on-error: true
        run: |
          mkdir -p benchmarks/baseline
          git fetch origin benchmark-data 2>/dev/null || true

          if git show origin/benchmark-data:results/latest.json > benchmarks/baseline/baseline.json.tmp 2>/dev/null; then
            mv benchmarks/baseline/baseline.json.tmp benchmarks/baseline/baseline.json
            echo "has_baseline=true" >> $GITHUB_OUTPUT
            echo "Baseline results downloaded from benchmark-data"
          else
            rm -f benchmarks/baseline/baseline.json.tmp
            echo "has_baseline=false" >> $GITHUB_OUTPUT
            echo "No baseline results found (this is the first release recorded to benchmark-data)"
          fi

      - name: Compare with previous release
        id: compare
        if: steps.baseline.outputs.has_baseline == 'true'
        run: |
          cd benchmarks
          python python/compare.py baseline/baseline.json "${{ steps.run_benchmark.outputs.results_file }}"

      - name: Record results to benchmark-data
        run: |
          VERSION="${{ needs.validate-version.outputs.version }}"
          RESULTS_FILE="$(pwd)/benchmarks/${{ steps.run_benchmark.outputs.results_file }}"

          git config user.name "github-actions[bot]"
          git config user.email "github-actions[bot]@users.noreply.github.com"

          git fetch origin benchmark-data
          git worktree add -B benchmark-data /tmp/benchmark-data origin/benchmark-data

          mkdir -p /tmp/benchmark-data/results
          cp "$RESULTS_FILE" "/tmp/benchmark-data/results/v${VERSION}.json"
          cp "$RESULTS_FILE" "/tmp/benchmark-data/results/latest.json"

          cd /tmp/benchmark-data
          git add "results/v${VERSION}.json" results/latest.json
          git commit -m "chore: record benchmark results for v${VERSION}"
          git push origin HEAD:benchmark-data

      - name: Open regression issue
        if: steps.compare.outputs.regression_detected == 'true'
        uses: actions/github-script@v9
        with:
          script: |
            const fs = require('fs');
            const comparison = JSON.parse(fs.readFileSync('benchmarks/comparison.json', 'utf8'));
            const version = '${{ needs.validate-version.outputs.version }}';

            let body = `Release v${version} is more than 10% slower than the previous release on the following benchmarks:\n\n`;
            body += '| Test | Previous | Current | Change |\n';
            body += '|------|----------|---------|--------|\n';
            for (const r of comparison.regressions) {
              body += `| ${r.name} | ${r.baseline_ms.toFixed(2)}ms | ${r.current_ms.toFixed(2)}ms | **${r.change_pct.toFixed(1)}%** |\n`;
            }

            await github.rest.issues.create({
              owner: context.repo.owner,
              repo: context.repo.repo,
              title: `Performance regression detected in v${version}`,
              body: body,
            });
```

- [ ] **Step 2: Validate the YAML**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/release.yml'))" && echo "YAML OK"
```

- [ ] **Step 3: Syntax-check the embedded JavaScript**

```bash
python3 -c "
import yaml
doc = yaml.safe_load(open('.github/workflows/release.yml'))
for job in doc['jobs'].values():
    for step in job.get('steps', []):
        if step.get('uses', '').startswith('actions/github-script'):
            script = step['with']['script']
            # Substitute the \${{ ... }} template expressions with placeholder
            # strings so plain node --check can parse the result.
            import re
            script = re.sub(r\"\\\$\{\{[^}]*\}\}\", \"'placeholder'\", script)
            open('/tmp/release-script-check.js', 'w').write('async function main(){' + script + '}')
            print('extracted', len(script), 'chars')
"
node --check /tmp/release-script-check.js && echo "JS syntax OK"
```

Expected: `JS syntax OK`.

- [ ] **Step 4: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "$(cat <<'EOF'
feat: record and compare release-time benchmarks to benchmark-data

Adds a post-release job that runs the full benchmark suite (same
always-latest comparison-target install steps as the PR job), commits
results to the benchmark-data branch as results/v<version>.json
(updating results/latest.json), and opens a fresh GitHub issue per
release if any test regresses more than 10% versus the previous
release's numbers. Runs after create-github-release, so it is
informational-only — publish-pypi/publish-crates have already
happened by this point in the workflow and cannot be blocked.

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: End-to-end verification

**Files:** none — this task only runs checks.

**Interfaces:** none.

- [ ] **Step 1: Run the full local test suite one more time**

```bash
uv run pytest tests/python/test_benchmark_compare.py -v
cargo test 2>&1 | tail -20
```

Expected: all pass, no regressions from anything touched in this plan (this plan touches no `src/*.rs` files, so `cargo test`'s result should be identical to whatever it was before this plan started).

- [ ] **Step 2: Open a throwaway PR to exercise the fixed `benchmark.yml` job end-to-end**

```bash
git push -u origin feat/benchmark-comparison-infra
gh pr create --title "Durable cross-implementation benchmark comparison" --body "$(cat <<'EOF'
## Summary
- New `benchmark-data` orphan branch for durable benchmark history (nothing else writes to it; `gh-pages` was being destroyed by every `mkdocs gh-deploy --force`).
- `benchmark.yml`'s PR/push job: jsonata-rs and jsonata-python now actually build/install and produce real numbers (previously always "NOT AVAILABLE" despite full calling code already existing in benchmark.py). Baseline comparison now points at benchmark-data instead of the dead gh-pages path.
- Extracted `compare.py` from an inline heredoc that had a real bug: `regression_detected` was never actually set (the quoted heredoc delimiter prevented `$GITHUB_OUTPUT` from ever being expanded, so it wrote to a literally-named `$GITHUB_OUTPUT` file instead of the real output path).
- New post-release job in `release.yml` that records history and opens a regression issue, run after `create-github-release` (informational-only, since publish has already happened by that point).
- All three external comparison targets install/build against their latest published release on every run now, not a pinned/locked version.

## Test plan
- [ ] `Run Benchmarks` check on this PR shows real numbers for jsonata-rs and jsonata-python (not "NOT AVAILABLE")
- [ ] `uv run pytest tests/python/test_benchmark_compare.py` passes
- [ ] The release-triggered job (release.yml) cannot be fully exercised without cutting a real release — the next real release after this merges is the true end-to-end test of that path

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

- [ ] **Step 3: Check the PR's `Run Benchmarks` job**

```bash
gh pr checks --watch
```

Once it completes, fetch the job log and confirm both previously-dead targets now report real numbers:

```bash
RUN_ID=$(gh run list --workflow=benchmark.yml --branch feat/benchmark-comparison-infra --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN_ID" --log 2>&1 | grep -i "jsonata-rs:\|jsonata-python:" | head -20
```

Expected: lines like `jsonata-rs:  X.XX ms (...)` and `jsonata-python:  X.XX ms (...)` — no `NOT AVAILABLE`. If either still shows `NOT AVAILABLE`, re-check the corresponding step's log (`Build jsonata-rs comparison binary` or `Install Python dependencies`) for the actual failure before considering this task done.

- [ ] **Step 4: Document what this task cannot verify**

The release job in `release.yml` (Task 4) has no automated test here — GitHub Actions `workflow_dispatch` release workflows cannot be safely dry-run against production PyPI/crates.io publishing as part of a plan's verification step. Record this explicitly rather than silently skip it: the first real release cut after this plan merges is the actual end-to-end test of the `benchmark-and-record` job, including whether `benchmark-data` receives its first real `results/v<version>.json` and `latest.json`, and whether the regression-issue-opening logic fires correctly (there's no baseline yet on the very first release, so `has_baseline` should be `false` and no comparison/issue step should run — confirm this specifically when that release happens).
