# Building from Source

## Prerequisites

1. **Rust** (latest stable)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source $HOME/.cargo/env
   ```

2. **Python** 3.10+ with development headers
   ```bash
   # Ubuntu/Debian
   sudo apt-get install python3-dev build-essential

   # Fedora/RHEL
   sudo dnf install python3-devel gcc

   # macOS
   xcode-select --install
   ```

3. **maturin** (Rust-Python build tool)
   ```bash
   pip install maturin
   ```

## Quick Build

```bash
git clone https://github.com/txjmb/jsonatapy.git
cd jsonatapy
git submodule update --init --recursive

# Development build
maturin develop --release

# Run tests
pytest tests/python/ -v
```

## Build Commands

**Development mode** (for local testing):
```bash
maturin develop --release
```

**Build wheel**:
```bash
maturin build --release --out dist
```

**Build for specific Python version**:
```bash
maturin build --release --interpreter python3.11
```

**Build for multiple Python versions**:
```bash
maturin build --release --interpreter python3.10 python3.11 python3.12 python3.13
```

## Testing

**Python tests**:
```bash
# All tests
pytest tests/python/ -v

# Reference suite only
pytest tests/python/test_reference_suite.py -v

# Parallel execution
pytest tests/python/ -n auto
```

**Rust tests**:
```bash
cargo test
cargo test --release
```

## Development Workflow

1. Make changes to Rust code
2. Rebuild: `maturin develop --release`
3. Test: `pytest tests/python/ -v`
4. Format: `cargo fmt`
5. Lint: `cargo clippy`

## Platform-Specific Notes

### Linux

For cross-compilation to aarch64:
```bash
sudo apt-get install gcc-aarch64-linux-gnu
rustup target add aarch64-unknown-linux-gnu
maturin build --release --target aarch64-unknown-linux-gnu
```

### macOS

**Build for both architectures**:
```bash
# ARM (native on M1/M2/M3)
maturin develop --release

# Intel (cross-compile on ARM)
rustup target add x86_64-apple-darwin
maturin build --release --target x86_64-apple-darwin
```

### Windows

Requires Visual Studio with C++ build tools:
```powershell
# Install Rust from https://rustup.rs/

# Build
maturin build --release
```

## Benchmarks

```bash
# Python benchmarks
python benchmarks/benchmark.py

# Update performance docs
python benchmarks/update_docs.py
```

## Troubleshooting

**maturin not found**:
```bash
pip install --user maturin
# Add ~/.local/bin to PATH
```

**Rust compiler errors**:
```bash
rustup update
cargo clean
maturin develop --release
```

**Test failures**:
```bash
# Clean build
cargo clean
rm -rf target/
maturin develop --release
pytest tests/python/ -v
```
