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
    /// Converted-on-first-access fields, heap-typed values only (String,
    /// Array, Object, LazyPyDict). Scalars and absent-key `Undefined` are
    /// deliberately not cached — see `get_field`.
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
    ///
    /// Any `PyErr` raised while reading or converting the value -- not just the
    /// `PyTypeError` from `convert()`'s own unconvertible-type check, but also e.g.
    /// `OverflowError` from a Python int too large to fit an `i64`/`f64`, or an error
    /// from `dict.get_item` itself -- is captured here via `.to_string()` and wrapped in
    /// `LazyConvertError`, which `From<LazyConvertError> for EvaluatorError` maps to
    /// `PyConversionError` and ultimately a Python `TypeError` at the boundary. The
    /// original Python exception *type* is not preserved; only its message text is.
    pub fn get_field(&self, field: &str) -> Result<JValue, LazyConvertError> {
        if let Some(m) = self.materialized.get() {
            return Ok(m.get(field).cloned().unwrap_or(JValue::Undefined));
        }
        if let Some(v) = self.field_cache.borrow().get(field) {
            return Ok(v.clone());
        }
        Python::attach(|py| {
            let dict = self.obj.bind(py);
            let val = match dict
                .get_item(field)
                .map_err(|e| LazyConvertError(e.to_string()))?
            {
                Some(v) => convert(&v, true).map_err(|e| LazyConvertError(e.to_string()))?,
                None => JValue::Undefined,
            };
            // Cache only heap-typed conversions. Scalars (Number/Bool/Null) are
            // cheaper to reconvert (~one C-API call) than to cache (String key
            // alloc + HashMap insert), and Undefined (missing key) repeats are
            // rare. Heap values keep conversion reuse, and nested dicts MUST stay
            // cached so repeated access yields the same Rc-wrapped wrapper
            // (identity for PartialEq's Rc::ptr_eq fast path and transform target
            // detection).
            if matches!(
                val,
                JValue::String(_) | JValue::Array(_) | JValue::Object(_) | JValue::LazyPyDict(_)
            ) {
                self.field_cache
                    .borrow_mut()
                    .insert(field.to_string(), val.clone());
            }
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
        Python::attach(|py| self.obj.bind(py).contains(field).unwrap_or(false))
    }

    /// Emptiness check without converting anything (used for truthiness).
    pub fn is_empty(&self) -> bool {
        if let Some(m) = self.materialized.get() {
            return m.is_empty();
        }
        Python::attach(|py| self.obj.bind(py).is_empty())
    }

    /// Full materialization. Iterates the Python dict in insertion order,
    /// reusing per-field cached (heap-typed) conversions where available and
    /// reconverting uncached (scalar) fields directly. Memoized.
    ///
    /// Iteration is over the dict's actual keys, so absent keys never
    /// appear here regardless of caching — `field_cache` only ever holds
    /// present-key values (scalars aren't cached at all; see `get_field`).
    pub fn to_object(&self) -> Result<Rc<IndexMap<String, JValue>>, LazyConvertError> {
        if let Some(m) = self.materialized.get() {
            return Ok(m.clone());
        }
        let map = Python::attach(|py| -> Result<IndexMap<String, JValue>, LazyConvertError> {
            let dict = self.obj.bind(py);
            let cache = self.field_cache.borrow();
            let mut map = IndexMap::with_capacity(dict.len());
            for (k, v) in dict.iter() {
                let key: String = k
                    .extract::<String>()
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
    /// Returns `None` if conversion fails, and the failure is dropped here --
    /// this method has no way to report it. Callers that need the conversion
    /// error to actually surface (e.g. equality, concatenation) must not rely
    /// on this method; they should call `to_object()` (or the `normalize_lazy`
    /// helper in `evaluator.rs`) instead, which returns `Result` and propagates
    /// the error as a Python `TypeError` at the boundary.
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
        obj.get_type().name()?
    )))
}
