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

    use crate::functions;

    match name {
        // ── String functions ────────────────────────────────────────────
        "string" => {
            // Validate the optional prettify argument: must be a boolean.
            let prettify = match args.get(1) {
                None => None,
                Some(JValue::Bool(b)) => Some(*b),
                Some(_) => {
                    return Err(EvaluatorError::TypeError(
                        "string() prettify parameter must be a boolean".to_string(),
                    ))
                }
            };
            let arg = args.first().unwrap_or(&JValue::Null);
            Ok(functions::string::string(arg, prettify)?)
        }
        "length" => match args.first() {
            Some(JValue::String(s)) => Ok(functions::string::length(s)?),
            // Undefined input propagates (caught above by the undefined-propagation guard).
            Some(JValue::Undefined) => Ok(JValue::Undefined),
            // No argument: mirrors tree-walker "requires exactly 1 argument" (no error code,
            // so the test framework accepts it against any expected T-code).
            None => Err(EvaluatorError::EvaluationError(
                "length() requires exactly 1 argument".to_string(),
            )),
            // null and any other non-string type → T0410
            _ => Err(EvaluatorError::TypeError(
                "T0410: Argument 1 of function length does not match function signature"
                    .to_string(),
            )),
        },
        "uppercase" => match args.first() {
            Some(JValue::String(s)) => Ok(functions::string::uppercase(s)?),
            Some(JValue::Undefined) | None => Ok(JValue::Undefined),
            _ => Err(EvaluatorError::TypeError(
                "T0410: Argument 1 of function uppercase does not match function signature"
                    .to_string(),
            )),
        },
        "lowercase" => match args.first() {
            Some(JValue::String(s)) => Ok(functions::string::lowercase(s)?),
            Some(JValue::Undefined) | None => Ok(JValue::Undefined),
            _ => Err(EvaluatorError::TypeError(
                "T0410: Argument 1 of function lowercase does not match function signature"
                    .to_string(),
            )),
        },
        "trim" => match args.first() {
            None | Some(JValue::Null | JValue::Undefined) => Ok(JValue::Null),
            Some(JValue::String(s)) => Ok(functions::string::trim(s)?),
            _ => Err(EvaluatorError::TypeError(
                "trim() requires a string argument".to_string(),
            )),
        },
        "substring" => {
            if args.len() < 2 {
                return Err(EvaluatorError::EvaluationError(
                    "substring() requires at least 2 arguments".to_string(),
                ));
            }
            match (&args[0], &args[1]) {
                (JValue::String(s), JValue::Number(start)) => {
                    // Optional 3rd arg (length) must be a number if provided.
                    let length = match args.get(2) {
                        None => None,
                        Some(JValue::Number(l)) => Some(*l as i64),
                        Some(_) => {
                            return Err(EvaluatorError::TypeError(
                                "T0410: Argument 3 of function substring does not match function signature"
                                    .to_string(),
                            ))
                        }
                    };
                    Ok(functions::string::substring(s, *start as i64, length)?)
                }
                (JValue::String(s), JValue::Undefined) => Ok(
                    crate::evaluator::substring_with_undefined_start(s, args.len() > 2),
                ),
                _ => Err(EvaluatorError::TypeError(
                    "T0410: Argument 1 of function substring does not match function signature"
                        .to_string(),
                )),
            }
        }
        "substringBefore" => {
            if args.len() != 2 {
                return Err(EvaluatorError::TypeError(
                    "T0411: Context value is not a compatible type with argument 2 of function substringBefore".to_string(),
                ));
            }
            match (&args[0], &args[1]) {
                (JValue::String(s), JValue::String(sep)) => {
                    Ok(functions::string::substring_before(s, sep)?)
                }
                (JValue::String(s), JValue::Undefined) => {
                    Ok(functions::string::substring_before(s, crate::evaluator::JS_UNDEFINED_AS_STRING)?)
                }
                // Undefined propagates; null is a type error.
                (JValue::Undefined, _) => Ok(JValue::Undefined),
                _ => Err(EvaluatorError::TypeError(
                    "T0410: Argument 1 of function substringBefore does not match function signature".to_string(),
                )),
            }
        }
        "substringAfter" => {
            if args.len() != 2 {
                return Err(EvaluatorError::TypeError(
                    "T0411: Context value is not a compatible type with argument 2 of function substringAfter".to_string(),
                ));
            }
            match (&args[0], &args[1]) {
                (JValue::String(s), JValue::String(sep)) => {
                    Ok(functions::string::substring_after(s, sep)?)
                }
                (JValue::String(s), JValue::Undefined) => {
                    Ok(functions::string::substring_after(s, crate::evaluator::JS_UNDEFINED_AS_STRING)?)
                }
                // Undefined propagates; null is a type error.
                (JValue::Undefined, _) => Ok(JValue::Undefined),
                _ => Err(EvaluatorError::TypeError(
                    "T0410: Argument 1 of function substringAfter does not match function signature".to_string(),
                )),
            }
        }
        "contains" => {
            if args.len() != 2 {
                return Err(EvaluatorError::EvaluationError(
                    "contains() requires exactly 2 arguments".to_string(),
                ));
            }
            // jsonata-js #809: $contains returns undefined when either argument
            // (the string OR the pattern) is undefined.
            if args[0].is_undefined() || args[1].is_undefined() {
                return Ok(JValue::Undefined);
            }
            match &args[0] {
                JValue::Null => Ok(JValue::Null),
                JValue::String(s) => Ok(functions::string::contains(s, &args[1])?),
                _ => Err(EvaluatorError::TypeError(
                    "contains() requires a string as the first argument".to_string(),
                )),
            }
        }
        "split" => {
            if args.len() < 2 {
                return Err(EvaluatorError::EvaluationError(
                    "split() requires at least 2 arguments".to_string(),
                ));
            }
            match &args[0] {
                JValue::Null | JValue::Undefined => Ok(JValue::Null),
                JValue::String(s) => {
                    // Validate the optional limit argument — must be a positive number.
                    let limit = match args.get(2) {
                        None => None,
                        Some(JValue::Number(n)) => {
                            if *n < 0.0 {
                                return Err(EvaluatorError::EvaluationError(
                                    "D3020: Third argument of split function must be a positive number"
                                        .to_string(),
                                ));
                            }
                            Some(n.floor() as usize)
                        }
                        Some(_) => {
                            return Err(EvaluatorError::TypeError(
                                "split() limit must be a number".to_string(),
                            ))
                        }
                    };
                    Ok(functions::string::split(s, &args[1], limit)?)
                }
                _ => Err(EvaluatorError::TypeError(
                    "split() requires a string as the first argument".to_string(),
                )),
            }
        }
        "join" => {
            if args.is_empty() {
                return Err(EvaluatorError::TypeError(
                    "T0410: Argument 1 of function $join does not match function signature"
                        .to_string(),
                ));
            }
            match &args[0] {
                JValue::Null | JValue::Undefined => Ok(JValue::Null),
                // Signature: <a<s>s?:s> — first arg must be an array of strings.
                JValue::Bool(_) | JValue::Number(_) | JValue::Object(_) => {
                    Err(EvaluatorError::TypeError(
                        "T0412: Argument 1 of function $join must be an array of String"
                            .to_string(),
                    ))
                }
                #[cfg(feature = "python")]
                JValue::LazyPyDict(_) => Err(EvaluatorError::TypeError(
                    "T0412: Argument 1 of function $join must be an array of String".to_string(),
                )),
                JValue::Array(arr) => {
                    // All elements must be strings.
                    for item in arr.iter() {
                        if !matches!(item, JValue::String(_)) {
                            return Err(EvaluatorError::TypeError(
                                "T0412: Argument 1 of function $join must be an array of String"
                                    .to_string(),
                            ));
                        }
                    }
                    // Validate separator: must be a string if provided.
                    let separator = match args.get(1) {
                        None | Some(JValue::Undefined) => None,
                        Some(JValue::String(s)) => Some(&**s),
                        Some(_) => {
                            return Err(EvaluatorError::TypeError(
                                "T0410: Argument 2 of function $join does not match function signature (expected String)"
                                    .to_string(),
                            ))
                        }
                    };
                    Ok(functions::string::join(arr, separator)?)
                }
                JValue::String(s) => Ok(JValue::String(s.clone())),
                _ => Err(EvaluatorError::TypeError(
                    "T0412: Argument 1 of function $join must be an array of String".to_string(),
                )),
            }
        }

        // ── Numeric functions ───────────────────────────────────────────
        "number" => match args.first() {
            Some(v) => Ok(functions::numeric::number(v)?),
            None => Err(EvaluatorError::EvaluationError(
                "number() requires at least 1 argument".to_string(),
            )),
        },
        "floor" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Number(n)) => Ok(functions::numeric::floor(*n)?),
            _ => Err(EvaluatorError::TypeError(
                "floor() requires a number argument".to_string(),
            )),
        },
        "ceil" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Number(n)) => Ok(functions::numeric::ceil(*n)?),
            _ => Err(EvaluatorError::TypeError(
                "ceil() requires a number argument".to_string(),
            )),
        },
        "round" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Number(n)) => {
                let precision = args.get(1).and_then(|v| {
                    if let JValue::Number(p) = v {
                        Some(*p as i32)
                    } else {
                        None
                    }
                });
                Ok(functions::numeric::round(*n, precision)?)
            }
            _ => Err(EvaluatorError::TypeError(
                "round() requires a number argument".to_string(),
            )),
        },
        "abs" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Number(n)) => Ok(functions::numeric::abs(*n)?),
            _ => Err(EvaluatorError::TypeError(
                "abs() requires a number argument".to_string(),
            )),
        },
        "sqrt" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Number(n)) => Ok(functions::numeric::sqrt(*n)?),
            _ => Err(EvaluatorError::TypeError(
                "sqrt() requires a number argument".to_string(),
            )),
        },

        // ── Aggregation functions ───────────────────────────────────────
        "sum" => match args.first() {
            Some(v) if v.is_undefined() => Ok(JValue::Undefined),
            None => Err(EvaluatorError::EvaluationError(
                "sum() requires exactly 1 argument".to_string(),
            )),
            Some(JValue::Null) => Ok(JValue::Null),
            Some(JValue::Array(arr)) => Ok(crate::evaluator::aggregation::sum(arr)?),
            Some(JValue::Number(n)) => Ok(JValue::Number(*n)),
            Some(other) => Ok(functions::numeric::sum(&[other.clone()])?),
        },
        "max" => match args.first() {
            Some(v) if v.is_undefined() => Ok(JValue::Undefined),
            Some(JValue::Null) | None => Ok(JValue::Null),
            Some(JValue::Array(arr)) => Ok(crate::evaluator::aggregation::max(arr)?),
            Some(v @ JValue::Number(_)) => Ok(v.clone()),
            _ => Err(EvaluatorError::TypeError(
                "max() requires an array or number argument".to_string(),
            )),
        },
        "min" => match args.first() {
            Some(v) if v.is_undefined() => Ok(JValue::Undefined),
            Some(JValue::Null) | None => Ok(JValue::Null),
            Some(JValue::Array(arr)) => Ok(crate::evaluator::aggregation::min(arr)?),
            Some(v @ JValue::Number(_)) => Ok(v.clone()),
            _ => Err(EvaluatorError::TypeError(
                "min() requires an array or number argument".to_string(),
            )),
        },
        "average" => match args.first() {
            Some(v) if v.is_undefined() => Ok(JValue::Undefined),
            Some(JValue::Null) | None => Ok(JValue::Null),
            Some(JValue::Array(arr)) => Ok(crate::evaluator::aggregation::average(arr)?),
            Some(v @ JValue::Number(_)) => Ok(v.clone()),
            _ => Err(EvaluatorError::TypeError(
                "average() requires an array or number argument".to_string(),
            )),
        },
        "count" => match args.first() {
            Some(v) if v.is_undefined() => Ok(JValue::from(0i64)),
            Some(JValue::Null) | None => Ok(JValue::from(0i64)),
            Some(JValue::Array(arr)) => Ok(functions::array::count(arr)?),
            _ => Ok(JValue::from(1i64)),
        },

        // ── Boolean / logic ─────────────────────────────────────────────
        "boolean" => match args.first() {
            Some(v) => Ok(functions::boolean::boolean(v)?),
            None => Err(EvaluatorError::EvaluationError(
                "boolean() requires 1 argument".to_string(),
            )),
        },
        "not" => match args.first() {
            Some(v) => Ok(JValue::Bool(!crate::evaluator::compiled_is_truthy(v))),
            None => Err(EvaluatorError::EvaluationError(
                "not() requires 1 argument".to_string(),
            )),
        },

        // ── Array functions ─────────────────────────────────────────────
        "append" => {
            if args.len() != 2 {
                return Err(EvaluatorError::EvaluationError(
                    "append() requires exactly 2 arguments".to_string(),
                ));
            }
            let first = &args[0];
            let second = &args[1];
            // Only a *missing* operand is skipped; an explicit null is a value
            // and gets appended. Mirrors the tree-walker arm.
            if matches!(second, JValue::Undefined) {
                return Ok(first.clone());
            }
            if matches!(first, JValue::Undefined) {
                return Ok(second.clone());
            }
            let arr = match first {
                JValue::Array(a) => a.to_vec(),
                other => vec![other.clone()],
            };
            let second_len = match second {
                JValue::Array(a) => a.len(),
                _ => 1,
            };
            crate::evaluator::check_sequence_length(arr.len() + second_len, options)?;
            Ok(functions::array::append(&arr, second)?)
        }
        "reverse" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Array(arr)) => Ok(functions::array::reverse(arr)?),
            _ => Err(EvaluatorError::TypeError(
                "reverse() requires an array argument".to_string(),
            )),
        },
        "distinct" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Array(arr)) if arr.len() > 1 => Ok(functions::array::distinct(arr)?),
            // Non-array input, and arrays of length <= 1, pass through unchanged
            // (jsonata-js functions.js: `if(!Array.isArray(arr) || arr.length <= 1) return arr;`)
            Some(other) => Ok(other.clone()),
        },

        // ── Object functions ────────────────────────────────────────────
        "keys" => match args.first() {
            Some(JValue::Null | JValue::Undefined) | None => Ok(JValue::Null),
            Some(JValue::Lambda { .. } | JValue::Builtin { .. }) => Ok(JValue::Null),
            Some(JValue::Object(obj)) => {
                if obj.is_empty() {
                    Ok(JValue::Null)
                } else {
                    let keys: Vec<JValue> = obj.keys().map(|k| JValue::string(k.clone())).collect();
                    crate::evaluator::check_sequence_length(keys.len(), options)?;
                    if keys.len() == 1 {
                        Ok(keys.into_iter().next().unwrap())
                    } else {
                        Ok(JValue::array(keys))
                    }
                }
            }
            Some(JValue::Array(arr)) => {
                let mut all_keys: Vec<JValue> = Vec::new();
                for item in arr.iter() {
                    let normalized_item = crate::evaluator::normalize_lazy(item)?;
                    if let JValue::Object(obj) = &normalized_item {
                        for key in obj.keys() {
                            let k = JValue::string(key.clone());
                            if !all_keys.contains(&k) {
                                all_keys.push(k);
                            }
                        }
                    }
                }
                if all_keys.is_empty() {
                    Ok(JValue::Null)
                } else if all_keys.len() == 1 {
                    Ok(all_keys.into_iter().next().unwrap())
                } else {
                    crate::evaluator::check_sequence_length(all_keys.len(), options)?;
                    Ok(JValue::array(all_keys))
                }
            }
            _ => Ok(JValue::Null),
        },
        "merge" => match args.len() {
            0 => Err(EvaluatorError::EvaluationError(
                "merge() requires at least 1 argument".to_string(),
            )),
            1 => match &args[0] {
                JValue::Array(arr) => Ok(functions::object::merge(arr)?),
                JValue::Null | JValue::Undefined => Ok(JValue::Null),
                JValue::Object(_) => Ok(args[0].clone()),
                _ => Err(EvaluatorError::TypeError(
                    "merge() requires objects or an array of objects".to_string(),
                )),
            },
            _ => Ok(functions::object::merge(args)?),
        },

        _ => unreachable!("dispatch_pure called with non-pure builtin: {}", name),
    }
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
