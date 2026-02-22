// jsonatapy - High-performance Python implementation of JSONata
// Copyright (c) 2025 jsonatapy contributors
// Licensed under the MIT License

//! # jsonatapy
//!
//! A high-performance Rust implementation of JSONata - the JSON query and
//! transformation language - with optional Python bindings via PyO3.
//!
//! ## Rust API
//!
//! ```rust,ignore
//! use jsonata_core::parser;
//! use jsonata_core::evaluator::Evaluator;
//! use jsonata_core::value::JValue;
//!
//! let ast = parser::parse("user.name").unwrap();
//! let data = JValue::from_json_str(r#"{"user":{"name":"Alice"}}"#).unwrap();
//! let result = Evaluator::new().evaluate(&ast, &data).unwrap();
//! ```
//!
//! ## Architecture
//!
//! - `parser` - Expression parser (converts JSONata strings to AST)
//! - `evaluator` - Expression evaluator (executes AST against data)
//! - `functions` - Built-in function implementations
//! - `datetime` - Date/time handling functions
//! - `signature` - Function signature validation
//! - `ast` - Abstract Syntax Tree definitions
//! - `value` - JValue type (the runtime value representation)

pub mod ast;
mod compiler;
mod datetime;
pub mod evaluator;
pub mod functions;
pub mod parser;
mod signature;
pub mod value;
mod vm;

// ── Python bindings (only when the "python" feature is enabled) ───────────────

#[cfg(feature = "python")]
use crate::value::JValue;
#[cfg(feature = "python")]
use indexmap::IndexMap;
#[cfg(feature = "python")]
use pyo3::exceptions::{PyTypeError, PyValueError};
#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString};

/// Pre-converted data handle for efficient repeated evaluation.
///
/// Convert Python data to an internal representation once, then reuse it
/// across multiple evaluations to avoid repeated Python↔Rust conversion overhead.
///
/// # Examples
///
/// ```python
/// import jsonatapy
///
/// data = jsonatapy.JsonataData({"orders": [{"price": 150}, {"price": 50}]})
/// expr = jsonatapy.compile("orders[price > 100]")
/// result = expr.evaluate_with_data(data)
/// ```
#[cfg(feature = "python")]
#[pyclass(unsendable)]
struct JsonataData {
    data: JValue,
}

#[cfg(feature = "python")]
#[pymethods]
impl JsonataData {
    /// Create from a Python object (dict, list, etc.)
    #[new]
    fn new(py: Python, data: PyObject) -> PyResult<Self> {
        let jvalue = python_to_json(py, &data)?;
        Ok(JsonataData { data: jvalue })
    }

    /// Create from a JSON string (fastest path).
    #[staticmethod]
    fn from_json(json_str: &str) -> PyResult<Self> {
        let data = JValue::from_json_str(json_str)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;
        Ok(JsonataData { data })
    }
}

/// A compiled JSONata expression that can be evaluated against data.
///
/// This is the main entry point for using JSONata. Compile an expression once,
/// then evaluate it multiple times against different data.
///
/// # Examples
///
/// ```python
/// import jsonatapy
///
/// # Compile once
/// expr = jsonatapy.compile("orders[price > 100].product")
///
/// # Evaluate many times
/// data1 = {"orders": [{"product": "A", "price": 150}]}
/// result1 = expr.evaluate(data1)
///
/// data2 = {"orders": [{"product": "B", "price": 50}]}
/// result2 = expr.evaluate(data2)
/// ```
#[cfg(feature = "python")]
#[pyclass(unsendable)]
struct JsonataExpression {
    /// The parsed Abstract Syntax Tree
    ast: ast::AstNode,
    /// Lazily compiled bytecode — populated on first evaluate() call.
    /// `Some(bc)` = fast VM path; `None` = must use tree-walker.
    /// `OnceCell` ensures compilation happens at most once per expression instance.
    bytecode: std::cell::OnceCell<Option<vm::BytecodeProgram>>,
}

#[cfg(feature = "python")]
#[pymethods]
impl JsonataExpression {
    /// Evaluate this expression against the provided data.
    ///
    /// # Arguments
    ///
    /// * `data` - A Python object (typically dict) to query/transform
    /// * `bindings` - Optional additional variable bindings
    ///
    /// # Returns
    ///
    /// The result of evaluating the expression
    ///
    /// # Errors
    ///
    /// Returns ValueError if evaluation fails
    #[pyo3(signature = (data, bindings=None))]
    fn evaluate(
        &self,
        py: Python,
        data: PyObject,
        bindings: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let json_data = python_to_json(py, &data)?;
        let result = if bindings.is_none() {
            let bytecode = self.bytecode.get_or_init(|| {
                evaluator::try_compile_expr(&self.ast)
                    .map(|ce| compiler::BytecodeCompiler::compile(&ce))
            });
            if let Some(bc) = bytecode {
                vm::Vm::new(bc).run(&json_data, None).map_err(evaluator_error_to_py)?
            } else {
                let mut ev = evaluator::Evaluator::new();
                ev.evaluate(&self.ast, &json_data)
                    .map_err(evaluator_error_to_py)?
            }
        } else {
            let mut ev = create_evaluator(py, bindings)?;
            ev.evaluate(&self.ast, &json_data)
                .map_err(evaluator_error_to_py)?
        };
        json_to_python(py, &result)
    }

    /// Evaluate with a pre-converted data handle (fastest for repeated evaluation).
    ///
    /// # Arguments
    ///
    /// * `data` - A JsonataData handle (pre-converted from Python to internal format)
    /// * `bindings` - Optional additional variable bindings
    ///
    /// # Returns
    ///
    /// The result of evaluating the expression
    #[pyo3(signature = (data, bindings=None))]
    fn evaluate_with_data(
        &self,
        py: Python,
        data: &JsonataData,
        bindings: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let result = if bindings.is_none() {
            let bytecode = self.bytecode.get_or_init(|| {
                evaluator::try_compile_expr(&self.ast)
                    .map(|ce| compiler::BytecodeCompiler::compile(&ce))
            });
            if let Some(bc) = bytecode {
                vm::Vm::new(bc).run(&data.data, None).map_err(evaluator_error_to_py)?
            } else {
                let mut ev = evaluator::Evaluator::new();
                ev.evaluate(&self.ast, &data.data)
                    .map_err(evaluator_error_to_py)?
            }
        } else {
            let mut ev = create_evaluator(py, bindings)?;
            ev.evaluate(&self.ast, &data.data)
                .map_err(evaluator_error_to_py)?
        };
        json_to_python(py, &result)
    }

    /// Evaluate with a pre-converted data handle, return JSON string (zero-overhead output).
    ///
    /// # Arguments
    ///
    /// * `data` - A JsonataData handle (pre-converted from Python to internal format)
    /// * `bindings` - Optional additional variable bindings
    ///
    /// # Returns
    ///
    /// The result as a JSON string
    #[pyo3(signature = (data, bindings=None))]
    fn evaluate_data_to_json(
        &self,
        py: Python,
        data: &JsonataData,
        bindings: Option<PyObject>,
    ) -> PyResult<String> {
        let result = if bindings.is_none() {
            let bytecode = self.bytecode.get_or_init(|| {
                evaluator::try_compile_expr(&self.ast)
                    .map(|ce| compiler::BytecodeCompiler::compile(&ce))
            });
            if let Some(bc) = bytecode {
                vm::Vm::new(bc).run(&data.data, None).map_err(evaluator_error_to_py)?
            } else {
                let mut ev = evaluator::Evaluator::new();
                ev.evaluate(&self.ast, &data.data)
                    .map_err(evaluator_error_to_py)?
            }
        } else {
            let mut ev = create_evaluator(py, bindings)?;
            ev.evaluate(&self.ast, &data.data)
                .map_err(evaluator_error_to_py)?
        };
        result
            .to_json_string()
            .map_err(|e| PyValueError::new_err(format!("Failed to serialize result: {}", e)))
    }

    /// Evaluate the expression with JSON string input/output (faster for large data).
    ///
    /// This method avoids Python↔Rust conversion overhead by accepting and returning
    /// JSON strings directly. This is significantly faster for large datasets.
    ///
    /// # Arguments
    ///
    /// * `json_str` - Input data as a JSON string
    /// * `bindings` - Optional dict of variable bindings (default: None)
    ///
    /// # Returns
    ///
    /// The result as a JSON string
    ///
    /// # Errors
    ///
    /// Returns ValueError if JSON parsing or evaluation fails
    #[pyo3(signature = (json_str, bindings=None))]
    fn evaluate_json(
        &self,
        py: Python,
        json_str: &str,
        bindings: Option<PyObject>,
    ) -> PyResult<String> {
        let json_data = JValue::from_json_str(json_str)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;
        let result = if bindings.is_none() {
            let bytecode = self.bytecode.get_or_init(|| {
                evaluator::try_compile_expr(&self.ast)
                    .map(|ce| compiler::BytecodeCompiler::compile(&ce))
            });
            if let Some(bc) = bytecode {
                vm::Vm::new(bc).run(&json_data, None).map_err(evaluator_error_to_py)?
            } else {
                let mut ev = evaluator::Evaluator::new();
                ev.evaluate(&self.ast, &json_data)
                    .map_err(evaluator_error_to_py)?
            }
        } else {
            let mut ev = create_evaluator(py, bindings)?;
            ev.evaluate(&self.ast, &json_data)
                .map_err(evaluator_error_to_py)?
        };
        result
            .to_json_string()
            .map_err(|e| PyValueError::new_err(format!("Failed to serialize result: {}", e)))
    }
}

/// Compile a JSONata expression into an executable form.
///
/// # Arguments
///
/// * `expression` - A JSONata query/transformation expression string
///
/// # Returns
///
/// A compiled JsonataExpression that can be evaluated
///
/// # Errors
///
/// Returns ValueError if the expression cannot be parsed
///
/// # Examples
///
/// ```python
/// import jsonatapy
///
/// expr = jsonatapy.compile("$.name")
/// result = expr.evaluate({"name": "Alice"})
/// print(result)  # "Alice"
/// ```
#[cfg(feature = "python")]
#[pyfunction]
fn compile(expression: &str) -> PyResult<JsonataExpression> {
    let ast = parser::parse(expression)
        .map_err(|e| PyValueError::new_err(format!("Parse error: {}", e)))?;

    Ok(JsonataExpression {
        ast,
        bytecode: std::cell::OnceCell::new(),
    })
}

/// Evaluate a JSONata expression against data in one step.
///
/// This is a convenience function that compiles and evaluates in one call.
/// For repeated evaluations of the same expression, use `compile()` instead.
///
/// # Arguments
///
/// * `expression` - A JSONata query/transformation expression string
/// * `data` - A Python object (typically dict) to query/transform
/// * `bindings` - Optional additional variable bindings
///
/// # Returns
///
/// The result of evaluating the expression
///
/// # Errors
///
/// Returns ValueError if parsing or evaluation fails
///
/// # Examples
///
/// ```python
/// import jsonatapy
///
/// result = jsonatapy.evaluate("$uppercase(name)", {"name": "alice"})
/// print(result)  # "ALICE"
/// ```
#[cfg(feature = "python")]
#[pyfunction]
#[pyo3(signature = (expression, data, bindings=None))]
fn evaluate(
    py: Python,
    expression: &str,
    data: PyObject,
    bindings: Option<PyObject>,
) -> PyResult<PyObject> {
    let expr = compile(expression)?;
    expr.evaluate(py, data, bindings)
}

/// Convert a Python object to a JValue.
///
/// Handles conversion of Python types:
/// - None -> Null
/// - bool -> Bool (checked before int since bool is a subclass of int)
/// - int, float -> Number
/// - str -> String
/// - list -> Array
/// - dict -> Object
#[cfg(feature = "python")]
fn python_to_json(py: Python, obj: &PyObject) -> PyResult<JValue> {
    python_to_json_bound(obj.bind(py))
}

/// Inner conversion using Bound API for zero-overhead type checks.
///
/// Uses is_instance_of::<T>() which compiles to C-level type pointer comparisons
/// (PyBool_Check, PyLong_Check, etc.) — single pointer comparison vs qualname()
/// which allocates a Python string and does string comparison.
#[cfg(feature = "python")]
fn python_to_json_bound(obj: &Bound<'_, PyAny>) -> PyResult<JValue> {
    if obj.is_none() {
        return Ok(JValue::Null);
    }

    // Check bool before int — Python bool is a subclass of int
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
    if let Ok(list) = obj.downcast::<PyList>() {
        let mut result = Vec::with_capacity(list.len());
        for item in list.iter() {
            result.push(python_to_json_bound(&item)?);
        }
        return Ok(JValue::array(result));
    }
    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut result = IndexMap::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key_str = key.extract::<String>()?;
            result.insert(key_str, python_to_json_bound(&value)?);
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

/// Convert a JValue to a Python object.
///
/// Handles conversion of JValue variants to Python types:
/// - Null/Undefined -> None
/// - Bool -> bool
/// - Number -> int (if whole number) or float
/// - String -> str
/// - Array -> list (batch-constructed via PyList::new for fewer C API calls)
/// - Object -> dict
/// - Lambda/Builtin/Regex -> None
#[cfg(feature = "python")]
fn json_to_python(py: Python, value: &JValue) -> PyResult<PyObject> {
    match value {
        JValue::Null | JValue::Undefined => Ok(py.None()),

        JValue::Bool(b) => Ok(b.into_pyobject(py).unwrap().to_owned().into_any().unbind()),

        JValue::Number(n) => {
            // If it's a whole number that fits in i64, return as Python int
            if n.fract() == 0.0 && n.is_finite() && *n >= i64::MIN as f64 && *n <= i64::MAX as f64
            {
                Ok((*n as i64).into_pyobject(py).unwrap().into_any().unbind())
            } else {
                Ok(n.into_pyobject(py).unwrap().into_any().unbind())
            }
        }

        JValue::String(s) => Ok((&**s).into_pyobject(py).unwrap().into_any().unbind()),

        JValue::Array(arr) => {
            // Array of objects with shared keys: intern first object's keys as
            // Python strings to avoid repeated UTF-8 -> PyString conversion.
            let all_objects = arr.len() >= 2
                && arr.iter().all(|item| matches!(item, JValue::Object(_)));
            if all_objects {
                let first_obj = match arr.first() {
                    Some(JValue::Object(obj)) => obj,
                    _ => unreachable!("all_objects guard ensures first element is an object"),
                };

                // Intern keys: store (&str, Py<PyString>) — no String clone needed
                // since first_obj borrows from arr which outlives this block
                let interned_keys: Vec<(&str, Py<PyString>)> = first_obj
                    .keys()
                    .map(|k| (k.as_str(), PyString::new(py, k).unbind()))
                    .collect();

                let items: Vec<PyObject> = arr
                    .iter()
                    .map(|item| {
                        // Safe to unwrap: all_objects guarantees every element is Object
                        let obj = match item {
                            JValue::Object(obj) => obj,
                            _ => unreachable!(),
                        };
                        let dict = PyDict::new(py);
                        for (key_str, py_key) in &interned_keys {
                            if let Some(value) = obj.get(*key_str) {
                                dict.set_item(py_key.bind(py), json_to_python(py, value)?)?;
                            }
                        }
                        // Handle any extra keys not in first object
                        for (key, value) in obj.iter() {
                            if !first_obj.contains_key(key) {
                                dict.set_item(key, json_to_python(py, value)?)?;
                            }
                        }
                        Ok(dict.unbind().into())
                    })
                    .collect::<PyResult<Vec<_>>>()?;
                return Ok(PyList::new(py, &items)?.unbind().into());
            }

            // General array: batch construction
            let items: Vec<PyObject> = arr
                .iter()
                .map(|item| json_to_python(py, item))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, &items)?.unbind().into())
        }

        JValue::Object(obj) => {
            let dict = PyDict::new(py);
            for (key, value) in obj.iter() {
                dict.set_item(key, json_to_python(py, value)?)?;
            }
            Ok(dict.unbind().into())
        }

        JValue::Lambda { .. } | JValue::Builtin { .. } | JValue::Regex { .. } => Ok(py.None()),
    }
}

/// Create an evaluator, optionally configured with Python bindings
#[cfg(feature = "python")]
fn create_evaluator(py: Python, bindings: Option<PyObject>) -> PyResult<evaluator::Evaluator> {
    if let Some(bindings_obj) = bindings {
        let bindings_json = python_to_json(py, &bindings_obj)?;

        let mut context = evaluator::Context::new();
        if let JValue::Object(map) = bindings_json {
            for (key, value) in map.iter() {
                context.bind(key.clone(), value.clone());
            }
        } else {
            return Err(PyTypeError::new_err("bindings must be a dictionary"));
        }
        Ok(evaluator::Evaluator::with_context(context))
    } else {
        Ok(evaluator::Evaluator::new())
    }
}

/// Convert an EvaluatorError to a PyErr
#[cfg(feature = "python")]
fn evaluator_error_to_py(e: evaluator::EvaluatorError) -> PyErr {
    match e {
        evaluator::EvaluatorError::TypeError(msg) => PyValueError::new_err(msg),
        evaluator::EvaluatorError::ReferenceError(msg) => PyValueError::new_err(msg),
        evaluator::EvaluatorError::EvaluationError(msg) => PyValueError::new_err(msg),
    }
}

/// JSONata Python module
#[cfg(feature = "python")]
#[pymodule]
fn _jsonatapy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate, m)?)?;
    m.add_class::<JsonataExpression>()?;
    m.add_class::<JsonataData>()?;

    // Add version info
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("__jsonata_version__", "2.1.0")?; // Reference implementation version

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_creation() {
        // Basic smoke test
        assert_eq!(env!("CARGO_PKG_VERSION"), "2.1.2");
    }
}
