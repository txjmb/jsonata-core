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
pub mod ast_transform;
mod compiler;
mod datetime;
pub mod evaluator;
pub mod functions;
pub mod parser;
mod signature;
pub mod value;
mod vm;

// ── Benchmarking facade (only when the "bench" feature is enabled) ────────────
//
// Exposes the compiler/VM pipeline for Criterion benchmarks without making
// the internals part of the permanent public API.

/// Internal benchmarking API — do not use in production code.
///
/// Enabled with `--features bench`. Provides access to the bytecode compiler
/// and VM so that Criterion benchmarks can measure tree-walker vs VM directly.
#[cfg(feature = "bench")]
pub mod _bench {
    use crate::ast::AstNode;
    pub use crate::evaluator::EvaluatorError;
    use crate::value::JValue;

    /// An opaque handle to a compiled bytecode program.
    pub struct CompiledProgram(crate::vm::BytecodeProgram);

    /// Compile an AST node to bytecode.
    ///
    /// Returns `None` if the expression contains constructs the compiler
    /// doesn't handle (e.g. wildcards, `$eval`, higher-order functions at
    /// the top level). In that case, fall back to `Evaluator::new().evaluate()`.
    pub fn compile(ast: &AstNode) -> Option<CompiledProgram> {
        crate::evaluator::try_compile_expr(ast)
            .map(|ce| CompiledProgram(crate::compiler::BytecodeCompiler::compile(&ce)))
    }

    /// Execute a compiled program against `data`.
    pub fn run(prog: &CompiledProgram, data: &JValue) -> Result<JValue, EvaluatorError> {
        crate::vm::Vm::with_options(&prog.0, crate::evaluator::EvaluatorOptions::default())
            .run(data, None)
    }
}

// ── Python bindings (only when the "python" feature is enabled) ───────────────

/// The JSONata reference implementation version this library targets.
const JSONATA_REFERENCE_VERSION: &str = "2.1.0";

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
    fn new(py: Python, data: Py<PyAny>) -> PyResult<Self> {
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
    /// Default guardrail options set at `compile()` time. Per-call `evaluate*()`
    /// kwargs override these on a field-by-field basis (see the `.or(...)` merges
    /// in the `#[pymethods]` impl below).
    default_options: evaluator::EvaluatorOptions,
}

#[cfg(feature = "python")]
impl JsonataExpression {
    /// Evaluate the compiled expression against pre-converted data.
    /// Uses bytecode VM when available, falls back to tree-walker.
    fn run_eval(
        &self,
        py: Python,
        data: &JValue,
        bindings: Option<Py<PyAny>>,
        options: evaluator::EvaluatorOptions,
    ) -> PyResult<JValue> {
        if bindings.is_none() {
            let bytecode = self.bytecode.get_or_init(|| {
                evaluator::try_compile_expr(&self.ast)
                    .map(|ce| compiler::BytecodeCompiler::compile(&ce))
            });
            if let Some(bc) = bytecode {
                vm::Vm::with_options(bc, options.clone())
                    .run(data, None)
                    .map_err(evaluator_error_to_py)
            } else {
                let mut ev = evaluator::Evaluator::with_options(evaluator::Context::new(), options);
                ev.evaluate(&self.ast, data).map_err(evaluator_error_to_py)
            }
        } else {
            let mut ev = create_evaluator(py, bindings, options)?;
            ev.evaluate(&self.ast, data).map_err(evaluator_error_to_py)
        }
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl JsonataExpression {
    /// Returns ValueError if evaluation fails
    #[pyo3(signature = (data, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
    #[allow(clippy::too_many_arguments)]
    fn evaluate(
        &self,
        py: Python,
        data: Py<PyAny>,
        bindings: Option<Py<PyAny>>,
        timeout: Option<u64>,
        max_stack_depth: Option<usize>,
        max_sequence_length: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let json_data = python_to_json(py, &data)?;
        let options = evaluator::EvaluatorOptions {
            timeout_ms: timeout.or(self.default_options.timeout_ms),
            max_stack_depth: max_stack_depth.or(self.default_options.max_stack_depth),
            max_sequence_length: max_sequence_length.or(self.default_options.max_sequence_length),
        };
        json_to_python(py, &self.run_eval(py, &json_data, bindings, options)?)
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
    #[pyo3(signature = (data, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
    #[allow(clippy::too_many_arguments)]
    fn evaluate_with_data(
        &self,
        py: Python,
        data: &JsonataData,
        bindings: Option<Py<PyAny>>,
        timeout: Option<u64>,
        max_stack_depth: Option<usize>,
        max_sequence_length: Option<usize>,
    ) -> PyResult<Py<PyAny>> {
        let options = evaluator::EvaluatorOptions {
            timeout_ms: timeout.or(self.default_options.timeout_ms),
            max_stack_depth: max_stack_depth.or(self.default_options.max_stack_depth),
            max_sequence_length: max_sequence_length.or(self.default_options.max_sequence_length),
        };
        json_to_python(py, &self.run_eval(py, &data.data, bindings, options)?)
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
    #[pyo3(signature = (data, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
    #[allow(clippy::too_many_arguments)]
    fn evaluate_data_to_json(
        &self,
        py: Python,
        data: &JsonataData,
        bindings: Option<Py<PyAny>>,
        timeout: Option<u64>,
        max_stack_depth: Option<usize>,
        max_sequence_length: Option<usize>,
    ) -> PyResult<String> {
        let options = evaluator::EvaluatorOptions {
            timeout_ms: timeout.or(self.default_options.timeout_ms),
            max_stack_depth: max_stack_depth.or(self.default_options.max_stack_depth),
            max_sequence_length: max_sequence_length.or(self.default_options.max_sequence_length),
        };
        self.run_eval(py, &data.data, bindings, options)?
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
    #[pyo3(signature = (json_str, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
    #[allow(clippy::too_many_arguments)]
    fn evaluate_json(
        &self,
        py: Python,
        json_str: &str,
        bindings: Option<Py<PyAny>>,
        timeout: Option<u64>,
        max_stack_depth: Option<usize>,
        max_sequence_length: Option<usize>,
    ) -> PyResult<String> {
        let json_data = JValue::from_json_str(json_str)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;
        let options = evaluator::EvaluatorOptions {
            timeout_ms: timeout.or(self.default_options.timeout_ms),
            max_stack_depth: max_stack_depth.or(self.default_options.max_stack_depth),
            max_sequence_length: max_sequence_length.or(self.default_options.max_sequence_length),
        };
        self.run_eval(py, &json_data, bindings, options)?
            .to_json_string()
            .map_err(|e| PyValueError::new_err(format!("Failed to serialize result: {}", e)))
    }

    /// Evaluate with JSON string input, distinguishing an Undefined result
    /// (returns Python None) from an explicit JSON null result (returns
    /// the string "null"). evaluate_json() cannot make this distinction --
    /// JSON serialization has no way to represent "undefined" separately
    /// from "null" -- so this method checks the raw evaluated JValue's
    /// is_undefined() BEFORE serializing, exposing the same signal the
    /// Rust CLI (src/bin/jsonata/main.rs) already uses internally.
    ///
    /// `json_str=None` means no input document at all -- the top-level
    /// context (`$`) is bound to a true `Undefined`, matching the Rust
    /// CLI's `--null-input` behavior. This is distinct from passing the
    /// text `"null"`, which binds `$` to an explicit JSON null.
    ///
    /// # Errors
    ///
    /// Returns ValueError if JSON parsing or evaluation fails
    #[pyo3(signature = (json_str, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
    #[allow(clippy::too_many_arguments)]
    fn evaluate_json_or_none(
        &self,
        py: Python,
        json_str: Option<&str>,
        bindings: Option<Py<PyAny>>,
        timeout: Option<u64>,
        max_stack_depth: Option<usize>,
        max_sequence_length: Option<usize>,
    ) -> PyResult<Option<String>> {
        let json_data = match json_str {
            Some(s) => JValue::from_json_str(s)
                .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?,
            None => JValue::Undefined,
        };
        let options = evaluator::EvaluatorOptions {
            timeout_ms: timeout.or(self.default_options.timeout_ms),
            max_stack_depth: max_stack_depth.or(self.default_options.max_stack_depth),
            max_sequence_length: max_sequence_length.or(self.default_options.max_sequence_length),
        };
        let result = self.run_eval(py, &json_data, bindings, options)?;
        if result.is_undefined() {
            return Ok(None);
        }
        result
            .to_json_string()
            .map(Some)
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
#[pyo3(signature = (expression, timeout=None, max_stack_depth=None, max_sequence_length=None))]
fn compile(
    expression: &str,
    timeout: Option<u64>,
    max_stack_depth: Option<usize>,
    max_sequence_length: Option<usize>,
) -> PyResult<JsonataExpression> {
    let ast = parser::parse(expression).map_err(parser_error_to_py)?;

    Ok(JsonataExpression {
        ast,
        bytecode: std::cell::OnceCell::new(),
        default_options: build_evaluator_options(timeout, max_stack_depth, max_sequence_length),
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
#[pyo3(signature = (expression, data, bindings=None, timeout=None, max_stack_depth=None, max_sequence_length=None))]
#[allow(clippy::too_many_arguments)]
fn evaluate(
    py: Python,
    expression: &str,
    data: Py<PyAny>,
    bindings: Option<Py<PyAny>>,
    timeout: Option<u64>,
    max_stack_depth: Option<usize>,
    max_sequence_length: Option<usize>,
) -> PyResult<Py<PyAny>> {
    let expr = compile(expression, None, None, None)?;
    expr.evaluate(
        py,
        data,
        bindings,
        timeout,
        max_stack_depth,
        max_sequence_length,
    )
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
fn python_to_json(py: Python, obj: &Py<PyAny>) -> PyResult<JValue> {
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
    if let Ok(list) = obj.cast::<PyList>() {
        let mut result = Vec::with_capacity(list.len());
        for item in list.iter() {
            result.push(python_to_json_bound(&item)?);
        }
        return Ok(JValue::array(result));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
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
fn json_to_python(py: Python, value: &JValue) -> PyResult<Py<PyAny>> {
    match value {
        JValue::Null | JValue::Undefined => Ok(py.None()),

        JValue::Bool(b) => Ok(b.into_pyobject(py).unwrap().to_owned().into_any().unbind()),

        JValue::Number(n) => {
            // If it's a whole number that fits in i64, return as Python int
            if n.fract() == 0.0 && n.is_finite() && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                Ok((*n as i64).into_pyobject(py).unwrap().into_any().unbind())
            } else {
                Ok(n.into_pyobject(py).unwrap().into_any().unbind())
            }
        }

        JValue::String(s) => Ok((&**s).into_pyobject(py).unwrap().into_any().unbind()),

        JValue::Array(arr) => {
            // Array of objects with shared keys: intern first object's keys as
            // Python strings to avoid repeated UTF-8 -> PyString conversion.
            let all_objects =
                arr.len() >= 2 && arr.iter().all(|item| matches!(item, JValue::Object(_)));
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

                let items: Vec<Py<PyAny>> = arr
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
            let items: Vec<Py<PyAny>> = arr
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

/// Build an `EvaluatorOptions` from the Python-facing `timeout`/`max_stack_depth`/
/// `max_sequence_length` kwargs used by `compile()` and the free-standing `evaluate()`.
#[cfg(feature = "python")]
fn build_evaluator_options(
    timeout: Option<u64>,
    max_stack_depth: Option<usize>,
    max_sequence_length: Option<usize>,
) -> evaluator::EvaluatorOptions {
    evaluator::EvaluatorOptions {
        timeout_ms: timeout,
        max_stack_depth,
        max_sequence_length,
    }
}

/// Create an evaluator, optionally configured with Python bindings
#[cfg(feature = "python")]
fn create_evaluator(
    py: Python,
    bindings: Option<Py<PyAny>>,
    options: evaluator::EvaluatorOptions,
) -> PyResult<evaluator::Evaluator> {
    let mut context = evaluator::Context::new();
    if let Some(bindings_obj) = bindings {
        let bindings_json = python_to_json(py, &bindings_obj)?;
        if let JValue::Object(map) = bindings_json {
            for (key, value) in map.iter() {
                context.bind(key.clone(), value.clone());
            }
        } else {
            return Err(PyTypeError::new_err("bindings must be a dictionary"));
        }
    }
    Ok(evaluator::Evaluator::with_options(context, options))
}

/// Convert an EvaluatorError to a PyErr
#[cfg(feature = "python")]
fn evaluator_error_to_py(e: evaluator::EvaluatorError) -> PyErr {
    PyValueError::new_err(e.message().to_string())
}

/// Convert a ParserError to a PyErr
#[cfg(feature = "python")]
fn parser_error_to_py(e: parser::ParserError) -> PyErr {
    PyValueError::new_err(e.display_message())
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
    m.add("__jsonata_version__", JSONATA_REFERENCE_VERSION)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_module_creation() {
        // Basic smoke test
        assert!(!env!("CARGO_PKG_VERSION").is_empty());
    }

    mod parser_error_handling {
        use super::super::*;

        // These test ParserError::display_message() directly (a plain string
        // method) rather than parser_error_to_py(), which constructs a
        // PyErr -- that requires an initialized Python interpreter, which
        // isn't available under a bare `cargo test --all-features` run (no
        // embedded interpreter, unlike the maturin-built extension loaded
        // into a live Python process). The formatting logic under test is
        // identical either way.

        #[test]
        fn test_parser_error_to_py_coded_error_no_prefix() {
            // Test that coded errors (like S0214) are passed through without "Parse error: " prefix
            let coded_error = parser::ParserError::Coded {
                code: "S0214",
                message: "Expected a variable reference after @".to_string(),
            };
            let msg = coded_error.display_message();

            // The message should start with the code, not "Parse error: "
            assert!(
                msg.starts_with("S0214:"),
                "Expected message to start with 'S0214:', got: {}",
                msg
            );
            assert!(
                !msg.starts_with("Parse error:"),
                "Expected no 'Parse error:' prefix, got: {}",
                msg
            );
        }

        #[test]
        fn test_parser_error_to_py_non_coded_error_with_prefix() {
            // Test that non-coded errors still get the "Parse error: " prefix
            let non_coded_error = parser::ParserError::UnexpectedToken("foo".to_string());
            let msg = non_coded_error.display_message();

            // The message should have the "Parse error: " prefix
            assert!(
                msg.starts_with("Parse error:"),
                "Expected message to start with 'Parse error:', got: {}",
                msg
            );
        }
    }
}
