# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Deprecated

### Removed

### Fixed

### Security

## [2.1.7] - 2026-07-08

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
