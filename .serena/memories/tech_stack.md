## Tech stack

- **Rust**: edition 2021, `rust-version = "1.70"` (stable channel only, no nightly features per
  CLAUDE.MD). Crate name `jsonata-core` (lib name `jsonata_core`), cdylib+rlib.
- **Key Rust deps**: `pyo3` 0.29 (optional, feature `python`), `serde`/`serde_json` (preserve_order),
  `simd-json` 0.17 (optional, feature `simd`, default-on — SIMD JSON parsing fast path),
  `indexmap` (ordered maps for JSONata objects), `regex`, `chrono`, `thiserror`, `stacker`
  (stack growth for deep recursion), `rand`.
- **Cargo features**: `default = ["simd"]`, `simd` (dep:simd-json), `python` (dep:pyo3,
  enables the PyO3 boundary), `bench` (exposes `_bench` facade for Criterion).
- **Python**: >=3.10, published wheels for 3.10–3.14. Build backend: **maturin** (module name
  `jsonatapy._jsonatapy`, python-source = `python`).
- **Package manager**: **uv** (uv.lock present) — prefer `uv run <cmd>` over bare `pip`/`python`.
- **Python dev tooling**: ruff (lint+format, line-length 100, double quotes, target py310),
  mypy (strict mode), pytest (+pytest-cov, pytest-xdist for parallel runs).
- **Benchmarking**: Criterion (Rust, `cargo bench`, `evaluator_bench`), plus Python-side
  benchmarks comparing against `jsonata` (JS) and `jsonata-rs`.
- **Docs**: mkdocs + mkdocs-material (`docs/`, `mkdocs.yml`), not Sphinx despite CLAUDE.MD
  mentioning Sphinx — mkdocs is what's actually wired up in pyproject.toml `[project.optional-dependencies.docs]`.
