# Lazy Python Views (LazyPyDict) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Eliminate the ~93% per-call Python→JValue conversion tax in `evaluate(dict)` by reading Python dict fields lazily on demand and passing untouched input subtrees through to output as the original Python objects.

**Architecture:** A new `#[cfg(feature = "python")] JValue::LazyPyDict(Rc<lazy::LazyPyDict>)` variant wraps the original `Py<PyDict>` and converts fields on first access (per-field cache + memoized full materialization). Lazy values are handled at *consumption* choke points (field access hot paths, builtin dispatch normalization, equality/truthiness/type tests); everywhere else they flow opaquely. Output conversion returns the original Python dict for untouched lazy values. The default `evaluate(dict)` path flips to lazy only in the final integration task; until then a private `_evaluate_lazy` test hook exercises the lazy path so every task lands with green default tests.

**Tech Stack:** Rust (PyO3 0.29, indexmap, serde), maturin, pytest, the 1258-test jsonata-js reference suite.

**Spec:** `docs/superpowers/specs/2026-07-12-lazy-python-views-design.md` (approved). Read it before starting any task.

## Global Constraints

- Branch: `feature/lazy-python-views` (already exists; all commits go here).
- ALL lazy code is behind the `python` cargo feature. `cargo test` (no python feature) and `cargo build --features cli` must stay green after every task.
- The reference suite must stay green on the DEFAULT path after every task: `uv run pytest tests/python/test_reference_suite.py -q` → 1258 passed.
- Build the extension before any pytest run: `uv run maturin develop --release` (from repo root). If `uv run` re-builds on its own, that is fine — but never test against a stale build.
- Lazy dicts are NEVER tuple wrappers. Tuple objects (`__tuple__: true`) are constructed by the engine as plain `JValue::Object`s; a `LazyPyDict` wraps caller data only. Therefore lazy arms never need `__tuple__` checks, and existing `matches!(x, JValue::Object(o) if o.get("__tuple__") …)` tuple probes are automatically correct for lazy values (they return false).
- `JValue` numbers: Python ints convert to `JValue::Number(f64)` (existing behavior — do not change).
- Performance guardrail (verified in Task 9): dense-access "Complex transformation" benchmark must not regress more than 10% vs the pre-change baseline.
- Commit after every task with a conventional-commit message ending in the repo's standard trailers (see any recent `git log` entry from this session for the format).

---

### Task 1: Tree-walker force toggle + spec testing-section amendment

The spec's testing section originally assumed "bindings-forced tree-walker suite runs" — investigation showed non-empty bindings force the tree-walker for only 7 of 1289 suite cases. This task adds a real toggle and fixes the spec text.

**Files:**
- Modify: `src/lib.rs` (`JsonataExpression::run_eval`, ~line 172)
- Modify: `docs/superpowers/specs/2026-07-12-lazy-python-views-design.md` (Testing section)

**Interfaces:**
- Produces: env var `JSONATAPY_FORCE_TREE_WALKER` — when set to any value other than `0` or empty, `run_eval` skips the bytecode VM and always uses the tree-walking `Evaluator`. Later tasks run the reference suite with this set to cover tree-walker code paths.

- [ ] **Step 1: Modify run_eval to honor the env var**

In `src/lib.rs`, `run_eval` currently begins:

```rust
        if bindings.is_none() {
            let bytecode = self.bytecode.get_or_init(|| {
```

Change the condition and add a helper just above `impl JsonataExpression`:

```rust
/// Test-support toggle: set JSONATAPY_FORCE_TREE_WALKER=1 to bypass the
/// bytecode VM and exercise the tree-walking evaluator on every call.
/// Read per-call (not cached) so tests can flip it via monkeypatch.setenv;
/// the ~100ns env read is noise next to a µs-scale evaluation.
#[cfg(feature = "python")]
fn force_tree_walker() -> bool {
    std::env::var_os("JSONATAPY_FORCE_TREE_WALKER").is_some_and(|v| !v.is_empty() && v != "0")
}
```

and in `run_eval`:

```rust
        if bindings.is_none() && !force_tree_walker() {
```

- [ ] **Step 2: Verify the toggle exercises the tree-walker**

Run: `uv run maturin develop --release && JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py -q`
Expected: 1258 passed (tree-walker passes the full suite — this is the pre-existing guarantee).

Then run without the env var: `uv run pytest tests/python/test_reference_suite.py -q`
Expected: 1258 passed.

- [ ] **Step 3: Amend the spec's Testing section**

In `docs/superpowers/specs/2026-07-12-lazy-python-views-design.md`, replace the line:

```
- Full 1258-test reference suite via `evaluate(dict)` (exercises lazy path on the
  VM engine), **plus a bindings-forced run** covering the tree-walker engine.
```

with:

```
- Full 1258-test reference suite via `evaluate(dict)` (exercises lazy path on the
  VM engine), **plus a run with `JSONATAPY_FORCE_TREE_WALKER=1`** covering the
  tree-walker engine. (An earlier draft assumed non-empty `bindings` forces the
  tree-walker suite-wide; in reality only 7 of 1289 suite cases carry non-empty
  bindings, so an explicit env toggle was added in `run_eval`.)
```

- [ ] **Step 4: Commit**

```bash
git add src/lib.rs docs/superpowers/specs/2026-07-12-lazy-python-views-design.md
git commit -m "test: add JSONATAPY_FORCE_TREE_WALKER toggle; fix spec's tree-walker test mechanism"
```

---

### Task 2: LazyPyDict core, JValue variant, and compile-driven arms

**Files:**
- Create: `src/lazy.rs`
- Modify: `src/lib.rs` (module decl, `python_to_json_bound` delegation, `json_to_python` arm, `evaluator_error_to_py`)
- Modify: `src/value.rs` (variant + `is_object`/`as_object`/`PartialEq`/`Display`/`Serialize` + any other arm `cargo check` flags)
- Modify: `src/evaluator.rs` (new `EvaluatorError` variant + `From<LazyConvertError>`)
- Modify: `src/signature.rs` (`type_symbol`, `ParamType::matches`)

**Interfaces:**
- Produces (used by Tasks 3–8):
  - `lazy::LazyPyDict` with `pub fn get_field(&self, field: &str) -> Result<JValue, LazyConvertError>`, `pub fn to_object(&self) -> Result<Rc<IndexMap<String, JValue>>, LazyConvertError>`, `pub fn to_object_ref(&self) -> Option<&IndexMap<String, JValue>>`, `pub fn py_object(&self) -> &Py<PyDict>`, `pub fn is_empty(&self) -> bool`, `pub fn contains_field(&self, field: &str) -> bool`
  - `lazy::LazyConvertError(pub String)` with `impl From<LazyConvertError> for EvaluatorError` (maps to the new `EvaluatorError::PyConversionError`)
  - `lazy::convert(obj: &Bound<'_, PyAny>, lazy: bool) -> PyResult<JValue>` — the single Python→JValue conversion routine; `lazy=false` reproduces today's eager behavior, `lazy=true` wraps dicts in `LazyPyDict` (lists stay eager `Vec`s whose dict elements are wrapped lazily)
  - `JValue::LazyPyDict(Rc<lazy::LazyPyDict>)` variant (cfg python)
  - `EvaluatorError::PyConversionError(String)` (cfg python) — converted to Python `TypeError` at the boundary

- [ ] **Step 1: Create `src/lazy.rs`**

```rust
//! Lazy Python dict views.
//!
//! A `LazyPyDict` wraps the caller's original `Py<PyDict>` and converts
//! fields to `JValue` on first access instead of eagerly converting the
//! whole tree at the boundary. Untouched fields are never converted, and
//! `json_to_python` passes untouched wrappers back out as the original
//! Python dict object.
//!
//! Invariant: lazy dicts wrap *caller data only* — the engine never wraps
//! its own constructed objects, so a `LazyPyDict` is never a tuple wrapper
//! (`__tuple__`) and never contains engine-internal keys.

use crate::value::JValue;
use indexmap::IndexMap;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};
use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// Conversion failure while lazily reading Python data mid-evaluation.
/// Carries the message of the underlying Python TypeError.
#[derive(Debug, Clone)]
pub struct LazyConvertError(pub String);

pub struct LazyPyDict {
    obj: Py<PyDict>,
    /// Converted-on-first-access fields. `JValue::Undefined` marks a key
    /// known to be absent (so repeat misses skip the Python lookup).
    field_cache: RefCell<HashMap<String, JValue>>,
    /// Memoized full materialization (for consumers that need a real object).
    materialized: OnceCell<Rc<IndexMap<String, JValue>>>,
}

impl std::fmt::Debug for LazyPyDict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyPyDict").finish_non_exhaustive()
    }
}

impl LazyPyDict {
    pub fn new(obj: Py<PyDict>) -> Self {
        LazyPyDict {
            obj,
            field_cache: RefCell::new(HashMap::new()),
            materialized: OnceCell::new(),
        }
    }

    /// The wrapped Python dict (for output pass-through and identity checks).
    pub fn py_object(&self) -> &Py<PyDict> {
        &self.obj
    }

    /// Pointer identity of the wrapped dict (cheap equality fast path).
    pub fn same_object(&self, other: &LazyPyDict) -> bool {
        self.obj.as_ptr() == other.obj.as_ptr()
    }

    /// Read one field, converting on first access. Absent key → Undefined.
    pub fn get_field(&self, field: &str) -> Result<JValue, LazyConvertError> {
        if let Some(m) = self.materialized.get() {
            return Ok(m.get(field).cloned().unwrap_or(JValue::Undefined));
        }
        if let Some(v) = self.field_cache.borrow().get(field) {
            return Ok(v.clone());
        }
        Python::with_gil(|py| {
            let dict = self.obj.bind(py);
            let val = match dict
                .get_item(field)
                .map_err(|e| LazyConvertError(e.to_string()))?
            {
                Some(v) => convert(&v, true).map_err(|e| LazyConvertError(e.to_string()))?,
                None => JValue::Undefined,
            };
            self.field_cache
                .borrow_mut()
                .insert(field.to_string(), val.clone());
            Ok(val)
        })
    }

    /// Key-presence check without converting the value.
    pub fn contains_field(&self, field: &str) -> bool {
        if let Some(m) = self.materialized.get() {
            return m.contains_key(field);
        }
        if let Some(v) = self.field_cache.borrow().get(field) {
            return !v.is_undefined();
        }
        Python::with_gil(|py| self.obj.bind(py).contains(field).unwrap_or(false))
    }

    /// Emptiness check without converting anything (used for truthiness).
    pub fn is_empty(&self) -> bool {
        if let Some(m) = self.materialized.get() {
            return m.is_empty();
        }
        Python::with_gil(|py| self.obj.bind(py).is_empty())
    }

    /// Full materialization. Iterates the Python dict in insertion order,
    /// reusing per-field cached conversions. Memoized.
    ///
    /// Absent-key `Undefined` markers in `field_cache` are naturally
    /// excluded: iteration is over the dict's actual keys, and a present
    /// key never caches as Undefined (conversion never yields Undefined).
    pub fn to_object(&self) -> Result<Rc<IndexMap<String, JValue>>, LazyConvertError> {
        if let Some(m) = self.materialized.get() {
            return Ok(m.clone());
        }
        let map = Python::with_gil(|py| -> Result<IndexMap<String, JValue>, LazyConvertError> {
            let dict = self.obj.bind(py);
            let cache = self.field_cache.borrow();
            let mut map = IndexMap::with_capacity(dict.len());
            for (k, v) in dict.iter() {
                let key: String = k
                    .extract()
                    .map_err(|e| LazyConvertError(e.to_string()))?;
                let val = match cache.get(&key) {
                    Some(cached) => cached.clone(),
                    None => convert(&v, true).map_err(|e| LazyConvertError(e.to_string()))?,
                };
                map.insert(key, val);
            }
            Ok(map)
        })?;
        let rc = Rc::new(map);
        let _ = self.materialized.set(rc.clone());
        Ok(rc)
    }

    /// Borrow the materialized map, materializing on first call.
    /// Returns None if conversion fails (caller treats value as non-object;
    /// the error still surfaces on paths that use `get_field`/`to_object`).
    pub fn to_object_ref(&self) -> Option<&IndexMap<String, JValue>> {
        if self.materialized.get().is_none() {
            let _ = self.to_object();
        }
        self.materialized.get().map(|rc| &**rc)
    }
}

/// Single Python→JValue conversion routine.
/// `lazy=false` — today's eager deep conversion (used by JsonataData and bindings).
/// `lazy=true` — dicts become LazyPyDict wrappers; lists convert to eager Vecs
/// whose elements are converted with `lazy=true` (so dict elements wrap lazily).
pub fn convert(obj: &Bound<'_, PyAny>, lazy: bool) -> PyResult<JValue> {
    use pyo3::exceptions::PyTypeError;

    if obj.is_none() {
        return Ok(JValue::Null);
    }
    if obj.is_instance_of::<PyBool>() {
        return Ok(JValue::Bool(obj.extract::<bool>()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(JValue::Number(obj.extract::<i64>()? as f64));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(JValue::Number(obj.extract::<f64>()?));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(JValue::string(obj.extract::<String>()?));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut result = Vec::with_capacity(list.len());
        for item in list.iter() {
            result.push(convert(&item, lazy)?);
        }
        return Ok(JValue::array(result));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        if lazy {
            return Ok(JValue::LazyPyDict(Rc::new(LazyPyDict::new(
                dict.clone().unbind(),
            ))));
        }
        let mut result = IndexMap::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key_str = key.extract::<String>()?;
            result.insert(key_str, convert(&value, false)?);
        }
        return Ok(JValue::object(result));
    }

    // Fallback for subclasses, numpy types, etc.
    if let Ok(b) = obj.extract::<bool>() {
        return Ok(JValue::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>() {
        return Ok(JValue::Number(i as f64));
    }
    if let Ok(f) = obj.extract::<f64>() {
        return Ok(JValue::Number(f));
    }
    if let Ok(s) = obj.extract::<String>() {
        return Ok(JValue::string(s));
    }

    Err(PyTypeError::new_err(format!(
        "Cannot convert Python object to JSON: {}",
        obj.get_type().name().map(|n| n.to_string()).unwrap_or_default()
    )))
}
```

NOTE: the trailing error message above must match the existing wording in `python_to_json_bound` (`src/lib.rs:531`) — read that function and copy its exact `format!` string so error text does not change.

- [ ] **Step 2: Register the module and delegate the existing converter**

In `src/lib.rs`, next to the other module declarations:

```rust
#[cfg(feature = "python")]
pub mod lazy;
```

Replace the entire body of `python_to_json_bound` (keep its signature and doc comment) with:

```rust
    lazy::convert(obj, false)
```

Delete the now-duplicated conversion logic from `python_to_json_bound` (it moved verbatim into `lazy::convert`).

- [ ] **Step 3: Add the JValue variant**

In `src/value.rs`, add to the enum after `Regex { .. }`:

```rust
    /// Lazily-converted Python dict (see src/lazy.rs). Python-builds only.
    #[cfg(feature = "python")]
    LazyPyDict(Rc<crate::lazy::LazyPyDict>),
```

- [ ] **Step 4: Run cargo check and add every arm it demands**

Run: `cargo check --features python 2>&1 | grep -E "^error" | head -50`

Expected errors: non-exhaustive matches in `value.rs` (`Serialize`, `Display`), `src/lib.rs` (`json_to_python`), `src/signature.rs` (`type_symbol`). Fix each as follows (and if cargo flags additional exhaustive matches not listed here, handle them with the same rule: *pure type tests treat lazy as Object; value consumers materialize via `to_object()`/`to_object_ref()`*):

`value.rs` `is_object` (line ~77) — replace body:

```rust
    #[inline]
    pub fn is_object(&self) -> bool {
        match self {
            JValue::Object(_) => true,
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => true,
            _ => false,
        }
    }
```

`value.rs` `as_object` (line ~153) — add an arm (materializes on demand; None on conversion failure):

```rust
    pub fn as_object(&self) -> Option<&IndexMap<String, JValue>> {
        match self {
            JValue::Object(map) => Some(map),
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => lazy.to_object_ref(),
            _ => None,
        }
    }
```

`value.rs` `PartialEq` (line ~344) — add before the `_ => false` catch-all:

```rust
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(a), JValue::LazyPyDict(b)) => {
                Rc::ptr_eq(a, b)
                    || a.same_object(b)
                    || matches!((a.to_object_ref(), b.to_object_ref()), (Some(x), Some(y)) if x == y)
            }
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(a), JValue::Object(b)) => {
                a.to_object_ref().is_some_and(|x| x == &**b)
            }
            #[cfg(feature = "python")]
            (JValue::Object(a), JValue::LazyPyDict(b)) => {
                b.to_object_ref().is_some_and(|x| &**a == x)
            }
```

`value.rs` `Display` (line ~379) — add an arm that materializes and reuses the Object formatting by recursing:

```rust
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => match lazy.to_object() {
                Ok(map) => write!(f, "{}", JValue::Object(map)),
                Err(_) => write!(f, "{{}}"),
            },
```

`value.rs` `Serialize` (line ~446) — add an arm:

```rust
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => match lazy.to_object() {
                Ok(map) => {
                    let mut m = serializer.serialize_map(Some(map.len()))?;
                    for (k, v) in map.iter() {
                        m.serialize_entry(k, v)?;
                    }
                    m.end()
                }
                Err(e) => Err(serde::ser::Error::custom(e.0)),
            },
```

`src/lib.rs` `json_to_python` (line ~548) — add the pass-through arm (this is the output-side win: the original dict goes back out untouched):

```rust
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => Ok(lazy.py_object().clone_ref(py).into_any()),
```

(If `clone_ref(py).into_any()` does not typecheck against `Py<PyAny>`, use the equivalent that compiles — the intent is: clone the `Py<PyDict>` reference and upcast to `Py<PyAny>`, converting NOTHING.)

`src/signature.rs` `type_symbol` (line ~111) — add:

```rust
        #[cfg(feature = "python")]
        JValue::LazyPyDict(_) => 'o',
```

`src/signature.rs` `ParamType::matches` (line ~61) — add next to the Object arm:

```rust
            #[cfg(feature = "python")]
            (ParamType::Object, JValue::LazyPyDict(_)) => true,
```

- [ ] **Step 5: Add the EvaluatorError variant and From impl**

In `src/evaluator.rs`, extend the error enum (line ~2514):

```rust
    /// Python→JValue conversion failed during lazy field access.
    /// Surfaces as Python TypeError at the boundary (matching what eager
    /// conversion would have raised at call time).
    #[cfg(feature = "python")]
    #[error("Type error: {0}")]
    PyConversionError(String),
```

Extend `EvaluatorError::message()` (line ~2545) with the matching arm:

```rust
            #[cfg(feature = "python")]
            EvaluatorError::PyConversionError(m) => m,
```

Add below the impl:

```rust
#[cfg(feature = "python")]
impl From<crate::lazy::LazyConvertError> for EvaluatorError {
    fn from(e: crate::lazy::LazyConvertError) -> Self {
        EvaluatorError::PyConversionError(e.0)
    }
}
```

In `src/lib.rs`, replace `evaluator_error_to_py`:

```rust
fn evaluator_error_to_py(e: evaluator::EvaluatorError) -> PyErr {
    match e {
        evaluator::EvaluatorError::PyConversionError(m) => PyTypeError::new_err(m),
        other => PyValueError::new_err(other.message().to_string()),
    }
}
```

- [ ] **Step 6: Verify all builds and existing tests**

Run each; all must pass:

```bash
cargo check --features python
cargo test                     # pure-Rust suite, no python feature
uv run maturin develop --release
uv run pytest tests/python/test_reference_suite.py -q          # 1258 passed
JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py -q  # 1258 passed
```

(Lazy values are unreachable so behavior is unchanged; this task only proves compilation and no regression.)

- [ ] **Step 7: Commit**

```bash
git add src/lazy.rs src/lib.rs src/value.rs src/evaluator.rs src/signature.rs
git commit -m "feat(lazy): LazyPyDict core, JValue variant, and boundary arms (unreachable until flip)"
```

---

### Task 3: `_evaluate_lazy` test hook + VM field access

**Files:**
- Modify: `src/lib.rs` (new pymethod on `JsonataExpression`)
- Modify: `src/vm.rs` (`get_field_cached`, line ~644)
- Test: `tests/python/test_lazy_views.py` (create)

**Interfaces:**
- Consumes: `lazy::convert(obj, true)`, `LazyPyDict::get_field` (Task 2).
- Produces: private pymethod `JsonataExpression._evaluate_lazy(data, bindings=None)` — identical contract to `evaluate` but converts `data` lazily. TEMPORARY test hook, removed in Task 8. Tasks 4–7 write their tests against it.

- [ ] **Step 1: Add the hook**

In `src/lib.rs`, inside `#[pymethods] impl JsonataExpression`, add after `evaluate`:

```rust
    /// TEMPORARY (removed when lazy becomes the default): evaluate with
    /// lazy data conversion. Private test hook for the lazy-views rollout.
    #[pyo3(signature = (data, bindings=None))]
    fn _evaluate_lazy(
        &self,
        py: Python,
        data: Py<PyAny>,
        bindings: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let json_data = lazy::convert(data.bind(py), true)?;
        json_to_python(
            py,
            &self.run_eval(py, &json_data, bindings, self.default_options.clone())?,
        )
    }
```

- [ ] **Step 2: Write failing tests**

Create `tests/python/test_lazy_views.py`:

```python
"""Lazy Python views (LazyPyDict) behavior tests.

Written against the temporary JsonataExpression._evaluate_lazy hook while
the lazy path is being built out; Task 8 switches these to evaluate().
"""
import pytest
import jsonatapy


PRODUCTS = {
    "products": [
        {"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5, "inStock": i % 2 == 0}
        for i in range(10)
    ]
}


def lazy_eval(expr, data):
    return jsonatapy.compile(expr)._evaluate_lazy(data)


def eager_eval(expr, data):
    return jsonatapy.compile(expr).evaluate(data)


@pytest.mark.parametrize(
    "expr",
    [
        "products.price",                      # array field mapping
        "$sum(products.price)",                # fused aggregate
        "products[price > 20].id",             # filter + field
        "$count(products)",                    # array passthrough
        "products[0].name",                    # index + field
        "products.{'n': name, 'p': price}",    # object construction per element
    ],
)
def test_lazy_matches_eager_vm(expr):
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


def test_lazy_missing_field_is_undefined():
    # Missing path → None (undefined), same as eager
    assert lazy_eval("products[0].nosuch", PRODUCTS) is None
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `uv run maturin develop --release && uv run pytest tests/python/test_lazy_views.py -q`
Expected: FAIL — lazy values hit `get_field_cached`'s `_ => Ok(JValue::Undefined)` arm, so results are None/empty instead of the eager values.

- [ ] **Step 4: Add the VM lazy arm**

In `src/vm.rs` `get_field_cached` (line ~644), add an arm between the `Object` and `Array` arms:

```rust
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field)?),
```

(The positional shape cache is Object-specific and simply not used for lazy values; `LazyPyDict` has its own per-field cache. The `?` works via `From<LazyConvertError> for EvaluatorError` from Task 2.)

The `Array` arm's recursive `get_field_cached(item, ...)` call handles lazy *elements* automatically through the new arm — no further VM changes needed.

- [ ] **Step 5: Run tests to verify they pass**

Run: `uv run maturin develop --release && uv run pytest tests/python/test_lazy_views.py -q`
Expected: PASS (all — these expressions all compile to VM bytecode; if any individual expression falls back to the tree-walker and fails, move it to Task 4's test list rather than fixing the tree-walker here).

Also run: `uv run pytest tests/python/test_reference_suite.py -q` → 1258 passed (default path untouched).

- [ ] **Step 6: Commit**

```bash
git add src/lib.rs src/vm.rs tests/python/test_lazy_views.py
git commit -m "feat(lazy): VM field access on lazy dicts + _evaluate_lazy test hook"
```

---

### Task 4: Tree-walker field access

**Files:**
- Modify: `src/evaluator.rs` (8 sites, all located by function name — line numbers are anchors, verify by reading)
- Test: `tests/python/test_lazy_views.py` (extend)

**Interfaces:**
- Consumes: `LazyPyDict::get_field` (Task 2), `_evaluate_lazy` + `JSONATAPY_FORCE_TREE_WALKER` (Tasks 1, 3).

**General pattern** — every site below mirrors its existing `JValue::Object` sibling. `get_field` already returns `Undefined` for missing keys (matching `.get(field).cloned().unwrap_or(JValue::Undefined)`), and lazy dicts are never tuples so tuple probes are skipped. In array-mapping loops, mirror the loop's null/undefined-skip and array-flattening behavior exactly.

- [ ] **Step 1: Write failing tests**

Append to `tests/python/test_lazy_views.py`:

```python
@pytest.fixture()
def force_tree_walker(monkeypatch):
    monkeypatch.setenv("JSONATAPY_FORCE_TREE_WALKER", "1")


@pytest.mark.parametrize(
    "expr",
    [
        "products.price",
        "$sum(products.price)",
        "products[price > 20].id",
        "products[0].name",
        "products^(price).name",               # sort by field
        "products#$i.name",                    # index binding (tuple stream)
        "products.name[0]",                    # stage on mapped field
    ],
)
def test_lazy_matches_eager_tree_walker(expr, force_tree_walker):
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


def test_lazy_two_step_var_field(force_tree_walker):
    expr = "($p := products; $p.price)"
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


def test_lazy_dynamic_key_lookup(force_tree_walker):
    # evaluate_path_step (Object, String) arm
    expr = 'products[0].("na" & "me")'
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)
```

Run: `uv run pytest tests/python/test_lazy_views.py -q` → new tests FAIL (lazy values fall through Object-only arms).

- [ ] **Step 2: Add lazy arms at each site**

**(a) `compiled_field_step` (~line 1791):** after the `JValue::Object(obj) => { ... }` arm add:

```rust
        #[cfg(feature = "python")]
        JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field)?),
```

The Array arm needs no change: lazy elements don't match `(Some(ref sh), JValue::Object(obj))` so they take the `else { compiled_field_step(field, item, options)? }` recursion, which lands in the new arm.

**(b) Single-step fast path in `evaluate_path` (~line 4370):** in the `return match data { ... }`, add after the Object arm:

```rust
                    #[cfg(feature = "python")]
                    JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field_name)?),
```

In the same fast path's `JValue::Array(arr)` non-tuple loop (~line 4394), the loop body is `if let JValue::Object(obj) = item { ... } else if let JValue::Array(inner_arr) = item { ... }`. Add a lazy branch mirroring the Object branch's null-skip/flatten:

```rust
                                } else if let JValue::LazyPyDict(lazy) = item {
                                    #[cfg(feature = "python")]
                                    {
                                        let val = lazy.get_field(field_name)?;
                                        if !val.is_null() && !val.is_undefined() {
                                            match val {
                                                JValue::Array(arr_val) => {
                                                    result.extend(arr_val.iter().cloned());
                                                }
                                                other => result.push(other),
                                            }
                                        }
                                    }
                                }
```

(If the `#[cfg]`-inside-else-if construction fights the borrow checker or cfg attribute placement rules, gate the whole `else if` block with `#[cfg(feature = "python")]` on a helper: extract `fn lazy_field_mapped(lazy: &crate::lazy::LazyPyDict, field: &str, result: &mut Vec<JValue>) -> Result<(), EvaluatorError>` and call it — the shape shown is the required behavior, not the required syntax.)

**(c) First-step `AstNode::Name` arm (~line 4623):** add after the Object arm of `match data`:

```rust
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => {
                            let val = lazy.get_field(field_name)?;
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
```

In its Array loop (~line 4641) `match item` add a lazy arm mirroring the `JValue::Object(obj)` arm (same stage handling, same null/undefined skip, same flatten), with `let val = lazy.get_field(field_name)?;` replacing the `.get().cloned().unwrap_or(...)` line.

**(d) Multi-step `AstNode::Name` arm (~line 5037):** in `match &current` add after the Object arm:

```rust
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => {
                            did_array_mapping = false;
                            let val = lazy.get_field(field_name)?;
                            if !stages.is_empty() {
                                self.apply_stages(val, stages)?
                            } else {
                                val
                            }
                        }
```

In the fast non-tuple Array loop (~line 5072) `match item` add:

```rust
                                        #[cfg(feature = "python")]
                                        JValue::LazyPyDict(lazy) => {
                                            let val = lazy.get_field(field_name)?;
                                            if !val.is_null() && !val.is_undefined() {
                                                match val {
                                                    JValue::Array(arr_val) => {
                                                        result.extend(arr_val.iter().cloned())
                                                    }
                                                    other => result.push(other),
                                                }
                                            }
                                        }
```

In the tuple-supporting slow branch of the same loop (~line 5104), `match item` has a `JValue::Object(obj)` arm computing `(actual_obj, tuple_bindings)`. Add before the catch-all:

```rust
                                    #[cfg(feature = "python")]
                                    JValue::LazyPyDict(lazy) => {
                                        // Lazy dicts are never tuples; read directly.
                                        let val = lazy.get_field(field_name)?;
                                        // …then feed `val` through the same post-processing the
                                        // Object arm applies to its `val` (stages / flatten / skip):
                                        // replicate the code that follows `let val = actual_obj.get(...)`
                                        // in the Object arm, with tuple_bindings = None.
                                    }
```

Read the Object arm's continuation carefully and replicate it exactly for the lazy arm (it applies stages and pushes with flattening; there is too much surrounding context to inline here — the invariant is: lazy behaves like a non-tuple Object whose `.get(field)` is `get_field`).

**(e) 2-step `$var.field` fast path (~line 4536):** in `match value` add:

```rust
                            #[cfg(feature = "python")]
                            JValue::LazyPyDict(lazy) => {
                                let v = lazy.get_field(field_name)?;
                                return Ok(if v.is_undefined() { JValue::Null } else { v });
                            }
```

(The Object arm returns `unwrap_or(JValue::Null)` — mirror that: undefined → Null here.) In its Array loop add a lazy element arm mirroring the Object element handling (null-skip + flatten) as in (b).

**(f) `evaluate_leaf` `AstNode::Name` (~line 3305):** extend the match:

```rust
            AstNode::Name(field_name) => match data {
                JValue::Object(obj) => Some(Ok(obj
                    .get(field_name)
                    .cloned()
                    .unwrap_or(JValue::Undefined))),
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => {
                    Some(lazy.get_field(field_name).map_err(EvaluatorError::from))
                }
                _ => None,
            },
```

**(g) `eval_compiled_inner` `CompiledExpr::FieldLookup` (~line 959) and `NestedFieldLookup` (~line 978):**

```rust
        CompiledExpr::FieldLookup(field) => match data {
            // …existing Object arm…
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => Ok(lazy.get_field(field)?),
            _ => Ok(JValue::Undefined),
        },
```

For `NestedFieldLookup`, handle lazy at both levels: outer `data` lazy → `lazy.get_field(outer)?` then match the result (`Object` → `.get(inner)`, lazy → `.get_field(inner)?`, other → Undefined). Also extend the existing Object arm's inner match (line ~991: `JValue::Object(nested) => nested.get(...)`) with a lazy-inner arm.

**(h) `evaluate_path_step` dynamic-key arm (~line 5987):** in `match (current, &step_value)` add:

```rust
                #[cfg(feature = "python")]
                (JValue::LazyPyDict(lazy), JValue::String(key)) => lazy.get_field(&**key)?,
```

- [ ] **Step 3: Run tests to verify they pass**

```bash
uv run maturin develop --release
uv run pytest tests/python/test_lazy_views.py -q       # all pass
uv run pytest tests/python/test_reference_suite.py -q  # 1258 passed
cargo test                                              # pure-Rust still green
```

- [ ] **Step 4: Commit**

```bash
git add src/evaluator.rs tests/python/test_lazy_views.py
git commit -m "feat(lazy): tree-walker field access on lazy dicts (8 sites)"
```

---

### Task 5: Object consumers — builtins, sort, transform, equality, truthiness

**Files:**
- Modify: `src/evaluator.rs` (dispatch normalization + listed sites)
- Modify: `src/functions.rs` (`values_equal`, `merge`, plus any `JValue::Object` arm found by grep)
- Test: `tests/python/test_lazy_views.py` (extend)

**Interfaces:**
- Consumes: `LazyPyDict::{get_field, to_object, to_object_ref, is_empty, contains_field}` (Task 2).
- Produces: helper `fn normalize_lazy(value: &JValue) -> Result<JValue, EvaluatorError>` in `src/evaluator.rs`, used by later triage work (Task 6).

- [ ] **Step 1: Write failing tests**

Append to `tests/python/test_lazy_views.py` (each runs via both engines — parametrize the fixture):

```python
OBJ = {"a": 1, "b": {"c": 2}, "d": [1, 2]}


@pytest.fixture(params=["vm", "tree"])
def engine(request, monkeypatch):
    if request.param == "tree":
        monkeypatch.setenv("JSONATAPY_FORCE_TREE_WALKER", "1")
    return request.param


@pytest.mark.parametrize(
    "expr,data",
    [
        ("$keys($)", OBJ),
        ("$spread($)", OBJ),
        ("$lookup($, 'a')", OBJ),
        ("$merge([$, {'e': 5}])", OBJ),
        ("$each($, function($v, $k) { $k })", OBJ),
        ("$sift($, function($v) { $v = 1 })", OBJ),
        ("$string($)", OBJ),
        ("$type($)", OBJ),
        ("$boolean($)", OBJ),
        ("$boolean($)", {}),                      # empty dict → false
        ("$exists(b.c)", OBJ),
        ("'a' in $", OBJ),
        ("$ = {'a': 1, 'b': {'c': 2}, 'd': [1, 2]}", OBJ),   # deep equality lazy vs constructed
        ("$distinct([b, b, {'c': 2}])", OBJ),
        ("products^(price)", PRODUCTS),           # specialized sort comparator keys
        ("$sort(products, function($l, $r) { $l.price > $r.price })", PRODUCTS),
        ("$ ~> | b | {'c': 99} |", OBJ),          # transform operator
        ("products#$i.($i & ':' & name)", PRODUCTS),  # tuple stream (# index binding) over lazy elements
    ],
)
# NOTE: the `@` tuple-binding operator is NOT implemented in this codebase
# (deferred work) — do not add `@` expressions to these tests.
def test_lazy_consumers_match_eager(expr, data, engine):
    assert lazy_eval(expr, data) == eager_eval(expr, data)
```

Run: `uv run pytest tests/python/test_lazy_views.py -q` — new tests FAIL (varied wrong results / type errors).

- [ ] **Step 2: Add the normalization helper**

In `src/evaluator.rs` near `call_pure_builtin`:

```rust
/// Materialize a top-level lazy dict into a plain Object. Non-lazy values
/// pass through unchanged. Does NOT recurse into arrays/objects — element-
/// level laziness is handled by the specific consumers that need it.
#[cfg(feature = "python")]
pub(crate) fn normalize_lazy(value: &JValue) -> Result<JValue, EvaluatorError> {
    match value {
        JValue::LazyPyDict(lazy) => Ok(JValue::Object(lazy.to_object()?)),
        _ => Ok(value.clone()),
    }
}

#[cfg(not(feature = "python"))]
pub(crate) fn normalize_lazy(value: &JValue) -> Result<JValue, EvaluatorError> {
    Ok(value.clone())
}
```

- [ ] **Step 3: Normalize at the two dispatch choke points**

**(a) `call_pure_builtin` (~line 1923):** immediately after `effective_args` is finalized (after the context-insertion `if/else` chain, before the `propagates_undefined` check), insert:

```rust
    // Materialize top-level lazy args so every builtin sees plain Objects.
    #[cfg(feature = "python")]
    let lazy_storage: Vec<JValue>;
    #[cfg(feature = "python")]
    let effective_args: &[JValue] = if effective_args
        .iter()
        .any(|a| matches!(a, JValue::LazyPyDict(_)))
    {
        lazy_storage = effective_args
            .iter()
            .map(normalize_lazy)
            .collect::<Result<Vec<_>, _>>()?;
        &lazy_storage
    } else {
        effective_args
    };
```

**(b) `evaluate_function_call` main builtin path (~line 6965):** after the `evaluated_args` loop and the context-insertion block (i.e., just before `match name {` at ~line 7014), insert:

```rust
        #[cfg(feature = "python")]
        for arg in evaluated_args.iter_mut() {
            if matches!(arg, JValue::LazyPyDict(_)) {
                *arg = normalize_lazy(arg)?;
            }
        }
```

Do NOT normalize the lambda-dispatch paths (lines ~6777, ~6785, ~6796) — lambdas access args via field lookups which are lazy-aware (Tasks 3–4), and normalizing would destroy the pass-through win for HOF pipelines.

- [ ] **Step 4: Fix the consumers that bypass `evaluated_args`**

**(a) `$each` (~line 8894):** `obj_value` comes from `data.clone()` or a fresh `evaluate_internal` — add before the match: `let obj_value = normalize_lazy(&obj_value)?;`

**(b) `$sift` 1-arg form (~line 8040):** the `match data` has `JValue::Object(o)` and an Array loop with `if let JValue::Object(o) = item`. Insert at the top of the 1-arg branch:

```rust
                    #[cfg(feature = "python")]
                    let data = &normalize_lazy(data)?;
```

and in the Array loop, handle lazy elements by materializing:

```rust
                            for item in arr.iter() {
                                #[cfg(feature = "python")]
                                let item = &normalize_lazy(item)?;
                                if let JValue::Object(o) = item {
```

(2-arg `$sift` reads `evaluated_args[0]`, already normalized by Step 3b.)

**(c) `functions::object::merge` (src/functions.rs ~line 1795):** array elements may be lazy. `functions.rs` must not depend on `EvaluatorError`, so materialize inline via `to_object_ref`:

```rust
        for obj in objects {
            #[cfg(feature = "python")]
            if let JValue::LazyPyDict(lazy) = obj {
                match lazy.to_object_ref() {
                    Some(map) => {
                        for (k, v) in map.iter() {
                            result.insert(k.clone(), v.clone());
                        }
                        continue;
                    }
                    None => {
                        return Err(FunctionError::TypeError(
                            "merge() argument could not be converted".to_string(),
                        ))
                    }
                }
            }
            match obj {
                JValue::Object(map) => { /* existing */ }
```

**(d) evaluator's `$merge`/`$spread` single-object passthroughs (lines ~2383 and ~8348):** these match `JValue::Object(_) => Ok(args[0].clone())`. Extend the patterns:

```rust
                JValue::Object(_) => Ok(effective_args[0].clone()),
```
→ after Step 3's normalization these can no longer see lazy values; verify by reading the surrounding code that the value flows from the normalized slice, and if any path can still deliver a lazy value, extend the pattern with `#[cfg(feature = "python")] JValue::LazyPyDict(_) => …normalize…`.

**(e) Sort — three spots:**
1. `merge_sort_specialized` key extraction (~line 3112): add to the `match item`:

```rust
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => match lazy.get_field(&spec.field) {
                    Ok(JValue::Number(n)) => SortKey::Num(n),
                    Ok(JValue::String(s)) => SortKey::Str(s.clone()),
                    _ => SortKey::None,
                },
```

2. `evaluate_sort_term` single-field path (~line 4982 area, `if let JValue::Object(obj) = &actual_element`): convert the `if let` to a match with a lazy arm:

```rust
                    match &actual_element {
                        JValue::Object(obj) => {
                            return match obj.get(field_name) {
                                Some(val) => Ok(val.clone()),
                                None => Ok(JValue::Undefined),
                            };
                        }
                        #[cfg(feature = "python")]
                        JValue::LazyPyDict(lazy) => {
                            return Ok(lazy.get_field(field_name)?);
                        }
                        _ => return Ok(JValue::Undefined),
                    }
```

(`get_field` distinguishes present-but-null (`Null`) from missing (`Undefined`) exactly as the Object arm does.)

3. `evaluate_sort_term` tuple probe (~line 10967) and `evaluate_sort`'s tuple probes (~lines 11051, 11063): these are `if let JValue::Object(obj) = element` tuple checks — lazy elements fall through to the non-tuple path, which is correct. No change; just verify.

**(f) `apply_transform_deep` (~line 9461):** two changes:
- The transform branch: `if let JValue::Object(map_rc) = value.clone()` — a lazy target would silently pass through unchanged. Materialize first:

```rust
            if targets.iter().any(|t| t == value) {
                #[cfg(feature = "python")]
                let value = &normalize_lazy(value)?;
                if let JValue::Object(map_rc) = value.clone() {
```

- The recursion `match value`: add a lazy arm above the Object arm that materializes and recurses:

```rust
                #[cfg(feature = "python")]
                JValue::LazyPyDict(lazy) => {
                    let obj = JValue::Object(lazy.to_object().map_err(EvaluatorError::from)?);
                    apply_transform_deep(evaluator, &obj, targets, update, delete_fields)
                }
```

Also check the `targets` construction (~line 9425) — it matches `JValue::Object(_) => vec![located_objects]`; extend:

```rust
            JValue::Object(_) => vec![located_objects],
            #[cfg(feature = "python")]
            JValue::LazyPyDict(_) => vec![located_objects],
```

**(g) `values_equal` (src/functions.rs ~line 1730):** add before the catch-all:

```rust
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(x), _) => x
                .to_object_ref()
                .is_some_and(|m| values_equal(&JValue::Object(std::rc::Rc::new(m.clone())), b)),
            #[cfg(feature = "python")]
            (_, JValue::LazyPyDict(y)) => y
                .to_object_ref()
                .is_some_and(|m| values_equal(a, &JValue::Object(std::rc::Rc::new(m.clone())))),
```

NOTE: the `m.clone()` above clones the IndexMap — acceptable for the rare deep-equality case, but if a cheaper formulation compiles (e.g. comparing entry-by-entry against `to_object_ref()` maps directly, mirroring the `(Object, Object)` arm's logic), prefer it:

```rust
            #[cfg(feature = "python")]
            (JValue::LazyPyDict(x), JValue::Object(bm)) => x
                .to_object_ref()
                .is_some_and(|am| am.len() == bm.len()
                    && am.iter().all(|(k, v)| bm.get(k).is_some_and(|v2| values_equal(v, v2)))),
```
…and the symmetric arms (`(Object, Lazy)`, `(Lazy, Lazy)` via both `to_object_ref()`s).

**(h) `in_operator` (~line 11843):** add to `match right`:

```rust
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => {
                if let JValue::String(key) = left {
                    Ok(JValue::Bool(lazy.contains_field(key)))
                } else {
                    Ok(JValue::Bool(false))
                }
            }
```

**(i) `is_truthy` (~line 11245):** add:

```rust
            #[cfg(feature = "python")]
            JValue::LazyPyDict(lazy) => !lazy.is_empty(),
```

**(j) Undefined-propagation field probe (~line 6869):** `matches!(data, JValue::Object(obj) if obj.contains_key(field_name))` — extend to also accept lazy:

```rust
            matches!(data, JValue::Object(obj) if obj.contains_key(field_name))
                || {
                    #[cfg(feature = "python")]
                    { matches!(data, JValue::LazyPyDict(l) if l.contains_field(field_name)) }
                    #[cfg(not(feature = "python"))]
                    { false }
                }
```

(Read the actual site first — replicate its exact boolean context.)

**(k) Pure type-test pattern extensions** — at each of these `src/evaluator.rs` sites, extend the pattern so lazy classifies as "object" (add `` #[cfg(feature = "python")] `` arms or widen the pattern using a leading `|` alternative gated appropriately; where a `|`-pattern can't carry a cfg, add a separate cfg'd arm with the same body):
  - `:1581` string coercion set `JValue::Number(_) | JValue::Bool(_) | JValue::Array(_) | JValue::Object(_)`
  - `:2148` `$join` T0412 rejection set
  - `:8960` `$type()` → `"object"`
  - `:11118` sort-key type-name classification → `"object"`
  - `:11221`/`:11226`/`:11231`/`:11236`/`:11238` `compare_values` ordering arms (lazy sorts like Object)
  - `:11541` `type_name` helper → `"object"`
  - `:11687` `value_to_concat_string` coercion set

**(l) `src/functions.rs` residual Object arms:** run `grep -n "JValue::Object" src/functions.rs` and inspect each hit. `keys`/`lookup`/`spread` take `&IndexMap` (normalized by dispatch — no change). `stringify_value_custom` (used by `functions::string::string`, line ~133 region) needs a lazy arm that materializes via `to_object_ref()` and formats like Object (or is unreachable if dispatch normalization covers all `$string` calls — verify by test `$string($)` on both engines before deciding no change is needed).

- [ ] **Step 5: Run tests to verify they pass**

```bash
uv run maturin develop --release
uv run pytest tests/python/test_lazy_views.py -q       # all pass
uv run pytest tests/python/test_reference_suite.py -q  # 1258 passed
cargo test
```

- [ ] **Step 6: Commit**

```bash
git add src/evaluator.rs src/functions.rs tests/python/test_lazy_views.py
git commit -m "feat(lazy): object consumers — builtin dispatch normalization, sort, transform, equality, truthiness"
```

---

### Task 6: Lazy-mode reference suite green (both engines)

**Files:**
- Modify: `tests/python/test_reference_suite.py` (env-gated lazy mode)
- Modify: `src/evaluator.rs` / `src/functions.rs` / `src/vm.rs` (triage fixes only)

**Interfaces:**
- Consumes: everything above.
- Produces: env var `JSONATAPY_TEST_LAZY=1` making the reference suite call `_evaluate_lazy` instead of `evaluate` (TEMPORARY, removed in Task 8).

- [ ] **Step 1: Plumb lazy mode into the suite**

In `tests/python/test_reference_suite.py`, the evaluation call (~line 171) is:

```python
        result = compiled.evaluate(data, bindings) if bindings else compiled.evaluate(data)
```

Replace with:

```python
        import os
        if os.environ.get("JSONATAPY_TEST_LAZY") == "1":
            result = compiled._evaluate_lazy(data, bindings) if bindings else compiled._evaluate_lazy(data)
        else:
            result = compiled.evaluate(data, bindings) if bindings else compiled.evaluate(data)
```

(Put the `import os` at the top of the file, not inline, matching the file's style.)

- [ ] **Step 2: Run the lazy-mode suite and triage to zero — VM mode**

Run: `JSONATAPY_TEST_LAZY=1 uv run pytest tests/python/test_reference_suite.py -q`

For each failure, apply the standard recipe:
1. Reproduce minimally: `uv run python -c "import jsonatapy; print(jsonatapy.compile('<expr>')._evaluate_lazy(<data>))"`.
2. Find the consuming code site (grep for the operator/builtin in `src/evaluator.rs` / `src/vm.rs` / `src/functions.rs`).
3. Fix using the established patterns, in order of preference:
   - **Hot field access** → add a `JValue::LazyPyDict(lazy) => lazy.get_field(field)?` arm mirroring the Object arm.
   - **Whole-object consumer** → `normalize_lazy(...)` at the point of consumption.
   - **Pure type test** → extend the pattern to classify lazy as Object.
4. Add a regression case to `tests/python/test_lazy_views.py` reproducing the expression shape.

Iterate until: 1258 passed.

- [ ] **Step 3: Same for tree-walker mode**

Run: `JSONATAPY_TEST_LAZY=1 JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py -q`
Triage with the same recipe until: 1258 passed.

- [ ] **Step 4: Re-verify all four suite modes + Rust**

```bash
uv run pytest tests/python/test_reference_suite.py -q                                     # 1258
JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py -q       # 1258
JSONATAPY_TEST_LAZY=1 uv run pytest tests/python/test_reference_suite.py -q               # 1258
JSONATAPY_TEST_LAZY=1 JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py -q  # 1258
cargo test
uv run pytest tests/python/test_lazy_views.py -q
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "test(lazy): full reference suite green in lazy mode on both engines"
```

---

### Task 7: Pass-through identity, value fidelity, and lazy-error semantics

**Files:**
- Test: `tests/python/test_lazy_views.py` (extend; implementation should already exist — fix only if a test exposes a gap)

- [ ] **Step 1: Write the tests**

```python
class TestPassThrough:
    def test_filter_returns_original_dict_objects(self):
        data = {"products": [{"id": 1, "big": list(range(100))}, {"id": 2}]}
        expr = jsonatapy.compile("products[id = 1]")
        result = expr._evaluate_lazy(data)
        assert result is data["products"][0]          # identity, not a copy

    def test_pass_through_preserves_int_fidelity(self):
        data = {"items": [{"n": 1}]}
        result = jsonatapy.compile("items[n = 1]")._evaluate_lazy(data)
        assert result is data["items"][0]
        assert isinstance(result["n"], int)

    def test_mutation_visible_between_calls(self):
        data = {"a": 1}
        expr = jsonatapy.compile("a")
        assert expr._evaluate_lazy(data) == 1
        data["a"] = 2
        assert expr._evaluate_lazy(data) == 2         # no implicit caching


class TestLazyErrors:
    BAD = {"good": 1, "bad": {1, 2, 3}}               # a set is not convertible

    def test_untouched_bad_field_succeeds(self):
        assert jsonatapy.compile("good")._evaluate_lazy(self.BAD) == 1

    def test_touched_bad_field_raises_typeerror(self):
        with pytest.raises(TypeError):
            jsonatapy.compile("bad")._evaluate_lazy(self.BAD)

    def test_materializing_bad_object_raises_typeerror(self):
        with pytest.raises(TypeError):
            jsonatapy.compile("$keys($)")._evaluate_lazy(self.BAD)
```

- [ ] **Step 2: Run and fix any gaps**

Run: `uv run pytest tests/python/test_lazy_views.py -q`

Expected mostly-pass; likely gap: `test_untouched_bad_field_succeeds` — the top-level dict is wrapped lazily so `bad` is only converted if touched; `good` access must not convert `bad`. If it fails, the culprit is an eager materialization somewhere on the simple-path route — find it (it violates the lazy design) and fix. If `test_touched_bad_field_raises_typeerror` raises `ValueError` instead of `TypeError`, the `PyConversionError → PyTypeError` mapping from Task 2 Step 5 is broken — fix there. IMPORTANT: `bad` reaches the output boundary as an *unconverted set inside a lazy wrapper only if untouched*; when the expression's RESULT contains the bad value, conversion happens and the TypeError is correct behavior.

- [ ] **Step 3: Commit**

```bash
git add tests/python/test_lazy_views.py src/  # src only if gaps were fixed
git commit -m "test(lazy): pass-through identity, int fidelity, mutation visibility, lazy TypeError semantics"
```

---

### Task 8: Flip the default and remove the scaffolding

**Files:**
- Modify: `src/lib.rs` (`JsonataExpression::evaluate`, module-level `evaluate` function ~line 443, remove `_evaluate_lazy`)
- Modify: `tests/python/test_reference_suite.py` (remove `JSONATAPY_TEST_LAZY` plumbing)
- Modify: `tests/python/test_lazy_views.py` (switch `lazy_eval` to use `evaluate`)

**Interfaces:**
- Consumes: everything above.
- Produces: `evaluate(dict)` and the module-level one-shot `evaluate(expr, data)` convert data lazily by default. `JsonataData` remains eager (do NOT touch it). Bindings conversion in `create_evaluator` remains eager (do NOT touch it).

- [ ] **Step 1: Flip the two entry points**

In `JsonataExpression::evaluate` (~line 214), change:

```rust
        let json_data = python_to_json(py, &data)?;
```
to:
```rust
        let json_data = lazy::convert(data.bind(py), true)?;
```

In the module-level `evaluate` pyfunction (~line 443), find its `python_to_json(py, &data)` call for the *data* argument (NOT any bindings conversion) and apply the same change.

Leave unchanged: `JsonataData::new`, `create_evaluator` (bindings), `evaluate_json*` (JSON-string paths have no Python objects).

- [ ] **Step 2: Remove the scaffolding**

- Delete the `_evaluate_lazy` pymethod from `src/lib.rs`.
- In `tests/python/test_reference_suite.py`, revert Step 1 of Task 6 (plain `compiled.evaluate(...)` again).
- In `tests/python/test_lazy_views.py`, change the helpers:

```python
def lazy_eval(expr, data):
    return jsonatapy.compile(expr).evaluate(data)


def eager_eval(expr, data):
    # Eager reference behavior via the pre-converted data handle.
    return jsonatapy.compile(expr).evaluate_with_data(jsonatapy.JsonataData(data))
```

and update `TestPassThrough`/`TestLazyErrors` to call `.evaluate(...)` instead of `._evaluate_lazy(...)`.

Keep `JSONATAPY_FORCE_TREE_WALKER` (generally useful for engine-coverage testing).

- [ ] **Step 3: Full verification**

```bash
uv run maturin develop --release
uv run pytest tests/python/test_reference_suite.py -q                                # 1258
JSONATAPY_FORCE_TREE_WALKER=1 uv run pytest tests/python/test_reference_suite.py -q  # 1258
uv run pytest tests/python/ -q                                                       # whole Python test dir
cargo test
cargo clippy --features python -- -D warnings
cargo fmt --check
```

If any pre-existing Python test asserts deep-copy output semantics (a test that mutates a result and checks the input is unchanged), it will now fail by design — update that test to the new aliasing contract and note it in the commit message.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat!: evaluate(dict) converts lazily by default — results may alias input dicts (matches jsonata-js)"
```

---

### Task 9: Benchmark verification against spec targets

**Files:**
- Create: `benchmarks/python/lazy_check.py` (small target-gate script; not wired into CI)

- [ ] **Step 1: Write the gate script**

```python
"""Quick gate: lazy-views performance targets from the 2026-07-12 spec.

Run manually: uv run python benchmarks/python/lazy_check.py
"""
import time
import jsonatapy

def best_us(fn, n, trials=5):
    fn()
    best = float("inf")
    for _ in range(trials):
        t0 = time.perf_counter()
        for _ in range(n):
            fn()
        best = min(best, (time.perf_counter() - t0) / n * 1e6)
    return best

products = {"products": [{"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5,
                          "inStock": i % 2 == 0} for i in range(100)]}
ecommerce = {"products": [{"id": i, "name": f"Product {i}",
                           "category": ["Electronics", "Clothing", "Books", "Home"][i % 4],
                           "price": 10.0 + i * 5.5, "inStock": i % 3 != 0,
                           "rating": 3.0 + (i % 3) * 0.5, "reviews": i * 2,
                           "tags": [f"tag{j}" for j in range(i % 5)],
                           "vendor": {"name": f"Vendor {i % 10}", "rating": 4.0 + (i % 5) * 0.2}}
                          for i in range(100)]}

CASES = [
    ("products.price", products, 2000, 10.0),
    ('products[category = "Electronics"]', ecommerce, 1000, 15.0),
    ("$sum(products[inStock].price)", ecommerce, 1000, 20.0),
    ('products[price > 50 and inStock].{"name": name, "price": price, "vendor": vendor.name}',
     ecommerce, 500, 177.0),   # 160.9µs baseline × 1.10 regression gate
]

failed = False
for src, data, iters, target in CASES:
    e = jsonatapy.compile(src)
    us = best_us(lambda: e.evaluate(data), iters)
    ok = us <= target
    failed |= not ok
    print(f"{'PASS' if ok else 'FAIL'}  {us:8.1f}µs (target ≤{target}µs)  {src[:60]}")

raise SystemExit(1 if failed else 0)
```

- [ ] **Step 2: Run it**

Run: `uv run maturin develop --release && uv run python benchmarks/python/lazy_check.py`
Expected: all PASS.

**If the dense-transformation row FAILS (>177µs):** STOP. Do not implement the contingency (compile-time field-usage analysis) unilaterally — report the numbers back for a decision, per the spec's "added only if measurement demands it" clause.

- [ ] **Step 3: Run the real benchmark suite for the record**

Run the repo's Python benchmark comparison (see `benchmarks/README.md` for the exact invocation) and capture the array/realistic-workload rows before/after in the commit message.

- [ ] **Step 4: Commit**

```bash
git add benchmarks/python/lazy_check.py
git commit -m "bench(lazy): performance gate for spec targets (all passing)"
```

---

### Task 10: Docs, changelog, version bump

**Files:**
- Modify: `docs/changelog.md` (new entry at top; follow the existing entry format exactly)
- Modify: `docs/optimization-tips.md` and `docs/usage.md` (aliasing + laziness callouts)
- Modify: `Cargo.toml` (version `2.2.3` → `2.3.0`); run `grep -rn "2\.2\.3" pyproject.toml python/ docs/` and update any other hardcoded version the repo convention updates on bump (check how commit 9a431a2 "chore: Bump version to 2.2.3" did it: `git show 9a431a2 --stat`)

- [ ] **Step 1: Changelog entry**

Add a `## 2.3.0` entry covering, in the file's established style:
- `evaluate(dict)` now converts Python data lazily — fields the expression never touches are never converted; large speedups on array workloads (cite the Task 9 numbers).
- **Behavior change:** results containing unmodified input subtrees now reference the caller's original dicts (aliasing, matches jsonata-js). Mutating a result mutates the input. Passed-through values keep exact Python types (`int` stays `int`).
- **Behavior change:** unconvertible values (e.g. a `set`) now raise `TypeError` only when the expression actually touches them.
- New `JSONATAPY_FORCE_TREE_WALKER=1` testing toggle.

- [ ] **Step 2: Docs callouts**

In `docs/optimization-tips.md` and the performance note in `docs/usage.md`: explain lazy conversion is now the default for `evaluate(dict)`; `JsonataData` remains the fastest path for *repeated* evaluation of the same data (conversion paid once); results may alias input dicts — copy explicitly if you plan to mutate.

- [ ] **Step 3: Version bump + final verification**

```bash
git show 9a431a2 --stat        # replicate the bump-commit's file set
# edit the files it shows
cargo build --features python  # Cargo.lock refresh
uv run maturin develop --release && uv run pytest tests/python/ -q && cargo test
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "chore: bump version to 2.3.0; changelog and docs for lazy Python views"
```

---

## Post-plan

After all tasks: use superpowers:finishing-a-development-branch (PR to `main`, CI green, benchmarks re-run on the Mac Mini runner via CI).
