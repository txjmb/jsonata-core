## jsonatapy: source map

Rust-based Python extension implementing JSONata (JSON query/transform language), targeting
near-100% compatibility with the reference `jsonata-js` v2.2.2 test suite (1686/1686 passing).
Two published artifacts from one codebase: `jsonata-core` (crates.io, pure Rust) and
`jsonatapy` (PyPI, PyO3 wrapper). See root `CLAUDE.MD` for full project charter/design goals —
this memory only covers what CLAUDE.MD omits or what changes faster than that doc.

### Layout
- `src/` — Rust crate (`jsonata-core`). Core files: `ast.rs`, `parser.rs` (+ `src/parser/README.md`),
  `evaluator.rs` (~8700 lines, tree-walking evaluator — the reference/fallback path),
  `compiler.rs` + `vm.rs` (bytecode compiler + VM — the fast path, falls back to the
  tree-walker for anything not compilable), `value.rs` (`JValue`), `functions.rs`, `datetime.rs`,
  `signature.rs`, `lib.rs` (PyO3 Python boundary).
- `python/jsonatapy/` — thin Python package; `__init__.py` re-exports the compiled `_jsonatapy`
  extension module and adds optional orjson-backed `json_dumps`/`json_loads`.
- `tests/jsonata-js/` — git submodule, the upstream JS reference suite (source of the 1686 cases).
- `tests/python/test_reference_suite.py` — runs the submodule's cases against the compiled extension.
- `tests/integration_test.rs` — Rust-side integration tests (separate from inline `#[cfg(test)]`).
- `benchmarks/` — cross-language perf comparisons (Python/JS/Rust) driving the "faster than JS" claim.
- `scripts/setup-uv.sh` — canonical from-scratch dev bootstrap (installs uv+Rust, builds, runs both test suites).

### Further memories
- `mem:tech_stack` — toolchain, package managers, version pins.
- `mem:suggested_commands` — day-to-day dev/build/test commands.
- `mem:conventions` — code style and architectural conventions specific to this codebase.
- `mem:task_completion` — what to run before considering a change done.
- `mem:architecture` — JValue/evaluator/compiler pipeline internals, non-obvious invariants.

### Non-obvious invariants
- Two independent execution paths must stay behaviorally identical: the tree-walking
  `Evaluator` (evaluator.rs) and the bytecode `Vm` (vm.rs/compiler.rs). The VM is preferred when
  compilable; anything it can't compile falls back to the tree-walker. Changes to evaluation
  semantics generally need to be made in both, or the fallback path silently diverges.
- Repo root has multiple stale/duplicate Python venvs (`.venv`, `.venv-wsl`, `.venv.bak`) — don't
  assume one is canonical without checking; `uv` manages its own env from `pyproject.toml`/`uv.lock`.
- Serena's language auto-detection for this project defaults to `typescript` (misled by the
  `tests/jsonata-js` submodule / `node_modules`) unless `.serena/project.yml`'s `languages:` list
  is corrected to `[rust, python]` — check this after any Serena project reset.
