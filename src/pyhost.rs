//! Python host functions, and functions passed across the Python boundary.
//!
//! A Python callable registered with `JsonataExpression.register` (or given
//! in the bindings, or returned by a host function) runs as a
//! [`evaluator::HostFn`]. Functions go both ways, as they do between
//! jsonata-js and JavaScript:
//!
//! - a JSONata function (a lambda, `$sum`, a regex, a partial application)
//!   in a host function's arguments reaches Python as a [`JsonataFunction`],
//!   a callable that applies it, as jsonata-js wraps a lambda it hands to a
//!   native function;
//! - a Python callable in what a host function returns, or in the bindings,
//!   becomes a JSONata function value, like the function jsonata-js gets
//!   from JavaScript: a custom matcher for `$match`, a closure, an object
//!   with methods.
//!
//! A [`JsonataFunction`] calls back into the evaluation that is running the
//! host function. The evaluator is borrowed by the Rust frames below the
//! Python call, which Python code can't reach, so for the length of each host
//! call the evaluator is moved out of its [`evaluator::HostCtx`] into
//! `HOST_EVALUATORS`, a stack per thread, and moved back afterwards. A
//! `JsonataFunction` takes the evaluator off the top of the stack while it
//! runs, so a host function it calls in turn pushes it again. One kept past
//! its host call, or called from another evaluation, finds a different
//! evaluator there (or none) and raises instead.

use crate::evaluator::{self, Evaluator, EvaluatorError, HostCtx};
use crate::value::JValue;
use crate::{json_to_python, lazy, pystack};
use indexmap::IndexMap;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    /// The evaluators running a Python host function on this thread, innermost
    /// last (see the module docs).
    static HOST_EVALUATORS: RefCell<Vec<Evaluator>> = const { RefCell::new(Vec::new()) };

    /// The error a `JsonataFunction` call raised to Python as this exception,
    /// so that a host function that lets it propagate raises the original
    /// error, code and all, rather than a wrapped one.
    static CALLBACK_ERROR: RefCell<Option<(Py<PyAny>, EvaluatorError)>> =
        const { RefCell::new(None) };
}

/// Bridges a Python callable to the core [`evaluator::HostFn`] trait. On each
/// call it re-acquires the GIL, marshals the JSONata arguments to Python,
/// invokes the callable, and marshals the result back.
pub(crate) struct PyHostFn {
    pub(crate) func: Py<PyAny>,
    /// See `python_arity`.
    pub(crate) arity: Option<usize>,
    /// Where the evaluation calling this function started, so a call made
    /// deep inside it can be given enough stack (see `pystack`).
    pub(crate) entry: Option<pystack::StackEntry>,
}

impl PyHostFn {
    pub(crate) fn new(
        func: &Bound<'_, PyAny>,
        arity: Option<usize>,
        entry: Option<pystack::StackEntry>,
    ) -> Self {
        PyHostFn {
            func: func.clone().unbind(),
            arity,
            entry,
        }
    }
}

/// A host function's exception as an evaluation error. An exception a
/// `JsonataFunction` raised for an evaluation error is that error again.
fn pyerr_to_evaluator_error(py: Python<'_>, e: PyErr) -> EvaluatorError {
    let stashed = CALLBACK_ERROR.with(|c| c.borrow_mut().take());
    if let Some((exception, error)) = stashed {
        if e.value(py).is(exception.bind(py)) {
            return error;
        }
    }
    EvaluatorError::EvaluationError(format!("host function raised {e}"))
}

/// A Python callable's arity as jsonata-js reads a JavaScript function's
/// `length`: the positional parameters before the first one with a default,
/// a `*args`, or a keyword-only one. A higher-order function such as `$map`
/// passes a callback only that many of (value, index, array), so
/// `$map([1, 4], $sqrt)` with `math.sqrt` gets one argument, as
/// `Math.sqrt` does in jsonata-js. `None` when `inspect.signature` can't
/// read the callable (some builtins), which passes every argument.
pub(crate) fn python_arity(func: &Bound<'_, PyAny>) -> Option<usize> {
    if let Ok(function) = func.cast::<JsonataFunction>() {
        return function.borrow().arity;
    }
    let py = func.py();
    let inspect = py.import("inspect").ok()?;
    let signature = inspect.call_method1("signature", (func,)).ok()?;
    let empty = inspect.getattr("Parameter").ok()?.getattr("empty").ok()?;
    let parameters = signature
        .getattr("parameters")
        .ok()?
        .call_method0("values")
        .ok()?;
    let mut count = 0;
    for parameter in parameters.try_iter().ok()? {
        let parameter = parameter.ok()?;
        // POSITIONAL_ONLY = 0, POSITIONAL_OR_KEYWORD = 1
        let kind: i64 = parameter.getattr("kind").ok()?.extract().ok()?;
        if kind > 1 || !parameter.getattr("default").ok()?.is(&empty) {
            break;
        }
        count += 1;
    }
    Some(count)
}

impl evaluator::HostFn for PyHostFn {
    fn arity(&self) -> Option<usize> {
        self.arity
    }

    fn call(&self, args: &[JValue], ctx: &mut HostCtx<'_>) -> Result<JValue, EvaluatorError> {
        Python::attach(|py| {
            pystack::with_python_stack(py, self.entry, || self.call_python(py, args, ctx))
        })
    }
}

impl PyHostFn {
    fn call_python(
        &self,
        py: Python<'_>,
        args: &[JValue],
        ctx: &mut HostCtx<'_>,
    ) -> Result<JValue, EvaluatorError> {
        let evaluator_id = ctx.evaluator().id();
        let py_args: Vec<Py<PyAny>> = args
            .iter()
            .map(|a| to_python(py, a, evaluator_id, self.entry))
            .collect::<PyResult<_>>()
            .map_err(|e| pyerr_to_evaluator_error(py, e))?;
        let args_tuple = PyTuple::new(py, &py_args).map_err(|e| pyerr_to_evaluator_error(py, e))?;

        // Lend the evaluator to Python for the call (see the module docs).
        let evaluator = std::mem::take(ctx.evaluator());
        HOST_EVALUATORS.with(|s| s.borrow_mut().push(evaluator));
        let result = self.func.bind(py).call1(&args_tuple);
        let evaluator = HOST_EVALUATORS
            .with(|s| s.borrow_mut().pop())
            .expect("the evaluator lent to a host function is on top of the stack again");
        *ctx.evaluator() = evaluator;
        let result = result.map_err(|e| pyerr_to_evaluator_error(py, e))?;

        // The synchronous core cannot await, so an `async def` (which returns
        // a coroutine) has no meaningful result. Reject it with actionable
        // guidance rather than silently converting the coroutine object.
        if result.hasattr("__await__").unwrap_or(false) {
            return Err(EvaluatorError::EvaluationError(
                "host function returned a coroutine; async functions are not \
                 supported. Use a synchronous function, or perform the async I/O \
                 outside jsonata and pass the result in via bindings."
                    .to_string(),
            ));
        }

        to_jvalue(&result, ctx.evaluator(), self.entry).map_err(|e| pyerr_to_evaluator_error(py, e))
    }
}

/// A JSONata function handed to a Python host function: calling it applies
/// the function, with the positional arguments as its arguments. It can be
/// called only while the host function it was passed to (or one that host
/// function's evaluation calls) is running.
#[pyclass(unsendable, module = "jsonatapy")]
pub(crate) struct JsonataFunction {
    function: JValue,
    /// `Evaluator::id` of the evaluation the function belongs to.
    evaluator_id: u64,
    /// The function's parameter count, for `python_arity` when Python passes
    /// it on to another function: a lambda's parameters, `None` otherwise.
    arity: Option<usize>,
    /// The stack entry of the evaluation (see `pystack`), for the callables
    /// Python passes to the function.
    entry: Option<pystack::StackEntry>,
}

#[pymethods]
impl JsonataFunction {
    #[pyo3(signature = (*args))]
    fn __call__(&self, py: Python<'_>, args: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
        let evaluator = HOST_EVALUATORS.with(|s| {
            let mut stack = s.borrow_mut();
            match stack.last() {
                Some(top) if top.id() == self.evaluator_id => stack.pop(),
                _ => None,
            }
        });
        let Some(mut evaluator) = evaluator else {
            return Err(PyValueError::new_err(
                "a JSONata function can only be called while the evaluation it came \
                 from is running a host function",
            ));
        };
        let result = (|| {
            let mut values = Vec::with_capacity(args.len());
            for arg in args.iter() {
                values.push(to_jvalue(&arg, &mut evaluator, self.entry)?);
            }
            Ok::<_, PyErr>(evaluator.call_function_from_host(&self.function, &values))
        })();
        HOST_EVALUATORS.with(|s| s.borrow_mut().push(evaluator));
        match result? {
            Ok(value) => to_python(py, &value, self.evaluator_id, self.entry),
            Err(error) => {
                let exception = PyValueError::new_err(error.message().to_string());
                let value = exception.value(py).clone().into_any().unbind();
                CALLBACK_ERROR.with(|c| *c.borrow_mut() = Some((value, error)));
                Err(exception)
            }
        }
    }

    fn __repr__(&self) -> String {
        match &self.function {
            JValue::Builtin { name } if !name.starts_with('\u{1}') => {
                format!("<jsonata function ${name}>")
            }
            _ => "<jsonata function>".to_string(),
        }
    }
}

/// Whether `value` holds a function anywhere.
fn contains_function(value: &JValue) -> bool {
    match value {
        JValue::Lambda { .. } | JValue::Builtin { .. } | JValue::Regex { .. } => true,
        JValue::Array(items) => items.iter().any(contains_function),
        JValue::Object(map) => map.values().any(contains_function),
        _ => false,
    }
}

/// A value for a host function's arguments, with each function in it as a
/// `JsonataFunction` of evaluation `evaluator_id`.
fn to_python(
    py: Python<'_>,
    value: &JValue,
    evaluator_id: u64,
    entry: Option<pystack::StackEntry>,
) -> PyResult<Py<PyAny>> {
    if !contains_function(value) {
        return json_to_python(py, value);
    }
    match value {
        JValue::Array(items) => {
            let items = items
                .iter()
                .map(|item| to_python(py, item, evaluator_id, entry))
                .collect::<PyResult<Vec<_>>>()?;
            Ok(PyList::new(py, &items)?.into_any().unbind())
        }
        JValue::Object(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map.iter() {
                dict.set_item(key, to_python(py, item, evaluator_id, entry)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        function => {
            let arity = match function {
                JValue::Lambda(lambda) => Some(lambda.params.len()),
                _ => None,
            };
            let function = JsonataFunction {
                function: function.clone(),
                evaluator_id,
                arity,
                entry,
            };
            Ok(Py::new(py, function)?.into_any())
        }
    }
}

/// A Python value as a JSONata value, with each callable in it as a function
/// value of `evaluator` (a `JsonataFunction` is the function it wraps).
pub(crate) fn to_jvalue(
    obj: &Bound<'_, PyAny>,
    evaluator: &mut Evaluator,
    entry: Option<pystack::StackEntry>,
) -> PyResult<JValue> {
    // Most values hold no callable: the usual conversion takes them, and
    // fails on the first callable it meets.
    match lazy::convert(obj, false) {
        Ok(value) => Ok(value),
        Err(_) => convert_with_functions(obj, evaluator, entry),
    }
}

fn convert_with_functions(
    obj: &Bound<'_, PyAny>,
    evaluator: &mut Evaluator,
    entry: Option<pystack::StackEntry>,
) -> PyResult<JValue> {
    if let Ok(function) = obj.cast::<JsonataFunction>() {
        // A host function value names an entry in its own evaluator's
        // registry, so in another evaluation it would be a different function.
        // A lambda can hold one too (a captured `$f := $make()`), so every
        // function is held to its evaluation, not only host function values.
        let function = function.borrow();
        if function.evaluator_id != evaluator.id() {
            return Err(PyValueError::new_err(
                "a JSONata function can only be used in the evaluation it came from",
            ));
        }
        return Ok(function.function.clone());
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let mut map = IndexMap::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            map.insert(
                key.extract::<String>()?,
                convert_with_functions(&value, evaluator, entry)?,
            );
        }
        return Ok(JValue::object(map));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut items = Vec::with_capacity(list.len());
        for item in list.iter() {
            items.push(convert_with_functions(&item, evaluator, entry)?);
        }
        return Ok(JValue::array(items));
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        let mut items = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            items.push(convert_with_functions(&item, evaluator, entry)?);
        }
        return Ok(JValue::array(items));
    }
    if obj.is_callable() {
        // Keyed by address: the registry holds the callable, so the address
        // is not reused while the evaluation can see it.
        let key = obj.as_ptr() as usize;
        return Ok(evaluator.host_function_value_keyed(key, || {
            Rc::new(PyHostFn::new(obj, python_arity(obj), entry))
        }));
    }
    lazy::convert(obj, false)
}
