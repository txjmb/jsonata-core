# `jsonata` CLI flag and exit-code contract

This document is the canonical, cross-language contract for the `jsonata` CLI.
The Rust binary (`src/bin/jsonata/`) implements it; Phase 2's Python
`jsonatapy` entry point mirrors it exactly. `cli_fixtures.json` in this same
directory is the shared, executable test-case list both implementations are
tested against — do not let this document and that file drift from each
other or from either implementation.

## Usage

```
jsonata [OPTIONS] [EXPRESSION] [FILE]
jsonata [OPTIONS] --from-file <EXPR_FILE> [FILE]
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

## Input resolution

- No `-n`, no `-f`: first positional argument is the expression; second
  positional argument (optional) is the input file, else stdin.
- `-f <EXPR_FILE>`, no `-n`: expression comes from `EXPR_FILE`; the (only)
  positional argument, if given, is the input file, else stdin.
- `-n`: input is never read (`$` = `Undefined`), regardless of `-f`. A data
  file positional argument combined with `-n` is a usage error (exit 2).
- Multi-JSON-document input (e.g. NDJSON) is **not** supported — input must
  be exactly one JSON value; trailing content after it is a parse error
  (exit 3). No slurp/streaming mode exists in this version.

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

Precedence: usage errors (code 2), including malformed `--arg`/`--argjson`
bindings, are validated immediately after argument resolution — before any
input is read and before the expression is parsed. This means a malformed
binding always exits 2, even if the expression would also fail to parse
(otherwise code 1) or the input would also be invalid JSON (otherwise code
3).

## Error message format

Errors go to stderr. JSONata spec-coded errors (e.g. from evaluation or
parsing — codes match `^[TDUS]\d{4}:`) are printed exactly as `CODE: message`
with no extra prefix, so scripts/agents can pattern-match on the code
directly. All other errors are prefixed with `error: ` (or, for
non-spec-coded parse errors specifically, `Parse error: `).

## Python (`jsonatapy`) implementation notes

The Python CLI (`jsonatapy`, this same package's console script) implements
this exact contract, using a new library method, `evaluate_json_or_none()`
(added in Phase 2 specifically for this purpose), to correctly distinguish
an Undefined *result* from an explicit null result -- both `evaluate()` and
`evaluate_json()` collapse that distinction, `evaluate_json_or_none()`
does not. `evaluate_json_or_none()`'s `json_str` parameter also accepts
`None` (in addition to a JSON string) to bind the top-level context (`$`)
to a true `Undefined`, which `-n`/`--null-input` uses -- this is distinct
from passing the text `"null"`, which binds `$` to an explicit JSON null.

One disclosed divergence remains:

- **The literal word `mcp` as the first argument is reserved for the MCP
  subcommand (`jsonatapy mcp ...`) and cannot be used to evaluate an
  expression literally named `mcp`.** `jsonatapy mcp` always launches the
  MCP server, even if you intended to evaluate the bare field-access
  expression `mcp` against stdin. To evaluate an expression literally named
  `mcp`, use `--from-file` to supply it instead of the first positional
  argument (e.g. write `mcp` to a file and pass `-f thatfile`), or
  reference it via a longer path expression that doesn't start with the
  bare token (e.g. `$.mcp` if your data structure allows it). This
  divergence does not exist in the Rust CLI, which has no subcommand
  concept — `jsonata mcp` there simply evaluates the expression `mcp`.
