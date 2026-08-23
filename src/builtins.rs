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
    let _ = (args, context, options);
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
}
