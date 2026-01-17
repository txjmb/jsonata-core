#!/usr/bin/env python3
"""
Build verification script for jsonatapy

This script checks if all prerequisites are installed and attempts to build the project.
Run this before trying to build the extension.
"""

import subprocess
import sys
import os
from pathlib import Path


def run_command(cmd, description):
    """Run a command and return success status"""
    print(f"\n{'='*60}")
    print(f"Checking: {description}")
    print(f"{'='*60}")
    print(f"Command: {' '.join(cmd)}")

    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=5
        )

        if result.returncode == 0:
            print(f"✓ SUCCESS")
            if result.stdout.strip():
                print(f"Output: {result.stdout.strip()}")
            return True
        else:
            print(f"✗ FAILED")
            if result.stderr.strip():
                print(f"Error: {result.stderr.strip()}")
            return False

    except FileNotFoundError:
        print(f"✗ FAILED - Command not found")
        return False
    except subprocess.TimeoutExpired:
        print(f"✗ FAILED - Command timed out")
        return False
    except Exception as e:
        print(f"✗ FAILED - {e}")
        return False


def check_file_exists(filepath, description):
    """Check if a file exists"""
    print(f"\n{'='*60}")
    print(f"Checking: {description}")
    print(f"{'='*60}")
    print(f"File: {filepath}")

    if os.path.exists(filepath):
        print(f"✓ EXISTS")
        return True
    else:
        print(f"✗ MISSING")
        return False


def main():
    print("""
╔════════════════════════════════════════════════════════════╗
║         JSONataPy Build Verification Script                ║
╚════════════════════════════════════════════════════════════╝
""")

    results = {}

    # Check Python
    results['python'] = run_command(
        ['python', '--version'],
        'Python Installation'
    )

    # Check Rust
    results['rustc'] = run_command(
        ['rustc', '--version'],
        'Rust Compiler (rustc)'
    )

    results['cargo'] = run_command(
        ['cargo', '--version'],
        'Cargo (Rust Package Manager)'
    )

    # Check pip
    results['pip'] = run_command(
        ['pip', '--version'],
        'pip (Python Package Manager)'
    )

    # Check maturin
    results['maturin'] = run_command(
        ['maturin', '--version'],
        'Maturin (Rust-Python Bridge)'
    )

    # Check key source files
    print(f"\n{'='*60}")
    print("Checking Source Files")
    print(f"{'='*60}")

    source_files = [
        'src/lib.rs',
        'src/parser.rs',
        'src/evaluator.rs',
        'src/functions.rs',
        'src/ast.rs',
        'Cargo.toml',
        'pyproject.toml',
    ]

    files_ok = True
    for filepath in source_files:
        if not os.path.exists(filepath):
            print(f"✗ MISSING: {filepath}")
            files_ok = False
        else:
            print(f"✓ EXISTS: {filepath}")

    results['source_files'] = files_ok

    # Print summary
    print(f"\n{'='*60}")
    print("SUMMARY")
    print(f"{'='*60}\n")

    all_ok = True
    for check, passed in results.items():
        status = "✓ PASS" if passed else "✗ FAIL"
        print(f"{status:10} - {check}")
        if not passed:
            all_ok = False

    print(f"\n{'='*60}")

    if all_ok:
        print("✓ All checks passed! Ready to build.")
        print(f"{'='*60}")
        print("\nNext steps:")
        print("  1. Run: cargo check")
        print("  2. Run: cargo test")
        print("  3. Run: maturin develop")
        print("  4. Run: pytest tests/python/ -v")
        print("\nSee BUILD_INSTRUCTIONS.md for detailed instructions.")
        return 0
    else:
        print("✗ Some checks failed. Please fix the issues above.")
        print(f"{'='*60}")
        print("\nInstallation instructions:")

        if not results['rustc'] or not results['cargo']:
            print("\nInstall Rust:")
            print("  Windows: winget install Rustlang.Rustup")
            print("  Linux/macOS: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh")

        if not results['maturin']:
            print("\nInstall maturin:")
            print("  pip install maturin")

        if not results['source_files']:
            print("\nSource files are missing. Make sure you're in the project root directory.")

        print("\nSee BUILD_INSTRUCTIONS.md for detailed instructions.")
        return 1


if __name__ == '__main__':
    try:
        sys.exit(main())
    except KeyboardInterrupt:
        print("\n\nInterrupted by user.")
        sys.exit(1)
