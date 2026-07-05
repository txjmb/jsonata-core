## Definition of done

Run all of these before considering a change complete; all are zero-tolerance (project charter
says 100% test compatibility and zero-clippy-warnings are non-negotiable):

```bash
cargo fmt --check          # or `cargo fmt` to auto-fix, then verify
cargo clippy -- -D warnings
cargo test
maturin develop --release  # rebuild extension so Python tests exercise the new Rust code
uv run pytest tests/python/test_reference_suite.py   # full 1258-case JS compat suite
uv run pytest tests/python/ -v                       # rest of the Python suite
uv run ruff check python/ tests/
uv run mypy python/
```

If the change touches evaluator/compiler semantics, confirm both execution paths agree:
the reference suite should pass under the default (VM-preferred) path AND when the VM/compiler
fast path is bypassed (forcing the tree-walker) — see `mem:architecture` for how prior migrations
verified this dual-path parity.

If the change is user-facing (new function, new signature, changed error), update
`CHANGELOG.md` per Keep a Changelog format (per CLAUDE.MD).
