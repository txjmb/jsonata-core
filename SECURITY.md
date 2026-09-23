# Security Policy

This policy covers the **jsonata-core** Rust crate, the **jsonatapy** Python
package, the `jsonata` command-line tool, and the C API in `bindings/c`, all
published from this repository.

## Supported Versions

Security fixes are released as a new patch version of the latest minor line,
published to [crates.io](https://crates.io/crates/jsonata-core) and
[PyPI](https://pypi.org/project/jsonatapy/). Older lines do not receive
backports; upgrade to the latest release to pick up a fix.

| Version | Supported |
| ------- | --------- |
| 2.2.x (latest patch) | ✅ |
| < 2.2 | ❌ |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.**

Report them privately instead: go to the repository's
[**Security** tab](https://github.com/txjmb/jsonata-core/security) and choose
**Report a vulnerability**. Only the maintainers can see the report.

Please include:

- The affected component (Rust crate, Python package, CLI, or C API) and version
- A minimal reproduction: the JSONata expression and input data that trigger
  the issue
- What happens (crash, hang, memory growth, wrong result) and what you expected
- Your assessment of the impact, if you have one

What to expect:

- We aim to acknowledge your report within 5 business days, and will keep you
  updated as we investigate.
- If the report is accepted, we fix it in a new patch release, then publish a
  GitHub Security Advisory and request a CVE where appropriate. We coordinate
  the disclosure date with you and credit you in the advisory unless you prefer
  to stay anonymous.
- If the report is declined, we explain why.

Please give us a reasonable chance to release a fix before disclosing the issue
publicly.
