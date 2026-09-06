## Suggested commands (Linux/WSL)

### Build
```bash
maturin develop --release   # build Rust ext + install into active venv (release, use for perf-sensitive testing)
maturin develop              # debug build, faster to compile, use for quick iteration
maturin build --release      # produce a wheel in dist/
```

### Test
```bash
cargo test                                              # Rust unit + tests/integration_test.rs
uv run pytest tests/python/test_reference_suite.py       # 1686 JSONata-js reference cases (primary compat gate; 1686 pass)
uv run pytest tests/python/ -v                           # full Python suite
uv run pytest tests/python/test_functions.py::test_string_functions -v   # single test
```
Reference suite depends on the `tests/jsonata-js` git submodule being initialized:
`git submodule update --init --recursive`.

### Lint / format (zero-warnings policy on Rust side)
```bash
cargo fmt
cargo clippy -- -D warnings
uv run ruff check python/ tests/
uv run ruff format python/ tests/
uv run mypy python/
```

### Benchmark
```bash
cargo bench                       # Criterion, uses `bench` feature
uv run python benchmarks/python/... # cross-language comparisons (see benchmarks/ for entrypoints)
```

### Linux/WSL-specific notes
- Standard GNU coreutils apply (no BSD `find`/`sed` quirks).
- Compiled extension `.so` files live directly in `python/jsonatapy/` (`_jsonatapy.cpython-3XX-x86_64-linux-gnu.so`)
  after `maturin develop` — one per Python minor version tested.
- Repo has leftover `.venv`, `.venv-wsl`, `.venv.bak` dirs; prefer `uv run` which resolves its
  own environment rather than assuming a particular venv is active.
