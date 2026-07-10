/// Where the JSONata expression text comes from.
#[derive(Debug, PartialEq)]
pub enum ExpressionSource {
    Inline(String),
    File(String),
}

/// Where the input JSON document comes from.
#[derive(Debug, PartialEq)]
pub enum InputSource {
    Stdin,
    File(String),
    Null,
}

/// Resolves the expression source and input source from parsed CLI
/// arguments. `--from-file` shifts what a bare positional argument means
/// (it becomes the input data file, not the expression); `--null-input`
/// suppresses reading input entirely and conflicts with supplying a data
/// file. See Task 4 of the Phase 1 plan for the full truth table this
/// implements.
pub fn resolve(cli: &crate::Cli) -> Result<(ExpressionSource, InputSource), String> {
    let (expr_source, data_file) = match (&cli.from_file, &cli.positional1, &cli.positional2) {
        (Some(expr_file), data_file, None) => {
            (ExpressionSource::File(expr_file.clone()), data_file.clone())
        }
        (Some(_), _, Some(_)) => {
            return Err(
                "with --from-file, only one positional argument (the input file) is allowed"
                    .to_string(),
            );
        }
        (None, Some(expr), data_file) => {
            (ExpressionSource::Inline(expr.clone()), data_file.clone())
        }
        (None, None, _) => {
            return Err("missing required argument: EXPRESSION (or use --from-file)".to_string());
        }
    };

    if cli.null_input {
        if data_file.is_some() {
            return Err("--null-input cannot be combined with an input file argument".to_string());
        }
        Ok((expr_source, InputSource::Null))
    } else {
        match data_file {
            Some(f) => Ok((expr_source, InputSource::File(f))),
            None => Ok((expr_source, InputSource::Stdin)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Cli;

    fn cli(
        from_file: Option<&str>,
        positional1: Option<&str>,
        positional2: Option<&str>,
        null_input: bool,
    ) -> Cli {
        Cli {
            compact: false,
            raw_output: false,
            null_input,
            from_file: from_file.map(String::from),
            arg: Vec::new(),
            argjson: Vec::new(),
            positional1: positional1.map(String::from),
            positional2: positional2.map(String::from),
        }
    }

    #[test]
    fn plain_expression_and_stdin() {
        let c = cli(None, Some("name"), None, false);
        assert_eq!(
            resolve(&c),
            Ok((ExpressionSource::Inline("name".into()), InputSource::Stdin))
        );
    }

    #[test]
    fn plain_expression_and_file() {
        let c = cli(None, Some("name"), Some("data.json"), false);
        assert_eq!(
            resolve(&c),
            Ok((
                ExpressionSource::Inline("name".into()),
                InputSource::File("data.json".into())
            ))
        );
    }

    #[test]
    fn missing_expression_is_an_error() {
        let c = cli(None, None, None, false);
        assert!(resolve(&c).is_err());
    }

    #[test]
    fn null_input_with_no_data_file_is_null_source() {
        let c = cli(None, Some("$now()"), None, true);
        assert_eq!(
            resolve(&c),
            Ok((ExpressionSource::Inline("$now()".into()), InputSource::Null))
        );
    }

    #[test]
    fn null_input_with_data_file_is_an_error() {
        let c = cli(None, Some("name"), Some("data.json"), true);
        assert!(resolve(&c).is_err());
    }

    #[test]
    fn from_file_shifts_positional1_to_the_data_file() {
        let c = cli(Some("expr.jsonata"), Some("data.json"), None, false);
        assert_eq!(
            resolve(&c),
            Ok((
                ExpressionSource::File("expr.jsonata".into()),
                InputSource::File("data.json".into())
            ))
        );
    }

    #[test]
    fn from_file_with_no_positionals_reads_stdin() {
        let c = cli(Some("expr.jsonata"), None, None, false);
        assert_eq!(
            resolve(&c),
            Ok((
                ExpressionSource::File("expr.jsonata".into()),
                InputSource::Stdin
            ))
        );
    }

    #[test]
    fn from_file_with_two_positionals_is_an_error() {
        let c = cli(Some("expr.jsonata"), Some("extra1"), Some("extra2"), false);
        assert!(resolve(&c).is_err());
    }
}
