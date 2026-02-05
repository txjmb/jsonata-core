// jsonatapy - High-performance Python implementation of JSONata
// Copyright (c) 2025 jsonatapy contributors
// Licensed under the MIT License

//! # jsonatapy
//!
//! A high-performance Python implementation of JSONata - the JSON query and transformation language.
//!
//! This library provides Python bindings to a Rust implementation of JSONata,
//! offering significantly better performance than JavaScript wrapper solutions
//! while maintaining 100% compatibility with the reference implementation.
//!
//! ## Architecture
//!
//! The implementation mirrors the structure of the reference JavaScript implementation
//! (jsonata-js) to facilitate maintenance and upstream synchronization:
//!
//! - `parser` - Expression parser (converts JSONata strings to AST)
//! - `evaluator` - Expression evaluator (executes AST against data)
//! - `functions` - Built-in function implementations
//! - `datetime` - Date/time handling functions
//! - `signature` - Function signature validation
//! - `utils` - Utility functions and helpers
//! - `ast` - Abstract Syntax Tree definitions

use pyo3::prelude::*;
use pyo3::exceptions::{PyValueError, PyTypeError};
use pyo3::types::{PyDict, PyList};
use serde_json::Value;

pub mod ast;
pub mod parser;
pub mod evaluator;
pub mod functions;
mod datetime;
mod signature;
mod utils;

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
#[pyclass]
struct JsonataExpression {
    /// The parsed Abstract Syntax Tree
    ast: ast::AstNode,
}

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
    fn evaluate(&self, py: Python, data: PyObject, bindings: Option<PyObject>) -> PyResult<PyObject> {
        let json_data = python_to_json(py, &data)?;
        let mut evaluator = create_evaluator(py, bindings)?;
        let result = evaluator.evaluate(&self.ast, &json_data)
            .map_err(evaluator_error_to_py)?;
        json_to_python(py, &result)
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
    fn evaluate_json(&self, py: Python, json_str: &str, bindings: Option<PyObject>) -> PyResult<String> {
        let json_data: Value = serde_json::from_str(json_str)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;
        let mut evaluator = create_evaluator(py, bindings)?;
        let result = evaluator.evaluate(&self.ast, &json_data)
            .map_err(evaluator_error_to_py)?;
        serde_json::to_string(&result)
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
#[pyfunction]
fn compile(expression: &str) -> PyResult<JsonataExpression> {
    // Parse the expression into an AST
    let ast = parser::parse(expression)
        .map_err(|e| PyValueError::new_err(format!("Parse error: {}", e)))?;

    Ok(JsonataExpression { ast })
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
#[pyfunction]
#[pyo3(signature = (expression, data, bindings=None))]
fn evaluate(py: Python, expression: &str, data: PyObject, bindings: Option<PyObject>) -> PyResult<PyObject> {
    let expr = compile(expression)?;
    expr.evaluate(py, data, bindings)
}

/// Convert a Python object to a serde_json::Value
///
/// Handles conversion of Python types to JSON-compatible values:
/// - None -> Null
/// - bool -> Bool
/// - int, float -> Number
/// - str -> String
/// - list -> Array
/// - dict -> Object
fn python_to_json(py: Python, obj: &PyObject) -> PyResult<Value> {
    // Check for None/null
    if obj.is_none(py) {
        return Ok(Value::Null);
    }

    // Use type() to get the actual Python type for faster dispatch
    let bound = obj.bind(py);
    let obj_type = bound.get_type();

    // Fast path: check type name first to avoid failed extract attempts
    if let Ok(type_name) = obj_type.qualname() {
        let name = type_name.to_str().unwrap_or("");
        match name {
            "bool" => {
                // Boolean (must be before number check since bool is subclass of int in Python)
                if let Ok(b) = obj.extract::<bool>(py) {
                    return Ok(Value::Bool(b));
                }
            }
            "int" => {
                // Integer
                if let Ok(i) = obj.extract::<i64>(py) {
                    return Ok(serde_json::json!(i));
                }
            }
            "float" => {
                // Float - direct path without trying int first
                if let Ok(f) = obj.extract::<f64>(py) {
                    return Ok(serde_json::json!(f));
                }
            }
            "str" => {
                // String
                if let Ok(s) = obj.extract::<String>(py) {
                    return Ok(Value::String(s));
                }
            }
            "list" => {
                // List/array
                if let Ok(list) = obj.downcast_bound::<PyList>(py) {
                    let mut result = Vec::with_capacity(list.len());
                    for item in list.iter() {
                        let item_obj = item.unbind();
                        result.push(python_to_json(py, &item_obj)?);
                    }
                    return Ok(Value::Array(result));
                }
            }
            "dict" => {
                // Dict/object
                if let Ok(dict) = obj.downcast_bound::<PyDict>(py) {
                    let mut result = serde_json::Map::new();
                    for (key, value) in dict.iter() {
                        let key_str = key.extract::<String>()?;
                        let value_obj = value.unbind();
                        let value_json = python_to_json(py, &value_obj)?;
                        result.insert(key_str, value_json);
                    }
                    return Ok(Value::Object(result));
                }
            }
            _ => {}
        }
    }

    // Fallback: try all conversions (for subclasses, numpy types, etc.)
    if let Ok(b) = obj.extract::<bool>(py) {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = obj.extract::<i64>(py) {
        return Ok(serde_json::json!(i));
    }
    if let Ok(f) = obj.extract::<f64>(py) {
        return Ok(serde_json::json!(f));
    }
    if let Ok(s) = obj.extract::<String>(py) {
        return Ok(Value::String(s));
    }
    if let Ok(list) = obj.downcast_bound::<PyList>(py) {
        let mut result = Vec::with_capacity(list.len());
        for item in list.iter() {
            let item_obj = item.unbind();
            result.push(python_to_json(py, &item_obj)?);
        }
        return Ok(Value::Array(result));
    }
    if let Ok(dict) = obj.downcast_bound::<PyDict>(py) {
        let mut result = serde_json::Map::new();
        for (key, value) in dict.iter() {
            let key_str = key.extract::<String>()?;
            let value_obj = value.unbind();
            let value_json = python_to_json(py, &value_obj)?;
            result.insert(key_str, value_json);
        }
        return Ok(Value::Object(result));
    }

    // Unsupported type
    Err(PyTypeError::new_err(format!(
        "Cannot convert Python object to JSON: {}",
        obj.bind(py).get_type().name()?
    )))
}

/// Convert a serde_json::Value to a Python object
///
/// Handles conversion of JSON values to Python types:
/// - Null -> None
/// - Bool -> bool
/// - Number -> float or int
/// - String -> str
/// - Array -> list
/// - Object -> dict
fn json_to_python(py: Python, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),

        Value::Bool(b) => Ok(b.to_object(py)),

        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.to_object(py))
            } else if let Some(u) = n.as_u64() {
                Ok(u.to_object(py))
            } else if let Some(f) = n.as_f64() {
                Ok(f.to_object(py))
            } else {
                Err(PyTypeError::new_err("Invalid number"))
            }
        }

        Value::String(s) => Ok(s.to_object(py)),

        Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_to_python(py, item)?)?;
            }
            Ok(list.unbind().into())
        }

        Value::Object(obj) => {
            // Check if this is the undefined marker - return None
            if crate::evaluator::is_undefined(value) {
                return Ok(py.None());
            }
            let dict = PyDict::new(py);
            for (key, value) in obj {
                dict.set_item(key, json_to_python(py, value)?)?;
            }
            Ok(dict.unbind().into())
        }
    }
}

/// Create an evaluator, optionally configured with Python bindings
fn create_evaluator(py: Python, bindings: Option<PyObject>) -> PyResult<evaluator::Evaluator> {
    if let Some(bindings_obj) = bindings {
        let bindings_json = python_to_json(py, &bindings_obj)?;

        let mut context = evaluator::Context::new();
        if let Value::Object(map) = bindings_json {
            for (key, value) in map {
                context.bind(key, value);
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
fn evaluator_error_to_py(e: evaluator::EvaluatorError) -> PyErr {
    match e {
        evaluator::EvaluatorError::TypeError(msg) => PyValueError::new_err(msg),
        evaluator::EvaluatorError::ReferenceError(msg) => PyValueError::new_err(msg),
        evaluator::EvaluatorError::EvaluationError(msg) => PyValueError::new_err(msg),
    }
}

/// JSONata Python module
#[pymodule]
fn _jsonatapy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(evaluate, m)?)?;
    m.add_class::<JsonataExpression>()?;

    // Add version info
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("__jsonata_version__", "2.1.0")?;  // Reference implementation version

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_creation() {
        // Basic smoke test
        assert_eq!(env!("CARGO_PKG_VERSION"), "0.1.0");
    }
}
