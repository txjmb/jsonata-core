# Installation Guide

This guide covers installing jsonatapy in various environments.

## Quick Install

### Using pip (Recommended)

```bash
pip install jsonatapy
```

This installs pre-built wheels for:
- **Python**: 3.8, 3.9, 3.10, 3.11, 3.12
- **Platforms**: Windows, Linux (x86_64, aarch64), macOS (x86_64, arm64)

### Using pip with specific Python version

```bash
python3.11 -m pip install jsonatapy
```

### Upgrade to latest version

```bash
pip install --upgrade jsonatapy
```

## Virtual Environments

### Using venv (Standard Library)

```bash
# Create virtual environment
python -m venv .venv

# Activate (Linux/macOS)
source .venv/bin/activate

# Activate (Windows)
.venv\Scripts\activate

# Install jsonatapy
pip install jsonatapy
```

### Using conda

```bash
# Create environment
conda create -n myproject python=3.11
conda activate myproject

# Install jsonatapy
pip install jsonatapy
```

### Using Poetry

```bash
# Add to project
poetry add jsonatapy

# Or install in development mode
poetry add --dev jsonatapy
```

```toml
# pyproject.toml
[tool.poetry.dependencies]
python = "^3.8"
jsonatapy = "^0.1.0"
```

### Using uv (Fast Package Installer)

```bash
# Install uv
pip install uv

# Install jsonatapy
uv pip install jsonatapy

# Or in a project
uv add jsonatapy
```

## Verify Installation

After installation, verify that jsonatapy works correctly:

```python
import jsonatapy

# Check version
print(f"jsonatapy version: {jsonatapy.__version__}")
print(f"JSONata spec version: {jsonatapy.__jsonata_version__}")

# Run a simple test
data = {"name": "World"}
result = jsonatapy.evaluate('"Hello, " & name', data)
print(result)  # Should print: Hello, World

print("✅ jsonatapy is working correctly!")
```

Save this as `test_install.py` and run:

```bash
python test_install.py
```

## Building from Source

If pre-built wheels are not available for your platform, or you want to build from source:

### Prerequisites

1. **Rust** (latest stable)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Python** 3.8 or later with development headers
   ```bash
   # Ubuntu/Debian
   sudo apt-get install python3-dev

   # Fedora/RHEL
   sudo dnf install python3-devel

   # macOS (via Homebrew)
   brew install python
   ```

3. **maturin** (Rust-Python build tool)
   ```bash
   pip install maturin
   ```

### Build and Install

```bash
# Clone the repository
git clone https://github.com/yourusername/jsonatapy.git
cd jsonatapy

# Build and install in development mode
maturin develop --release

# Or build a wheel
maturin build --release

# Install the wheel
pip install target/wheels/jsonatapy-*.whl
```

For detailed build instructions, see [Building from Source](building.md).

## Platform-Specific Notes

### Windows

**Prerequisites:**
- Visual Studio 2019 or later with C++ build tools
- Or: Windows SDK with MSVC compiler

```bash
# Install Rust
# Download from: https://rustup.rs/

# Verify installation
rustc --version
cargo --version

# Install jsonatapy
pip install jsonatapy
```

**Note for Windows Subsystem for Linux (WSL):**
- Use Linux installation instructions inside WSL
- Don't mix Windows and WSL Python environments

### macOS

**Prerequisites:**
- Xcode Command Line Tools
  ```bash
  xcode-select --install
  ```

**Apple Silicon (M1/M2/M3):**
- Pre-built wheels available for arm64
- Supports both native arm64 and Rosetta x86_64 Python

```bash
# Install jsonatapy (works on both architectures)
pip install jsonatapy
```

### Linux

**Ubuntu/Debian:**
```bash
# Install build dependencies (only for building from source)
sudo apt-get update
sudo apt-get install python3-dev build-essential

# Install jsonatapy
pip install jsonatapy
```

**Fedora/RHEL/CentOS:**
```bash
# Install build dependencies (only for building from source)
sudo dnf install python3-devel gcc

# Install jsonatapy
pip install jsonatapy
```

**Alpine Linux:**
```bash
# Install build dependencies
apk add python3-dev gcc musl-dev

# Install jsonatapy
pip install jsonatapy
```

**ARM/aarch64 Systems:**
- Pre-built wheels available for aarch64
- Raspberry Pi 4 and newer supported

## Docker

### Using Pre-built Wheels

```dockerfile
FROM python:3.11-slim

# Install jsonatapy
RUN pip install --no-cache-dir jsonatapy

# Copy your application
COPY . /app
WORKDIR /app

CMD ["python", "app.py"]
```

### Building from Source in Docker

```dockerfile
FROM rust:1.75 as builder

# Install Python and maturin
RUN apt-get update && apt-get install -y python3-dev python3-pip
RUN pip3 install maturin

# Build jsonatapy
COPY . /build
WORKDIR /build
RUN maturin build --release

# Runtime stage
FROM python:3.11-slim
COPY --from=builder /build/target/wheels/*.whl /tmp/
RUN pip install /tmp/*.whl && rm /tmp/*.whl

CMD ["python"]
```

## Troubleshooting

### Import Error: DLL load failed (Windows)

**Problem:**
```
ImportError: DLL load failed while importing _jsonatapy
```

**Solution:**
Install Visual C++ Redistributable:
- Download from: https://aka.ms/vs/17/release/vc_redist.x64.exe
- Or: Install full Visual Studio with C++ support

### Import Error: No module named '_jsonatapy'

**Problem:**
```
ModuleNotFoundError: No module named '_jsonatapy'
```

**Solutions:**
1. Verify installation: `pip list | grep jsonatapy`
2. Reinstall: `pip uninstall jsonatapy && pip install jsonatapy`
3. Check Python version compatibility (3.8+)
4. Try building from source

### Rust Compiler Not Found (Building from Source)

**Problem:**
```
error: failed to run custom build command for `jsonatapy`
```

**Solution:**
1. Install Rust: https://rustup.rs/
2. Add to PATH: `source $HOME/.cargo/env`
3. Verify: `rustc --version`

### maturin Build Fails

**Problem:**
```
maturin failed with error
```

**Solutions:**
1. Update maturin: `pip install -U maturin`
2. Update Rust: `rustup update`
3. Clear build cache: `cargo clean`
4. Check Python dev headers installed

### Wheel Not Found for Platform

**Problem:**
```
ERROR: Could not find a version that satisfies the requirement jsonatapy
```

**Solution:**
Build from source (see above) or request pre-built wheels for your platform.

## System Requirements

### Minimum Requirements
- **Python**: 3.8 or later
- **RAM**: 512 MB (for typical use)
- **Disk**: 10 MB installed size

### Recommended
- **Python**: 3.11 or later (better performance)
- **RAM**: 2 GB or more
- **Disk**: 50 MB for source builds

### Supported Platforms

| Platform | Architecture | Python Versions | Status |
|----------|-------------|-----------------|--------|
| Windows | x86_64 | 3.8-3.12 | ✅ Pre-built |
| Linux | x86_64 | 3.8-3.12 | ✅ Pre-built |
| Linux | aarch64 | 3.8-3.12 | ✅ Pre-built |
| macOS | x86_64 | 3.8-3.12 | ✅ Pre-built |
| macOS | arm64 (M1/M2/M3) | 3.8-3.12 | ✅ Pre-built |

## Uninstalling

```bash
pip uninstall jsonatapy
```

To also remove cached wheels:

```bash
pip cache remove jsonatapy
```

## Next Steps

- [Learn the Python API](api.md)
- [Explore usage patterns](usage.md)
- [Understand performance](performance.md)
- [Build from source](building.md)

---

**Having Issues?**
- Check [Troubleshooting](#troubleshooting) section above
- Review [Build Guide](building.md) for detailed build instructions
- File an issue on GitHub with your platform details
