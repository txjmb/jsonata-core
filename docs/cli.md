# Command-line interface

Both packages ship a command-line tool with an identical flag/exit-code
contract — `jsonata` (Rust, from `jsonata-core`) and `jsonatapy` (Python,
from `jsonatapy`). Pick whichever matches how you installed the library;
scripts written against one work unchanged against the other.

The shape deliberately mirrors `jq`: `<tool> [OPTIONS] EXPRESSION [FILE]`,
reading JSON from a file argument or stdin, printing the result to stdout.
The expression syntax itself is JSONata, not `jq`'s — but where `jq` often
needs several piped filters to filter, aggregate, and reshape data, a single
JSONata expression usually does all three at once. Path expressions
(`orders[price > 100].product`) will also look familiar if you've used
JSONPath.

## Installation

```bash
# Python
pip install jsonatapy
jsonatapy 'orders[price > 100].product' data.json

# Rust (from source)
cargo install jsonata-core --features cli
jsonata 'orders[price > 100].product' data.json
```

## Usage

```
jsonata   [OPTIONS] [EXPRESSION] [FILE]
jsonatapy [OPTIONS] [EXPRESSION] [FILE]

<tool> [OPTIONS] --from-file <EXPR_FILE> [FILE]
```

If `FILE` is omitted, input is read from stdin.

```bash
echo '{"name": "Alice"}' | jsonatapy 'name'
# "Alice"

jsonatapy 'name' data.json
# "Alice"
```

## Flags

| Flag | Description |
|---|---|
| `-c`, `--compact` | Compact JSON output (default: pretty-printed) |
| `-r`, `--raw-output` | Print string results without surrounding quotes (non-string results are unaffected) |
| `-n`, `--null-input` | Don't read input; `$` is `Undefined`. Cannot be combined with a data-file argument. |
| `-f`, `--from-file <FILE>` | Read the expression from `FILE` instead of the first positional argument. The (now single) remaining positional argument, if given, is the input data file. |
| `--arg NAME=VALUE` | Bind `$NAME` to the string `VALUE`. Repeatable. `VALUE` may itself contain `=` characters (only the first `=` splits name from value). |
| `--argjson NAME=JSON` | Bind `$NAME` to the JSON value parsed from `JSON`. Repeatable. |
| `-V`, `--version` | Print version and exit 0. |
| `-h`, `--help` | Print help and exit 0. |

```bash
jsonatapy --arg region=us --argjson limit=5 '$region & ": " & $limit' -n
# "us: 5"
```

(The quotes are part of the JSON output — the result is a string. Use `-r` to print it unquoted.)

## `-n`/`--null-input`

Evaluates the expression with no input document at all: `$` is a true
JSONata `Undefined`, not an explicit `null`. Useful for expressions that
don't depend on external data (`$now()`, arithmetic, `--arg`/`--argjson`
bindings):

```bash
jsonatapy -n '1 + 1'
# 2
```

This is only observable for expressions that reference `$` directly — an
explicit `null` context would print `null` for the bare expression `$`,
while `Undefined` prints nothing:

```bash
jsonatapy -n '$'
# (no output, exit 0)
```

## Output

- A JSONata `Undefined` result prints nothing to stdout, exit 0.
- A JSON `null` result prints the literal text `null`, exit 0.
- Otherwise, the result is printed as JSON (pretty by default, single-line
  with `-c`), followed by a trailing newline. With `-r`, string results are
  printed unquoted/unescaped instead; non-string results are unaffected by
  `-r`.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success (including an `Undefined` result, and `--version`/`--help`) |
| 1 | Expression parse error or evaluation error |
| 2 | Usage/invocation error: bad flags, malformed `--arg`/`--argjson`, an incompatible flag combination (e.g. `-n` + data file), or an expression/input file that could not be read |
| 3 | Input was read successfully but is not valid JSON |

## Error message format

Errors go to stderr. JSONata spec-coded errors (e.g. from evaluation or
parsing — codes match `^[TDUS]\d{4}:`) are printed exactly as `CODE: message`
with no extra prefix, so scripts/agents can pattern-match on the code
directly:

```bash
jsonatapy 'null + 1' -n
# T2002: The left side of the + operator must evaluate to a number
```

All other errors are prefixed with `error: ` (or, for non-spec-coded parse
errors specifically, `Parse error: `).

## MCP server (Python only)

`jsonatapy` can also run as an [MCP](https://modelcontextprotocol.io/) server,
exposing JSONata evaluation as tools for AI agents:

```bash
pip install 'jsonatapy[mcp]'
jsonatapy mcp          # stdio transport
jsonatapy mcp --http   # HTTP transport (default port 8000)
```

Because `mcp` is reserved as the first argument for this subcommand, it
cannot be used as a literal expression naming a field called `mcp` — use
`--from-file` or a longer path expression (e.g. `$.mcp`) instead.

## Full contract

`study/cli_spec.md` in the repository is the canonical, executable
flag/exit-code contract both implementations are tested against
(`study/cli_fixtures.json`) — consult it if you're contributing to either
CLI implementation rather than just using it.
