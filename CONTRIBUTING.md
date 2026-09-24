# Contributing to jsonata-core

Thanks for your interest in contributing! This repository publishes four things
from one Rust implementation of [JSONata](https://jsonata.org/):

- **jsonata-core** – the Rust crate ([crates.io](https://crates.io/crates/jsonata-core))
- **jsonatapy** – the Python package, built with PyO3 and maturin ([PyPI](https://pypi.org/project/jsonatapy/))
- **jsonata** – the command-line tool
- the **C API** in `bindings/c`

Bug reports, conformance fixes, performance work, documentation and tests are
all welcome.

By participating in this project you agree to abide by our
[Code of Conduct](https://github.com/txjmb/jsonata-core/blob/main/CODE_OF_CONDUCT.md).

## Ways to contribute

- **Report a bug** – open a
  [bug report](https://github.com/txjmb/jsonata-core/issues/new/choose). Include
  the JSONata expression, the input data, what you got and what you expected.
- **Report a conformance difference** – if this library gives a different
  result from the jsonata-js reference, use the *compatibility* issue template
  and include the result from the [JSONata playground](https://try.jsonata.org/).
- **Ask a question or propose an idea** – use
  [GitHub Discussions](https://github.com/txjmb/jsonata-core/discussions).
  For larger changes, please open an issue or discussion before starting work
  so we can agree on the approach.
- **Report a security vulnerability** – do **not** open a public issue. Follow
  the [security policy](https://github.com/txjmb/jsonata-core/blob/main/SECURITY.md).

## Guiding principles

- **The reference implementation is the specification.** Behavior should match
  [jsonata-js](https://github.com/jsonata-js/jsonata). If you believe the
  reference is wrong, raise it upstream rather than diverging here.
- **The Rust code mirrors the JavaScript structure** where it can (parser,
  evaluator, functions, datetime, signature), so upstream changes can be ported
  quickly. See the
  [architecture notes](https://github.com/txjmb/jsonata-core/blob/main/docs/development/architecture.md).
- **Performance matters.** This project exists to be fast. Changes on hot paths
  should come with benchmark numbers.

## Development setup

You need:

- Rust (stable; minimum supported version is set by `rust-version` in `Cargo.toml`)
- Python 3.10+
- [uv](https://docs.astral.sh/uv/) (recommended) or pip
- Git

```bash
# Fork on GitHub, then:
git clone https://github.com/<your-username>/jsonata-core.git
cd jsonata-core

# The reference test suite is a git submodule
git submodule update --init --recursive

# Python environment and development build of the extension
uv venv
uv pip install maturin pytest pytest-xdist ruff mypy
uv run maturin develop --release
```

`scripts/setup-uv.sh` automates most of this. More detail is in the
[building](https://github.com/txjmb/jsonata-core/blob/main/docs/development/building.md)
and [testing](https://github.com/txjmb/jsonata-core/blob/main/docs/development/testing.md)
guides.

## Making a change

1. Create a branch from `main`:

   ```bash
   git checkout -b fix/short-description
   ```

2. Make your change, with tests. Bug fixes should include a test that fails
   without the fix.

3. Run the checks below before pushing. They are what CI runs.

4. Add an entry under `## [Unreleased]` in
   [`CHANGELOG.md`](https://github.com/txjmb/jsonata-core/blob/main/CHANGELOG.md)
   for any user-visible change. The project follows
   [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). If your change
   alters the result of an existing expression, say so clearly.

5. Push to your fork and open a pull request against `main`. Fill in the pull
   request template, and link any related issue (`Fixes #123`).

## Checks

### Rust

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --doc
```

If you change `src/capi.rs`, also run `cargo test --lib --features capi capi::`.

### Python

```bash
uv run maturin develop --release   # rebuild after any Rust change

# Unit and integration tests
uv run pytest tests/python/ --ignore=tests/python/test_reference_suite.py

# The jsonata-js reference suite (must stay at 100%)
uv run pytest tests/python/test_reference_suite.py

# The same suite with the bytecode VM disabled
JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py

# Formatting, linting and type checking
ruff format --check python/ tests/ benchmarks/
ruff check python/ tests/ benchmarks/
mypy python/ --strict
```

### Benchmarks

For changes to the parser, evaluator or other hot paths, run the benchmarks
before and after your change and include the results in the pull request. See
[`benchmarks/README.md`](https://github.com/txjmb/jsonata-core/blob/main/benchmarks/README.md).

## Code style

**Rust**

- `cargo fmt` formatting and zero `clippy` warnings.
- Rustdoc comments (`///`) on public APIs.
- Return `Result` for fallible operations; do not panic on user input.
- Keep error codes and messages consistent with the jsonata-js reference.

**Python**

- Formatted and linted with `ruff`; type-checked with `mypy --strict`.
- Type hints and docstrings on public APIs.

**General**

- Match the style of the surrounding code.
- Keep pull requests focused. Unrelated refactors belong in a separate PR.

## Commit messages

CI checks that every commit in a pull request follows
[Conventional Commits](https://www.conventionalcommits.org/):

```
type(optional-scope): description
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
`chore`, `ci`, `build`, `revert`.

Examples:

```
fix(parser): Handle escaped backticks in field names
feat(cli): Add an option to read the expression from a file
docs: Clarify timezone handling in $toMillis
```

## Pull request review

- CI must be green: tests on all platforms, code quality and security checks.
- A maintainer reviews every pull request. We may ask for changes, tests or
  benchmark numbers.
- Once approved, a maintainer merges the pull request. Releases are cut by the
  maintainers; see the
  [release process](https://github.com/txjmb/jsonata-core/blob/main/docs/development/releasing.md).

## AI-assisted contributions

Much of this project was built with AI coding assistants under human review,
and AI-assisted contributions are welcome. You are responsible for what you
submit: read and understand every line, make sure it is tested, and be ready to
explain it in review. `CLAUDE.MD` in the repository root holds project guidance
for AI assistants.

## License

By contributing, you agree that your contributions will be licensed under the
project's [MIT License](https://github.com/txjmb/jsonata-core/blob/main/LICENSE).

## Recognition

Contributors are listed in
[`CONTRIBUTORS.md`](https://github.com/txjmb/jsonata-core/blob/main/CONTRIBUTORS.md).
Feel free to add yourself in your pull request.
