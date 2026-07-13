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
