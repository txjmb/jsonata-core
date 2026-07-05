# Durable cross-implementation benchmark comparison

## Context

The goal is to maintain release-triggered benchmark comparisons between this project
(`jsonata-core`/`jsonatapy`) and three other implementations — jsonata-rs (pure Rust
competitor), jsonata-python (rayokota's Python wrapper), and jsonata-js (the JavaScript
reference) — so performance regressions are always caught and the "performant
implementation" claim stays substantiated over time, not just asserted once in a README.

Today none of this actually works, for reasons discovered during investigation rather than
assumed upfront:

- **Historical comparison data has never survived.** The `benchmark.yml` workflow's
  `publish-results` job commits `benchmark-results/latest.json` to the `gh-pages` branch on
  every push to `main` — and it does write successfully (confirmed: commit `6f61316`,
  "Update benchmark results - 2026-07-04", added a real `benchmark-results/latest.json`).
  But `docs.yml` also triggers on every push to `main` and runs `mkdocs gh-deploy --force`,
  which unconditionally force-pushes a fresh commit containing *only* the mkdocs site
  output — deleting `benchmark-results/` entirely. The very next commit after `6f61316`
  (`701f64d`, "Deployed ... with MkDocs") removed all 4 files. This publish-then-destroy
  cycle repeats every time both workflows fire, so no history has ever accumulated. The
  existing "Compare with baseline" step in `benchmark.yml` (guarded by
  `hashFiles('benchmarks/baseline/baseline.json') != ''`) has therefore never actually run
  on any PR — it always finds no baseline.
- **Two of the four comparison targets never produce numbers in CI.** `benchmarks/rust/`
  is a real, working subproject (`jsonata_rs_bench.rs`, confirmed buildable directly —
  `cargo build --release` succeeds and the resulting binary runs, reading a JSON envelope
  from stdin and printing `{"elapsed_ms": ...}`), but CI never runs `cargo build --release`
  in that directory, so the benchmark script always reports "jsonata-rs binary not found".
  Separately, `jsonata` (rayokota's package, imported as `jsonata_python`) is listed in
  `pyproject.toml`'s `bench` extras but the workflow's install step
  (`uv pip install --system maturin rich matplotlib pandas`) never installs it, so it's
  always "NOT AVAILABLE". `benchmark.py` already has full calling code for both
  (`_run_jsonata_rs_benchmark`, `JSONATA_PYTHON_AVAILABLE` gating throughout) — these are
  pure CI-environment gaps, not missing code.
- **No release-triggered run exists at all.** `release.yml` is `workflow_dispatch`-only
  (manual version input); there is no hook tying benchmark recording to a release.
- **Comparison targets can silently go stale.** `benchmarks/javascript/package.json` pins
  `jsonata: "^2.1.0"` (capped below a hypothetical 3.0.0, no lock file committed);
  `benchmarks/rust/Cargo.lock` *is* committed and pins whatever `jsonata-rs` 0.3.x version
  was resolved when it was last generated. Neither guarantees comparison against each
  competitor's actual latest release.

## Scope

- **Storage:** move durable benchmark history off `gh-pages` entirely, onto a new,
  dedicated orphan branch (`benchmark-data`) that only the benchmark workflow ever
  touches.
- **PR/push job (`benchmark.yml`):** fix the two dead comparison targets (build
  `jsonata-rs-bench`, install `jsonata`), point baseline lookup at the new branch, and
  always fetch each comparison target's latest published release rather than a pinned/
  locked version. This job continues to run on every PR/push exactly as it does today
  (informational, now with working comparisons) — it never writes to `benchmark-data`.
- **Release job (new job in `release.yml`):** runs after `create-github-release`,
  compares against `benchmark-data`'s `results/latest.json`, commits a new
  `results/v<version>.json` + updates `latest.json`, and opens a fresh GitHub issue per
  release if a >10% regression (the existing threshold in `compare.py`) is detected. This
  is informational-only — by the time it runs, `publish-pypi`/`publish-crates` have
  already happened, so it cannot block a release.

**Non-goals** (explicitly out of scope for this work):
- Changing the regression threshold (keeping `compare.py`'s existing >10% as-is).
- Adding new benchmark test cases or expressions — this is entirely about making the
  *comparison infrastructure* work, not expanding *what* gets measured (a separate,
  legitimate follow-up: several existing array-operation benchmarks show 10-90x slowdowns
  vs JS that are worth root-causing, but that's a different piece of work from this one).
- Retroactively backfilling history for past releases — `benchmark-data` starts empty and
  accumulates from the next release forward.
- Touching the weekly `schedule` or manual `workflow_dispatch` triggers on `benchmark.yml`
  — they keep running exactly as today.
- Changing our own project's dependency pins (`Cargo.lock`, `uv.lock` at the repo root) —
  the always-latest requirement applies only to the three external comparison targets'
  installation steps inside the benchmark harness.

## Design

### 1. Storage: `benchmark-data` orphan branch

```
benchmark-data (branch, unrelated history to main)
└── results/
    ├── v2.1.5.json
    ├── v2.2.0.json
    ├── v2.2.1.json      (one committed per release, filename = release version)
    └── latest.json      (copy of the newest — the one stable path every reader uses)
```

Each file is the same JSON shape `benchmark.py` already produces
(`implementations`, `results[]` with per-test `jsonatapy_ms`/`js_ms`/`jsonata_rs_ms`/
`jsonata_python_ms`/speedups) — no schema change, just a new commit location and a
version-stamped filename instead of a timestamp-stamped one.

Nothing else on this branch. `docs.yml`'s `gh-deploy --force` only ever touches
`gh-pages`, so there is no longer any workflow capable of colliding with this branch.

### 2. PR/push job changes (`benchmark.yml`)

- **Build the jsonata-rs comparison binary:** add a step
  `cd benchmarks/rust && cargo update -p jsonata-rs && cargo build --release` before the
  benchmark suite runs. The `cargo update -p jsonata-rs` (scoped to just that one
  dependency, not the whole lockfile) ensures the comparison is always against the latest
  published `jsonata-rs` release matching the `"0.3"` range in `benchmarks/rust/Cargo.toml`,
  not whatever was locked when `Cargo.lock` was last committed.
- **Install the jsonata-python comparison package:** add `jsonata` to the
  `uv pip install --system` step, with `--upgrade` so it's always the latest published
  version from PyPI rather than a cached/older wheel:
  `uv pip install --system --upgrade maturin rich matplotlib pandas jsonata`.
- **Always fetch latest jsonata-js:** change the JS dependency install step to
  `npm install jsonata@latest` explicitly (bypassing `package.json`'s `"^2.1.0"` pin, which
  is capped below a hypothetical 3.0.0 release) rather than a bare `npm install` that
  respects the committed range.
- **Point baseline lookup at the new branch:** in the "Download baseline" step, replace
  `git show gh-pages:benchmark-results/latest.json` with
  `git show origin/benchmark-data:results/latest.json`. Keep the existing graceful
  "no baseline found" fallback (`|| true` / continue-on-error pattern already there) for
  the case where `benchmark-data` doesn't exist yet (true on the very first run after this
  ships) or the branch fetch fails.
- **No changes needed to "Compare with baseline" or "Comment on PR"** — that logic is
  already correct; it's only ever been fed no data. Once `latest.json` is real, PR comments
  start showing genuine before/after regressions instead of only raw current-run numbers.
- This job continues to **only read** from `benchmark-data`, never write — keeps it
  side-effect-free and unable to race with the release job's writes.

### 3. Release job (new job in `release.yml`)

New job `benchmark-and-record`, `needs: [create-github-release]`, so it only runs after a
release is fully published (and therefore cannot gate or block the release itself — that
ship has already sailed by this point in the workflow):

1. Run the full benchmark suite, using the same always-latest install steps as the PR job
   (build `jsonata-rs-bench` fresh, install `jsonata` with `--upgrade`, `npm install
   jsonata@latest`).
2. Fetch `benchmark-data`'s `results/latest.json` (the prior release's numbers) and run the
   existing `compare.py` regression logic against the just-produced results.
3. Commit the new results to `benchmark-data` as `results/v<version>.json` (using the same
   `version` input `release.yml` already takes and threads through
   `validate-version`/`generate-changelog`/etc.), and overwrite `results/latest.json` with
   a copy of the same content.
4. If any regression (>10% slower, per the existing threshold) is detected: open a **new**
   GitHub issue (one per release, not an accumulating/reopened issue — keeps each release's
   regression report scoped to that release rather than conflating multiple releases'
   findings into one thread), titled `Performance regression detected in vX.Y.Z`, body
   listing each regressed test with before/after ms and % change (reusing `compare.py`'s
   existing regression list output). No issue is opened when there are no regressions.

### 4. Testing / verification

CI workflow changes are inherently harder to unit-test than application code; verification
here is necessarily staged:

- Validate YAML syntax and any embedded script logic (the `actions/github-script` JS block,
  `compare.py`) locally before pushing — same approach used for the earlier benchmark
  results-path fix in this project's history.
- Exercise `benchmarks/rust/`'s build and the `jsonata` pip install locally first (already
  spot-checked directly: `cargo build --release` in `benchmarks/rust/` succeeds and the
  binary runs correctly against a stdin JSON envelope).
- The real end-to-end test is a live PR: open one touching `src/**`, confirm the
  `Run Benchmarks` job shows all 4 comparison targets with real numbers (no more
  `NOT AVAILABLE` for jsonata-rs/jsonata-python), and confirm the baseline-download step
  degrades gracefully (not an error) when `benchmark-data` doesn't exist yet.
- The release-job path cannot be fully exercised without cutting an actual release. The
  first real release after this ships is the true end-to-end test of that path — flagged
  explicitly here rather than treated as covered by anything short of that.

## Definition of done

- `benchmark-data` branch exists, starts empty, and only `release.yml`'s new job ever
  writes to it.
- `benchmark.yml`'s PR/push job shows real numbers (not `NOT AVAILABLE`) for jsonata-rs and
  jsonata-python, and its baseline comparison actually runs (not silently skipped) once
  `benchmark-data` has at least one release's data.
- `release.yml` has a new post-publish job that records history and opens a regression
  issue when warranted, verified against the next real release cut after this ships.
- All three external comparison targets (jsonata-js, jsonata-rs, jsonata-python) are
  fetched/built fresh against their latest published release on every benchmark run, in
  both the PR job and the release job.
