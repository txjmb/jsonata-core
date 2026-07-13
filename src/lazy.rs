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
        Python::attach(|py| {
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
    /// reusing per-field cached conversions. Memoized.
    ///
    /// Absent-key `Undefined` markers in `field_cache` are naturally
    /// excluded: iteration is over the dict's actual keys, and a present
    /// key never caches as Undefined (conversion never yields Undefined).
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
        obj.get_type().name()?
    )))
}
