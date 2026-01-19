# Building from Source

Complete guide for building jsonatapy from source code.

## Table of Contents

- [Prerequisites](#prerequisites)
- [Quick Build](#quick-build)
- [Development Setup](#development-setup)
- [Building](#building)
- [Testing](#testing)
- [Development Workflow](#development-workflow)
- [CI/CD](#cicd)
- [Troubleshooting](#troubleshooting)

## Prerequisites

### Required Tools

1. **Rust** (latest stable, 1.70+)
   ```bash
   # Install Rust
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

   # Verify installation
   rustc --version
   cargo --version
   ```

2. **Python** (3.8 or later) with development headers
   ```bash
   # Ubuntu/Debian
   sudo apt-get install python3-dev python3-pip

   # Fedora/RHEL
   sudo dnf install python3-devel python3-pip

   # macOS
   brew install python

   # Windows
   # Download from python.org (includes development headers)
   ```

3. **maturin** (Rust-Python build tool)
   ```bash
   pip install maturin
   ```

### Optional Tools

- **cargo-watch** - Auto-rebuild on changes
  ```bash
  cargo install cargo-watch
  ```

- **pytest** - Python testing
  ```bash
  pip install pytest pytest-cov
  ```

- **ruff** - Python linting
  ```bash
  pip install ruff
  ```

- **black** - Python formatting
  ```bash
  pip install black
  ```

## Quick Build

```bash
# Clone repository
git clone https://github.com/yourusername/jsonatapy.git
cd jsonatapy

# Build and install in development mode
maturin develop --release

# Verify installation
python -c "import jsonatapy; print(jsonatapy.__version__)"
```

## Development Setup

### 1. Clone Repository

```bash
git clone https://github.com/yourusername/jsonatapy.git
cd jsonatapy
```

### 2. Create Virtual Environment

```bash
# Using venv
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# or
.venv\Scripts\activate  # Windows

# Using conda
conda create -n jsonatapy python=3.11
conda activate jsonatapy
```

### 3. Install Development Dependencies

```bash
# Install maturin
pip install maturin

# Install Python dev tools
pip install pytest pytest-cov black ruff mypy

# Install optional tools
cargo install cargo-watch
```

### 4. Build in Development Mode

```bash
# Build and install (debug mode)
maturin develop

# Or with optimizations (slower build, faster runtime)
maturin develop --release
```

## Building

### Development Build

Fast build for development (debug mode):

```bash
maturin develop
```

### Release Build

Optimized build with full optimizations:

```bash
# Build and install locally
maturin develop --release

# Or build wheel only
maturin build --release
```

Built wheels appear in `target/wheels/`.

### Building for Multiple Python Versions

```bash
# Build for specific Python versions
maturin build --release --interpreter python3.8 python3.9 python3.10 python3.11 python3.12

# Or using uv (faster)
pip install uv
maturin build --release --uv
```

### Cross-Platform Builds

#### Linux (using Docker)

```bash
# Build manylinux wheels
docker run --rm -v $(pwd):/io ghcr.io/pyo3/maturin build --release

# Wheels in target/wheels/ work on most Linux distributions
```

#### macOS Universal Binaries

```bash
# Build for both x86_64 and arm64
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin

maturin build --release --target universal2-apple-darwin
```

#### Windows

```bash
# Build in PowerShell or cmd
maturin build --release
```

## Testing

### Run Rust Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_evaluate_path

# With output
cargo test -- --nocapture

# Run tests and show timing
cargo test -- --test-threads=1
```

### Run Python Tests

```bash
# All tests
pytest tests/python/ -v

# Specific test file
pytest tests/python/test_lambda.py -v

# With coverage
pytest tests/python/ --cov=jsonatapy --cov-report=html

# Run specific test
pytest tests/python/test_lambda.py::test_map_with_lambda -v
```

### Run All Tests

```bash
# Rust + Python
cargo test && pytest tests/python/ -v
```

### Benchmarking

```bash
# Run benchmarks (if configured)
cargo bench

# Run Python performance tests
python tests/python/test_perf.py
```

## Development Workflow

### Typical Development Cycle

1. **Make changes** to Rust source (`src/*.rs`)

2. **Rebuild and test**:
   ```bash
   maturin develop && pytest tests/python/ -v
   ```

3. **Run specific tests**:
   ```bash
   pytest tests/python/test_lambda.py -v
   ```

4. **Check code quality**:
   ```bash
   cargo clippy
   cargo fmt --check
   ```

### Auto-Rebuild on Changes

```bash
# Watch Rust files and rebuild on change
cargo watch -x 'build --release' -s 'maturin develop --release'

# In another terminal, run tests
pytest tests/python/ -v --watch
```

### Debugging

#### Rust Debugging

```bash
# Build with debug symbols
maturin develop

# Use rust-gdb or rust-lldb
rust-gdb --args python -c "import jsonatapy; jsonatapy.evaluate('expression', {})"
```

#### Python Debugging

```python
import jsonatapy
import pdb

# Set breakpoint
pdb.set_trace()
result = jsonatapy.evaluate("expression", data)
```

### Code Formatting

```bash
# Rust
cargo fmt

# Python
black python/ tests/
```

### Linting

```bash
# Rust (strict)
cargo clippy -- -D warnings

# Python
ruff check python/ tests/
```

## Project Structure

```
jsonatapy/
├── src/                    # Rust source code
│   ├── lib.rs             # Python bindings (PyO3)
│   ├── parser.rs          # JSONata parser
│   ├── evaluator.rs       # Expression evaluator
│   ├── functions.rs       # Built-in functions
│   ├── context.rs         # Evaluation context
│   └── ast.rs             # Abstract Syntax Tree
├── python/                # Python package
│   └── jsonatapy/
│       ├── __init__.py    # Python API
│       └── py.typed       # Type hints marker
├── tests/                 # Test suite
│   └── python/            # Python integration tests
├── docs/                  # Documentation
├── Cargo.toml             # Rust dependencies
├── pyproject.toml         # Python package config
└── README.md
```

## CI/CD

### GitHub Actions Workflows

The project uses GitHub Actions for CI/CD:

#### `.github/workflows/test.yml`
- Runs on every push and PR
- Tests on multiple platforms (Windows, Linux, macOS)
- Tests multiple Python versions (3.8-3.12)
- Runs Rust tests with `cargo test`
- Runs Python tests with `pytest`

#### `.github/workflows/release.yml`
- Triggered on version tags (`v*`)
- Builds wheels for all platforms
- Publishes to PyPI

### Local CI Testing

```bash
# Install act (GitHub Actions local runner)
# https://github.com/nektos/act

# Run workflows locally
act push
act pull_request
```

## Building Documentation

### API Documentation

```bash
# Rust docs
cargo doc --open

# Python docs (if using Sphinx)
cd docs/
pip install sphinx sphinx-rtd-theme
make html
```

## Performance Profiling

### Profiling Rust Code

```bash
# Install flamegraph
cargo install flamegraph

# Profile
cargo flamegraph --bin jsonatapy

# Or use perf (Linux)
cargo build --release
perf record --call-graph=dwarf target/release/jsonatapy
perf report
```

### Profiling Python Code

```python
import cProfile
import jsonatapy

data = {"items": [...]}  # Large dataset
expr = jsonatapy.compile("items[price > 100]")

cProfile.run('expr.evaluate(data)', sort='cumtime')
```

## Troubleshooting

### Build Errors

#### "failed to run custom build command"

**Cause:** Missing Rust compiler or Python dev headers

**Solution:**
```bash
# Install/update Rust
rustup update

# Install Python dev headers
# Ubuntu: sudo apt-get install python3-dev
# macOS: brew install python
# Windows: Reinstall Python with dev headers
```

#### "PyO3 version mismatch"

**Cause:** Incompatible PyO3 version

**Solution:**
```bash
# Update dependencies
cargo update -p pyo3

# Clean and rebuild
cargo clean
maturin develop --release
```

#### "linker error" (Windows)

**Cause:** Missing Visual C++ build tools

**Solution:**
Install Visual Studio 2019+ with C++ build tools, or:
- Download: https://aka.ms/vs/17/release/vs_BuildTools.exe
- Install "Desktop development with C++"

### Test Failures

#### Python tests fail after Rust changes

**Solution:**
```bash
# Rebuild extension
maturin develop --release

# Clear Python cache
find . -type d -name __pycache__ -exec rm -rf {} +

# Re-run tests
pytest tests/python/ -v
```

#### "ModuleNotFoundError: No module named '_jsonatapy'"

**Cause:** Extension not built or wrong Python environment

**Solution:**
```bash
# Verify correct environment is active
which python

# Rebuild
maturin develop --release

# Verify installation
pip list | grep jsonatapy
```

### Performance Issues

#### Slow build times

**Solutions:**
- Use `maturin develop` (debug) during development
- Use `cargo build` instead of `--release` for faster iteration
- Install `sccache` for caching:
  ```bash
  cargo install sccache
  export RUSTC_WRAPPER=sccache
  ```

#### Slow tests

**Solutions:**
- Run specific tests: `pytest tests/python/test_file.py`
- Use pytest markers: `pytest -m "not slow"`
- Run in parallel: `pytest -n auto`

## Advanced Topics

### Custom Rust Features

Edit `Cargo.toml` to enable features:

```toml
[features]
default = []
experimental = []  # Enable experimental features
```

Build with features:
```bash
cargo build --features experimental
```

### Extending the Parser

1. Edit `src/parser.rs`
2. Add grammar rules
3. Update AST in `src/ast.rs`
4. Rebuild: `maturin develop --release`
5. Test: `pytest tests/python/ -v`

### Adding Built-in Functions

1. Edit `src/functions.rs`
2. Add function implementation
3. Register in `register_builtin_functions()`
4. Add tests in `tests/python/test_functions.py`
5. Rebuild and test

## Contributing

See [CLAUDE.md](../CLAUDE.md) for:
- Code style guidelines
- Architecture overview
- Synchronization with JavaScript reference
- Pull request process

## Next Steps

- [Review installation options](installation.md)
- [Learn the API](api.md)
- [Explore usage patterns](usage.md)
- [Optimize performance](performance.md)

---

**Need Help?**
- Check [Troubleshooting](#troubleshooting) section
- Review [CLAUDE.md](../CLAUDE.md) for architecture
- File an issue on GitHub
