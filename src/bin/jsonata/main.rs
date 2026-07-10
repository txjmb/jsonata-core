use clap::Parser;

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

fn main() {
    let _cli = Cli::parse();
}
