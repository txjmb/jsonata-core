# JSONataPy Build Checklist

Use this checklist to verify your build is successful.

## 📋 Pre-Build Checklist

- [ ] **Rust installed**: Run `rustc --version`
- [ ] **Cargo installed**: Run `cargo --version`
- [ ] **Python 3.8+ installed**: Run `python --version` or `python3 --version`
- [ ] **maturin installed**: Run `pip install maturin` or `pip3 install maturin`
- [ ] **In project directory**: Run `pwd` or `cd` to verify

## 🔨 Build Steps

### Step 1: Check Compilation
```bash
cargo check
```

**Expected Output**:
```
    Checking jsonatapy v0.1.0
    Finished dev [unoptimized + debuginfo] target(s) in X.XXs
```

- [ ] ✅ Command completed successfully
- [ ] ✅ No compilation errors

**If Failed**: Read error messages carefully. Common issues:
- Missing dependencies in Cargo.toml
- Syntax errors in Rust code
- See BUILD_INSTRUCTIONS.md for troubleshooting

---

### Step 2: Run Rust Tests
```bash
cargo test
```

**Expected Output**:
```
running 85+ tests
test result: ok. 85 passed; 0 failed; 0 ignored
```

- [ ] ✅ All tests passed
- [ ] ✅ No test failures

**If Failed**: Check which tests failed and why. Tests are in:
- `src/parser.rs` - Parser tests
- `src/evaluator.rs` - Evaluator tests
- `src/functions.rs` - Function tests
- Other modules

---

### Step 3: Build Python Extension
```bash
maturin develop
```

**Expected Output**:
```
📦 Built wheel to target/wheels/jsonatapy-0.1.0-...whl
✅ Installed jsonatapy-0.1.0
```

- [ ] ✅ Wheel built successfully
- [ ] ✅ Extension installed

**If Failed**: Common issues:
- maturin not installed: `pip install maturin`
- Wrong Python version: Use Python 3.8+
- Compilation errors: Check cargo output

---

### Step 4: Verify Python Import
```bash
python -c "import jsonatapy; print(jsonatapy.__version__)"
```

**Expected Output**:
```
0.1.0
```

- [ ] ✅ Module imports successfully
- [ ] ✅ Version displays correctly

**If Failed**:
- Extension not installed: Re-run `maturin develop`
- Wrong Python: Make sure you're using the same Python maturin used

---

### Step 5: Run Python Tests
```bash
pip install pytest pytest-cov
pytest tests/python/ -v
```

**Expected Output**:
```
====== 100+ passed in X.XXs ======
```

- [ ] ✅ All tests passed
- [ ] ✅ No import errors

**If Failed**: Check specific test failures. Tests are in:
- `tests/python/test_basic.py` - Basic API tests
- `tests/python/test_integration.py` - Integration tests

---

### Step 6: Run Examples
```bash
python examples/basic_usage.py
```

**Expected Output**:
```
============================================================
JSONataPy - Basic Usage Examples
============================================================

1. Simple Literals
----------------------------------------
jsonatapy.evaluate("42", {}) = 42
...

============================================================
Examples complete!
============================================================
```

- [ ] ✅ Examples run without errors
- [ ] ✅ Output matches expected results

**If Failed**: Check error messages. Common issues:
- Module not installed
- Missing test data
- Python version compatibility

---

## 🎯 Success Criteria

If all steps above passed, you have:

- [x] ✅ Working Rust implementation
- [x] ✅ All Rust tests passing (85+)
- [x] ✅ Working Python extension
- [x] ✅ All Python tests passing (100+)
- [x] ✅ Working examples

**Congratulations! 🎉 Your JSONataPy build is successful!**

---

## 🚀 What's Next?

### For Development
```bash
# Make changes to Rust code
# Then rebuild
cargo check
cargo test
maturin develop
pytest tests/python/
```

### For Testing
```bash
# Run specific tests
cargo test test_name
pytest tests/python/test_integration.py::TestLiterals -v

# Run with coverage
cargo tarpaulin
pytest tests/python/ --cov=jsonatapy --cov-report=html
```

### For Production Build
```bash
# Build optimized wheels
maturin build --release

# Wheels will be in target/wheels/
ls target/wheels/
```

---

## 📊 Performance Validation

Test the performance improvement over JavaScript:

```python
import jsonatapy
import time

# Test data
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
print(f"Result: {result}")
```

Expected performance:
- **Development build**: ~0.1-0.5ms per evaluation
- **Release build**: ~0.01-0.1ms per evaluation

---

## 🔍 Verification Commands Summary

Run all checks in sequence:

### Quick Check (WSL/Linux/macOS)
```bash
cargo check && \
cargo test && \
maturin develop && \
python -c "import jsonatapy; print('Version:', jsonatapy.__version__)" && \
pytest tests/python/ -v && \
python examples/basic_usage.py
```

### Quick Check (Windows PowerShell)
```powershell
cargo check; if ($?) {
  cargo test; if ($?) {
    maturin develop; if ($?) {
      python -c "import jsonatapy; print('Version:', jsonatapy.__version__)"; if ($?) {
        pytest tests/python/ -v; if ($?) {
          python examples/basic_usage.py
        }
      }
    }
  }
}
```

---

## 📝 Troubleshooting

### Build Still Failing?

1. **Check error messages carefully** - They usually tell you exactly what's wrong
2. **See BUILD_INSTRUCTIONS.md** - Comprehensive troubleshooting guide
3. **Check CURRENT_STATUS.md** - Known limitations and issues
4. **Try WSL** - Often easier on Windows (see WSL_SETUP.md)

### Common Issues

| Issue | Solution |
|-------|----------|
| `cargo: command not found` | Install Rust from https://rustup.rs/ |
| `maturin: command not found` | Run `pip install maturin` |
| `Import Error: No module named jsonatapy` | Run `maturin develop` |
| Tests failing | Check specific error messages |
| Slow builds | Use `--release` for production builds |

---

## ✅ Final Verification

Run this to verify everything:

```bash
# Test the full API
python3 << 'EOF'
import jsonatapy

# Test compilation
expr = jsonatapy.compile("$sum(numbers)")
print("✓ Compilation works")

# Test evaluation
result = expr.evaluate({"numbers": [1, 2, 3, 4, 5]})
assert result == 15, f"Expected 15, got {result}"
print("✓ Evaluation works")

# Test functions
result = jsonatapy.evaluate('$uppercase("hello")', {})
assert result == "HELLO", f"Expected HELLO, got {result}"
print("✓ Functions work")

# Test path traversal
result = jsonatapy.evaluate("user.name", {"user": {"name": "Alice"}})
assert result == "Alice", f"Expected Alice, got {result}"
print("✓ Path traversal works")

print("\n🎉 All verifications passed! JSONataPy is working correctly!")
EOF
```

---

**Status**: Use this checklist every time you build
**Last Updated**: 2026-01-17
