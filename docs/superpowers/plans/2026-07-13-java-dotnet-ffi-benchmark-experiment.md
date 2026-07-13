# Java/.NET FFI Benchmark Experiment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Measure whether jsonata-core, consumed over a C-ABI FFI boundary, is significantly faster than dashjoin/jsonata-java and Jsonata.Net.Native — producing a report the user uses to make the merge decision.

**Architecture:** A minimal `capi` cargo feature exposes 6 `extern "C"` functions over the existing engine (JSON-as-UTF-8-strings both directions). A shared JSON corpus file (extracted from `benchmarks/python/benchmark.py`) drives a Java JMH harness (FFM wrapper vs dashjoin) and a .NET BenchmarkDotNet harness (`[LibraryImport]` wrapper vs Jsonata.Net.Native). A correctness gate runs before timing in each language; a Python report script merges all results into one markdown report.

**Tech Stack:** Rust (existing crate), Java 25 (Temurin, FFM final API), Maven 3.9 + JMH 1.37 + Jackson 2.17.2 + `com.dashjoin:jsonata` 0.9.10, .NET SDK (LTS channel) targeting net8.0 + BenchmarkDotNet 0.15.8 + `Jsonata.Net.Native` 3.0.0, Python 3 stdlib + jsonatapy for the report.

**Spec:** `docs/superpowers/specs/2026-07-13-java-dotnet-ffi-benchmark-experiment-design.md`

## Global Constraints

- All work happens on branch `experiment/java-dotnet-bindings`. Nothing merges to `main` in this plan.
- All Rust changes are additive behind the `capi` feature; existing suites must stay green (`cargo test` = 137+ tests, `uv run pytest tests/python/test_reference_suite.py` = 1258 tests — run both in the final task).
- Toolchains install user-local ONLY: JDK + Maven under `~/.local/toolchains/`, .NET under `~/.dotnet`. Never sudo, never system package managers, never edit shell rc files — export env vars inline in each command instead.
- Pinned versions (from verified 2026-07-13 research): `com.dashjoin:jsonata` 0.9.10, `Jsonata.Net.Native` 3.0.0, JMH 1.37, BenchmarkDotNet 0.15.8, Jackson 2.17.2, JUnit 5.10.2, Temurin JDK 25, Maven 3.9.11 (fall back to any 3.9.x from archive.apache.org if dlcdn 404s), .NET SDK via `--channel LTS`, project TFM `net8.0`.
- Java FFM: Java 22+ final API names (`allocateFrom`, `getString`) — NOT the Java 21 preview names (`allocateUtf8String`, `getUtf8String`). Always pass `--enable-native-access=ALL-UNNAMED` (Java 24+ restricted-method warnings).
- .NET: returned `char*` that our library owns MUST cross as `IntPtr` + `Marshal.PtrToStringUTF8` + `jsonata_free_string`. Never declare a native-owned `string` return with `StringMarshalling.Utf8` (its unmarshaller calls `Marshal.FreeCoTaskMem` — wrong allocator, UB on Linux).
- The shared library under test is always `target/release/libjsonata_core.so`, built with `cargo build --release --features capi`, passed to harnesses via `JSONATA_CORE_LIB` env var (or `-Djsonata.core.lib` sysprop in Java).
- Engine is `Rc`-based / non-`Send`: one `JsonataExpr*` per thread. Both harnesses stay single-threaded.
- Undefined-result convention across the whole stack: `jsonata_evaluate` returns NULL with the thread-local error slot EMPTY. Wrappers surface this as Java `null` / C# `null`. NULL with a non-empty error slot is an error.
- Commit after every task. Commit messages end with:
  `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`

---

### Task 1: C ABI (`src/capi.rs` behind `capi` feature)

**Files:**
- Modify: `Cargo.toml` (add `capi = []` to `[features]`)
- Modify: `src/lib.rs` (add `#[cfg(feature = "capi")] pub mod capi;` after the existing `mod vm;` line)
- Create: `src/capi.rs` (implementation + inline `#[cfg(test)]` tests)

**Interfaces:**
- Consumes: `crate::parser::parse`, `crate::evaluator::{try_compile_expr, Evaluator, Context, EvaluatorOptions}`, `crate::compiler::BytecodeCompiler`, `crate::vm::Vm`, `crate::value::JValue` (`from_json_str`, `to_json_string`, `is_undefined`). Mirrors the `run_eval` pattern in `src/lib.rs:180-206`.
- Produces (the C surface every later task links against):
  ```c
  JsonataExpr* jsonata_compile(const char* expr_utf8);        // NULL on error
  char*        jsonata_evaluate(JsonataExpr*, const char* json_utf8); // NULL: error OR undefined (error slot empty)
  void         jsonata_free_expr(JsonataExpr*);
  void         jsonata_free_string(char*);
  char*        jsonata_last_error_message(void);              // NULL if no error; caller frees
  const char*  jsonata_version(void);                         // static, never freed
  ```

- [ ] **Step 1: Feature + module wiring + write the module with tests first**

In `Cargo.toml` `[features]` add:

```toml
capi = []
```

In `src/lib.rs`, directly after `mod vm;`:

```rust
#[cfg(feature = "capi")]
pub mod capi;
```

Create `src/capi.rs`:

```rust
//! Minimal C ABI over the engine (spike scope — see
//! docs/superpowers/specs/2026-07-13-java-dotnet-ffi-benchmark-experiment-design.md).
//!
//! JSON crosses the boundary as UTF-8 C strings in both directions. Errors go
//! through a thread-local slot: a NULL return from `jsonata_evaluate` with an
//! EMPTY slot means the JSONata result was undefined (not an error). Handles
//! are not Send — one `JsonataExpr*` per thread.

use std::cell::{OnceCell, RefCell};
use std::ffi::{c_char, CStr, CString};

use crate::evaluator::{self, EvaluatorOptions};
use crate::value::JValue;
use crate::{compiler, parser, vm};

pub struct JsonataExpr {
    ast: crate::ast::AstNode,
    bytecode: OnceCell<Option<vm::BytecodeProgram>>,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error(msg: String) {
    let c = CString::new(msg).unwrap_or_else(|_| CString::new("error message contained NUL").unwrap());
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
}

fn clear_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

/// # Safety
/// `expr_utf8` must be a valid NUL-terminated C string pointer or NULL.
#[no_mangle]
pub unsafe extern "C" fn jsonata_compile(expr_utf8: *const c_char) -> *mut JsonataExpr {
    if expr_utf8.is_null() {
        set_error("expression pointer is NULL".to_string());
        return std::ptr::null_mut();
    }
    let expr = match CStr::from_ptr(expr_utf8).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_error("expression is not valid UTF-8".to_string());
            return std::ptr::null_mut();
        }
    };
    match parser::parse(expr) {
        Ok(ast) => {
            clear_error();
            Box::into_raw(Box::new(JsonataExpr { ast, bytecode: OnceCell::new() }))
        }
        Err(e) => {
            set_error(e.display_message());
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `expr` must be a pointer returned by `jsonata_compile` (not yet freed);
/// `json_utf8` must be a valid NUL-terminated C string pointer or NULL.
/// Returned string must be released with `jsonata_free_string`.
#[no_mangle]
pub unsafe extern "C" fn jsonata_evaluate(
    expr: *mut JsonataExpr,
    json_utf8: *const c_char,
) -> *mut c_char {
    if expr.is_null() {
        set_error("expression handle is NULL".to_string());
        return std::ptr::null_mut();
    }
    if json_utf8.is_null() {
        set_error("input JSON pointer is NULL".to_string());
        return std::ptr::null_mut();
    }
    let expr = &*expr;
    let json = match CStr::from_ptr(json_utf8).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_error("input JSON is not valid UTF-8".to_string());
            return std::ptr::null_mut();
        }
    };
    let data = match JValue::from_json_str(json) {
        Ok(v) => v,
        Err(e) => {
            set_error(format!("invalid input JSON: {}", e));
            return std::ptr::null_mut();
        }
    };
    // Same pattern as JsonataExpression::run_eval in lib.rs: VM when the
    // expression compiles to bytecode, tree-walker otherwise.
    let bytecode = expr.bytecode.get_or_init(|| {
        evaluator::try_compile_expr(&expr.ast).map(|ce| compiler::BytecodeCompiler::compile(&ce))
    });
    let result = if let Some(bc) = bytecode {
        vm::Vm::with_options(bc, EvaluatorOptions::default()).run(&data, None)
    } else {
        let mut ev = evaluator::Evaluator::with_options(
            evaluator::Context::new(),
            EvaluatorOptions::default(),
        );
        ev.evaluate(&expr.ast, &data)
    };
    match result {
        Ok(v) => {
            clear_error();
            if v.is_undefined() {
                return std::ptr::null_mut(); // undefined: NULL + empty error slot
            }
            match v.to_json_string() {
                Ok(s) => match CString::new(s) {
                    Ok(c) => c.into_raw(),
                    Err(_) => {
                        set_error("result contained interior NUL".to_string());
                        std::ptr::null_mut()
                    }
                },
                Err(e) => {
                    set_error(format!("could not serialize result: {}", e));
                    std::ptr::null_mut()
                }
            }
        }
        Err(e) => {
            set_error(e.message().to_string());
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `expr` must be NULL or a pointer returned by `jsonata_compile`, freed at most once.
#[no_mangle]
pub unsafe extern "C" fn jsonata_free_expr(expr: *mut JsonataExpr) {
    if !expr.is_null() {
        drop(Box::from_raw(expr));
    }
}

/// # Safety
/// `s` must be NULL or a pointer returned by this library, freed at most once.
#[no_mangle]
pub unsafe extern "C" fn jsonata_free_string(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

/// Returns a copy of the thread-local error message (caller frees with
/// `jsonata_free_string`), or NULL if the slot is empty.
#[no_mangle]
pub extern "C" fn jsonata_last_error_message() -> *mut c_char {
    LAST_ERROR.with(|e| match &*e.borrow() {
        Some(c) => c.clone().into_raw(),
        None => std::ptr::null_mut(),
    })
}

#[no_mangle]
pub extern "C" fn jsonata_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn eval_str(expr: &str, data: &str) -> Option<String> {
        let ce = CString::new(expr).unwrap();
        let cd = CString::new(data).unwrap();
        let h = jsonata_compile(ce.as_ptr());
        assert!(!h.is_null(), "compile failed: {:?}", last_error());
        let r = jsonata_evaluate(h, cd.as_ptr());
        let out = if r.is_null() {
            None
        } else {
            let s = CStr::from_ptr(r).to_str().unwrap().to_string();
            jsonata_free_string(r);
            Some(s)
        };
        jsonata_free_expr(h);
        out
    }

    fn last_error() -> Option<String> {
        let p = jsonata_last_error_message();
        if p.is_null() {
            return None;
        }
        unsafe {
            let s = CStr::from_ptr(p).to_str().unwrap().to_string();
            jsonata_free_string(p);
            Some(s)
        }
    }

    #[test]
    fn round_trip_simple_path() {
        let out = unsafe { eval_str("user.name", r#"{"user":{"name":"Alice"}}"#) };
        assert_eq!(out.as_deref(), Some(r#""Alice""#));
    }

    #[test]
    fn round_trip_object_result() {
        let out = unsafe { eval_str(r#"{"n": a + b}"#, r#"{"a":1,"b":2}"#) };
        assert_eq!(out.as_deref(), Some(r#"{"n":3}"#));
    }

    #[test]
    fn undefined_result_is_null_with_empty_error() {
        let out = unsafe { eval_str("missing.path", r#"{"a":1}"#) };
        assert_eq!(out, None);
        assert_eq!(last_error(), None);
    }

    #[test]
    fn parse_error_sets_message() {
        let ce = CString::new("a.b[").unwrap();
        let h = unsafe { jsonata_compile(ce.as_ptr()) };
        assert!(h.is_null());
        let msg = last_error().expect("parse error should set message");
        assert!(!msg.is_empty());
    }

    #[test]
    fn eval_error_sets_message() {
        // "a" + string is a type error (T2002-family) at evaluation time
        let ce = CString::new(r#"a + b"#).unwrap();
        let cd = CString::new(r#"{"a":1,"b":"x"}"#).unwrap();
        let h = unsafe { jsonata_compile(ce.as_ptr()) };
        assert!(!h.is_null());
        let r = unsafe { jsonata_evaluate(h, cd.as_ptr()) };
        assert!(r.is_null());
        let msg = last_error().expect("eval error should set message");
        assert!(!msg.is_empty());
        unsafe { jsonata_free_expr(h) };
    }

    #[test]
    fn invalid_input_json_sets_message() {
        let out_err = unsafe {
            let ce = CString::new("a").unwrap();
            let cd = CString::new("{not json").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(r.is_null());
            jsonata_free_expr(h);
            last_error()
        };
        assert!(out_err.unwrap().contains("invalid input JSON"));
    }

    #[test]
    fn multibyte_utf8_round_trip() {
        let out = unsafe { eval_str("$uppercase(name)", r#"{"name":"héllo wörld ✓ 日本語"}"#) };
        assert_eq!(out.as_deref(), Some(r#""HÉLLO WÖRLD ✓ 日本語""#));
    }

    #[test]
    fn version_is_crate_version() {
        let v = unsafe { CStr::from_ptr(jsonata_version()).to_str().unwrap() };
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }
}
```

- [ ] **Step 2: Run the capi tests**

Run: `cargo test --features capi capi::`
Expected: all 8 tests PASS. (If `$uppercase` output ordering or number formatting differs, fix the test's expected literal to the engine's actual output — the engine is the oracle here, these tests pin the FFI plumbing, not JSONata semantics.)

- [ ] **Step 3: Verify no regressions and that symbols export**

Run: `cargo test` (without the feature)
Expected: passes exactly as before (capi module not compiled).

Run: `cargo build --release --features capi && nm -D target/release/libjsonata_core.so | grep -c ' T jsonata_'`
Expected: `6`

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml src/lib.rs src/capi.rs
git commit -m "feat(capi): minimal C ABI behind capi feature (spike)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Shared benchmark corpus (`benchmarks/corpus/`)

**Files:**
- Create: `benchmarks/corpus/generate_corpus.py`
- Create: `benchmarks/corpus/corpus.json` (generated, committed)

**Interfaces:**
- Produces: `benchmarks/corpus/corpus.json` — a JSON array of 33 objects `{"name": str, "category": str, "expression": str, "data": <json>, "iterations": int}` (iterations kept for provenance only; JMH/BDN control their own timing). Java's `Corpus.load` (Task 3) and .NET's `CorpusFile.Load` (Task 6) both read this file. Scenario `name` values are unique and used as benchmark params everywhere.

- [ ] **Step 1: Write the generator**

The scenario set mirrors Parts 1–7 of `benchmarks/python/benchmark.py:1146-1546` exactly (Part 8 is jsonatapy-internal, excluded). Duplication is deliberate spike scope — a comment points back at the source.

Create `benchmarks/corpus/generate_corpus.py`:

```python
#!/usr/bin/env python3
"""Emit corpus.json: the shared expression/data corpus for the Java/.NET FFI
benchmark experiment.

Scenario definitions mirror benchmarks/python/benchmark.py main() Parts 1-7
(Part 8, "Evaluation Path Comparison", is jsonatapy-internal and excluded).
If benchmark.py's scenarios change, re-derive this file from it.

Usage: python3 generate_corpus.py   (writes corpus.json next to itself)
"""

import json
from pathlib import Path


def scenarios() -> list[dict]:
    array_100 = {"values": list(range(100))}
    array_1000 = {"values": list(range(1000))}
    array_10000 = {"values": list(range(10000))}
    products_100 = {
        "products": [
            {"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5, "inStock": i % 2 == 0}
            for i in range(100)
        ]
    }
    deep_data = {
        "a": {"b": {"c": {"d": {"e": {"f": {"g": {"h": {"i": {"j": {"k": {"l": 42}}}}}}}}}}}
    }
    nested_arrays = {
        "data": [[[[i, i + 1, i + 2] for i in range(0, 30, 3)] for _ in range(3)] for _ in range(3)]
    }
    numbers_data = {"numbers": list(range(1, 101))}
    ecommerce_data = {
        "products": [
            {
                "id": i,
                "name": f"Product {i}",
                "category": ["Electronics", "Clothing", "Books", "Home"][i % 4],
                "price": 10.0 + i * 5.5,
                "inStock": i % 3 != 0,
                "rating": 3.0 + (i % 3) * 0.5,
                "reviews": i * 2,
                "tags": [f"tag{j}" for j in range(i % 5)],
                "vendor": {"name": f"Vendor {i % 10}", "rating": 4.0 + (i % 5) * 0.2},
            }
            for i in range(100)
        ]
    }
    group_by_expression = """
            {
                "Electronics": $sum(products[category = "Electronics"].price),
                "Clothing": $sum(products[category = "Clothing"].price),
                "Books": $sum(products[category = "Books"].price),
                "Home": $sum(products[category = "Home"].price)
            }
        """

    def s(name, category, expression, data, iterations):
        return {
            "name": name,
            "category": category,
            "expression": expression,
            "data": data,
            "iterations": iterations,
        }

    return [
        # Part 1: Simple Paths
        s("Simple Path", "Simple Paths", "user.name",
          {"user": {"name": "Alice", "age": 30}}, 10000),
        s("Deep Path (5 levels)", "Simple Paths", "a.b.c.d.e",
          {"a": {"b": {"c": {"d": {"e": 42}}}}}, 10000),
        s("Array Index Access", "Simple Paths", "values[50]",
          {"values": list(range(100))}, 5000),
        s("Arithmetic Expression", "Simple Paths", "price * quantity",
          {"price": 10.5, "quantity": 3}, 10000),
        # Part 2: Array Operations
        s("Array Sum (100 elements)", "Array Operations", "$sum(values)", array_100, 1000),
        s("Array Max (100 elements)", "Array Operations", "$max(values)", array_100, 1000),
        s("Array Count (100 elements)", "Array Operations", "$count(values)", array_100, 2000),
        s("Array Sum (1000 elements)", "Array Operations", "$sum(values)", array_1000, 200),
        s("Array Max (1000 elements)", "Array Operations", "$max(values)", array_1000, 200),
        s("Array Sum (10000 elements)", "Array Operations", "$sum(values)", array_10000, 50),
        s("Array Mapping (extract field)", "Array Operations", "products.price",
          products_100, 1000),
        s("Array Mapping + Sum", "Array Operations", "$sum(products.price)",
          products_100, 1000),
        s("Array Filtering (predicate)", "Array Operations", "products[price > 100]",
          products_100, 500),
        # Part 3: Complex Transformations
        s("Object Construction (simple)", "Complex Transformations",
          '{"fullName": first & " " & last, "age": age}',
          {"first": "John", "last": "Doe", "age": 30}, 5000),
        s("Object Construction (nested)", "Complex Transformations",
          '{"user": {"name": name, "contact": {"email": email, "phone": phone}}}',
          {"name": "Alice", "email": "alice@example.com", "phone": "555-1234"}, 5000),
        s("Conditional Expression", "Complex Transformations",
          'age >= 18 ? "adult" : "minor"', {"age": 25}, 5000),
        s("Multiple Nested Functions", "Complex Transformations",
          "$length($uppercase(name))", {"name": "JSONata Performance Test"}, 5000),
        # Part 4: Deep Nesting
        s("Deep Path (12 levels)", "Deep Nesting", "a.b.c.d.e.f.g.h.i.j.k.l",
          deep_data, 5000),
        s("Nested Array Access", "Deep Nesting", "data[1][1][1][1]", nested_arrays, 2000),
        # Part 5: String Operations
        s("String Uppercase", "String Operations", "$uppercase(name)",
          {"name": "hello world"}, 10000),
        s("String Lowercase", "String Operations", "$lowercase(name)",
          {"name": "HELLO WORLD"}, 10000),
        s("String Length", "String Operations", "$length(name)",
          {"name": "JSONata Performance Benchmark Suite"}, 10000),
        s("String Concatenation", "String Operations", '$join([first, last], " ")',
          {"first": "John", "last": "Doe"}, 5000),
        s("String Substring", "String Operations", "$substring(text, 0, 10)",
          {"text": "This is a long string that we will extract a substring from"}, 5000),
        s("String Contains", "String Operations", '$contains(text, "JSONata")',
          {"text": "JSONata is a query and transformation language for JSON"}, 5000),
        # Part 6: Higher-Order Functions
        s("$map with lambda", "Higher-Order Functions",
          "$map(numbers, function($v) { $v * 2 })", numbers_data, 200),
        s("$filter with lambda", "Higher-Order Functions",
          "$filter(numbers, function($v) { $v > 50 })", numbers_data, 200),
        s("$reduce with lambda", "Higher-Order Functions",
          "$reduce(numbers, function($acc, $v) { $acc + $v }, 0)", numbers_data, 200),
        # Part 7: Realistic Workload (E-Commerce)
        s("Filter by category", "Realistic Workload",
          'products[category = "Electronics"]', ecommerce_data, 500),
        s("Calculate total value", "Realistic Workload",
          "$sum(products[inStock].price)", ecommerce_data, 500),
        s("Complex transformation", "Realistic Workload",
          'products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}',
          ecommerce_data, 200),
        s("Group by category (aggregate)", "Realistic Workload",
          group_by_expression, ecommerce_data, 200),
        s("Top rated products", "Realistic Workload",
          "$sort(products[rating >= 4], function($l, $r) { $r.rating - $l.rating })",
          ecommerce_data, 100),
    ]


def main() -> None:
    out = Path(__file__).parent / "corpus.json"
    items = scenarios()
    names = [x["name"] for x in items]
    assert len(names) == len(set(names)), "scenario names must be unique"
    assert len(items) == 33, f"expected 33 scenarios, got {len(items)}"
    out.write_text(json.dumps(items, indent=2) + "\n")
    print(f"wrote {out} ({len(items)} scenarios)")


if __name__ == "__main__":
    main()
```

- [ ] **Step 2: Generate and sanity-check**

Run: `python3 benchmarks/corpus/generate_corpus.py`
Expected: `wrote .../corpus.json (33 scenarios)`

Run: `python3 -c "import json; d=json.load(open('benchmarks/corpus/corpus.json')); print(len(d), d[0]['name'], d[-1]['name'])"`
Expected: `33 Simple Path Top rated products`

- [ ] **Step 3: Commit**

```bash
git add benchmarks/corpus/generate_corpus.py benchmarks/corpus/corpus.json
git commit -m "feat(benchmarks): shared FFI-experiment corpus (33 scenarios from benchmark.py parts 1-7)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Java toolchain + Maven project + FFM wrapper + smoke test

**Files:**
- Create: `benchmarks/java/pom.xml`
- Create: `benchmarks/java/.gitignore` (containing `target/`)
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/JsonataException.java`
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/JsonataCore.java`
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/Corpus.java`
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/JsonCompare.java`
- Test: `benchmarks/java/src/test/java/dev/jsonatapy/bench/SmokeTest.java`

**Interfaces:**
- Consumes: `target/release/libjsonata_core.so` (Task 1), sysprop `jsonata.core.lib` or env `JSONATA_CORE_LIB`.
- Produces (used by Tasks 4–5):
  - `JsonataCore.compile(String) -> JsonataCore` (throws `JsonataException`), instance `String evaluate(String dataJson)` (returns `null` for undefined), `void close()`, static `String version()`.
  - `Corpus.Scenario` record `(String name, String category, String expression, JsonNode data)`; `static List<Corpus.Scenario> load(Path)`.
  - `JsonCompare.semanticEquals(JsonNode a, JsonNode b) -> boolean` (numbers compared as double with tolerance `1e-9 * max(1,|a|,|b|)`; objects key-order-insensitive; arrays ordered).

- [ ] **Step 1: Install Temurin 25 + Maven 3.9.11 user-locally (skip any part already present)**

```bash
mkdir -p ~/.local/toolchains && cd ~/.local/toolchains
curl -fsSL -o temurin25.tar.gz \
  "https://api.adoptium.net/v3/binary/latest/25/ga/linux/x64/jdk/hotspot/normal/eclipse"
tar xzf temurin25.tar.gz && rm temurin25.tar.gz
curl -fsSL -o maven.tar.gz \
  "https://dlcdn.apache.org/maven/maven-3/3.9.11/binaries/apache-maven-3.9.11-bin.tar.gz" \
  || curl -fsSL -o maven.tar.gz \
  "https://archive.apache.org/dist/maven/maven-3/3.9.11/binaries/apache-maven-3.9.11-bin.tar.gz"
tar xzf maven.tar.gz && rm maven.tar.gz
ls -d ~/.local/toolchains/jdk-25* ~/.local/toolchains/apache-maven-3.9.11
```

Verify (this env pattern is used by every Java command below — call it **JAVA ENV**):

```bash
export JAVA_HOME=$(ls -d ~/.local/toolchains/jdk-25*)
export PATH=$JAVA_HOME/bin:~/.local/toolchains/apache-maven-3.9.11/bin:$PATH
java -version && mvn -version
```

Expected: `openjdk version "25...` and `Apache Maven 3.9.11`.

- [ ] **Step 2: Write pom.xml**

`benchmarks/java/pom.xml`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<project xmlns="http://maven.apache.org/POM/4.0.0"
         xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
         xsi:schemaLocation="http://maven.apache.org/POM/4.0.0 http://maven.apache.org/xsd/maven-4.0.0.xsd">
  <modelVersion>4.0.0</modelVersion>
  <groupId>dev.jsonatapy</groupId>
  <artifactId>jsonata-ffi-bench</artifactId>
  <version>0.1.0-SNAPSHOT</version>
  <packaging>jar</packaging>

  <properties>
    <maven.compiler.release>25</maven.compiler.release>
    <project.build.sourceEncoding>UTF-8</project.build.sourceEncoding>
    <jmh.version>1.37</jmh.version>
    <!-- overridden on the command line: -Djsonata.core.lib=/abs/path/libjsonata_core.so -->
    <jsonata.core.lib></jsonata.core.lib>
  </properties>

  <dependencies>
    <dependency>
      <groupId>com.dashjoin</groupId>
      <artifactId>jsonata</artifactId>
      <version>0.9.10</version>
    </dependency>
    <dependency>
      <groupId>com.fasterxml.jackson.core</groupId>
      <artifactId>jackson-databind</artifactId>
      <version>2.17.2</version>
    </dependency>
    <dependency>
      <groupId>org.openjdk.jmh</groupId>
      <artifactId>jmh-core</artifactId>
      <version>${jmh.version}</version>
    </dependency>
    <dependency>
      <groupId>org.openjdk.jmh</groupId>
      <artifactId>jmh-generator-annprocess</artifactId>
      <version>${jmh.version}</version>
      <scope>provided</scope>
    </dependency>
    <dependency>
      <groupId>org.junit.jupiter</groupId>
      <artifactId>junit-jupiter</artifactId>
      <version>5.10.2</version>
      <scope>test</scope>
    </dependency>
  </dependencies>

  <build>
    <plugins>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-surefire-plugin</artifactId>
        <version>3.2.5</version>
        <configuration>
          <argLine>--enable-native-access=ALL-UNNAMED -Djsonata.core.lib=${jsonata.core.lib}</argLine>
        </configuration>
      </plugin>
      <plugin>
        <groupId>org.apache.maven.plugins</groupId>
        <artifactId>maven-shade-plugin</artifactId>
        <version>3.5.1</version>
        <executions>
          <execution>
            <phase>package</phase>
            <goals><goal>shade</goal></goals>
            <configuration>
              <finalName>benchmarks</finalName>
              <transformers>
                <transformer implementation="org.apache.maven.plugins.shade.resource.ManifestResourceTransformer">
                  <mainClass>org.openjdk.jmh.Main</mainClass>
                </transformer>
              </transformers>
              <filters>
                <filter>
                  <artifact>*:*</artifact>
                  <excludes>
                    <exclude>META-INF/*.SF</exclude>
                    <exclude>META-INF/*.DSA</exclude>
                    <exclude>META-INF/*.RSA</exclude>
                  </excludes>
                </filter>
              </filters>
            </configuration>
          </execution>
        </executions>
      </plugin>
    </plugins>
  </build>
</project>
```

- [ ] **Step 3: Write the wrapper + helpers**

`benchmarks/java/src/main/java/dev/jsonatapy/bench/JsonataException.java`:

```java
package dev.jsonatapy.bench;

public class JsonataException extends RuntimeException {
    public JsonataException(String message) {
        super(message);
    }
}
```

`benchmarks/java/src/main/java/dev/jsonatapy/bench/JsonataCore.java`:

```java
package dev.jsonatapy.bench;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.file.Path;

/**
 * FFM wrapper over the jsonata-core C ABI (spike scope: loads the .so from
 * -Djsonata.core.lib or $JSONATA_CORE_LIB; no resource bundling).
 * Handles are not thread-safe: one instance per thread.
 */
public final class JsonataCore implements AutoCloseable {
    private static final Linker LINKER = Linker.nativeLinker();
    private static final MethodHandle COMPILE;
    private static final MethodHandle EVALUATE;
    private static final MethodHandle FREE_EXPR;
    private static final MethodHandle FREE_STRING;
    private static final MethodHandle LAST_ERROR;
    private static final MethodHandle VERSION;

    static {
        String libPath = System.getProperty("jsonata.core.lib");
        if (libPath == null || libPath.isEmpty()) {
            libPath = System.getenv("JSONATA_CORE_LIB");
        }
        if (libPath == null || libPath.isEmpty()) {
            throw new IllegalStateException(
                    "Set -Djsonata.core.lib=/path/to/libjsonata_core.so or $JSONATA_CORE_LIB");
        }
        SymbolLookup lib = SymbolLookup.libraryLookup(Path.of(libPath), Arena.global());
        COMPILE = LINKER.downcallHandle(lib.find("jsonata_compile").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        EVALUATE = LINKER.downcallHandle(lib.find("jsonata_evaluate").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.ADDRESS, ValueLayout.ADDRESS));
        FREE_EXPR = LINKER.downcallHandle(lib.find("jsonata_free_expr").orElseThrow(),
                FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
        FREE_STRING = LINKER.downcallHandle(lib.find("jsonata_free_string").orElseThrow(),
                FunctionDescriptor.ofVoid(ValueLayout.ADDRESS));
        LAST_ERROR = LINKER.downcallHandle(lib.find("jsonata_last_error_message").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
        VERSION = LINKER.downcallHandle(lib.find("jsonata_version").orElseThrow(),
                FunctionDescriptor.of(ValueLayout.ADDRESS));
    }

    private MemorySegment handle;

    private JsonataCore(MemorySegment handle) {
        this.handle = handle;
    }

    public static JsonataCore compile(String expression) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment h = (MemorySegment) COMPILE.invokeExact(arena.allocateFrom(expression));
            if (h.equals(MemorySegment.NULL)) {
                throw new JsonataException(String.valueOf(takeLastError()));
            }
            return new JsonataCore(h);
        } catch (JsonataException e) {
            throw e;
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    /** Result JSON text, or {@code null} when the JSONata result is undefined. */
    public String evaluate(String dataJson) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment r = (MemorySegment) EVALUATE.invokeExact(handle, arena.allocateFrom(dataJson));
            if (r.equals(MemorySegment.NULL)) {
                String err = takeLastError();
                if (err == null) {
                    return null; // undefined
                }
                throw new JsonataException(err);
            }
            return readAndFree(r);
        } catch (JsonataException e) {
            throw e;
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    public static String version() {
        try {
            MemorySegment v = (MemorySegment) VERSION.invokeExact();
            return v.reinterpret(Long.MAX_VALUE).getString(0); // static string: do NOT free
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    private static String takeLastError() {
        try {
            MemorySegment p = (MemorySegment) LAST_ERROR.invokeExact();
            if (p.equals(MemorySegment.NULL)) {
                return null;
            }
            return readAndFree(p);
        } catch (Throwable t) {
            throw new RuntimeException(t);
        }
    }

    private static String readAndFree(MemorySegment cstr) throws Throwable {
        String s = cstr.reinterpret(Long.MAX_VALUE).getString(0);
        FREE_STRING.invokeExact(cstr);
        return s;
    }

    @Override
    public void close() {
        if (handle != null) {
            try {
                FREE_EXPR.invokeExact(handle);
            } catch (Throwable t) {
                throw new RuntimeException(t);
            }
            handle = null;
        }
    }
}
```

`benchmarks/java/src/main/java/dev/jsonatapy/bench/Corpus.java`:

```java
package dev.jsonatapy.bench;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

public final class Corpus {
    public record Scenario(String name, String category, String expression, JsonNode data) {}

    private Corpus() {}

    public static List<Scenario> load(Path corpusJson) throws IOException {
        ObjectMapper mapper = new ObjectMapper();
        JsonNode root = mapper.readTree(corpusJson.toFile());
        List<Scenario> out = new ArrayList<>();
        for (JsonNode n : root) {
            out.add(new Scenario(
                    n.get("name").asText(),
                    n.get("category").asText(),
                    n.get("expression").asText(),
                    n.get("data")));
        }
        return out;
    }
}
```

`benchmarks/java/src/main/java/dev/jsonatapy/bench/JsonCompare.java`:

```java
package dev.jsonatapy.bench;

import com.fasterxml.jackson.databind.JsonNode;
import java.util.HashSet;
import java.util.Iterator;
import java.util.Set;

/**
 * Semantic JSON equality: numbers compared as double with relative tolerance
 * (JSONata numbers are IEEE doubles; implementations differ on int-vs-double
 * node types), object key order ignored, array order significant.
 */
public final class JsonCompare {
    private JsonCompare() {}

    public static boolean semanticEquals(JsonNode a, JsonNode b) {
        if (a == null || b == null) {
            return a == b;
        }
        if (a.isNumber() && b.isNumber()) {
            double x = a.doubleValue();
            double y = b.doubleValue();
            return Math.abs(x - y) <= 1e-9 * Math.max(1.0, Math.max(Math.abs(x), Math.abs(y)));
        }
        if (a.isArray() && b.isArray()) {
            if (a.size() != b.size()) {
                return false;
            }
            for (int i = 0; i < a.size(); i++) {
                if (!semanticEquals(a.get(i), b.get(i))) {
                    return false;
                }
            }
            return true;
        }
        if (a.isObject() && b.isObject()) {
            if (a.size() != b.size()) {
                return false;
            }
            Set<String> keys = new HashSet<>();
            for (Iterator<String> it = a.fieldNames(); it.hasNext(); ) {
                keys.add(it.next());
            }
            for (String k : keys) {
                if (!b.has(k) || !semanticEquals(a.get(k), b.get(k))) {
                    return false;
                }
            }
            return true;
        }
        return a.equals(b);
    }
}
```

- [ ] **Step 4: Write the smoke test**

`benchmarks/java/src/test/java/dev/jsonatapy/bench/SmokeTest.java`:

```java
package dev.jsonatapy.bench;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class SmokeTest {

    @Test
    void versionMatchesCrate() {
        String v = JsonataCore.version();
        assertNotNull(v);
        assertEquals("2.2.4", v);
    }

    @Test
    void simplePath() {
        try (JsonataCore e = JsonataCore.compile("user.name")) {
            assertEquals("\"Alice\"", e.evaluate("{\"user\":{\"name\":\"Alice\"}}"));
        }
    }

    @Test
    void objectResult() {
        try (JsonataCore e = JsonataCore.compile("{\"n\": a + b}")) {
            assertEquals("{\"n\":3}", e.evaluate("{\"a\":1,\"b\":2}"));
        }
    }

    @Test
    void undefinedIsNull() {
        try (JsonataCore e = JsonataCore.compile("missing.path")) {
            assertNull(e.evaluate("{\"a\":1}"));
        }
    }

    @Test
    void parseErrorThrows() {
        assertThrows(JsonataException.class, () -> JsonataCore.compile("a.b["));
    }

    @Test
    void evalErrorThrows() {
        try (JsonataCore e = JsonataCore.compile("a + b")) {
            assertThrows(JsonataException.class, () -> e.evaluate("{\"a\":1,\"b\":\"x\"}"));
        }
    }

    @Test
    void multibyteUtf8() {
        try (JsonataCore e = JsonataCore.compile("$uppercase(name)")) {
            assertEquals("\"HÉLLO ✓ 日本語\"", e.evaluate("{\"name\":\"héllo ✓ 日本語\"}"));
        }
    }
}
```

- [ ] **Step 5: Build and run the smoke test**

(Uses **JAVA ENV** from Step 1.)

```bash
cd benchmarks/java
mvn -q test -Djsonata.core.lib=$HOME/source/jsonatapy/target/release/libjsonata_core.so
```

Expected: `BUILD SUCCESS`, 7 tests pass. (If the `version` assertion fails because the crate version moved past 2.2.4, update the assertion to the current `CARGO_PKG_VERSION` — it pins "wrapper reads the real library", not a specific number.)

- [ ] **Step 6: Commit**

```bash
git add benchmarks/java
git commit -m "feat(benchmarks/java): FFM wrapper over capi + smoke tests (spike)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: Java correctness gate

**Files:**
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/CorrectnessGate.java`
- Create (generated, committed): `benchmarks/java/gate_results.json`

**Interfaces:**
- Consumes: `JsonataCore`, `Corpus`, `JsonCompare` (Task 3); `com.dashjoin.jsonata.Jsonata.jsonata(String)`, `.evaluate(Object)`, `com.dashjoin.jsonata.json.Json.parseJson(String)`.
- Produces: `benchmarks/java/gate_results.json` — array of `{"scenario": str, "status": "match"|"mismatch"|"error", "detail": str}`. Task 8's report script reads it; scenarios with status != "match" are excluded from speedup aggregates and flagged.

- [ ] **Step 1: Write the gate**

One subtlety, solved empirically at startup: dashjoin represents "undefined" as Java `null`, and may represent explicit JSON `null` as either `null` or a sentinel object depending on version. The gate detects the null representation once by evaluating the literal expression `null`, then normalizes with it.

`benchmarks/java/src/main/java/dev/jsonatapy/bench/CorrectnessGate.java`:

```java
package dev.jsonatapy.bench;

import com.dashjoin.jsonata.Jsonata;
import com.dashjoin.jsonata.json.Json;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Pre-benchmark correctness gate: for every corpus scenario, jsonata-core
 * (via FFI, string->string) and dashjoin/jsonata-java must produce
 * semantically equal results. Mismatches don't fail the run — they are
 * recorded so the report excludes and flags those scenarios.
 *
 * Usage: CorrectnessGate <corpus.json> <gate_results.json>
 */
public final class CorrectnessGate {

    public static void main(String[] args) throws Exception {
        Path corpusPath = Path.of(args[0]);
        Path outPath = Path.of(args[1]);
        ObjectMapper mapper = new ObjectMapper();

        // How does dashjoin represent explicit JSON null? (null or sentinel)
        Object nullRep = Jsonata.jsonata("null").evaluate(new HashMap<String, Object>());

        List<ObjectNode> results = new ArrayList<>();
        int mismatches = 0;
        for (Corpus.Scenario s : Corpus.load(corpusPath)) {
            String status;
            String detail = "";
            try {
                String dataJson = mapper.writeValueAsString(s.data());
                String ours;
                try (JsonataCore c = JsonataCore.compile(s.expression())) {
                    ours = c.evaluate(dataJson);
                }
                Object theirsRaw = Jsonata.jsonata(s.expression()).evaluate(Json.parseJson(dataJson));
                Object theirs = normalize(theirsRaw, nullRep);

                if (ours == null || theirs == null) {
                    // ours==null is undefined; theirs==null is undefined (or
                    // explicit null when nullRep==null — lenient by design,
                    // disclosed in the report).
                    status = (ours == null && theirs == null) ? "match" : "mismatch";
                    if (status.equals("mismatch")) {
                        detail = "ours=" + ours + " theirs=" + theirs;
                    }
                } else {
                    JsonNode ourNode = mapper.readTree(ours);
                    JsonNode theirNode = mapper.valueToTree(theirs);
                    if (JsonCompare.semanticEquals(ourNode, theirNode)) {
                        status = "match";
                    } else {
                        status = "mismatch";
                        detail = "ours=" + trim(ours) + " theirs=" + trim(mapper.writeValueAsString(theirs));
                    }
                }
            } catch (Exception e) {
                status = "error";
                detail = e.getClass().getSimpleName() + ": " + e.getMessage();
            }
            if (!status.equals("match")) {
                mismatches++;
            }
            ObjectNode r = mapper.createObjectNode();
            r.put("scenario", s.name());
            r.put("status", status);
            r.put("detail", detail);
            results.add(r);
            System.out.printf("%-40s %s%s%n", s.name(), status, detail.isEmpty() ? "" : "  " + detail);
        }
        mapper.writerWithDefaultPrettyPrinter().writeValue(outPath.toFile(), results);
        System.out.printf("%nGate: %d/%d match -> %s%n", results.size() - mismatches, results.size(), outPath);
    }

    /** Recursively replace dashjoin's JSON-null representation with Java null
     *  and rebuild containers so Jackson can serialize them. */
    private static Object normalize(Object v, Object nullRep) {
        if (v == null) {
            return null;
        }
        if (nullRep != null && v.equals(nullRep)) {
            return null;
        }
        if (v instanceof Map<?, ?> m) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<?, ?> e : m.entrySet()) {
                out.put(String.valueOf(e.getKey()), normalize(e.getValue(), nullRep));
            }
            return out;
        }
        if (v instanceof List<?> l) {
            List<Object> out = new ArrayList<>(l.size());
            for (Object o : l) {
                out.add(normalize(o, nullRep));
            }
            return out;
        }
        return v;
    }

    private static String trim(String s) {
        return s.length() > 200 ? s.substring(0, 200) + "..." : s;
    }
}
```

- [ ] **Step 2: Build the uber-jar and run the gate**

(Uses **JAVA ENV**.)

```bash
cd benchmarks/java
mvn -q -DskipTests package
java --enable-native-access=ALL-UNNAMED \
     -Djsonata.core.lib=$HOME/source/jsonatapy/target/release/libjsonata_core.so \
     -cp target/benchmarks.jar dev.jsonatapy.bench.CorrectnessGate \
     ../corpus/corpus.json gate_results.json
```

Expected: 33 lines, overwhelmingly `match`, ending `Gate: N/33 match -> gate_results.json`. Investigate any mismatch before proceeding: if it's a genuine engine disagreement, record it (stays excluded + flagged); if it's a gate bug (comparison too strict, null-normalization wrong), fix the gate. Do not "fix" a mismatch by loosening `JsonCompare` beyond the documented number tolerance.

- [ ] **Step 3: Commit**

```bash
git add benchmarks/java/src/main/java/dev/jsonatapy/bench/CorrectnessGate.java benchmarks/java/gate_results.json
git commit -m "feat(benchmarks/java): correctness gate vs dashjoin jsonata-java

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: Java JMH benchmarks + run

**Files:**
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/FfiBenchmark.java`
- Create: `benchmarks/java/src/main/java/dev/jsonatapy/bench/BenchRunner.java`
- Create (generated, committed): `benchmarks/java/jmh_results.json`

**Interfaces:**
- Consumes: `JsonataCore`, `Corpus` (Task 3).
- Produces: `benchmarks/java/jmh_results.json` — standard JMH JSON result format (array of objects with `benchmark`, `params.scenario`, `primaryMetric.score` in us/op). Task 8 reads it. Benchmark method names (referenced by the report): `coreSsCompiled`, `coreSsCompileEach`, `dashjoinSsCompiled`, `dashjoinSsCompileEach`, `dashjoinHomeTurfCompiled`.

- [ ] **Step 1: Write the benchmark class and runner**

Method semantics (from the spec's methodology):
- `coreSsCompiled` — our FFI, string→string, pre-compiled handle. Also serves as our side of the home-turf comparison (we always pay the string boundary).
- `coreSsCompileEach` — our FFI, compile+evaluate per call.
- `dashjoinSsCompiled` — dashjoin pre-compiled; pays `Json.parseJson` + evaluate + Jackson serialize per call (symmetric string→string).
- `dashjoinSsCompileEach` — dashjoin compile+parse+evaluate+serialize per call.
- `dashjoinHomeTurfCompiled` — dashjoin pre-compiled, pre-parsed data, native result (no serialization) — the competitor's best realistic case.

`benchmarks/java/src/main/java/dev/jsonatapy/bench/FfiBenchmark.java`:

```java
package dev.jsonatapy.bench;

import com.dashjoin.jsonata.Jsonata;
import com.dashjoin.jsonata.json.Json;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.nio.file.Path;
import java.util.concurrent.TimeUnit;
import org.openjdk.jmh.annotations.Benchmark;
import org.openjdk.jmh.annotations.BenchmarkMode;
import org.openjdk.jmh.annotations.Mode;
import org.openjdk.jmh.annotations.OutputTimeUnit;
import org.openjdk.jmh.annotations.Param;
import org.openjdk.jmh.annotations.Scope;
import org.openjdk.jmh.annotations.Setup;
import org.openjdk.jmh.annotations.State;
import org.openjdk.jmh.annotations.TearDown;

@State(Scope.Benchmark)
@BenchmarkMode(Mode.AverageTime)
@OutputTimeUnit(TimeUnit.MICROSECONDS)
public class FfiBenchmark {

    @Param({"SET_BY_RUNNER"})
    public String scenario;

    String expression;
    String dataJson;
    Object dashjoinData;
    JsonataCore coreExpr;
    Jsonata dashjoinExpr;
    ObjectMapper mapper;

    @Setup
    public void setup() throws IOException {
        Corpus.Scenario s = Corpus.load(Path.of(System.getProperty("jsonata.corpus"))).stream()
                .filter(x -> x.name().equals(scenario))
                .findFirst()
                .orElseThrow(() -> new IllegalArgumentException("unknown scenario: " + scenario));
        mapper = new ObjectMapper();
        expression = s.expression();
        dataJson = mapper.writeValueAsString(s.data());
        coreExpr = JsonataCore.compile(expression);
        dashjoinExpr = Jsonata.jsonata(expression);
        dashjoinData = Json.parseJson(dataJson);
    }

    @TearDown
    public void tearDown() {
        coreExpr.close();
    }

    @Benchmark
    public String coreSsCompiled() {
        return coreExpr.evaluate(dataJson);
    }

    @Benchmark
    public String coreSsCompileEach() {
        try (JsonataCore c = JsonataCore.compile(expression)) {
            return c.evaluate(dataJson);
        }
    }

    @Benchmark
    public String dashjoinSsCompiled() throws IOException {
        Object data = Json.parseJson(dataJson);
        Object result = dashjoinExpr.evaluate(data);
        return mapper.writeValueAsString(result);
    }

    @Benchmark
    public String dashjoinSsCompileEach() throws IOException {
        Jsonata e = Jsonata.jsonata(expression);
        Object data = Json.parseJson(dataJson);
        return mapper.writeValueAsString(e.evaluate(data));
    }

    @Benchmark
    public Object dashjoinHomeTurfCompiled() {
        return dashjoinExpr.evaluate(dashjoinData);
    }
}
```

`benchmarks/java/src/main/java/dev/jsonatapy/bench/BenchRunner.java`:

```java
package dev.jsonatapy.bench;

import java.nio.file.Path;
import java.util.List;
import org.openjdk.jmh.results.format.ResultFormatType;
import org.openjdk.jmh.runner.Runner;
import org.openjdk.jmh.runner.options.Options;
import org.openjdk.jmh.runner.options.OptionsBuilder;
import org.openjdk.jmh.runner.options.TimeValue;

/** Usage: BenchRunner <corpus.json> <libjsonata_core.so> <out.json> */
public final class BenchRunner {

    public static void main(String[] args) throws Exception {
        String corpus = args[0];
        String lib = args[1];
        String out = args[2];
        List<Corpus.Scenario> scenarios = Corpus.load(Path.of(corpus));
        Options opt = new OptionsBuilder()
                .include(FfiBenchmark.class.getName())
                .param("scenario", scenarios.stream().map(Corpus.Scenario::name).toArray(String[]::new))
                .forks(1)
                .warmupIterations(3)
                .warmupTime(TimeValue.seconds(1))
                .measurementIterations(5)
                .measurementTime(TimeValue.seconds(1))
                .jvmArgsAppend(
                        "--enable-native-access=ALL-UNNAMED",
                        "-Djsonata.core.lib=" + lib,
                        "-Djsonata.corpus=" + corpus)
                .resultFormat(ResultFormatType.JSON)
                .result(out)
                .build();
        new Runner(opt).run();
    }
}
```

(Spike-scale JMH settings: 1 fork, 3×1s warmup, 5×1s measurement → ~8s per case, 33 scenarios × 5 methods ≈ 25 minutes. Disclosed in the report.)

- [ ] **Step 2: Build, run (background — ~25 min), verify output**

(Uses **JAVA ENV**.)

```bash
cd benchmarks/java
mvn -q -DskipTests package
java -cp target/benchmarks.jar dev.jsonatapy.bench.BenchRunner \
     ../corpus/corpus.json \
     $HOME/source/jsonatapy/target/release/libjsonata_core.so \
     jmh_results.json
```

Run this long command in the background and monitor; a quick pre-flight with a single scenario first is fine (temporarily pass `.param("scenario", "Simple Path")` equivalent by running `java -jar target/benchmarks.jar 'FfiBenchmark.coreSsCompiled' -p 'scenario=Simple Path' -f 1 -wi 1 -i 1 -Djsonata... ` — or just trust the smoke test and run the full thing).

Verify: `python3 -c "import json; d=json.load(open('jmh_results.json')); print(len(d))"`
Expected: `165` (33 scenarios × 5 methods).

- [ ] **Step 3: Commit**

```bash
git add benchmarks/java/src/main/java/dev/jsonatapy/bench/FfiBenchmark.java \
        benchmarks/java/src/main/java/dev/jsonatapy/bench/BenchRunner.java \
        benchmarks/java/jmh_results.json
git commit -m "feat(benchmarks/java): JMH suite core-FFI vs dashjoin + results

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: .NET toolchain + project + P/Invoke wrapper + smoke

**Files:**
- Create: `benchmarks/dotnet/JsonataFfiBench.csproj`
- Create: `benchmarks/dotnet/.gitignore` (containing `bin/`, `obj/`, `BenchmarkDotNet.Artifacts/`)
- Create: `benchmarks/dotnet/Native.cs`
- Create: `benchmarks/dotnet/JsonataCoreExpression.cs`
- Create: `benchmarks/dotnet/CorpusFile.cs`
- Create: `benchmarks/dotnet/JsonCompare.cs`
- Create: `benchmarks/dotnet/Smoke.cs`
- Create: `benchmarks/dotnet/Program.cs`

**Interfaces:**
- Consumes: `target/release/libjsonata_core.so` via env `JSONATA_CORE_LIB`.
- Produces (used by Task 7):
  - `JsonataCoreExpression.Compile(string) -> JsonataCoreExpression` (throws `JsonataCoreException`), `string? Evaluate(string dataJson)` (null = undefined), `Dispose()`, static `string Version()`.
  - `CorpusFile.Load(string path) -> List<Scenario>` with `record Scenario(string Name, string Category, string Expression, string DataJson)` (DataJson = raw JSON text of the data element).
  - `JsonCompare.SemanticEquals(JsonElement a, JsonElement b) -> bool` (same tolerance rules as the Java version).
  - `Program` dispatches subcommands: `smoke`, `gate <corpus> <out>`, `bench <corpus>`.

- [ ] **Step 1: Install .NET SDK user-locally (skip if `~/.dotnet/dotnet` exists)**

```bash
curl -sSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel LTS
export DOTNET_ROOT=$HOME/.dotnet
export PATH=$HOME/.dotnet:$PATH
dotnet --version
```

Expected: a 10.x SDK version (current LTS channel). The project still targets `net8.0` per spec — SDK 10 builds it. The two exports above are the **DOTNET ENV** used by every dotnet command below.

- [ ] **Step 2: Write the project files**

`benchmarks/dotnet/JsonataFfiBench.csproj`:

```xml
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net8.0</TargetFramework>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <AllowUnsafeBlocks>true</AllowUnsafeBlocks>
    <RootNamespace>JsonataFfiBench</RootNamespace>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="BenchmarkDotNet" Version="0.15.8" />
    <PackageReference Include="Jsonata.Net.Native" Version="3.0.0" />
  </ItemGroup>
</Project>
```

`benchmarks/dotnet/Native.cs`:

```csharp
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace JsonataFfiBench;

// Raw C ABI. String RETURNS from the library cross as IntPtr and are freed
// with jsonata_free_string — never as marshalled string returns, whose
// unmarshaller frees with the CoTaskMem allocator (wrong allocator, UB).
// String ARGUMENTS use Utf8 marshalling (runtime-owned temp buffer, safe).
internal static partial class Native
{
    private const string Lib = "jsonata_core";

    [ModuleInitializer]
    internal static void Init()
    {
        NativeLibrary.SetDllImportResolver(typeof(Native).Assembly, (name, _, _) =>
            name == Lib
                ? NativeLibrary.Load(
                    Environment.GetEnvironmentVariable("JSONATA_CORE_LIB")
                    ?? throw new InvalidOperationException(
                        "Set JSONATA_CORE_LIB=/path/to/libjsonata_core.so"))
                : IntPtr.Zero);
    }

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr jsonata_compile(string exprUtf8);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr jsonata_evaluate(IntPtr expr, string jsonUtf8);

    [LibraryImport(Lib)]
    internal static partial void jsonata_free_expr(IntPtr expr);

    [LibraryImport(Lib)]
    internal static partial void jsonata_free_string(IntPtr s);

    [LibraryImport(Lib)]
    internal static partial IntPtr jsonata_last_error_message();

    [LibraryImport(Lib)]
    internal static partial IntPtr jsonata_version();
}
```

`benchmarks/dotnet/JsonataCoreExpression.cs`:

```csharp
using System.Runtime.InteropServices;

namespace JsonataFfiBench;

public sealed class JsonataCoreException(string message) : Exception(message);

/// <summary>
/// P/Invoke wrapper over the jsonata-core C ABI. Not thread-safe: one
/// instance per thread (engine handles are Rc-based).
/// </summary>
public sealed class JsonataCoreExpression : IDisposable
{
    private IntPtr _handle;

    private JsonataCoreExpression(IntPtr handle) => _handle = handle;

    public static JsonataCoreExpression Compile(string expression)
    {
        IntPtr h = Native.jsonata_compile(expression);
        if (h == IntPtr.Zero)
        {
            throw new JsonataCoreException(TakeLastError() ?? "compile failed");
        }
        return new JsonataCoreExpression(h);
    }

    /// <summary>Result JSON text, or null when the JSONata result is undefined.</summary>
    public string? Evaluate(string dataJson)
    {
        IntPtr r = Native.jsonata_evaluate(_handle, dataJson);
        if (r == IntPtr.Zero)
        {
            string? err = TakeLastError();
            if (err is null)
            {
                return null; // undefined
            }
            throw new JsonataCoreException(err);
        }
        try
        {
            return Marshal.PtrToStringUTF8(r)!;
        }
        finally
        {
            Native.jsonata_free_string(r);
        }
    }

    public static string Version() =>
        Marshal.PtrToStringUTF8(Native.jsonata_version())!; // static string: not freed

    private static string? TakeLastError()
    {
        IntPtr p = Native.jsonata_last_error_message();
        if (p == IntPtr.Zero)
        {
            return null;
        }
        try
        {
            return Marshal.PtrToStringUTF8(p);
        }
        finally
        {
            Native.jsonata_free_string(p);
        }
    }

    public void Dispose()
    {
        if (_handle != IntPtr.Zero)
        {
            Native.jsonata_free_expr(_handle);
            _handle = IntPtr.Zero;
        }
    }
}
```

`benchmarks/dotnet/CorpusFile.cs`:

```csharp
using System.Text.Json;

namespace JsonataFfiBench;

public sealed record Scenario(string Name, string Category, string Expression, string DataJson);

public static class CorpusFile
{
    public static List<Scenario> Load(string path)
    {
        using JsonDocument doc = JsonDocument.Parse(File.ReadAllText(path));
        var result = new List<Scenario>();
        foreach (JsonElement e in doc.RootElement.EnumerateArray())
        {
            result.Add(new Scenario(
                e.GetProperty("name").GetString()!,
                e.GetProperty("category").GetString()!,
                e.GetProperty("expression").GetString()!,
                e.GetProperty("data").GetRawText()));
        }
        return result;
    }
}
```

`benchmarks/dotnet/JsonCompare.cs`:

```csharp
using System.Text.Json;

namespace JsonataFfiBench;

/// <summary>
/// Semantic JSON equality: numbers compared as double with relative
/// tolerance, object key order ignored, array order significant.
/// (Mirror of the Java JsonCompare.)
/// </summary>
public static class JsonCompare
{
    public static bool SemanticEquals(JsonElement a, JsonElement b)
    {
        if (a.ValueKind == JsonValueKind.Number && b.ValueKind == JsonValueKind.Number)
        {
            double x = a.GetDouble();
            double y = b.GetDouble();
            return Math.Abs(x - y) <= 1e-9 * Math.Max(1.0, Math.Max(Math.Abs(x), Math.Abs(y)));
        }
        if (a.ValueKind != b.ValueKind)
        {
            return false;
        }
        switch (a.ValueKind)
        {
            case JsonValueKind.Array:
            {
                if (a.GetArrayLength() != b.GetArrayLength())
                {
                    return false;
                }
                using var ea = a.EnumerateArray().GetEnumerator();
                using var eb = b.EnumerateArray().GetEnumerator();
                while (ea.MoveNext() && eb.MoveNext())
                {
                    if (!SemanticEquals(ea.Current, eb.Current))
                    {
                        return false;
                    }
                }
                return true;
            }
            case JsonValueKind.Object:
            {
                var bProps = new Dictionary<string, JsonElement>();
                foreach (var p in b.EnumerateObject())
                {
                    bProps[p.Name] = p.Value;
                }
                int aCount = 0;
                foreach (var p in a.EnumerateObject())
                {
                    aCount++;
                    if (!bProps.TryGetValue(p.Name, out JsonElement bv) || !SemanticEquals(p.Value, bv))
                    {
                        return false;
                    }
                }
                return aCount == bProps.Count;
            }
            case JsonValueKind.String:
                return a.GetString() == b.GetString();
            default: // True/False/Null
                return true;
        }
    }
}
```

`benchmarks/dotnet/Smoke.cs`:

```csharp
namespace JsonataFfiBench;

public static class Smoke
{
    public static int Run()
    {
        int failures = 0;

        void Check(string name, Func<bool> test)
        {
            bool ok;
            string? err = null;
            try { ok = test(); }
            catch (Exception e) { ok = false; err = e.Message; }
            Console.WriteLine($"{(ok ? "PASS" : "FAIL")}  {name}{(err is null ? "" : $"  ({err})")}");
            if (!ok) failures++;
        }

        Check("version non-empty", () => JsonataCoreExpression.Version().Length > 0);
        Check("simple path", () =>
        {
            using var e = JsonataCoreExpression.Compile("user.name");
            return e.Evaluate("{\"user\":{\"name\":\"Alice\"}}") == "\"Alice\"";
        });
        Check("object result", () =>
        {
            using var e = JsonataCoreExpression.Compile("{\"n\": a + b}");
            return e.Evaluate("{\"a\":1,\"b\":2}") == "{\"n\":3}";
        });
        Check("undefined is null", () =>
        {
            using var e = JsonataCoreExpression.Compile("missing.path");
            return e.Evaluate("{\"a\":1}") is null;
        });
        Check("parse error throws", () =>
        {
            try { JsonataCoreExpression.Compile("a.b["); return false; }
            catch (JsonataCoreException) { return true; }
        });
        Check("eval error throws", () =>
        {
            using var e = JsonataCoreExpression.Compile("a + b");
            try { e.Evaluate("{\"a\":1,\"b\":\"x\"}"); return false; }
            catch (JsonataCoreException) { return true; }
        });
        Check("multibyte utf8", () =>
        {
            using var e = JsonataCoreExpression.Compile("$uppercase(name)");
            return e.Evaluate("{\"name\":\"héllo ✓ 日本語\"}") == "\"HÉLLO ✓ 日本語\"";
        });

        Console.WriteLine(failures == 0 ? "SMOKE OK" : $"SMOKE FAILED ({failures})");
        return failures == 0 ? 0 : 1;
    }
}
```

`benchmarks/dotnet/Program.cs`:

```csharp
namespace JsonataFfiBench;

public static class Program
{
    public static int Main(string[] args)
    {
        return args switch
        {
            ["smoke"] => Smoke.Run(),
            ["gate", var corpus, var outPath] => Gate.Run(corpus, outPath),
            ["bench", var corpus] => Benchmarks.RunAll(corpus),
            _ => Usage(),
        };
    }

    private static int Usage()
    {
        Console.Error.WriteLine("usage: JsonataFfiBench smoke | gate <corpus.json> <out.json> | bench <corpus.json>");
        return 2;
    }
}
```

(`Gate` and `Benchmarks` are Task 7 files — to keep this task compilable on its own, create them in Task 7 and, for this task only, temporarily stub the two switch arms out; simplest is to write Program.cs in this task with ONLY the `smoke` arm and add the other arms in Task 7.)

For this task, use this `Program.cs` instead (Task 7 replaces it):

```csharp
namespace JsonataFfiBench;

public static class Program
{
    public static int Main(string[] args)
    {
        return args switch
        {
            ["smoke"] => Smoke.Run(),
            _ => Usage(),
        };
    }

    private static int Usage()
    {
        Console.Error.WriteLine("usage: JsonataFfiBench smoke");
        return 2;
    }
}
```

- [ ] **Step 3: Build and run smoke**

(Uses **DOTNET ENV**.)

```bash
cd benchmarks/dotnet
JSONATA_CORE_LIB=$HOME/source/jsonatapy/target/release/libjsonata_core.so \
  dotnet run -c Release -- smoke
```

Expected: 7 × `PASS`, `SMOKE OK`, exit 0.

- [ ] **Step 4: Commit**

```bash
git add benchmarks/dotnet
git commit -m "feat(benchmarks/dotnet): LibraryImport wrapper over capi + smoke (spike)

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: .NET correctness gate + BenchmarkDotNet suite + run

**Files:**
- Create: `benchmarks/dotnet/Gate.cs`
- Create: `benchmarks/dotnet/Benchmarks.cs`
- Modify: `benchmarks/dotnet/Program.cs` (restore full 3-arm dispatch from Task 6 Step 2)
- Create (generated, committed): `benchmarks/dotnet/gate_results.json`
- Create (generated, committed): `benchmarks/dotnet/bdn_results.json` (copied from BDN artifacts)

**Interfaces:**
- Consumes: Task 6 wrapper/corpus/compare; `Jsonata.Net.Native.JsonataQuery` (ctor `JsonataQuery(string)`; extension `string Eval(string dataJson, bool indentResult)`; `JToken Eval(JToken)`), `Jsonata.Net.Native.Json.JToken.Parse(string)`, `JTokenType.Undefined`.
- Produces: `benchmarks/dotnet/gate_results.json` (same schema as Java's), `benchmarks/dotnet/bdn_results.json` (BDN "full JSON" export: `Benchmarks[].Method`, `.Parameters` (`"Scenario=..."`), `.Statistics.Mean` in ns). Benchmark method names (referenced by the report): `CoreSsCompiled`, `CoreSsCompileEach`, `JnnSsCompiled`, `JnnSsCompileEach`, `JnnHomeTurfCompiled`.

- [ ] **Step 1: Write the gate**

`benchmarks/dotnet/Gate.cs`:

```csharp
using System.Text.Json;
using Jsonata.Net.Native;
using Jsonata.Net.Native.Json;

namespace JsonataFfiBench;

public static class Gate
{
    public static int Run(string corpusPath, string outPath)
    {
        var results = new List<object>();
        int mismatches = 0;
        foreach (Scenario s in CorpusFile.Load(corpusPath))
        {
            string status;
            string detail = "";
            try
            {
                string? ours;
                using (var c = JsonataCoreExpression.Compile(s.Expression))
                {
                    ours = c.Evaluate(s.DataJson);
                }
                var query = new JsonataQuery(s.Expression);
                JToken theirs = query.Eval(JToken.Parse(s.DataJson));
                bool theirsUndefined = theirs.Type == JTokenType.Undefined;

                if (ours is null || theirsUndefined)
                {
                    status = (ours is null && theirsUndefined) ? "match" : "mismatch";
                    if (status == "mismatch")
                    {
                        detail = $"ours={(ours is null ? "undefined" : Trim(ours))} theirs={(theirsUndefined ? "undefined" : Trim(theirs.ToString()))}";
                    }
                }
                else
                {
                    using JsonDocument da = JsonDocument.Parse(ours);
                    using JsonDocument db = JsonDocument.Parse(theirs.ToString());
                    if (JsonCompare.SemanticEquals(da.RootElement, db.RootElement))
                    {
                        status = "match";
                    }
                    else
                    {
                        status = "mismatch";
                        detail = $"ours={Trim(ours)} theirs={Trim(theirs.ToString())}";
                    }
                }
            }
            catch (Exception e)
            {
                status = "error";
                detail = $"{e.GetType().Name}: {e.Message}";
            }
            if (status != "match")
            {
                mismatches++;
            }
            results.Add(new { scenario = s.Name, status, detail });
            Console.WriteLine($"{s.Name,-40} {status}{(detail.Length == 0 ? "" : "  " + detail)}");
        }
        File.WriteAllText(outPath, JsonSerializer.Serialize(results,
            new JsonSerializerOptions { WriteIndented = true }));
        Console.WriteLine($"\nGate: {results.Count - mismatches}/{results.Count} match -> {outPath}");
        return 0;
    }

    private static string Trim(string s) => s.Length > 200 ? s[..200] + "..." : s;
}
```

- [ ] **Step 2: Write the benchmarks**

`benchmarks/dotnet/Benchmarks.cs`:

```csharp
using BenchmarkDotNet.Attributes;
using BenchmarkDotNet.Configs;
using BenchmarkDotNet.Exporters.Json;
using BenchmarkDotNet.Jobs;
using BenchmarkDotNet.Running;
using Jsonata.Net.Native;
using Jsonata.Net.Native.Json;

namespace JsonataFfiBench;

public class FfiBenchmarks
{
    internal static string CorpusPath =
        Environment.GetEnvironmentVariable("JSONATA_CORPUS")
        ?? throw new InvalidOperationException("Set JSONATA_CORPUS=/path/to/corpus.json");

    public static IEnumerable<string> ScenarioNames =>
        CorpusFile.Load(CorpusPath).Select(s => s.Name);

    [ParamsSource(nameof(ScenarioNames))]
    public string Scenario = "";

    private string _expression = "";
    private string _dataJson = "";
    private JsonataCoreExpression _core = null!;
    private JsonataQuery _jnn = null!;
    private JToken _dataToken = null!;

    [GlobalSetup]
    public void Setup()
    {
        var s = CorpusFile.Load(CorpusPath).First(x => x.Name == Scenario);
        _expression = s.Expression;
        _dataJson = s.DataJson;
        _core = JsonataCoreExpression.Compile(_expression);
        _jnn = new JsonataQuery(_expression);
        _dataToken = JToken.Parse(_dataJson);
    }

    [GlobalCleanup]
    public void Cleanup() => _core.Dispose();

    [Benchmark]
    public string? CoreSsCompiled() => _core.Evaluate(_dataJson);

    [Benchmark]
    public string? CoreSsCompileEach()
    {
        using var c = JsonataCoreExpression.Compile(_expression);
        return c.Evaluate(_dataJson);
    }

    [Benchmark]
    public string JnnSsCompiled() => _jnn.Eval(_dataJson, indentResult: false);

    [Benchmark]
    public string JnnSsCompileEach() => new JsonataQuery(_expression).Eval(_dataJson, indentResult: false);

    [Benchmark]
    public JToken JnnHomeTurfCompiled() => _jnn.Eval(_dataToken);
}

public static class Benchmarks
{
    public static int RunAll(string corpusPath)
    {
        Environment.SetEnvironmentVariable("JSONATA_CORPUS", Path.GetFullPath(corpusPath));
        var config = ManualConfig.CreateMinimumViable()
            .AddJob(Job.ShortRun)   // 3 warmup + 3 measurement iterations; spike scale, disclosed in report
            .AddExporter(JsonExporter.Full);
        BenchmarkRunner.Run<FfiBenchmarks>(config);
        return 0;
    }
}
```

Restore the full `Program.cs` dispatch (3 arms: `smoke`, `gate`, `bench` — exact code in Task 6 Step 2's first `Program.cs` listing).

- [ ] **Step 3: Run gate, then benchmarks (background — expect ~1h), collect results**

(Uses **DOTNET ENV**.)

```bash
cd benchmarks/dotnet
export JSONATA_CORE_LIB=$HOME/source/jsonatapy/target/release/libjsonata_core.so
dotnet run -c Release -- gate ../corpus/corpus.json gate_results.json
```

Expected: 33 lines, overwhelmingly `match` (same investigation rule as Task 4 Step 2). Note: BDN child processes inherit env — keep `JSONATA_CORE_LIB` exported for the bench run:

```bash
dotnet run -c Release -- bench ../corpus/corpus.json
cp BenchmarkDotNet.Artifacts/results/JsonataFfiBench.FfiBenchmarks-report-full.json bdn_results.json
python3 -c "import json; d=json.load(open('bdn_results.json')); print(len(d['Benchmarks']))"
```

Expected final line: `165` (33 scenarios × 5 methods).

- [ ] **Step 4: Commit**

```bash
git add benchmarks/dotnet/Gate.cs benchmarks/dotnet/Benchmarks.cs benchmarks/dotnet/Program.cs \
        benchmarks/dotnet/gate_results.json benchmarks/dotnet/bdn_results.json
git commit -m "feat(benchmarks/dotnet): gate + BenchmarkDotNet suite vs Jsonata.Net.Native + results

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: Report generation + final verification

**Files:**
- Create: `benchmarks/scripts/ffi_report.py`
- Create (generated, committed): `benchmarks/results/ffi_experiment_report_2026-07-13.md`

**Interfaces:**
- Consumes: `benchmarks/corpus/corpus.json`, `benchmarks/java/gate_results.json`, `benchmarks/java/jmh_results.json`, `benchmarks/dotnet/gate_results.json`, `benchmarks/dotnet/bdn_results.json`.
- Produces: the experiment report — the spike's Definition of Done.

- [ ] **Step 1: Write the report script**

Uses jsonatapy itself for the JSON reshaping (project preference: dogfood jsonatapy for internal JSON transforms). Run under `uv run` so the locally built jsonatapy is importable.

`benchmarks/scripts/ffi_report.py`:

```python
#!/usr/bin/env python3
"""Merge Java/JMH + .NET/BDN results into the FFI experiment report.

Usage: uv run python benchmarks/scripts/ffi_report.py [out.md]
Reads (relative to repo root):
  benchmarks/corpus/corpus.json
  benchmarks/java/gate_results.json,   benchmarks/java/jmh_results.json
  benchmarks/dotnet/gate_results.json, benchmarks/dotnet/bdn_results.json
"""

import json
import math
import sys
from pathlib import Path

import jsonatapy

ROOT = Path(__file__).resolve().parents[2]

# JMH: [{benchmark: "...FfiBenchmark.coreSsCompiled", params: {scenario}, primaryMetric: {score us/op}}]
JMH_EXTRACT = jsonatapy.compile(
    '$.{"method": $split(benchmark, ".")[-1], "scenario": params.scenario,'
    ' "us": primaryMetric.score}[]'
)
# BDN full json: {Benchmarks: [{Method, Parameters: "Scenario=...", Statistics: {Mean ns}}]}
BDN_EXTRACT = jsonatapy.compile(
    'Benchmarks.{"method": Method, "scenario": $substringAfter(Parameters, "Scenario="),'
    ' "us": Statistics.Mean / 1000}[]'
)

CORE_SS = {"java": "coreSsCompiled", "dotnet": "CoreSsCompiled"}
CORE_SS_EACH = {"java": "coreSsCompileEach", "dotnet": "CoreSsCompileEach"}
COMP_SS = {"java": "dashjoinSsCompiled", "dotnet": "JnnSsCompiled"}
COMP_SS_EACH = {"java": "dashjoinSsCompileEach", "dotnet": "JnnSsCompileEach"}
COMP_HOME = {"java": "dashjoinHomeTurfCompiled", "dotnet": "JnnHomeTurfCompiled"}
COMPETITOR = {"java": "dashjoin/jsonata-java 0.9.10", "dotnet": "Jsonata.Net.Native 3.0.0"}


def load(path: str):
    return json.loads((ROOT / path).read_text())


def geomean(values: list[float]) -> float:
    return math.exp(sum(math.log(v) for v in values) / len(values)) if values else float("nan")


def index_results(rows: list[dict]) -> dict[tuple[str, str], float]:
    return {(r["scenario"], r["method"]): r["us"] for r in rows}


def lang_section(lang: str, rows: list[dict], gate: list[dict], categories: dict[str, str]) -> str:
    by = index_results(rows)
    passed = {g["scenario"] for g in gate if g["status"] == "match"}
    flagged = [g for g in gate if g["status"] != "match"]
    scenarios = sorted({r["scenario"] for r in rows}, key=lambda s: (categories.get(s, ""), s))

    lines = []
    lines.append(f"### Correctness gate\n")
    lines.append(f"{len(passed)}/{len(gate)} scenarios match. "
                 + ("All scenarios included.\n" if not flagged else
                    "Excluded from aggregates (flagged):\n"))
    for g in flagged:
        lines.append(f"- **{g['scenario']}**: {g['status']} — {g['detail']}\n")

    lines.append("\n### Per-scenario results (µs/op, lower is better)\n")
    lines.append("| Scenario | core s→s | comp s→s | speedup | core s→s (compile-each) |"
                 " comp s→s (compile-each) | speedup | comp home-turf | core-vs-home-turf |")
    lines.append("|---|---|---|---|---|---|---|---|---|")
    ss_speedups, each_speedups, home_speedups = [], [], []
    rows_out = []
    for s in scenarios:
        c = by.get((s, CORE_SS[lang]))
        k = by.get((s, COMP_SS[lang]))
        ce = by.get((s, CORE_SS_EACH[lang]))
        ke = by.get((s, COMP_SS_EACH[lang]))
        h = by.get((s, COMP_HOME[lang]))
        if None in (c, k, ce, ke, h):
            continue
        ss, ee, hh = k / c, ke / ce, h / c
        excl = s not in passed
        if not excl:
            ss_speedups.append(ss)
            each_speedups.append(ee)
            home_speedups.append(hh)
        flag = " ⚠️excluded" if excl else ""
        rows_out.append((s, ss))
        lines.append(f"| {s}{flag} | {c:.2f} | {k:.2f} | {ss:.2f}x | {ce:.2f} | {ke:.2f} |"
                     f" {ee:.2f}x | {h:.2f} | {hh:.2f}x |")

    lines.append("\n### Aggregates (gate-passed scenarios only)\n")
    lines.append(f"- **String→string, compiled (geomean): {geomean(ss_speedups):.2f}x** "
                 f"({COMPETITOR[lang]} time ÷ core time; >1 means core is faster)")
    lines.append(f"- String→string, compile-each (geomean): {geomean(each_speedups):.2f}x")
    lines.append(f"- Home-turf (competitor on pre-parsed native data, core still paying the"
                 f" string boundary; geomean): {geomean(home_speedups):.2f}x")
    ranked = sorted(rows_out, key=lambda t: t[1])
    if len(ranked) >= 3:
        worst = ", ".join(f"{n} ({v:.2f}x)" for n, v in ranked[:3])
        best = ", ".join(f"{n} ({v:.2f}x)" for n, v in ranked[-3:][::-1])
        lines.append(f"- Best 3 (s→s compiled): {best}")
        lines.append(f"- Worst 3 (s→s compiled): {worst}")
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "benchmarks/results/ffi_experiment_report_2026-07-13.md"
    corpus = load("benchmarks/corpus/corpus.json")
    categories = {c["name"]: c["category"] for c in corpus}

    jmh = JMH_EXTRACT.evaluate(load("benchmarks/java/jmh_results.json"))
    bdn = BDN_EXTRACT.evaluate(load("benchmarks/dotnet/bdn_results.json"))

    parts = [
        "# Java/.NET FFI Benchmark Experiment — Report (2026-07-13)\n",
        "Spike per docs/superpowers/specs/2026-07-13-java-dotnet-ffi-benchmark-experiment-design.md.",
        "jsonata-core consumed over the C ABI (`capi` feature), JSON-as-string boundary.",
        "Java: JMH 1.37, 1 fork, 3×1s warmup, 5×1s measure. .NET: BenchmarkDotNet 0.15.8, ShortRun job.",
        "Spike-scale iteration counts — treat small (<1.2x) differences as noise.\n",
        "**Reading the modes:** *s→s* = JSON text in/out for both sides (symmetric).",
        "*Home-turf* = competitor evaluates pre-parsed native objects with no result",
        "serialization (its best realistic case) while core still pays the full string",
        "boundary (its worst realistic case).\n",
        f"## Java — core (FFM) vs {COMPETITOR['java']}\n",
        lang_section("java", jmh, load("benchmarks/java/gate_results.json"), categories),
        f"## .NET — core (LibraryImport) vs {COMPETITOR['dotnet']}\n",
        lang_section("dotnet", bdn, load("benchmarks/dotnet/gate_results.json"), categories),
    ]
    out.write_text("\n".join(parts))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
```

Note for the implementer: the two jsonatapy extraction expressions are the first live consumers of `jsonatapy.compile(...).evaluate(...)` in this pipeline — if either returns an unexpected shape (e.g. `$split(...)[-1]` disagreement), verify the expression interactively with a small slice of the real JSON before changing the script structure. `evaluate` may also return a single object instead of a list when there's exactly one row — the trailing `[]` in both expressions forces list output.

- [ ] **Step 2: Generate the report**

```bash
uv run python benchmarks/scripts/ffi_report.py
```

Expected: `wrote .../benchmarks/results/ffi_experiment_report_2026-07-13.md`. Read the report end-to-end: every scenario present for both languages, no `nan` geomeans, excluded scenarios flagged.

- [ ] **Step 3: Full-suite regression check**

```bash
cargo test
cargo test --features capi
uv run pytest tests/python/test_reference_suite.py -q
```

Expected: all green (existing counts unchanged; capi adds 8).

- [ ] **Step 4: Commit and push the branch**

```bash
git add benchmarks/scripts/ffi_report.py benchmarks/results/ffi_experiment_report_2026-07-13.md
git commit -m "docs(benchmarks): FFI experiment report — Java + .NET vs native implementations

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
git push -u origin experiment/java-dotnet-bindings
```

- [ ] **Step 5: Present the report to the user**

Summarize: per-language geomeans for all three modes, best/worst scenarios, gate outcome, and where the string boundary hurts. The user makes the merge/graduate decision per language — that decision is explicitly NOT part of this plan.
