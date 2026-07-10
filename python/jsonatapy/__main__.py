"""Console-script entry point for the `jsonatapy` CLI.

Dispatches to the MCP server subcommand (`jsonatapy mcp ...`) or evaluate
mode (everything else). See study/cli_spec.md for the full contract.
"""

from __future__ import annotations

import sys

from ._cli.run import run


def main() -> int:
    return run(sys.argv[1:])


if __name__ == "__main__":
    sys.exit(main())
