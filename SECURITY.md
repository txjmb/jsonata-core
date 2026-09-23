# Security Policy

This policy covers the **jsonata-core** Rust crate, the **jsonatapy** Python
package, the `jsonata` command-line tool, and the C API in `bindings/c`, all
published from this repository.

## Supported versions

Security fixes are released as a new patch version of the latest minor line.
Older lines do not receive backports; upgrade to the latest release to pick up
a fix.

| Version | Supported |
| ------- | --------- |
| 2.2.x (latest patch) | ✅ |
| < 2.2 | ❌ |

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.**

Report them privately through GitHub instead: go to the repository's
[**Security** tab](https://github.com/txjmb/jsonata-core/security) and choose
**Report a vulnerability**. Only the maintainers can see the report.

Please include as much of the following as you can:

- The affected component (Rust crate, Python package, CLI, or C API) and version
- The JSONata expression and input data that trigger the issue, as a minimal
  reproduction
- What happens (crash, hang, memory growth, wrong result) and what you expected
- Any evaluation limits you had set (`timeout`, `max_stack_depth`,
  `max_sequence_length`)
- Your assessment of the impact, if you have one

## What to expect

1. **Acknowledgement.** We aim to acknowledge your report within 5 business
   days.
2. **Assessment.** We will confirm whether the issue is a vulnerability under
   this policy and keep you updated as we investigate.
3. **Fix and release.** Confirmed vulnerabilities are fixed in a new patch
   release published to crates.io and PyPI.
4. **Disclosure.** Once a fixed release is available, we publish a GitHub
   Security Advisory for this repository and request a CVE where appropriate.
   We coordinate the disclosure date with you, and we credit you in the
   advisory unless you prefer to stay anonymous.

Please give us a reasonable chance to release a fix before you disclose the
issue publicly.

## Scope

JSONata expressions cannot read files, open network connections, or run system
commands. The security-relevant risks in this project are therefore
denial of service and memory safety.

### Evaluating untrusted expressions or data

By default, evaluation has **no resource limits**: `timeout`, `max_stack_depth`
and `max_sequence_length` are all unset. If you evaluate expressions or data
from untrusted sources, set them explicitly.

- **Python:** pass them to `jsonatapy.compile(...)` as defaults, or per call to
  `evaluate(...)` / `evaluate_json(...)`.
- **Rust:** pass an `EvaluatorOptions` to `Expression::compile_with_options` or
  `Expression::evaluate_with_options`.

Known limitation: `max_sequence_length` does not currently apply to array
literals such as `[1, 2, 3]`, only to sequences produced by evaluation
(mapping, filtering, wildcards, ranges and similar).

Functions you register with `register` / `register_override` can be called by
any expression you evaluate. Only register functions that are safe to call
with attacker-controlled arguments.

### In scope

- A crash of the host process (abort, stack overflow, segmentation fault) or a
  Rust panic caused by any expression or input, with or without limits set
- Evaluation that exceeds a limit you configured: running past `timeout`,
  recursing past `max_stack_depth`, or building a sequence longer than
  `max_sequence_length` (apart from the array-literal limitation above)
- Memory-safety bugs in the C API or in any `unsafe` Rust code
- Any way for an expression to access data or functions it was not given

### Out of scope

- Slow or unbounded evaluation when no limits were configured. This is the
  documented default; see above.
- Behavior that matches the reference
  [jsonata-js](https://github.com/jsonata-js/jsonata) implementation. Please
  report those upstream.
- Vulnerabilities in third-party dependencies that are already publicly known.
  We track these through automated scanning; report them only if you have found
  a way to exploit them through this project.
- The benchmark and test code under `benchmarks/` and `tests/`. It is
  development tooling and is never compiled into or run by the library.
