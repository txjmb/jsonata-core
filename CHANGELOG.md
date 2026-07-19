# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Host-callable custom functions (Rust core): `Evaluator::register_fn` and
  `Evaluator::register_fn_override` let a host register native functions callable
  from an expression as `$name(...)` — the equivalent of jsonata-js's
  `registerFunction`. Functions are plain closures
  (`Fn(&[JValue]) -> Result<JValue, EvaluatorError>`) and resolve after the
  expression's own bindings/lambdas and before built-ins. `register_fn` rejects
  collisions with built-ins; `register_fn_override` allows deliberately replacing
  the impure built-ins (`$now`, `$millis`, `$random`, `$eval`) for determinism
  injection or sandboxing. Evaluation stays synchronous. See
  `examples/host_functions.rs` and the Rust crate docs. (Not yet exposed through
  the Python or C bindings.)

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [2.2.5] - 2026-07-14

### Added
- C API (`capi` cargo feature): use the engine from C, C++, or any language with C interop.
  Eight functions, JSON text in/out (`jsonata_compile`, `jsonata_evaluate`, `jsonata_bind_var`,
  `jsonata_free_expr`, `jsonata_free_string`, `jsonata_last_error_message`,
  `jsonata_last_error_code`, `jsonata_version`), thread-local error slot, and engine panics
  caught at the boundary instead of aborting the host process. Ships with a hand-written
  header (`bindings/c/jsonata.h`), build/link/CMake documentation (`bindings/c/README.md`),
  and a CI-gated smoke test compiled as both C and C++. Build with
  `cargo build --release --features capi`.

### Fixed
- Performance regression in v2.2.4 on small/fast expressions (issue #74): the
  `JSONATAPY_FORCE_TREE_WALKER` test toggle read the environment variable on every
  evaluation (both `evaluate()` and `evaluate_json()`), costing ~100-200ns per call —
  10-30% of a sub-microsecond expression. The toggle is now a process-wide atomic seeded
  from the environment once at import (whole-process forcing works unchanged) and flippable
  via a private test hook. Small-expression benchmarks recover 5-16%; the remaining few
  percent vs v2.2.3 on tiny payloads is the documented cost of lazy conversion (which makes
  realistic workloads up to 48% faster, see 2.2.4 notes).

### Changed
- Benchmark tooling: PR benchmark comments and release regression issues now state their
  comparison baseline explicitly (which commit/release, recorded when, on which runner),
  and the vs-jsonata-js comparison is labeled as such.

## [2.2.4] - 2026-07-13

### Added
- `JSONATAPY_FORCE_TREE_WALKER=1` environment variable (testing/debugging): forces every
  evaluation through the tree-walking evaluator, bypassing the default lazy/VM-preferred path.

### Changed
- `evaluate(dict)` now converts Python data lazily by default. Previously every call eagerly
  converted the entire input `dict` (and nested structures) to the internal value tree before
  evaluation began; now only the fields an expression actually touches are converted, and
  untouched input subtrees pass straight through to the output unchanged. Measured on the dev
  machine (min-of-5, vs. the prior eager `evaluate(dict)`):
  - `products.price` (100 objects): 34.9µs → 11.3µs (3.1x)
  - Filter by category (100 products, 9 fields each): 129.3µs → 23.4µs (5.5x)
  - `$sum(products[inStock].price)`: 116.4µs → 17.1µs (6.8x)
  - Complex dense transformation: 160.9µs → 79µs (2.0x)

  Every measured row now beats jsonata-js on identical data and machine (14.6/93.7/67.6/415.1µs
  respectively). See `benchmarks/python/lazy_check.py` for the reproducible gate.
- **Behavior change:** results containing an unmodified input subtree now reference the
  caller's *original* Python `dict`/`list` objects (result aliasing, matching jsonata-js),
  rather than a fresh copy. Mutating such a result mutates the corresponding input — copy
  explicitly (e.g. `copy.deepcopy`) first if you plan to mutate. Passed-through values also keep
  their exact Python type (an `int` field the expression never reads stays an `int`); fields the
  expression does touch still round-trip through the engine's number representation (whole
  values come back as Python `int`), as before.
- **Behavior change:** an unconvertible input value (e.g. a `set`) now raises `TypeError` only
  when the expression actually touches it, instead of eagerly for the whole input at the start
  of `evaluate()`. Any Python exception raised while lazily reading a field (e.g. `OverflowError`
  on an integer too large to represent) is likewise normalized to `TypeError` at this boundary,
  rather than propagating as its original exception type.

### Deprecated

### Removed

### Fixed

### Security

## [2.2.3] - 2026-07-12

### Added
- `evaluate_json_or_none()`'s `json_str` parameter now accepts `None` (in addition to a JSON
  string), binding the top-level context (`$`) to a true JSONata `Undefined` rather than an
  explicit `null`. (#68)
- `docs/cli.md`: a full command-line reference for both `jsonata` (Rust) and `jsonatapy`
  (Python), covering flags, input resolution, output/exit-code semantics, and the MCP
  subcommand — previously undocumented outside internal spec/plan files. (#68)

### Changed
- Release CLI binary archives are now named `jsonata-v<version>-<target>.tar.gz`/`.zip`
  (previously `jsonata-<target>.tar.gz`/`.zip` with no version embedded).

### Deprecated

### Removed

### Fixed
- The Python CLI's `-n`/`--null-input` now binds `$` to a true `Undefined`, matching the Rust
  CLI exactly (`jsonatapy -n '$'` now prints nothing, as `jsonata -n '$'` already did). It
  previously passed an explicit JSON `null` context instead, observable only for expressions
  referencing `$` directly. (#68)
- Release workflow: `publish-pypi` and `publish-crates` now require `build-cli-binaries` to
  succeed first. Previously the two registry publishes were independent of the CLI binary
  build, so a CLI build failure (untested in a real release prior to this) would have shipped
  a version to PyPI/crates.io with no CLI binaries attached and no way to reuse that version
  number.

### Security

## [2.2.2] - 2026-07-09

### Added
- Documented `JsonataData`, `evaluate_with_data`, and `evaluate_data_to_json` in `docs/api.md`
  (previously absent from the API reference entirely, despite being the "3-15x faster"
  pre-converted-data path highlighted in the README's Performance section). Added a cross-link
  from `docs/rust-crate.md` to the full auto-generated API reference on docs.rs. (#65)

### Changed
- Updated Rust dependencies to the latest versions compatible with existing (unpinned) semver
  requirements: `simd-json` 0.17.0 → 0.17.2 plus 6 transitive patch bumps. No security advisories
  found before or after (`cargo deny check`). (#65)

### Deprecated

### Removed

### Fixed
- SIMD-accelerated JSON parsing (`simd-json` feature, on by default) was consistently *slower*
  than the plain `serde_json` fallback for most payload sizes (up to 29% slower at 180KB), the
  opposite of its intent — caused by allocating fresh internal scratch buffers on every single
  parse call. Fixed by reusing a thread-local scratch buffer across calls; SIMD parsing now beats
  `serde_json` consistently (up to +22% faster) instead of losing at 3 of 4 tested sizes.
  Also corrected the README's "(optional feature)" wording for SIMD, which implied opt-in when
  it's actually enabled by default, including in published wheels. (#65)

### Security

## [2.2.1] - 2026-07-08

> Same code as [2.1.7](#217---2026-07-08) below, renumbered. This project's release versions
> track the jsonata-js major/minor version they target (patch numbers are independent — see
> README). `2.1.7` incorrectly continued the old `2.1.x` patch series even though this release's
> guardrails feature and signature-engine fixes target jsonata-js `2.2.0`/`2.2.1`; `2.1.7` is
> superseded immediately by this release and should not be used.

### Added
- Guardrails: `timeout` (ms, error code `D1012`), `max_stack_depth` (error code `D1011`), and
  `max_sequence_length` (error code `D2015`) keyword arguments on `compile()` and every
  `evaluate*()` call, enforced consistently across all three execution engines (tree-walker,
  compiled-expression fast path, bytecode VM). All default to `None` (unlimited) — no behavior
  change unless configured. See [Guardrails](docs/api.md#guardrails). (jsonata-js 2.2.1 Phase 2, #56)
- Documented the guardrails API in `docs/api.md`, `docs/usage.md`, and `docs/error-handling.md`
  (previously shipped with accurate Python docstrings but no user-facing docs), and corrected
  `docs/migration-from-js.md`'s stale claim that Python had no built-in timeout support.

### Fixed
- A deeply-nested expression (arithmetic chains, parenthesized/grouped expressions) no longer
  crashes the whole process (previously a native stack overflow) — now raises a graceful `U1002`
  error instead, via a depth guard in the parser and a second, defense-in-depth guard in the
  post-parse AST pass.
- `Instr::MakeArray`/`MakeObject`/`BlockEnd`'s bytecode operands (and `CallBuiltin`'s argument
  count, and internal constant-pool bookkeeping) no longer silently produce wrong, truncated
  results for oversized literals/blocks/calls (e.g. array literals with more than 65,535
  elements) — such cases now fall back to the always-correct tree-walker instead.
- `ast_transform.rs`'s depth-guard error messages no longer imply `%`/`@`/`#` ancestor-operator
  usage (e.g. "...while resolving ancestor/path metadata") when the guard fires for any
  sufficiently-nested expression, including plain arithmetic.
- Release workflow now fails loudly on a fresh dispatch when the target version tag already
  exists at a different commit, instead of silently reusing the wrong commit (#53).

## [2.1.7] - 2026-07-08

**Superseded by [2.2.1](#221---2026-07-08) above, published the same day.** This version was
numbered following a simple patch-increment from `2.1.6` rather than this project's actual
versioning policy (track jsonata-js's major/minor). It is fully functional and was not yanked —
package registries don't allow deleting a published version — but `2.2.1` is the version that
should be used going forward.

### Added
- Guardrails: `timeout` (ms, error code `D1012`), `max_stack_depth` (error code `D1011`), and
  `max_sequence_length` (error code `D2015`) keyword arguments on `compile()` and every
  `evaluate*()` call, enforced consistently across all three execution engines (tree-walker,
  compiled-expression fast path, bytecode VM). All default to `None` (unlimited) — no behavior
  change unless configured. See [Guardrails](docs/api.md#guardrails). (jsonata-js 2.2.1 Phase 2, #56)
- Documented the guardrails API in `docs/api.md`, `docs/usage.md`, and `docs/error-handling.md`
  (previously shipped with accurate Python docstrings but no user-facing docs), and corrected
  `docs/migration-from-js.md`'s stale claim that Python had no built-in timeout support.

### Fixed
- A deeply-nested expression (arithmetic chains, parenthesized/grouped expressions) no longer
  crashes the whole process (previously a native stack overflow) — now raises a graceful `U1002`
  error instead, via a depth guard in the parser and a second, defense-in-depth guard in the
  post-parse AST pass.
- `Instr::MakeArray`/`MakeObject`/`BlockEnd`'s bytecode operands (and `CallBuiltin`'s argument
  count, and internal constant-pool bookkeeping) no longer silently produce wrong, truncated
  results for oversized literals/blocks/calls (e.g. array literals with more than 65,535
  elements) — such cases now fall back to the always-correct tree-walker instead.
- `ast_transform.rs`'s depth-guard error messages no longer imply `%`/`@`/`#` ancestor-operator
  usage (e.g. "...while resolving ancestor/path metadata") when the guard fires for any
  sufficiently-nested expression, including plain arithmetic.
- Release workflow now fails loudly on a fresh dispatch when the target version tag already
  exists at a different commit, instead of silently reusing the wrong commit (#53).

## [2.1.6] - 2026-07-07

### Added
- `%` (parent-reference) and `@`/`#` (focus/index binding) operators.
- jsonata-js 2.2.1 Phase 1: signature engine rewrite adding `+`/`-` arity support (#36).
- Versioned documentation via `mike`, fixing a gh-pages deploy race (#39).

### Fixed
- Reference test-suite coverage gaps: loader fix, datetime picture-strings,
  `formatInteger`/`parseInteger` (Phases 0-2), and array-constructor/`distinct` stragglers
  (Phase 5, #44).
- `release.yml` never actually built macOS wheels, due to a broken `actions/setup-python`
  invocation (#41).
- Several benchmark accuracy/fairness corrections (await `jsonata-js` calls properly, substantiate
  pre-converted-data speedup claims, use clean CI-sourced numbers, use jsonata-python's `Context`
  for fair repeated-eval timing) (#45, #46, #49, #50).

### Changed
- macOS temporarily dropped from the main release matrix pending a self-hosted runner fix (#45,
  #47) — later restored via a self-hosted Mac Mini runner.

## [2.1.5] - 2026-07-04

### Fixed
- Native stack overflow on deep recursion — replaced with a graceful, coded error (fixes #34).
- Lambda IDs are now generated from a monotonic counter instead of an AST pointer address, fixing
  a ~0.5%-frequency wrong-closure bug from id aliasing across recursive/repeated evaluation
  (fixes #35).
- Tree-walker missing-path/field access now correctly produces `Undefined` instead of `Null` in
  the ~20 sites that predated the `Null`/`Undefined` distinction (fixes #32).
- CI repairs following the default-branch rename to `main`; patched `RUSTSEC-2026-0097`.

### Changed
- Bumped `pyo3` to 0.29 and `rand` to 0.10.

## [2.1.4] - 2026-03-22

### Added
- `cargo publish` step in the release workflow.

### Fixed
- Release workflow is now idempotent for version-bump and tag-creation steps.
- Corrected an incorrect expected value in the `substring` test suite.

## [2.1.3] - 2026-03-22

> Versions 2.1.1 and 2.1.2 were bumped internally but never published as standalone tagged
> releases — their changes are folded into this entry, the next version actually released.

### Added
- Bytecode VM (`compiler.rs` + `vm.rs`, "Phase 4") restored and wired into the Python execution
  path, with Criterion benchmarks comparing it against the tree-walker.
- `pyo3` made an optional dependency; Rust crate renamed to `jsonata-core` and published to
  crates.io independently of the `jsonatapy` PyPI package.

### Fixed
- PyO3 0.28 compatibility (`PyObject` → `Py<PyAny>`).
- Clippy deprecations and `cargo-deny` license-check failures.
- Upgraded Pillow (dev/docs dependency) to 12.1.1 for a CVE fix.

### Changed
- Multiple benchmark documentation and accuracy corrections.

## [2.1.0] - 2026-02-08

### Added
- Initial public release: Rust-based JSONata implementation targeting jsonata-js v2.1.0 semantics.
- Full jsonata-js v2.1.0 reference test-suite compatibility.
- Python bindings (PyO3), published as `jsonatapy` on PyPI.
- Rust core, published as `jsonata-core` on crates.io.

---

## Reference Implementation Tracking

This project tracks the [jsonata-js](https://github.com/jsonata-js/jsonata) reference implementation.

**Current status:** Full test-suite compatibility with jsonata-js v2.1.0 (+3 commits). jsonata-js
v2.2.1 compatibility work is in progress — Phase 1 (signature engine, `+`/`-` arity support) and
Phase 2 (resource guardrails) are done; see
`docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md` for the authoritative, up-to-date
status of this effort.

### Version History
- Target tracking v2.1.0 - Project initialization (2025-01-17)
