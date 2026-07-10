use clap::Parser;
use jsonata_core::evaluator::{Context, Evaluator};
use jsonata_core::parser;
use jsonata_core::value::JValue;
use std::io::Read;
use std::process::ExitCode;

/// Evaluate JSONata expressions against JSON data.
#[derive(Parser, Debug)]
#[command(
    name = "jsonata",
    version,
    about = "Evaluate JSONata expressions against JSON data"
)]
struct Cli {
    /// Compact JSON output (default: pretty-printed)
    #[arg(short = 'c', long)]
    compact: bool,

    /// Print string results without surrounding quotes
    #[arg(short = 'r', long = "raw-output")]
    raw_output: bool,

    /// Don't read input; $ is undefined
    #[arg(short = 'n', long = "null-input")]
    null_input: bool,

    /// Read the expression from a file instead of the first positional argument
    #[arg(short = 'f', long = "from-file", value_name = "FILE")]
    from_file: Option<String>,

    /// Bind $NAME to a string value: --arg NAME=VALUE
    #[arg(long = "arg", value_name = "NAME=VALUE", action = clap::ArgAction::Append)]
    arg: Vec<String>,

    /// Bind $NAME to a parsed JSON value: --argjson NAME=JSON
    #[arg(long = "argjson", value_name = "NAME=JSON", action = clap::ArgAction::Append)]
    argjson: Vec<String>,

    /// The JSONata expression (or, with --from-file, the input data file)
    #[arg(value_name = "EXPRESSION_OR_FILE")]
    positional1: Option<String>,

    /// The input data file (used only when --from-file supplies the expression)
    #[arg(value_name = "FILE")]
    positional2: Option<String>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> ExitCode {
    let expression = match &cli.positional1 {
        Some(expr) => expr.clone(),
        None => {
            eprintln!("error: missing required argument: EXPRESSION");
            return ExitCode::from(2);
        }
    };

    let data_source = cli.positional2.clone();

    let data = match read_input(data_source.as_deref()) {
        Ok(data) => data,
        Err((msg, code)) => {
            eprintln!("{}", msg);
            return ExitCode::from(code);
        }
    };

    let ast = match parser::parse(&expression) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    let mut evaluator = Evaluator::with_context(Context::new());
    let result = match evaluator.evaluate(&ast, &data) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {}", e);
            return ExitCode::from(1);
        }
    };

    if result.is_undefined() {
        return ExitCode::SUCCESS;
    }

    print_result(&result, cli.compact, cli.raw_output)
}

fn print_result(result: &JValue, compact: bool, raw_output: bool) -> ExitCode {
    if raw_output {
        if let JValue::String(s) = result {
            println!("{}", s);
            return ExitCode::SUCCESS;
        }
    }

    let text = if compact {
        result.to_json_string()
    } else {
        result.to_json_string_pretty()
    };

    match text {
        Ok(s) => {
            println!("{}", s);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: could not serialize result: {}", e);
            ExitCode::from(1)
        }
    }
}

// NOTE: multi-document stdin (e.g. `{"a":1}\n{"a":2}\n`) is NOT supported —
// JValue::from_json_str rejects trailing non-whitespace content after the
// first JSON value with a serde_json "trailing characters" error, surfaced
// here as exit code 3. jq's --slurp/streaming semantics are explicitly out
// of scope for this CLI (see Phase 1 of the design spec).

/// Reads and JSON-parses the input document from a file path, or stdin if
/// `path` is `None`. Returns `(stderr message, exit code)` on failure.
fn read_input(path: Option<&str>) -> Result<JValue, (String, u8)> {
    let raw = match path {
        Some(p) => std::fs::read_to_string(p)
            .map_err(|e| (format!("error: could not read input file {}: {}", p, e), 2))?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .map_err(|e| (format!("error: could not read stdin: {}", e), 2))?;
            buf
        }
    };
    JValue::from_json_str(&raw).map_err(|e| (format!("error: invalid JSON input: {}", e), 3))
}
