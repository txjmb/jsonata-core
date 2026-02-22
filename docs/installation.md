# Installation

## Quick Install

```bash
pip install jsonatapy
```

Pre-built wheels available for:
- **Python**: 3.10, 3.11, 3.12, 3.13
- **Platforms**: Windows (x64), Linux (x86_64, aarch64), macOS (Intel, ARM)

## Verify Installation

```python
import jsonatapy

print(jsonatapy.__version__)  # 2.1.0

# Test
data = {"name": "World"}
result = jsonatapy.evaluate('"Hello, " & name', data)
print(result)  # "Hello, World"
```

## Building from Source

### Prerequisites

1. Install Rust: https://rustup.rs/
2. Install maturin: `pip install maturin`
3. Python 3.10+ with development headers

### Build

```bash
git clone https://github.com/txjmb/jsonata-core.git
cd jsonatapy
maturin develop --release
```

See [Build Guide](development/building.md) for details.

## Platform-Specific Notes

### Windows

Requires Visual C++ Redistributable:
https://aka.ms/vs/17/release/vc_redist.x64.exe

### macOS

Requires Xcode Command Line Tools:
```bash
xcode-select --install
```

### Linux

For building from source:
```bash
# Ubuntu/Debian
sudo apt-get install python3-dev build-essential

# Fedora/RHEL
sudo dnf install python3-devel gcc
```

## Troubleshooting

### ImportError: DLL load failed (Windows)
Install Visual C++ Redistributable (link above).

### ModuleNotFoundError: No module named '_jsonatapy'
```bash
pip uninstall jsonatapy
pip install jsonatapy
```

### Wheel not found for your platform
Build from source or file an issue on GitHub.
