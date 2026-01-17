# JSONataPy Quick Reference

Your one-page cheat sheet for building and using JSONataPy.

## 🚀 First Time Setup (WSL)

```bash
# Navigate to project
cd /mnt/c/Users/mboha/source/repos/jsonatapy

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install UV
curl -LsSf https://astral.sh/uv/install.sh | sh

# Setup environment
uv venv
source .venv/bin/activate
uv pip install maturin pytest pytest-cov black ruff

# Build and test
cargo check && cargo test && maturin develop && uv run pytest tests/python/ -v
```

## ⚡ Quick Commands

| Task | Command |
|------|---------|
| **Check Rust code** | `cargo check` |
| **Run Rust tests** | `cargo test` |
| **Build extension (dev)** | `maturin develop` |
| **Build extension (release)** | `maturin develop --release` |
| **Run Python tests** | `uv run pytest tests/python/ -v` |
| **Run examples** | `uv run python examples/basic_usage.py` |
| **Format code** | `uv run black python/ tests/` |
| **Lint code** | `uv run ruff check .` |

## 🔄 Daily Development Workflow

```bash
# 1. Activate environment
source .venv/bin/activate

# 2. Make changes to Rust code (src/*.rs)

# 3. Quick check
cargo check

# 4. Run tests
cargo test

# 5. Rebuild extension
maturin develop

# 6. Test Python integration
uv run pytest tests/python/ -v
```

## 📦 UV Commands

| Task | Command |
|------|---------|
| **Install UV** | `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| **Create venv** | `uv venv` |
| **Install package** | `uv pip install package` |
| **Run without activation** | `uv run python script.py` |
| **Run pytest** | `uv run pytest tests/ -v` |
| **List packages** | `uv pip list` |

## 🧪 Testing

```bash
# Run all Rust tests
cargo test

# Run specific Rust test
cargo test test_name

# Run all Python tests
uv run pytest tests/python/ -v

# Run specific Python test file
uv run pytest tests/python/test_integration.py -v

# Run specific test
uv run pytest tests/python/test_integration.py::TestLiterals::test_number_literal -v

# Run with coverage
uv run pytest tests/python/ --cov=jsonatapy --cov-report=html
```

## 🎯 One-Command Operations

```bash
# Full build and test
cargo check && cargo test && maturin develop && uv run pytest tests/python/ -v

# Format and lint
uv run black python/ tests/ && uv run ruff check .

# Clean rebuild
cargo clean && cargo build && maturin develop --release
```

## 📝 Code Quality

```bash
# Format Python code
uv run black python/ tests/

# Lint Python code
uv run ruff check .

# Type check
uv run mypy python/jsonatapy/

# Format Rust code
cargo fmt

# Lint Rust code
cargo clippy
```

## 🔍 Debugging

```bash
# Verbose Rust tests
cargo test -- --nocapture

# Verbose Python tests
uv run pytest tests/python/ -v -s

# Run single test with output
cargo test test_name -- --nocapture
uv run pytest tests/python/test_integration.py::TestLiterals -v -s

# Check compilation errors
cargo check

# Show what maturin will do
maturin develop --help
```

## 📚 File Structure Quick Reference

```
src/
├── lib.rs          - Python bindings (PyO3)
├── parser.rs       - Lexer and parser
├── evaluator.rs    - Expression evaluator
├── functions.rs    - Built-in functions (33 total)
├── ast.rs          - AST definitions
├── datetime.rs     - DateTime functions
├── signature.rs    - Function signatures
└── utils.rs        - Utility functions

tests/
└── python/
    ├── test_basic.py        - Basic API tests
    └── test_integration.py  - Integration tests

examples/
└── basic_usage.py  - 12 usage examples
```

## 🎯 Essential Files

| File | Purpose |
|------|---------|
| **README_FIRST.md** | Start here! Step-by-step guide |
| **UV_SETUP.md** | Complete UV guide |
| **WSL_SETUP.md** | WSL-specific instructions |
| **BUILD_INSTRUCTIONS.md** | Comprehensive build guide |
| **BUILD_CHECKLIST.md** | Verification checklist |
| **CURRENT_STATUS.md** | Implementation status |

## 💡 Quick Test

Verify everything works:

```bash
uv run python << 'EOF'
import jsonatapy

# Test compilation
expr = jsonatapy.compile("$sum(numbers)")
print("✓ Compilation works")

# Test evaluation
result = expr.evaluate({"numbers": [1, 2, 3, 4, 5]})
assert result == 15
print("✓ Evaluation works")

# Test functions
result = jsonatapy.evaluate('$uppercase("hello")', {})
assert result == "HELLO"
print("✓ Functions work")

print("\n🎉 JSONataPy is working!")
EOF
```

## 🚨 Common Issues

| Problem | Solution |
|---------|----------|
| `cargo: command not found` | Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| `uv: command not found` | Install UV: `curl -LsSf https://astral.sh/uv/install.sh \| sh` |
| `maturin: command not found` | Install: `uv pip install maturin` |
| `Import Error: jsonatapy` | Build extension: `maturin develop` |
| Compilation errors | Check with: `cargo check` |
| Tests failing | See error message, fix code |

## 📊 Performance Testing

```python
import jsonatapy
import time

data = {"numbers": list(range(1000))}

# Warm up
for _ in range(10):
    jsonatapy.evaluate("$sum(numbers)", data)

# Benchmark
start = time.time()
for _ in range(1000):
    result = jsonatapy.evaluate("$sum(numbers)", data)
end = time.time()

print(f"1000 evaluations: {(end - start)*1000:.2f}ms")
print(f"Average: {(end - start):.3f}ms per evaluation")
```

## 🎓 Example Usage

```python
import jsonatapy

# Simple literals
jsonatapy.evaluate("42", {})  # 42
jsonatapy.evaluate('"hello"', {})  # "hello"

# Arithmetic
jsonatapy.evaluate("1 + 2 * 3", {})  # 7

# Path navigation
data = {"user": {"name": "Alice"}}
jsonatapy.evaluate("user.name", data)  # "Alice"

# String functions
jsonatapy.evaluate('$uppercase("hello")', {})  # "HELLO"
jsonatapy.evaluate('$length("hello")', {})  # 5

# Numeric functions
data = {"prices": [10, 20, 30]}
jsonatapy.evaluate("$sum(prices)", data)  # 60
jsonatapy.evaluate("$max(prices)", data)  # 30

# Array functions
data = {"items": [3, 1, 4, 1, 5]}
jsonatapy.evaluate("$sort(items)", data)  # [1, 1, 3, 4, 5]
jsonatapy.evaluate("$distinct(items)", data)  # [3, 1, 4, 5]

# Conditional
data = {"age": 25}
jsonatapy.evaluate('age >= 18 ? "adult" : "minor"', data)  # "adult"

# Compiled expressions (faster for reuse)
expr = jsonatapy.compile("$sum(numbers)")
expr.evaluate({"numbers": [1, 2, 3]})  # 6
expr.evaluate({"numbers": [4, 5, 6]})  # 15
```

## 🔗 Links

- **JSONata Playground**: https://try.jsonata.org/
- **JSONata Docs**: https://docs.jsonata.org/
- **UV Docs**: https://github.com/astral-sh/uv
- **PyO3 Guide**: https://pyo3.rs/
- **Rust Book**: https://doc.rust-lang.org/book/

---

**Quick Start**: `./setup-uv.sh` or see **README_FIRST.md**

**Status**: ✅ Core Implementation Complete | 🔄 Ready for Build & Test

**Last Updated**: 2026-01-17
