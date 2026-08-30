//! C ABI for jsonata-core (the `capi` feature).
//!
//! Design: docs/superpowers/specs/2026-07-09-multi-language-and-agentic-study-design.md
//! (Phase 4), validated by the 2026-07-13 Java/.NET FFI benchmark experiment.
//! The hand-written public header lives at `bindings/c/jsonata.h`; keep the
//! two in sync (CI compiles `bindings/c/examples/smoke.c` against both).
//!
//! Contract summary (full details in `bindings/c/README.md`):
//! - JSON crosses the boundary as UTF-8, NUL-terminated C strings in both
//!   directions. There is no C-visible structured value model.
//! - Errors go through a thread-local slot: a NULL return from
//!   `jsonata_evaluate` with an EMPTY slot means the JSONata result was
//!   *undefined* (not an error). `jsonata_last_error_message()` reads the
//!   slot; `jsonata_last_error_code()` extracts the JSONata error code
//!   (e.g. "T2002") when the message carries one.
//! - Strings returned by this library are owned by the caller and must be
//!   released with `jsonata_free_string` — except `jsonata_version()`,
//!   which returns a static string that must never be freed.
//! - Handles are NOT thread-safe (the engine is `Rc`-based): create and use
//!   a `JsonataExpr*` on one thread only. The error slot is thread-local,
//!   so different threads using their own handles never race on errors.
//! - Engine panics are caught at the boundary and reported as errors
//!   (message prefixed "internal error:"); they do not unwind into the
//!   caller or abort the host process.

use std::cell::{OnceCell, RefCell};
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::evaluator::{self, EvaluatorOptions};
use crate::value::JValue;
use crate::{parser, vm};

pub struct JsonataExpr {
    ast: crate::ast::AstNode,
    bytecode: OnceCell<Option<vm::BytecodeProgram>>,
    /// Variables registered via `jsonata_bind_var`, applied on every
    /// subsequent `jsonata_evaluate`. Non-empty bindings force the
    /// tree-walker (the VM path takes no user context), mirroring the
    /// Python bindings' behavior in `lib.rs::run_eval`.
    bindings: Vec<(String, JValue)>,
    /// Host functions registered via `jsonata_register_fn`, applied on every
    /// subsequent `jsonata_evaluate`. Like bindings, a non-empty list forces
    /// the tree-walker (the VM has no host registry).
    host_fns: Vec<CHostFnReg>,
    /// Evaluation guardrails set via `jsonata_set_limits`, applied on every
    /// subsequent `jsonata_evaluate`. Defaults to unlimited.
    options: EvaluatorOptions,
}

/// A host function callback.
///
/// - `user_data` is the pointer supplied at registration, opaque to jsonata.
/// - `args_json` is a NUL-terminated UTF-8 JSON *array* of the (already
///   evaluated) arguments.
///
/// It returns a NUL-terminated UTF-8 JSON string with the result — which must
/// stay valid until the call returns (jsonata copies it and does **not** free
/// it) — or NULL to signal an error.
pub type JsonataHostFn = unsafe extern "C" fn(*mut c_void, *const c_char) -> *const c_char;

/// A registered C host function stored on the expression.
struct CHostFnReg {
    name: String,
    func: JsonataHostFn,
    user_data: *mut c_void,
    is_override: bool,
}

/// Bridges a C function pointer to the core [`evaluator::HostFn`] trait: it
/// serializes the arguments to a JSON array, invokes the callback, and parses
/// the returned JSON string.
struct CHostFn {
    func: JsonataHostFn,
    user_data: *mut c_void,
}

impl evaluator::HostFn for CHostFn {
    fn call(
        &self,
        args: &[JValue],
        _ctx: &mut evaluator::HostCtx,
    ) -> Result<JValue, evaluator::EvaluatorError> {
        let err = |m: String| evaluator::EvaluatorError::EvaluationError(m);
        let args_json = JValue::from(args.to_vec())
            .to_json_string()
            .map_err(|e| err(format!("could not serialize host function arguments: {e}")))?;
        let args_c = CString::new(args_json)
            .map_err(|_| err("host function arguments contained interior NUL".to_string()))?;

        // SAFETY: `func`/`user_data` came from `jsonata_register_fn`; the caller
        // guarantees the pointer stays valid for the expression's lifetime. The
        // returned pointer is borrowed (copied below), never freed by us.
        let ret = unsafe { (self.func)(self.user_data, args_c.as_ptr()) };
        if ret.is_null() {
            return Err(err("host function returned an error (NULL)".to_string()));
        }
        let ret_str = unsafe { CStr::from_ptr(ret) }
            .to_str()
            .map_err(|_| err("host function result is not valid UTF-8".to_string()))?;
        JValue::from_json_str(ret_str)
            .map_err(|e| err(format!("host function returned invalid JSON: {e}")))
    }
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error(msg: String) {
    let c =
        CString::new(msg).unwrap_or_else(|_| CString::new("error message contained NUL").unwrap());
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(c));
}

fn clear_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

/// Runs `f` with panics converted to an error-slot entry + `default`.
/// Engine panics must never unwind across the `extern "C"` boundary
/// (undefined behavior) nor abort the host process.
fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(v) => v,
        Err(payload) => {
            let msg = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_string());
            set_error(format!("internal error: {}", msg));
            default
        }
    }
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
    guard(std::ptr::null_mut(), || match parser::parse(expr) {
        Ok(ast) => {
            clear_error();
            Box::into_raw(Box::new(JsonataExpr {
                ast,
                bytecode: OnceCell::new(),
                bindings: Vec::new(),
                host_fns: Vec::new(),
                options: EvaluatorOptions::default(),
            }))
        }
        Err(e) => {
            set_error(e.display_message());
            std::ptr::null_mut()
        }
    })
}

/// Registers a variable binding (`$name`) on the expression, from a JSON
/// value. Applies to every subsequent `jsonata_evaluate` on this handle.
/// Returns 0 on success, -1 on error (error slot set). Re-binding an
/// existing name replaces its value. A leading `$` on `name` is accepted
/// and stripped.
///
/// # Safety
/// `expr` must be a live pointer from `jsonata_compile`; `name` and
/// `json_value_utf8` must be valid NUL-terminated C strings or NULL.
#[no_mangle]
pub unsafe extern "C" fn jsonata_bind_var(
    expr: *mut JsonataExpr,
    name: *const c_char,
    json_value_utf8: *const c_char,
) -> c_int {
    if expr.is_null() || name.is_null() || json_value_utf8.is_null() {
        set_error("jsonata_bind_var: NULL argument".to_string());
        return -1;
    }
    let (name, json) = match (
        CStr::from_ptr(name).to_str(),
        CStr::from_ptr(json_value_utf8).to_str(),
    ) {
        (Ok(n), Ok(j)) => (n, j),
        _ => {
            set_error("jsonata_bind_var: argument is not valid UTF-8".to_string());
            return -1;
        }
    };
    let value = match JValue::from_json_str(json) {
        Ok(v) => v,
        Err(e) => {
            set_error(format!("invalid JSON for variable ${}: {}", name, e));
            return -1;
        }
    };
    let expr = &mut *expr;
    let name = name.strip_prefix('$').unwrap_or(name).to_string();
    if let Some(slot) = expr.bindings.iter_mut().find(|(n, _)| *n == name) {
        slot.1 = value;
    } else {
        expr.bindings.push((name, value));
    }
    clear_error();
    0
}

/// Register a host function callable from the expression as `$name(...)`.
/// Applies to every subsequent `jsonata_evaluate` on this handle. A leading
/// `$` on `name` is accepted and stripped; re-registering a name replaces it.
/// Returns 0 on success, -1 on error (error slot set) — including when `name`
/// collides with a built-in (use `jsonata_register_fn_override` to replace a
/// built-in deliberately).
///
/// `user_data` is passed back to the callback unchanged and must remain valid
/// for the lifetime of the expression handle. See [`JsonataHostFn`] for the
/// callback contract.
///
/// # Safety
/// `expr` must be a live pointer from `jsonata_compile`; `name` must be a valid
/// NUL-terminated C string or NULL; `func` must be a valid function pointer or
/// NULL.
#[no_mangle]
pub unsafe extern "C" fn jsonata_register_fn(
    expr: *mut JsonataExpr,
    name: *const c_char,
    func: Option<JsonataHostFn>,
    user_data: *mut c_void,
) -> c_int {
    register_host_fn(expr, name, func, user_data, false)
}

/// Like [`jsonata_register_fn`], but deliberately replaces a built-in of the
/// same name — the intended uses being determinism injection for the impure
/// built-ins (`$now`, `$millis`, `$random`) and sandboxing (disabling `$eval`).
/// Overriding a built-in on the compiled fast path returns -1 with an error.
///
/// # Safety
/// Same as [`jsonata_register_fn`].
#[no_mangle]
pub unsafe extern "C" fn jsonata_register_fn_override(
    expr: *mut JsonataExpr,
    name: *const c_char,
    func: Option<JsonataHostFn>,
    user_data: *mut c_void,
) -> c_int {
    register_host_fn(expr, name, func, user_data, true)
}

/// Shared body for the two registration entry points.
///
/// # Safety
/// See [`jsonata_register_fn`].
unsafe fn register_host_fn(
    expr: *mut JsonataExpr,
    name: *const c_char,
    func: Option<JsonataHostFn>,
    user_data: *mut c_void,
    is_override: bool,
) -> c_int {
    if expr.is_null() || name.is_null() {
        set_error("jsonata_register_fn: NULL argument".to_string());
        return -1;
    }
    let Some(func) = func else {
        set_error("jsonata_register_fn: function pointer is NULL".to_string());
        return -1;
    };
    let name = match CStr::from_ptr(name).to_str() {
        Ok(n) => n,
        Err(_) => {
            set_error("jsonata_register_fn: name is not valid UTF-8".to_string());
            return -1;
        }
    };
    let name = name.strip_prefix('$').unwrap_or(name).to_string();

    guard(-1, || {
        // Validate the collision/override rules now (fail at register-time, not
        // evaluate-time) by exercising the same core registration on a
        // throwaway evaluator.
        let mut probe = evaluator::Evaluator::new();
        let probed = CHostFn { func, user_data };
        let res = if is_override {
            probe.register_fn_override(name.clone(), probed)
        } else {
            probe.register_fn(name.clone(), probed)
        };
        if let Err(e) = res {
            set_error(e.message().to_string());
            return -1;
        }

        let expr = &mut *expr;
        expr.host_fns.retain(|r| r.name != name);
        expr.host_fns.push(CHostFnReg {
            name,
            func,
            user_data,
            is_override,
        });
        clear_error();
        0
    })
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
    guard(std::ptr::null_mut(), || {
        let data = match JValue::from_json_str(json) {
            Ok(v) => v,
            Err(e) => {
                set_error(format!("invalid input JSON: {}", e));
                return std::ptr::null_mut();
            }
        };
        // Same dispatch as JsonataExpression::run_eval in lib.rs, via the
        // shared expression::run_compiled: VM when the expression compiles to
        // bytecode AND no user bindings or host functions exist, tree-walker
        // otherwise (the VM takes no host registry).
        let result = if expr.bindings.is_empty() && expr.host_fns.is_empty() {
            crate::expression::run_compiled(&expr.ast, &expr.bytecode, &data, expr.options.clone())
        } else {
            let mut context = evaluator::Context::new();
            for (name, value) in &expr.bindings {
                context.bind(name.clone(), value.clone());
            }
            let mut ev = evaluator::Evaluator::with_options(context, expr.options.clone());
            for reg in &expr.host_fns {
                let hf = CHostFn {
                    func: reg.func,
                    user_data: reg.user_data,
                };
                let res = if reg.is_override {
                    ev.register_fn_override(reg.name.clone(), hf)
                } else {
                    ev.register_fn(reg.name.clone(), hf)
                };
                if let Err(e) = res {
                    set_error(e.message().to_string());
                    return std::ptr::null_mut();
                }
            }
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
    })
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

/// Returns the JSONata error code (e.g. "T2002", "S0201", "D1012") of the
/// last error, when its message carries one, or NULL when there is no error
/// or the error has no spec code (I/O-shaped errors like invalid input
/// JSON). Caller frees with `jsonata_free_string`.
///
/// jsonata-core stores spec codes at the START of the message ("T2002: ...")
/// rather than as a structured field; this extracts that prefix. There is no
/// `jsonata_last_error_position` — the engine does not currently track error
/// positions.
#[no_mangle]
pub extern "C" fn jsonata_last_error_code() -> *mut c_char {
    LAST_ERROR.with(|e| {
        let borrow = e.borrow();
        let Some(c) = &*borrow else {
            return std::ptr::null_mut();
        };
        let msg = c.to_string_lossy();
        match extract_error_code(&msg) {
            Some(code) => CString::new(code).unwrap().into_raw(),
            None => std::ptr::null_mut(),
        }
    })
}

/// A JSONata spec code is one uppercase ASCII letter followed by exactly
/// four digits, terminated by ':' (e.g. "T2002: ..."). The engine sometimes
/// stores it at the start of the message and sometimes behind a prose
/// prefix ("Runtime error: D3030: ..."), so scan for the first
/// token-boundary occurrence. Uncoded prose ("Parse error: something",
/// "invalid input JSON: ...") yields None.
fn extract_error_code(msg: &str) -> Option<String> {
    let bytes = msg.as_bytes();
    for i in 0..bytes.len().saturating_sub(5) {
        let at_boundary = i == 0 || bytes[i - 1] == b' ';
        if at_boundary
            && bytes[i].is_ascii_uppercase()
            && bytes[i + 1..i + 5].iter().all(|b| b.is_ascii_digit())
            && bytes[i + 5] == b':'
        {
            return Some(msg[i..i + 5].to_string());
        }
    }
    None
}

/// Set evaluation guardrails on the expression, applied to every subsequent
/// `jsonata_evaluate` on this handle. Each limit uses 0 for "unlimited" (the
/// default): `timeout_ms` bounds wall-clock evaluation time (D1012 on
/// breach), `max_stack_depth` bounds AST recursion depth (D1011), and
/// `max_sequence_length` bounds query-result sequences (D2015) — the same
/// three guardrails the Python bindings expose. Returns 0 on success, -1 on
/// a NULL handle (error slot set).
///
/// # Safety
/// `expr` must be a live pointer from `jsonata_compile` or NULL.
#[no_mangle]
pub unsafe extern "C" fn jsonata_set_limits(
    expr: *mut JsonataExpr,
    timeout_ms: u64,
    max_stack_depth: u64,
    max_sequence_length: u64,
) -> c_int {
    if expr.is_null() {
        set_error("jsonata_set_limits: NULL expression handle".to_string());
        return -1;
    }
    let expr = &mut *expr;
    expr.options = EvaluatorOptions {
        timeout_ms: (timeout_ms > 0).then_some(timeout_ms),
        max_stack_depth: (max_stack_depth > 0).then_some(max_stack_depth as usize),
        max_sequence_length: (max_sequence_length > 0).then_some(max_sequence_length as usize),
    };
    clear_error();
    0
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

    fn last_code() -> Option<String> {
        let p = jsonata_last_error_code();
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
    fn eval_error_sets_message_and_code() {
        let ce = CString::new(r#"a + b"#).unwrap();
        let cd = CString::new(r#"{"a":1,"b":"x"}"#).unwrap();
        let h = unsafe { jsonata_compile(ce.as_ptr()) };
        assert!(!h.is_null());
        let r = unsafe { jsonata_evaluate(h, cd.as_ptr()) };
        assert!(r.is_null());
        let msg = last_error().expect("eval error should set message");
        assert!(!msg.is_empty());
        unsafe { jsonata_free_expr(h) };

        // $number on a non-numeric string raises a spec-coded error (D3030) —
        // this engine stores the code at the start of the message.
        //
        // This used to pass an *array*, which raises T0410 in jsonata-js
        // (`<(nsb)-:n>` does not accept one) and did so here too once builtins
        // started validating against their signatures. Verified against the
        // reference: `$number([1])` is T0410 and `$number("x")` is D3030 (#102).
        let ce2 = CString::new("$number(b)").unwrap();
        let cd2 = CString::new(r#"{"b":"x"}"#).unwrap();
        let h2 = unsafe { jsonata_compile(ce2.as_ptr()) };
        let r2 = unsafe { jsonata_evaluate(h2, cd2.as_ptr()) };
        assert!(r2.is_null());
        assert_eq!(last_code().as_deref(), Some("D3030"));
        unsafe { jsonata_free_expr(h2) };
    }

    #[test]
    fn uncoded_error_has_null_code() {
        unsafe {
            let ce = CString::new("a").unwrap();
            let cd = CString::new("{not json").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(r.is_null());
            assert!(last_error().unwrap().contains("invalid input JSON"));
            let r2 = jsonata_evaluate(h, cd.as_ptr());
            assert!(r2.is_null());
            assert_eq!(last_code(), None);
            jsonata_free_expr(h);
        }
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
        let out =
            unsafe { eval_str("$uppercase(name)", r#"{"name":"héllo wörld ✓ 日本語"}"#) };
        assert_eq!(out.as_deref(), Some(r#""HÉLLO WÖRLD ✓ 日本語""#));
    }

    #[test]
    fn version_is_crate_version() {
        let v = unsafe { CStr::from_ptr(jsonata_version()).to_str().unwrap() };
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn null_pointer_guards() {
        unsafe {
            assert!(jsonata_compile(std::ptr::null()).is_null());
            assert!(last_error().unwrap().contains("NULL"));

            assert!(jsonata_evaluate(std::ptr::null_mut(), std::ptr::null()).is_null());
            assert!(last_error().unwrap().contains("NULL"));

            let ce = CString::new("a").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            assert!(jsonata_evaluate(h, std::ptr::null()).is_null());
            assert!(last_error().unwrap().contains("NULL"));

            assert_eq!(jsonata_bind_var(h, std::ptr::null(), std::ptr::null()), -1);
            assert!(last_error().unwrap().contains("NULL"));

            // free fns accept NULL without crashing
            jsonata_free_expr(std::ptr::null_mut());
            jsonata_free_string(std::ptr::null_mut());
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn bind_var_round_trip() {
        unsafe {
            let ce = CString::new("$x + n").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            assert!(!h.is_null());

            let name = CString::new("x").unwrap();
            let val = CString::new("40").unwrap();
            assert_eq!(jsonata_bind_var(h, name.as_ptr(), val.as_ptr()), 0);

            let cd = CString::new(r#"{"n":2}"#).unwrap();
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(!r.is_null(), "evaluate failed: {:?}", last_error());
            assert_eq!(CStr::from_ptr(r).to_str().unwrap(), "42");
            jsonata_free_string(r);
            jsonata_free_expr(h);

            // "$"-prefixed name accepted; structured JSON value binds
            let ce2 = CString::new("$sum($x.deep) + n").unwrap();
            let h2 = jsonata_compile(ce2.as_ptr());
            let name2 = CString::new("$x").unwrap();
            let val2 = CString::new(r#"{"deep": [1,2,3]}"#).unwrap();
            assert_eq!(jsonata_bind_var(h2, name2.as_ptr(), val2.as_ptr()), 0);
            let r2 = jsonata_evaluate(h2, cd.as_ptr());
            assert!(!r2.is_null(), "evaluate failed: {:?}", last_error());
            assert_eq!(CStr::from_ptr(r2).to_str().unwrap(), "8");
            jsonata_free_string(r2);

            // re-binding replaces the value
            let val3 = CString::new(r#"{"deep": [10]}"#).unwrap();
            assert_eq!(jsonata_bind_var(h2, name2.as_ptr(), val3.as_ptr()), 0);
            let r3 = jsonata_evaluate(h2, cd.as_ptr());
            assert!(!r3.is_null(), "evaluate failed: {:?}", last_error());
            assert_eq!(CStr::from_ptr(r3).to_str().unwrap(), "12");
            jsonata_free_string(r3);

            // invalid JSON value -> -1 + message
            let bad = CString::new("{nope").unwrap();
            assert_eq!(jsonata_bind_var(h2, name2.as_ptr(), bad.as_ptr()), -1);
            assert!(last_error().unwrap().contains("invalid JSON"));

            jsonata_free_expr(h2);
        }
    }

    #[test]
    fn extract_error_code_shapes() {
        assert_eq!(
            extract_error_code("T2002: not a number").as_deref(),
            Some("T2002")
        );
        assert_eq!(extract_error_code("S0214: bad %").as_deref(), Some("S0214"));
        assert_eq!(
            extract_error_code("Runtime error: D3030: Cannot convert").as_deref(),
            Some("D3030")
        );
        assert_eq!(extract_error_code("Parse error: something"), None);
        assert_eq!(extract_error_code("invalid input JSON: x"), None);
        assert_eq!(extract_error_code("T20: short"), None);
        assert_eq!(extract_error_code(""), None);
    }

    // ── Host functions ──────────────────────────────────────────────────────

    thread_local! {
        // Holds the most recent callback result so the returned pointer stays
        // valid until jsonata copies it (jsonata never frees host results).
        static CB_RESULT: RefCell<CString> = RefCell::new(CString::new("").unwrap());
    }

    unsafe fn store_result(json: String) -> *const c_char {
        CB_RESULT.with(|r| {
            *r.borrow_mut() = CString::new(json).unwrap();
            r.borrow().as_ptr()
        })
    }

    fn arg0(args_json: *const c_char) -> serde_json::Value {
        let s = unsafe { CStr::from_ptr(args_json) }.to_str().unwrap();
        let v: serde_json::Value = serde_json::from_str(s).unwrap();
        v.get(0).cloned().unwrap_or(serde_json::Value::Null)
    }

    unsafe extern "C" fn greet_cb(_ud: *mut c_void, args: *const c_char) -> *const c_char {
        let name = arg0(args);
        let name = name.as_str().unwrap_or("world");
        store_result(format!("\"hello {name}\""))
    }

    unsafe extern "C" fn double_cb(_ud: *mut c_void, args: *const c_char) -> *const c_char {
        let n = arg0(args).as_f64().unwrap_or(0.0);
        store_result(format!("{}", n * 2.0))
    }

    unsafe extern "C" fn frozen_now_cb(_ud: *mut c_void, _args: *const c_char) -> *const c_char {
        store_result("\"2020-01-01T00:00:00.000Z\"".to_string())
    }

    unsafe extern "C" fn boom_cb(_ud: *mut c_void, _args: *const c_char) -> *const c_char {
        std::ptr::null()
    }

    #[test]
    fn host_fn_round_trip() {
        unsafe {
            let ce = CString::new("$greet(name)").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("greet").unwrap();
            assert_eq!(
                jsonata_register_fn(h, name.as_ptr(), Some(greet_cb), std::ptr::null_mut()),
                0
            );
            let cd = CString::new(r#"{"name":"Ada"}"#).unwrap();
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(!r.is_null(), "evaluate failed: {:?}", last_error());
            assert_eq!(CStr::from_ptr(r).to_str().unwrap(), r#""hello Ada""#);
            jsonata_free_string(r);
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn host_fn_maps_over_sequence() {
        unsafe {
            let ce = CString::new("items.$double(qty)").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("double").unwrap();
            assert_eq!(
                jsonata_register_fn(h, name.as_ptr(), Some(double_cb), std::ptr::null_mut()),
                0
            );
            let cd = CString::new(r#"{"items":[{"qty":2},{"qty":5}]}"#).unwrap();
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(!r.is_null(), "evaluate failed: {:?}", last_error());
            assert_eq!(CStr::from_ptr(r).to_str().unwrap(), "[4,10]");
            jsonata_free_string(r);
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn host_fn_override_now() {
        unsafe {
            let ce = CString::new("$now()").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("now").unwrap();
            assert_eq!(
                jsonata_register_fn_override(
                    h,
                    name.as_ptr(),
                    Some(frozen_now_cb),
                    std::ptr::null_mut()
                ),
                0
            );
            let cd = CString::new("null").unwrap();
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(!r.is_null(), "evaluate failed: {:?}", last_error());
            assert_eq!(
                CStr::from_ptr(r).to_str().unwrap(),
                r#""2020-01-01T00:00:00.000Z""#
            );
            jsonata_free_string(r);
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn host_fn_collision_rejected() {
        unsafe {
            let ce = CString::new("$sum(x)").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("sum").unwrap();
            assert_eq!(
                jsonata_register_fn(h, name.as_ptr(), Some(greet_cb), std::ptr::null_mut()),
                -1
            );
            assert!(last_error().unwrap().contains("shadows a built-in"));
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn host_fn_override_compilable_rejected() {
        unsafe {
            let ce = CString::new("$round(x)").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("round").unwrap();
            assert_eq!(
                jsonata_register_fn_override(
                    h,
                    name.as_ptr(),
                    Some(double_cb),
                    std::ptr::null_mut()
                ),
                -1
            );
            assert!(last_error().unwrap().contains("compiled fast path"));
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn host_fn_null_return_is_error() {
        unsafe {
            let ce = CString::new("$boom()").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("boom").unwrap();
            assert_eq!(
                jsonata_register_fn(h, name.as_ptr(), Some(boom_cb), std::ptr::null_mut()),
                0
            );
            let cd = CString::new("null").unwrap();
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(r.is_null());
            assert!(last_error().unwrap().contains("host function"));
            jsonata_free_expr(h);
        }
    }

    #[test]
    fn host_fn_null_func_rejected() {
        unsafe {
            let ce = CString::new("$x()").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            let name = CString::new("x").unwrap();
            assert_eq!(
                jsonata_register_fn(h, name.as_ptr(), None, std::ptr::null_mut()),
                -1
            );
            assert!(last_error().unwrap().contains("NULL"));
            jsonata_free_expr(h);
        }
    }
    #[test]
    fn set_limits_enforced_and_resettable() {
        unsafe {
            let ce = CString::new("$map([1..100000], function($x) { $x })").unwrap();
            let cd = CString::new("{}").unwrap();
            let h = jsonata_compile(ce.as_ptr());
            assert!(!h.is_null());
            assert_eq!(jsonata_set_limits(std::ptr::null_mut(), 0, 0, 0), -1);
            assert_eq!(jsonata_set_limits(h, 0, 0, 10), 0);
            let r = jsonata_evaluate(h, cd.as_ptr());
            assert!(r.is_null(), "sequence limit must fail the evaluation");
            let err = last_error().expect("error slot set");
            assert!(err.contains("D2015"), "unexpected error: {err}");
            // Lifting the limit makes the same handle succeed again.
            assert_eq!(jsonata_set_limits(h, 0, 0, 0), 0);
            let r2 = jsonata_evaluate(h, cd.as_ptr());
            assert!(!r2.is_null());
            jsonata_free_string(r2);
            jsonata_free_expr(h);
        }
    }
}
