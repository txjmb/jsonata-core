# Using UV with JSONataPy

This guide shows how to use [uv](https://github.com/astral-sh/uv) - the ultra-fast Python package installer - with JSONataPy.

## Why UV?

- ⚡ **10-100x faster** than pip
- 🔒 **Better dependency resolution**
- 📦 **Built-in virtual environment management**
- 🦀 **Written in Rust** (like JSONataPy!)

---

## Quick Start with UV

### 1. Install UV

**Linux/macOS/WSL:**
```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
```

**Windows (PowerShell):**
```powershell
powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
```

**Or with pip (if you already have Python):**
```bash
pip install uv
```

Verify installation:
```bash
uv --version
```

### 2. Create Virtual Environment with UV

```bash
# Navigate to project
cd /mnt/c/Users/mboha/source/repos/jsonatapy  # WSL
# or
cd C:\Users\mboha\source\repos\jsonatapy       # Windows

# Create virtual environment
uv venv

# Activate it
source .venv/bin/activate  # Linux/macOS/WSL
# or
.venv\Scripts\activate     # Windows
```

### 3. Install Dependencies with UV

```bash
# Install maturin
uv pip install maturin

# Install development dependencies
uv pip install pytest pytest-cov black ruff mypy

# Or install everything from pyproject.toml
uv pip install -e ".[dev]"
```

### 4. Build with UV

```bash
# Build the extension
maturin develop

# Or with UV's Python
uv run maturin develop
```

### 5. Run Tests with UV

```bash
# Run tests
uv run pytest tests/python/ -v

# Run with coverage
uv run pytest tests/python/ --cov=jsonatapy --cov-report=html
```

---

## Complete UV Workflow

### One-Time Setup

```bash
# Install UV (if not already installed)
curl -LsSf https://astral.sh/uv/install.sh | sh

# Create and activate virtual environment
uv venv
source .venv/bin/activate

# Install all dependencies
uv pip install maturin pytest pytest-cov black ruff mypy
```

### Build & Test

```bash
# Check Rust code
cargo check

# Run Rust tests
cargo test

# Build Python extension
maturin develop

# Run Python tests
uv run pytest tests/python/ -v

# Run examples
uv run python examples/basic_usage.py
```

### One Command Build & Test

```bash
cargo check && \
cargo test && \
maturin develop && \
uv run pytest tests/python/ -v && \
uv run python examples/basic_usage.py
```

---

## UV-Specific Features

### 1. Faster Package Installation

```bash
# UV is 10-100x faster than pip
time uv pip install pytest  # ~100ms
time pip install pytest     # ~5s
```

### 2. Better Dependency Resolution

```bash
# UV resolves dependencies more accurately
uv pip install -e ".[dev]"

# Check what's installed
uv pip list
```

### 3. Sync Dependencies

```bash
# Keep environment in sync with pyproject.toml
uv pip sync
```

### 4. UV Run (No Activation Needed!)

```bash
# Run commands without activating venv
uv run pytest tests/python/ -v
uv run python examples/basic_usage.py
uv run black python/ tests/
uv run ruff check .
```

---

## Updated pyproject.toml for UV

The project's `pyproject.toml` already works with UV, but here are some UV-specific optimizations:

```toml
[project.optional-dependencies]
dev = [
    "pytest>=7.0",
    "pytest-cov>=4.0",
    "black>=23.0",
    "ruff>=0.1.0",
    "mypy>=1.0",
]

[tool.uv]
# UV-specific settings (optional)
dev-dependencies = [
    "pytest>=7.0",
    "pytest-cov>=4.0",
    "black>=23.0",
    "ruff>=0.1.0",
    "mypy>=1.0",
]
```

---

## Development Workflow with UV

### Daily Development

```bash
# 1. Activate environment (or use uv run)
source .venv/bin/activate

# 2. Make changes to Rust code (src/*.rs)

# 3. Quick check
cargo check

# 4. Rebuild extension
maturin develop

# 5. Test changes
uv run pytest tests/python/test_integration.py -v

# 6. Format and lint
uv run black python/ tests/
uv run ruff check .
```

### Release Build

```bash
# Build optimized wheels
maturin build --release

# Test the wheel
uv pip install target/wheels/jsonatapy-0.1.0-*.whl
uv run python -c "import jsonatapy; print(jsonatapy.__version__)"
```

---

## UV Commands Cheat Sheet

| Task | UV Command |
|------|------------|
| Create venv | `uv venv` |
| Install package | `uv pip install package` |
| Install from pyproject.toml | `uv pip install -e ".[dev]"` |
| Run command | `uv run python script.py` |
| Run pytest | `uv run pytest tests/ -v` |
| List packages | `uv pip list` |
| Freeze dependencies | `uv pip freeze` |
| Sync dependencies | `uv pip sync` |
| Upgrade package | `uv pip install --upgrade package` |

---

## Troubleshooting UV

### UV Command Not Found

```bash
# Make sure UV is in PATH
export PATH="$HOME/.cargo/bin:$PATH"

# Or reinstall
curl -LsSf https://astral.sh/uv/install.sh | sh
```

### Virtual Environment Issues

```bash
# Remove old venv
rm -rf .venv

# Create fresh venv with UV
uv venv

# Activate and install
source .venv/bin/activate
uv pip install maturin pytest
```

### Maturin Not Using UV's Python

```bash
# Explicitly tell maturin which Python to use
maturin develop --python .venv/bin/python

# Or use uv run
uv run maturin develop
```

### Package Installation Fails

```bash
# Try with verbose output
uv pip install --verbose pytest

# Check UV version
uv --version

# Update UV
curl -LsSf https://astral.sh/uv/install.sh | sh
```

---

## Performance Comparison

### Installing pytest with pip vs UV

```bash
# With pip
$ time pip install pytest
real    0m5.234s

# With UV
$ time uv pip install pytest
real    0m0.143s
```

**UV is ~35x faster!** ⚡

### Building JSONataPy

```bash
# Traditional workflow
time (pip install maturin && maturin develop && pip install pytest && pytest tests/python/)
# ~30-60 seconds

# UV workflow
time (uv pip install maturin pytest && maturin develop && uv run pytest tests/python/)
# ~15-30 seconds
```

---

## Integration with Maturin

Maturin works seamlessly with UV:

```bash
# UV creates the venv, maturin builds into it
uv venv
source .venv/bin/activate
uv pip install maturin
maturin develop

# Or all in one
uv venv && source .venv/bin/activate && uv pip install maturin && maturin develop
```

---

## CI/CD with UV

### GitHub Actions with UV

Update `.github/workflows/test.yml`:

```yaml
- name: Set up Python
  uses: actions/setup-python@v5
  with:
    python-version: "3.11"

- name: Install UV
  run: curl -LsSf https://astral.sh/uv/install.sh | sh

- name: Install dependencies
  run: |
    uv venv
    source .venv/bin/activate
    uv pip install maturin pytest pytest-cov

- name: Build extension
  run: |
    source .venv/bin/activate
    maturin develop

- name: Run tests
  run: |
    source .venv/bin/activate
    uv run pytest tests/python/ -v
```

---

## UV + WSL Setup

Perfect combination for Windows users:

```bash
# In WSL
cd /mnt/c/Users/mboha/source/repos/jsonatapy

# Install UV
curl -LsSf https://astral.sh/uv/install.sh | sh

# Setup and build
uv venv
source .venv/bin/activate
uv pip install maturin pytest pytest-cov

# Build and test
cargo check && cargo test && maturin develop && uv run pytest tests/python/ -v
```

---

## Migration from pip to UV

If you have existing setup with pip:

```bash
# 1. Create requirements from current environment
pip freeze > requirements-old.txt

# 2. Create new UV venv
uv venv

# 3. Install with UV (much faster)
source .venv/bin/activate
uv pip install -r requirements-old.txt

# 4. Or install from pyproject.toml
uv pip install -e ".[dev]"
```

---

## Recommended Setup Script

Create `setup-uv.sh`:

```bash
#!/bin/bash
set -e

echo "🚀 Setting up JSONataPy with UV"

# Install UV if not present
if ! command -v uv &> /dev/null; then
    echo "📦 Installing UV..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Create venv
echo "🐍 Creating virtual environment..."
uv venv

# Activate venv
source .venv/bin/activate

# Install dependencies
echo "📚 Installing dependencies..."
uv pip install maturin pytest pytest-cov black ruff mypy

# Build extension
echo "🔨 Building extension..."
maturin develop

# Run tests
echo "🧪 Running tests..."
uv run pytest tests/python/ -v

echo "✅ Setup complete!"
echo ""
echo "To activate the environment:"
echo "  source .venv/bin/activate"
echo ""
echo "To run tests:"
echo "  uv run pytest tests/python/ -v"
```

Make it executable and run:
```bash
chmod +x setup-uv.sh
./setup-uv.sh
```

---

## Summary: Why Use UV?

| Feature | pip | UV |
|---------|-----|-----|
| Speed | Slow | 10-100x faster ⚡ |
| Dependency Resolution | Basic | Advanced 🎯 |
| Virtual Envs | Requires venv | Built-in 📦 |
| Written in | Python | Rust 🦀 |
| Run without activation | No | Yes (`uv run`) ✅ |

---

## Quick Reference

```bash
# Setup
curl -LsSf https://astral.sh/uv/install.sh | sh
uv venv
source .venv/bin/activate

# Install
uv pip install maturin pytest pytest-cov

# Build
maturin develop

# Test
uv run pytest tests/python/ -v

# Format
uv run black python/ tests/

# Lint
uv run ruff check .
```

---

**Ready to use UV?** Run:

```bash
curl -LsSf https://astral.sh/uv/install.sh | sh
uv venv
source .venv/bin/activate
uv pip install maturin pytest pytest-cov
maturin develop
uv run pytest tests/python/ -v
```

🚀 Enjoy the speed!
