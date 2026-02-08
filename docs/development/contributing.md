# Contributing

Contributions to jsonatapy are welcome!

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Set up development environment

```bash
git clone https://github.com/YOUR_USERNAME/jsonatapy.git
cd jsonatapy
git submodule update --init --recursive

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install dependencies
pip install maturin pytest pytest-xdist

# Build and test
maturin develop --release
pytest tests/python/ -v
```

## Development Process

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
```

### 2. Make Changes

- Follow Rust and Python best practices
- Add tests for new functionality
- Update documentation as needed
- Run tests and linters

### 3. Test Your Changes

```bash
# Format code
cargo fmt

# Lint
cargo clippy

# Run Rust tests
cargo test

# Build extension
maturin develop --release

# Run Python tests
pytest tests/python/ -v
```

### 4. Commit

```bash
git add .
git commit -m "Brief description of changes"
```

Use conventional commit messages:
- `feat:` New feature
- `fix:` Bug fix
- `docs:` Documentation
- `test:` Tests
- `refactor:` Code refactoring
- `chore:` Maintenance

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a pull request on GitHub.

## Code Guidelines

### Rust Code

- Follow Rust 2021 edition standards
- Run `cargo fmt` before committing
- Pass `cargo clippy` with no warnings
- Add rustdoc comments for public APIs
- Use meaningful variable names

### Python Code

- Follow PEP 8
- Add type hints where applicable
- Include docstrings for public functions
- Write clear, descriptive tests

### Testing

- All new features must include tests
- Tests should be clear and focused
- Use descriptive test names
- Aim for high test coverage

### Documentation

- Update relevant documentation for changes
- Add examples for new features
- Keep documentation concise and clear
- No unnecessary emojis or commentary

## Architecture

jsonatapy mirrors the JavaScript reference implementation structure for easier synchronization:

- `src/parser.rs` - Expression parser
- `src/evaluator.rs` - Expression evaluator
- `src/functions.rs` - Built-in functions
- `src/datetime.rs` - Date/time functions
- `src/signature.rs` - Function signature validation

See [CLAUDE.MD](../../CLAUDE.MD) for detailed architecture notes.

## Reporting Issues

When reporting bugs:
- Include minimal reproduction case
- Specify Python version and platform
- Include error messages and stack traces
- Check if issue already exists

## Questions?

- Check existing documentation
- Review [CLAUDE.MD](../../CLAUDE.MD) for design guidelines
- Open an issue for discussion
