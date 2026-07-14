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
use std::ffi::{c_char, c_int, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::evaluator::{self, EvaluatorOptions};
use crate::value::JValue;
use crate::{compiler, parser, vm};

pub struct JsonataExpr {
    ast: crate::ast::AstNode,
    bytecode: OnceCell<Option<vm::BytecodeProgram>>,
    /// Variables registered via `jsonata_bind_var`, applied on every
    /// subsequent `jsonata_evaluate`. Non-empty bindings force the
    /// tree-walker (the VM path takes no user context), mirroring the
    /// Python bindings' behavior in `lib.rs::run_eval`.
    bindings: Vec<(String, JValue)>,
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
        // Same dispatch as JsonataExpression::run_eval in lib.rs: VM when the
        // expression compiles to bytecode AND no user bindings exist,
        // tree-walker otherwise.
        let result = if expr.bindings.is_empty() {
            let bytecode = expr.bytecode.get_or_init(|| {
                evaluator::try_compile_expr(&expr.ast)
                    .map(|ce| compiler::BytecodeCompiler::compile(&ce))
            });
            if let Some(bc) = bytecode {
                vm::Vm::with_options(bc, EvaluatorOptions::default()).run(&data, None)
            } else {
                let mut ev = evaluator::Evaluator::with_options(
                    evaluator::Context::new(),
                    EvaluatorOptions::default(),
                );
                ev.evaluate(&expr.ast, &data)
            }
        } else {
            let mut context = evaluator::Context::new();
            for (name, value) in &expr.bindings {
                context.bind(name.clone(), value.clone());
            }
            let mut ev = evaluator::Evaluator::with_options(context, EvaluatorOptions::default());
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

        // $number on an array raises a spec-coded error (D3030) — this
        // engine stores the code at the start of the message
        let ce2 = CString::new("$number(b)").unwrap();
        let cd2 = CString::new(r#"{"b":[1]}"#).unwrap();
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
}
