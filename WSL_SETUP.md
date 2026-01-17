# Running JSONataPy in WSL

This guide shows how to build and test JSONataPy in Windows Subsystem for Linux (WSL).

## Quick Start in WSL

### 1. Access Your Project in WSL

```bash
# From WSL, navigate to your Windows project directory
cd /mnt/c/Users/mboha/source/repos/jsonatapy

# Verify you're in the right place
ls -la
```

### 2. Install Rust in WSL

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Follow prompts (choose default installation)
# Then source the environment
source $HOME/.cargo/env

# Verify installation
rustc --version
cargo --version
```

### 3. Install Python Dependencies

```bash
# Update package list
sudo apt update

# Install Python and pip if needed
sudo apt install python3 python3-pip python3-venv

# Install maturin
pip3 install maturin

# Install test dependencies
pip3 install pytest pytest-cov
```

### 4. Build and Test

```bash
# Check compilation
cargo check

# Run Rust tests
cargo test

# Build Python extension
maturin develop

# Run Python tests
pytest tests/python/ -v

# Try the examples
python3 examples/basic_usage.py
```

## Troubleshooting

### Line Ending Issues

WSL uses Linux line endings (LF) while Windows uses CRLF. Git should handle this automatically, but if you see issues:

```bash
# Configure git to handle line endings
git config core.autocrlf input

# If files have wrong line endings
dos2unix verify_parser.sh
```

### Permission Issues

If shell scripts aren't executable:

```bash
chmod +x verify_parser.sh
chmod +x verify_build.py
```

### Python Module Import Errors

Make sure you're using the same Python that maturin built for:

```bash
# Check which python
which python3

# If maturin used a different python, specify it
maturin develop --python python3
```

### Build Performance

First builds can be slow. Use release mode for production:

```bash
# Development build (faster compilation, slower runtime)
maturin develop

# Release build (slower compilation, much faster runtime)
maturin develop --release
```

## Expected Output

### cargo check ✅
```
    Checking jsonatapy v0.1.0 (/mnt/c/Users/mboha/source/repos/jsonatapy)
    Finished dev [unoptimized + debuginfo] target(s) in 5.23s
```

### cargo test ✅
```
running 85 tests
test ast::tests::test_binary_node ... ok
test parser::tests::test_tokenize_number ... ok
test parser::tests::test_parse_binary ... ok
test evaluator::tests::test_eval_literal ... ok
test functions::tests::test_uppercase ... ok
...

test result: ok. 85 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### maturin develop ✅
```
🔗 Found pyo3 bindings
🐍 Found CPython 3.10 at python3
📡 Using build options features from pyproject.toml
   Compiling jsonatapy v0.1.0 (/mnt/c/Users/mboha/source/repos/jsonatapy)
    Finished dev [unoptimized + debuginfo] target(s) in 45.67s
📦 Built wheel for CPython 3.10 to /tmp/.tmpXXXXXX/jsonatapy-0.1.0-cp310-cp310-linux_x86_64.whl
✅ Installed jsonatapy-0.1.0
```

### pytest tests/python/ -v ✅
```
============================== test session starts ===============================
collected 100+ items

tests/python/test_basic.py::TestCompile::test_compile_returns_expression PASSED
tests/python/test_integration.py::TestLiterals::test_number_literal PASSED
tests/python/test_integration.py::TestLiterals::test_string_literal PASSED
tests/python/test_integration.py::TestArithmetic::test_addition PASSED
tests/python/test_integration.py::TestPathTraversal::test_simple_property PASSED
tests/python/test_integration.py::TestStringFunctions::test_uppercase PASSED
...

============================== 100+ passed in 2.34s ===============================
```

### python3 examples/basic_usage.py ✅
```
============================================================
JSONataPy - Basic Usage Examples
============================================================

1. Simple Literals
----------------------------------------
jsonatapy.evaluate("42", {}) = 42
jsonatapy.evaluate('"Hello, World!"', {}) = Hello, World!

2. Arithmetic Operations
----------------------------------------
jsonatapy.evaluate("1 + 2 * 3", {}) = 7
jsonatapy.evaluate("(10 - 5) / 2", {}) = 2.5

...
```

## Tips for WSL Development

### 1. Use WSL Terminal

Open WSL directly in your project:
```bash
# In PowerShell/CMD, navigate to project
cd C:\Users\mboha\source\repos\jsonatapy

# Launch WSL in this directory
wsl
```

### 2. VS Code Integration

If using VS Code:
```bash
# Install Remote-WSL extension
# Then in WSL:
code .
```

### 3. File Watching

If you want to auto-rebuild on file changes:
```bash
# Install cargo-watch
cargo install cargo-watch

# Watch for changes and rebuild
cargo watch -x check -x test
```

### 4. Faster Builds

Use sccache to cache compilation:
```bash
# Install sccache
cargo install sccache

# Configure cargo to use it
export RUSTC_WRAPPER=sccache
```

## One-Command Build & Test

Create an alias for the full workflow:

```bash
# Add to ~/.bashrc
alias jsonatapy-test='cargo check && cargo test && maturin develop && pytest tests/python/ -v'

# Reload shell
source ~/.bashrc

# Run everything
jsonatapy-test
```

## Performance Notes

- WSL2 has near-native Linux performance
- File I/O between Windows and WSL can be slower
- Building inside /mnt/c is slower than building in WSL home (~)
- For best performance, consider cloning to WSL filesystem

### Optional: Clone to WSL Filesystem

```bash
# Clone directly in WSL (faster builds)
cd ~
git clone /mnt/c/Users/mboha/source/repos/jsonatapy jsonatapy-wsl
cd jsonatapy-wsl

# Build here (much faster)
cargo build --release
```

## Next Steps

Once everything builds and tests pass in WSL:

1. ✅ All tests passing → Ready for v0.1.0
2. 🔧 Add more functions as needed
3. 📦 Build wheels for distribution: `maturin build --release`
4. 🚀 Publish to PyPI when ready

## Getting Help

- Full build instructions: `BUILD_INSTRUCTIONS.md`
- Current status: `CURRENT_STATUS.md`
- Quick start: `README_FIRST.md`
- Project overview: `README.md`

---

**Ready to build?** Run:

```bash
cargo check && cargo test && maturin develop && pytest tests/python/ -v
```

Good luck! 🚀
