//! One dispatcher for every builtin that needs nothing but its arguments.
//!
//! A builtin used to be implemented up to three times -- once in the compiled
//! path, once in the tree-walker, once again for by-reference calls -- and the
//! copies drifted. Twenty-four of them existed in exactly one of the three, so
//! `$map(arr, $type)` failed while `$type(x)` worked. This module is the single
//! implementation the three dispatch sites share.
//!
//! What does NOT live here: the ten builtins that need the evaluator itself
//! ($map, $filter, $reduce, $single, $sift, $each, $sort, $eval, $match,
//! $replace). They take AST arguments and call back into evaluation, so they
//! stay in `evaluate_function_call`. That line is a real boundary, not an
//! accident of this refactor -- jsonata-js draws it in the same place.

use crate::evaluator::{EvaluatorError, EvaluatorOptions};
use crate::value::JValue;

/// The builtins `dispatch_pure` can handle: everything that needs only its
/// arguments, the context value, and the evaluation options.
///
/// Paired with `dispatch_pure` the way `is_compilable_builtin` is paired with
/// the compiled path, so the dispatcher's match can keep an `unreachable!()`
/// fallback: a name only reaches it if this predicate admitted it. Adding a
/// name here without adding an arm there is therefore a panic, not a silent
/// wrong answer.
pub(crate) fn is_pure_builtin(name: &str) -> bool {
    matches!(
        name,
        "string"
            | "length"
            | "uppercase"
            | "lowercase"
            | "number"
            | "sum"
            | "count"
            | "substring"
            | "substringBefore"
            | "substringAfter"
            | "pad"
            | "trim"
            | "contains"
            | "split"
            | "join"
            | "max"
            | "min"
            | "average"
            | "abs"
            | "floor"
            | "ceil"
            | "round"
            | "sqrt"
            | "power"
            | "formatNumber"
            | "formatBase"
            | "formatInteger"
            | "parseInteger"
            | "append"
            | "reverse"
            | "shuffle"
            | "zip"
            | "distinct"
            | "exists"
            | "keys"
            | "lookup"
            | "spread"
            | "merge"
            | "boolean"
            | "not"
            | "type"
            | "base64encode"
            | "base64decode"
            | "encodeUrlComponent"
            | "decodeUrlComponent"
            | "encodeUrl"
            | "decodeUrl"
            | "error"
            | "assert"
            | "now"
            | "millis"
            | "toMillis"
            | "fromMillis"
    )
}

/// Dispatch a builtin that needs nothing but its arguments.
///
/// `context` is the JSONata context value (`$`) at the call site. It is used
/// for implicit-argument insertion and for the signature's `-` modifier, and
/// jsonata-js passes the same value (`input`) in all three dispatch positions.
pub(crate) fn dispatch_pure(
    name: &str,
    args: &[JValue],
    context: &JValue,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    // 1. Implicit context insertion. The union of what the two paths used to
    //    do separately: only the tree-walker had `fromMillis` here and
    //    `replace` below, which was invisible because neither compiles.
    let args_storage: Vec<JValue>;
    let args: &[JValue] = if args.is_empty() {
        match name {
            "string" => {
                // $string() with a null/undefined context is undefined, not "null".
                if context.is_undefined() || context.is_null() {
                    return Ok(JValue::Undefined);
                }
                args_storage = vec![context.clone()];
                &args_storage
            }
            "number" | "boolean" | "uppercase" | "lowercase" | "fromMillis" => {
                args_storage = vec![context.clone()];
                &args_storage
            }
            _ => args,
        }
    } else if args.len() == 1 {
        match name {
            "substringBefore" | "substringAfter" | "contains" | "split" | "replace" => {
                if matches!(context, JValue::String(_)) {
                    args_storage = std::iter::once(context.clone())
                        .chain(args.iter().cloned())
                        .collect();
                    &args_storage
                } else {
                    args
                }
            }
            _ => args,
        }
    } else {
        args
    };

    // 2. Materialize top-level lazy args so every builtin sees plain Objects.
    #[cfg(feature = "python")]
    let lazy_storage: Vec<JValue>;
    #[cfg(feature = "python")]
    let args: &[JValue] = if args.iter().any(|a| matches!(a, JValue::LazyPyDict(_))) {
        lazy_storage = args
            .iter()
            .map(crate::evaluator::normalize_lazy)
            .collect::<Result<Vec<_>, _>>()?;
        &lazy_storage
    } else {
        args
    };

    // 3. Validate and coerce against the jsonata-js signature.
    let sig_storage: Vec<JValue>;
    let args: &[JValue] = match crate::evaluator::validate_builtin_args(name, args, context)? {
        Some(coerced) => {
            sig_storage = coerced;
            &sig_storage
        }
        None => args,
    };

    // 4. Undefined propagation -- deliberately AFTER validation, which is the
    //    order jsonata-js works in. See the note in `validate_builtin_args`.
    if args.first().is_some_and(JValue::is_undefined)
        && crate::evaluator::propagates_undefined(name)
    {
        return Ok(JValue::Undefined);
    }

    let _ = (args, options);
    unreachable!("dispatch_pure called with non-pure builtin: {}", name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_admits_the_pure_set_and_rejects_the_evaluator_set() {
        for name in [
            "string",
            "length",
            "uppercase",
            "lowercase",
            "number",
            "sum",
            "count",
            "substring",
            "substringBefore",
            "substringAfter",
            "pad",
            "trim",
            "contains",
            "split",
            "join",
            "max",
            "min",
            "average",
            "abs",
            "floor",
            "ceil",
            "round",
            "sqrt",
            "power",
            "formatNumber",
            "formatBase",
            "formatInteger",
            "parseInteger",
            "append",
            "reverse",
            "shuffle",
            "zip",
            "distinct",
            "exists",
            "keys",
            "lookup",
            "spread",
            "merge",
            "boolean",
            "not",
            "type",
            "base64encode",
            "base64decode",
            "encodeUrlComponent",
            "decodeUrlComponent",
            "encodeUrl",
            "decodeUrl",
            "error",
            "assert",
            "now",
            "millis",
            "toMillis",
            "fromMillis",
        ] {
            assert!(is_pure_builtin(name), "{name} should be a pure builtin");
        }

        // These need the evaluator: they take AST arguments and call back into
        // evaluation. Admitting one here would route it into `unreachable!()`.
        for name in [
            "map", "filter", "reduce", "single", "sift", "each", "sort", "eval", "match", "replace",
        ] {
            assert!(!is_pure_builtin(name), "{name} needs the evaluator");
        }

        assert!(!is_pure_builtin("nosuchfunction"));
    }

    #[test]
    fn validation_runs_before_undefined_propagation() {
        // $substring(missing) binds its lone undefined argument to parameter 2
        // and takes parameter 1 from the context, so a non-string context is a
        // T0411 -- not undefined. Propagating first would swallow that, which
        // is the bug #106 fixed in the compiled path.
        let opts = EvaluatorOptions::default();
        let err = dispatch_pure(
            "substring",
            &[JValue::Undefined],
            &JValue::Number(5.0),
            &opts,
        )
        .expect_err("a numeric context cannot satisfy substring's first parameter");
        assert!(
            err.to_string().contains("T0411"),
            "expected T0411, got {err}"
        );
    }
}
