#!/bin/bash
set -e

echo "🚀 Setting up JSONataPy with UV"
echo "================================"
echo ""

# Install UV if not present
if ! command -v uv &> /dev/null; then
    echo "📦 Installing UV (ultra-fast Python package manager)..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
    export PATH="$HOME/.cargo/bin:$PATH"
    echo "✅ UV installed"
else
    echo "✅ UV already installed ($(uv --version))"
fi

echo ""

# Check for Rust
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust not found. Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    echo "✅ Rust installed"
else
    echo "✅ Rust already installed ($(rustc --version))"
fi

echo ""

# Create venv with UV
if [ ! -d ".venv" ]; then
    echo "🐍 Creating virtual environment with UV..."
    uv venv
    echo "✅ Virtual environment created"
else
    echo "✅ Virtual environment already exists"
fi

echo ""

# Activate venv
echo "🔧 Activating virtual environment..."
source .venv/bin/activate

echo ""

# Install dependencies with UV
echo "📚 Installing dependencies with UV..."
uv pip install maturin pytest pytest-cov black ruff mypy
echo "✅ Dependencies installed"

echo ""

# Check Rust compilation
echo "🔍 Checking Rust compilation..."
if cargo check; then
    echo "✅ Rust code compiles successfully"
else
    echo "❌ Rust compilation failed"
    exit 1
fi

echo ""

# Run Rust tests
echo "🧪 Running Rust tests..."
if cargo test --quiet; then
    echo "✅ All Rust tests passed"
else
    echo "❌ Some Rust tests failed"
    exit 1
fi

echo ""

# Build extension
echo "🔨 Building Python extension with maturin..."
if maturin develop; then
    echo "✅ Extension built and installed"
else
    echo "❌ Extension build failed"
    exit 1
fi

echo ""

# Run Python tests
echo "🧪 Running Python tests with UV..."
if uv run pytest tests/python/ -v; then
    echo "✅ All Python tests passed"
else
    echo "❌ Some Python tests failed"
    exit 1
fi

echo ""
echo "════════════════════════════════════════"
echo "✅ Setup complete! JSONataPy is ready!"
echo "════════════════════════════════════════"
echo ""
echo "Next steps:"
echo "  1. Activate the environment:"
echo "     source .venv/bin/activate"
echo ""
echo "  2. Run examples:"
echo "     uv run python examples/basic_usage.py"
echo ""
echo "  3. Run tests:"
echo "     uv run pytest tests/python/ -v"
echo ""
echo "  4. Format code:"
echo "     uv run black python/ tests/"
echo ""
echo "  5. Lint code:"
echo "     uv run ruff check ."
echo ""
echo "See UV_SETUP.md for more UV commands and tips."
echo ""
