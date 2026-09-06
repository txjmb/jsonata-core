## Codebase conventions

- **Mirror the JS reference structurally**: module names and function names should track
  `jsonata-js`'s `src/*.js` (parser.js → parser.rs, functions.js → functions.rs, etc.) so
  upstream diffs are easy to port. Don't restructure Rust modules away from this mapping
  without strong reason.
- **Two execution paths, one semantics**: tree-walking `Evaluator` (evaluator.rs) is the
  reference/fallback implementation; `compiler.rs`/`vm.rs` is a bytecode fast path that falls
  back to the tree-walker (`EvalFallback`) for anything not compilable. New JSONata semantics
  should be correct in the tree-walker first (it's the oracle used for compatibility testing),
  then optionally fast-pathed into the compiler/VM — see `mem:architecture` for the split's
  concrete rules (short-circuit and/or, explicit-null flags, array flattening, etc.).
- **`JValue` (src/value.rs) is the value type**, not `serde_json::Value` — `Rc`-wrapped
  variants for O(1) clone (`String(Rc<str>)`, `Array(Rc<Vec<JValue>>)`, `Object(Rc<IndexMap<...>>)`).
  Lambdas/builtins/regexes are first-class `JValue` variants, not tagged JSON objects.
  `serde_json::Value` is only used at I/O boundaries (JSON string parsing, test fixtures).
  See `mem:architecture` for the full variant list and common gotchas (`Rc<str>` vs
  `Borrow<String>`, `Rc<Vec>` iteration/mutation).
- **Null vs Undefined are semantically distinct** (JSONata quirk carried over from JS) — do not
  conflate them when writing new "field not found" or "value missing" logic; see
  `mem:architecture` for the correct reference pattern and history of a subtle prior bug here.
- **Rust style**: 2021 edition, `cargo fmt` + `cargo clippy -- -D warnings` (zero warnings,
  non-negotiable), `Result<T, E>` for fallible ops, rustdoc `///` on public APIs, comments should
  explain *deviations from JS behavior*, not restate what code does.
- **Python style**: PEP 8 via ruff, type hints required on public APIs (mypy strict), ruff
  double-quote/100-col formatting — no black despite CLAUDE.MD mentioning it (ruff format is
  what's actually configured in pyproject.toml).
- **Test-driven against the JS suite**: the JSONata-js reference suite
  (`tests/python/test_reference_suite.py` over the `tests/jsonata-js` submodule) is the
  compatibility oracle. Any evaluator/compiler change should be checked against the FULL
  1686-case suite, not just a hand-picked subset — subtle regressions have historically hidden
  in low-frequency edge cases (e.g. lambda id aliasing at ~0.5% call frequency).
