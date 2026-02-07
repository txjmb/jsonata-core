# CLAUDE.MD

This file provides context and guidelines for AI assistants working on this project.

## Project Overview

**Project Name:** jsonatapy

**Description:** A high-performance Python implementation of JSONata (JSON query and transformation language) built as a Rust-based Python extension. This project aims to provide native Python support for JSONata with significantly better performance than JavaScript wrapper implementations.

**Repository Reference:** https://github.com/jsonata-js/jsonata (JavaScript reference implementation)

**Tech Stack:**
- **Extension Language:** Rust (using PyO3 for Python bindings)
- **Python:** 3.8+ (target latest stable versions)
- **Build System:** maturin (Rust-Python bridge)
- **Testing:** pytest (Python), cargo test (Rust)
- **CI/CD:** GitHub Actions
- **Documentation:** Sphinx (Python docs) + rustdoc (Rust docs)

## Design Goals

### 1. Maintainability & Synchronization
The Rust codebase should mirror the structure of the reference JavaScript implementation to enable rapid adaptation of upstream updates. Key principles:
- Module structure should map 1:1 with JavaScript source files where possible
- Core algorithms should follow the same logical flow as the JS implementation
- Parser structure should align with the JS parser for easier diff-based updates
- Regular syncs with upstream jsonata-js releases

### 2. Test Suite Compatibility
- Must pass 100% of the reference JavaScript library's test suite
- Tests should run in Python using pytest with the compiled extension
- Consider implementing test adapter that can consume JS test cases directly
- Maintain parity with test coverage metrics (currently 100% in JS version)

### 3. Best-in-Class Quality
- **Documentation:** Comprehensive docs following JSONata documentation patterns
- **Code Standards:** Rust best practices, clippy-clean code, Python PEP 8 compliance
- **CI/CD:** Automated testing, coverage reporting, benchmarking, multi-platform builds
- **Performance:** Regular benchmarks against JS implementation and Python wrappers

## Reference Implementation Structure

The JavaScript reference (v2.1.0) consists of:

### Source Files (src/)
1. **jsonata.js** - Main entry point and core evaluation engine
2. **parser.js** - Expression parser (converts JSONata strings to AST)
3. **functions.js** - Built-in function implementations
4. **datetime.js** - Date/time handling functions
5. **signature.js** - Function signature validation and type checking
6. **utils.js** - Utility functions and helpers

### Test Structure (test/)
- **test-suite/** - Comprehensive test cases (JSON-based)
- **implementation-tests.js** - Core functionality tests
- **async-function.js** - Async operation tests
- **parser-recovery.js** - Error handling tests
- **parser-pluggable-regex.js** - Custom regex tests
- **run-test-suite.js** - Test orchestrator

## Project Structure

```
jsonatapy/
├── src/                    # Rust source code
│   ├── lib.rs             # Main library entry point (Python bindings)
│   ├── parser.rs          # JSONata expression parser (mirrors parser.js)
│   ├── evaluator.rs       # Expression evaluator (mirrors jsonata.js)
│   ├── functions.rs       # Built-in functions (mirrors functions.js)
│   ├── datetime.rs        # Date/time functions (mirrors datetime.js)
│   ├── signature.rs       # Function signature validation (mirrors signature.js)
│   ├── utils.rs           # Utility functions (mirrors utils.js)
│   └── ast.rs             # Abstract Syntax Tree definitions
├── python/                # Python package structure
│   └── jsonatapy/
│       ├── __init__.py    # Python API surface
│       └── py.typed       # PEP 561 marker for type checking
├── tests/                 # Test suite
│   ├── python/            # Python-based tests
│   │   ├── test_basic.py
│   │   ├── test_functions.py
│   │   └── test_compatibility.py
│   ├── rust/              # Rust unit tests (in src/ with #[cfg(test)])
│   └── jsonata-js/        # Reference JS test suite (git submodule)
├── benchmarks/            # Performance benchmarks
│   ├── python/            # Python benchmark scripts
│   ├── javascript/        # JS comparison benchmarks
│   ├── rust/              # Rust benchmarks
│   └── results/           # Benchmark results
├── docs/                  # Documentation
│   ├── source/            # Sphinx docs
│   └── api/               # API reference
├── .github/
│   └── workflows/         # CI/CD pipelines
│       ├── test.yml       # Test automation
│       ├── benchmark.yml  # Performance tracking
│       └── release.yml    # Release builds (wheels for multiple platforms)
├── Cargo.toml             # Rust dependencies and config
├── pyproject.toml         # Python package config (maturin)
├── README.md              # Project readme
├── CLAUDE.MD              # This file
└── LICENSE                # MIT License (matching upstream)
```

## Development Guidelines

### Code Style

**Rust:**
- Follow Rust 2021 edition standards
- Run `cargo fmt` before commits
- Pass `cargo clippy -- -D warnings` (zero warnings policy)
- Prefer explicit types and comprehensive error handling
- Use `Result<T, E>` for fallible operations
- Document public APIs with rustdoc comments (`///`)

**Python:**
- PEP 8 compliance (enforced by black and ruff)
- Type hints for all public APIs (PEP 484)
- Docstrings following NumPy/Sphinx style

### Naming Conventions
- Rust: snake_case for functions/variables, PascalCase for types
- Python: PEP 8 naming (snake_case functions, PascalCase classes)
- Mirror JavaScript function names in Rust where possible (e.g., `evaluate_expression`)

### Error Handling
- Rust: Custom error types using thiserror or anyhow
- Python: Raise appropriate Python exceptions from Rust (PyO3 conversion)
- Preserve error messages and context from JS implementation

### Testing
- **Unit Tests:** Rust tests in each module using `#[cfg(test)]`
- **Integration Tests:** Python tests using pytest
- **Compatibility Tests:** Automated import and execution of JS test suite
- **Coverage Target:** 100% (matching upstream)
- **Property-Based Testing:** Consider proptest for edge cases

### Documentation
- Every public API must have documentation
- Examples in docs should be tested (doctests in Rust, doctest in Python)
- Maintain a migration guide for users coming from JS JSONata
- Keep CHANGELOG.md updated following Keep a Changelog format

## Common Tasks

### Initial Setup
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Python dependencies
pip install maturin pytest black ruff mypy

# Clone with test suite submodule
git clone <repo-url>
cd jsonatapy
git submodule update --init --recursive
```

### Development
```bash
# Build the extension in development mode
maturin develop

# Run Python tests
pytest tests/python/ -v

# Run Rust tests
cargo test

# Run specific test suite
pytest tests/python/test_functions.py::test_string_functions -v
```

### Code Quality
```bash
# Format code
cargo fmt
black python/ tests/

# Lint
cargo clippy -- -D warnings
ruff check python/ tests/

# Type checking
mypy python/

# Coverage
cargo tarpaulin --out Html
pytest --cov=jsonatapy --cov-report=html tests/python/
```

### Building
```bash
# Development build
maturin develop --release

# Production build (wheel)
maturin build --release

# Build for multiple Python versions
maturin build --release --interpreter python3.8 python3.9 python3.10 python3.11 python3.12
```

### Benchmarking
```bash
# Run Rust benchmarks
cargo bench

# Compare with JavaScript implementation
python benches/compare_js.py
```

## Important Notes for AI Assistants

### Critical Principles
1. **Always verify against reference implementation** - When implementing features, check the JavaScript source for exact behavior
2. **Test-driven development** - Write/adapt tests before implementation
3. **Performance matters** - Profile before optimizing, but keep performance in mind
4. **Maintain structural alignment** - Keep module organization parallel to JS implementation

### Common Pitfalls to Avoid
- Don't diverge from JS behavior without documenting why
- Don't optimize prematurely at the expense of code clarity
- Don't skip tests - 100% compatibility is non-negotiable
- Don't forget to handle JavaScript's quirky type coercion rules
- Remember Python/Rust threading differences when implementing async features

### Implementation Strategy
1. Start with parser (most critical for compatibility)
2. Implement AST representation
3. Build evaluator core
4. Add built-in functions incrementally
5. Each feature should immediately have corresponding tests from JS suite

### Tracking Upstream
- Monitor jsonata-js releases: https://github.com/jsonata-js/jsonata/releases
- Track commit history for understanding rationale behind changes
- Keep reference implementation version in Cargo.toml and pyproject.toml
- Update CHANGELOG.md when syncing with new upstream versions

## Context for AI Assistants

### Preferences
- Favor correctness over performance in initial implementations
- Prefer explicit error handling to panics
- Use descriptive variable names matching JS implementation where practical
- Include comments explaining deviations from JS implementation
- When in doubt about behavior, test against live jsonata-js

### Priority Order for Implementation
1. **Phase 1: Parser & Core**
   - Lexer/tokenizer
   - Parser (expression to AST)
   - Basic evaluator (path queries, simple expressions)

2. **Phase 2: Functions**
   - String functions
   - Numeric functions
   - Array functions
   - Object functions

3. **Phase 3: Advanced Features**
   - Higher-order functions
   - Datetime functions
   - Async support
   - Custom function registration

4. **Phase 4: Optimization**
   - Performance profiling
   - Memory optimization
   - Parallel evaluation where safe

### Constraints
- Must maintain compatibility with Python 3.8+
- Rust stable channel only (no nightly features)
- Zero unsafe code in initial implementation (use only where profiled as necessary)
- Cross-platform support: Windows, Linux, macOS, aarch64, x86_64

## Additional Resources

- **JSONata Reference:** https://docs.jsonata.org/
- **Reference Implementation:** https://github.com/jsonata-js/jsonata
- **JSONata Playground:** https://try.jsonata.org/
- **PyO3 Guide:** https://pyo3.rs/
- **Maturin Docs:** https://maturin.rs/
- **JSONata Specification:** https://docs.jsonata.org/overview.html

## Performance Targets

Current Python wrappers around JS implementation typically show:
- 10-100x slower than native JS execution (due to bridge overhead)
- High memory overhead from JS engine embedding

**Goals for jsonatapy:**
- Match or exceed JS execution speed (target: 2-5x faster)
- Minimize memory overhead (Rust's zero-cost abstractions)
- Enable parallel evaluation where JSONata expressions are independent

## License

MIT License (matching upstream jsonata-js) - ensures maximum compatibility and adoption.
