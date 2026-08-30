use jsonata_core::evaluator::EvaluatorError;

/// Formats an `EvaluatorError` for CLI stderr output: `e.message()` (the
/// library's shared unwrap, see `EvaluatorError::message` in
/// `src/evaluator.rs`) already carries a JSONata spec code prefix like
/// "T2002: ..." when applicable; this adds an "error: " prefix only when
/// it doesn't. `ParserError::display_message()` needs no equivalent
/// wrapper here — it's already fully CLI-ready (see `src/parser.rs`), so
/// callers use it directly.
pub fn format_evaluator_error(e: &EvaluatorError) -> String {
    let msg = e.message();
    if e.code().is_some() {
        msg.to_string()
    } else {
        format!("error: {}", msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coded_evaluator_error_passes_through_unwrapped() {
        let e = EvaluatorError::TypeError(
            "T2002: The left side of the + operator must evaluate to a number".to_string(),
        );
        assert_eq!(
            format_evaluator_error(&e),
            "T2002: The left side of the + operator must evaluate to a number"
        );
    }

    #[test]
    fn uncoded_evaluator_error_gets_error_prefix() {
        let e = EvaluatorError::ReferenceError("$foo is not defined".to_string());
        assert_eq!(format_evaluator_error(&e), "error: $foo is not defined");
    }
}
