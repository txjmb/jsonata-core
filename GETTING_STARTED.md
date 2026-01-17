# Getting Started with jsonatapy Development

This guide will help you set up your development environment and start contributing to jsonatapy.

## Prerequisites

### 1. Install Rust

**Windows:**
```powershell
# Download and run rustup-init.exe from https://rustup.rs/
# Or use winget:
winget install Rustlang.Rustup
```

**Linux/macOS:**
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installation, verify:
```bash
rustc --version
cargo --version
```

### 2. Install Python 3.8+

Ensure you have Python 3.8 or later installed:
```bash
python --version
```

### 3. Install Development Tools

```bash
# Install maturin (Rust-Python bridge)
pip install maturin

# Install development dependencies
pip install pytest pytest-cov black ruff mypy
```

## Project Setup

### 1. Clone the Repository

```bash
git clone https://github.com/yourusername/jsonatapy.git
cd jsonatapy
```

### 2. Build the Extension

```bash
# Build and install the extension in development mode
# This allows you to make changes and rebuild quickly
maturin develop
```

### 3. Run Tests

```bash
# Run Rust tests
cargo test

# Run Python tests
pytest tests/python/ -v

# Run with coverage
pytest tests/python/ -v --cov=jsonatapy --cov-report=html
```

## Development Workflow

### Making Changes

1. **Edit Rust Code** (`src/*.rs`)
   - Make your changes
   - Run `cargo fmt` to format
   - Run `cargo clippy` to check for issues
   - Run `cargo test` to test Rust code

2. **Rebuild Extension**
   ```bash
   maturin develop
   ```

3. **Edit Python Code** (`python/jsonatapy/*.py`)
   - Make your changes
   - Run `black python/` to format
   - Run `ruff check python/` to lint
   - Run `mypy python/` for type checking

4. **Test**
   ```bash
   pytest tests/python/ -v
   ```

### Code Quality Checks

Before committing, run:

```bash
# Format Rust code
cargo fmt

# Check Rust code
cargo clippy -- -D warnings

# Format Python code
black python/ tests/

# Lint Python code
ruff check python/ tests/

# Type check Python
mypy python/

# Run all tests
cargo test && pytest tests/python/ -v
```

## Understanding the Codebase

### Rust Modules (src/)

- **lib.rs** - Python bindings entry point
- **ast.rs** - Abstract Syntax Tree definitions
- **parser.rs** - Expression parser (JSONata → AST)
- **evaluator.rs** - Expression evaluator (AST + data → result)
- **functions.rs** - Built-in function implementations
- **datetime.rs** - Date/time functions
- **signature.rs** - Function signature validation
- **utils.rs** - Utility functions

### Python Package (python/jsonatapy/)

- **__init__.py** - Python API surface
- **py.typed** - Type hint marker

### Test Structure (tests/)

- **python/** - Python integration tests
- **jsonata-suite/** - Reference test suite (to be added)

## Implementation Roadmap

See [CLAUDE.MD](CLAUDE.MD) for the detailed implementation strategy, but here's a quick overview:

### Phase 1: Parser & Core (Current)
- [ ] Lexer/tokenizer
- [ ] Parser (expression to AST)
- [ ] Basic evaluator (literals, simple paths)

### Phase 2: Functions
- [ ] String functions
- [ ] Numeric functions
- [ ] Array functions
- [ ] Object functions

### Phase 3: Advanced Features
- [ ] Higher-order functions
- [ ] DateTime functions
- [ ] Async support

### Phase 4: Optimization
- [ ] Performance profiling
- [ ] Memory optimization

## Working with the Reference Implementation

The JavaScript reference implementation is your best friend:

1. **Browse the code**: https://github.com/jsonata-js/jsonata
2. **Test your understanding**: https://try.jsonata.org/
3. **Read the docs**: https://docs.jsonata.org/

When implementing a feature:
1. Find the equivalent code in the JS implementation
2. Understand the algorithm
3. Implement in Rust following the same structure
4. Add tests from the JS test suite
5. Verify behavior matches exactly

## Common Commands

```bash
# Quick rebuild and test
maturin develop && pytest tests/python/test_basic.py -v

# Run specific test
pytest tests/python/test_basic.py::TestCompile::test_compile_returns_expression -v

# Check everything before commit
cargo fmt && cargo clippy -- -D warnings && cargo test && \
  black python/ tests/ && ruff check python/ tests/ && \
  pytest tests/python/ -v

# Build release wheel
maturin build --release

# Build for specific Python version
maturin build --release --interpreter python3.11
```

## Tips for Contributors

1. **Start Small**: Begin with simple literals and basic operations
2. **Test First**: Write tests before implementation when possible
3. **Check JS Reference**: Always verify behavior against the reference
4. **Ask Questions**: Open issues for clarification
5. **Document**: Add rustdoc comments to public APIs
6. **Performance Later**: Focus on correctness first, optimize later

## Resources

- **JSONata Docs**: https://docs.jsonata.org/
- **JSONata Playground**: https://try.jsonata.org/
- **Reference Implementation**: https://github.com/jsonata-js/jsonata
- **PyO3 Guide**: https://pyo3.rs/
- **Maturin Docs**: https://maturin.rs/
- **Rust Book**: https://doc.rust-lang.org/book/

## Getting Help

- Open an issue for bugs or questions
- See [CLAUDE.MD](CLAUDE.MD) for detailed architecture info
- Check the reference implementation when behavior is unclear

## Next Steps

1. Build the project: `maturin develop`
2. Run tests: `cargo test && pytest tests/python/ -v`
3. Pick an issue from the tracker or implement a basic feature
4. Read through `src/parser.rs` to understand the next priority
5. Join the discussion on implementation approaches

Happy coding!
