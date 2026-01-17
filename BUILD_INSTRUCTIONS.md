# Build Instructions for JSONataPy

This document provides step-by-step instructions for building and testing jsonatapy.

## Prerequisites

### 1. Install Rust

**Windows:**
```bash
# Using winget
winget install Rustlang.Rustup

# Or download from: https://rustup.rs/
```

**Linux/macOS:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Verify installation:
```bash
rustc --version
cargo --version
```

### 2. Install Python (3.8 or later)

**Windows:**
```bash
winget install Python.Python.3.12
```

**Linux:**
```bash
sudo apt install python3 python3-pip python3-venv
```

**macOS:**
```bash
brew install python@3.12
```

### 3. Install maturin

```bash
pip install maturin
```

## Building the Extension

### Development Build

For development, use `maturin develop` which builds and installs the extension in your current Python environment:

```bash
# Build and install in development mode
maturin develop

# Build with optimizations (slower build, faster runtime)
maturin develop --release
```

### Production Build

For production wheels:

```bash
# Build wheel for current platform
maturin build --release

# Build wheels for all platforms (requires docker)
maturin build --release --target all
```

Wheels will be created in the `target/wheels/` directory.

## Running Tests

### Rust Tests

Run Rust unit tests:

```bash
# Run all Rust tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_parser

# Run tests for a specific module
cargo test parser::
```

### Python Tests

After building with `maturin develop`, run Python tests:

```bash
# Install test dependencies
pip install pytest pytest-cov

# Run all Python tests
pytest tests/python/ -v

# Run specific test file
pytest tests/python/test_integration.py -v

# Run with coverage
pytest tests/python/ --cov=jsonatapy --cov-report=html
```

### Code Quality Checks

```bash
# Rust formatting
cargo fmt --check

# Rust linting
cargo clippy

# Python formatting
black tests/python/
black python/jsonatapy/

# Python linting
ruff check python/ tests/

# Type checking
mypy python/jsonatapy/
```

## Running Examples

After building with `maturin develop`:

```bash
python examples/basic_usage.py
```

## Troubleshooting

### Build Fails: "cargo: command not found"

Make sure Rust is installed and in your PATH:

```bash
# Restart your shell after installing Rust
source $HOME/.cargo/env  # Linux/macOS
# Or restart terminal on Windows
```

### Build Fails: Compilation Errors

1. Check that all Rust source files compile:
   ```bash
   cargo check
   ```

2. Look for specific error messages and fix issues in the Rust code.

### Import Error: "cannot import name 'compile'"

Make sure you've built the extension:

```bash
maturin develop
```

Then verify it's installed:

```bash
python -c "import jsonatapy; print(jsonatapy.__version__)"
```

### Tests Fail: Extension Not Available

The integration tests require the compiled extension. Build it first:

```bash
maturin develop
pytest tests/python/test_integration.py -v
```

### Performance Issues

For production use, always build with `--release`:

```bash
maturin develop --release
```

Release builds are significantly faster but take longer to compile.

## Development Workflow

### Recommended Workflow

1. **Make changes to Rust code** (src/*.rs)

2. **Check for compilation errors:**
   ```bash
   cargo check
   ```

3. **Run Rust tests:**
   ```bash
   cargo test
   ```

4. **Rebuild Python extension:**
   ```bash
   maturin develop
   ```

5. **Run Python tests:**
   ```bash
   pytest tests/python/ -v
   ```

6. **Run examples:**
   ```bash
   python examples/basic_usage.py
   ```

### Rapid Iteration

For faster iteration during development:

```bash
# 1. Keep cargo check running in watch mode (requires cargo-watch)
cargo install cargo-watch
cargo watch -x check

# 2. In another terminal, rebuild when ready
maturin develop && pytest tests/python/ -v
```

## CI/CD

The project includes GitHub Actions workflows:

- **test.yml**: Runs on every push/PR
  - Rust tests (format, clippy, unit tests)
  - Python tests (3.8-3.12 on Linux/Windows/macOS)
  - Code quality checks

- **release.yml**: Runs on version tags
  - Builds wheels for all platforms
  - Publishes to PyPI

## Platform-Specific Notes

### Windows

- Use PowerShell or Windows Terminal
- You may need Visual Studio Build Tools: https://visualstudio.microsoft.com/downloads/
- Path separators use backslashes: `tests\python\test_basic.py`

### Linux

- Install build essentials:
  ```bash
  sudo apt install build-essential
  ```

### macOS

- Install Xcode Command Line Tools:
  ```bash
  xcode-select --install
  ```

## Next Steps

After building successfully:

1. Run the examples: `python examples/basic_usage.py`
2. Read the API documentation in `README.md`
3. Check the implementation status in `IMPLEMENTATION_STATUS.md`
4. Start contributing! See `GETTING_STARTED.md`

## Additional Resources

- **Maturin Documentation**: https://www.maturin.rs/
- **PyO3 Guide**: https://pyo3.rs/
- **Rust Book**: https://doc.rust-lang.org/book/
- **JSONata Documentation**: https://docs.jsonata.org/

## Getting Help

If you encounter issues:

1. Check this troubleshooting guide
2. Review error messages carefully
3. Check `IMPLEMENTATION_STATUS.md` for known limitations
4. Open an issue on GitHub with:
   - Your operating system and version
   - Rust version (`rustc --version`)
   - Python version (`python --version`)
   - Complete error message
   - Steps to reproduce
