"""Console-script entry point for the `jsonatapy` CLI.

Dispatches to the MCP server subcommand (`jsonatapy mcp ...`) or evaluate
mode (everything else). See study/cli_spec.md for the full contract.
"""

from __future__ import annotations

import argparse
import sys

from ._cli.run import run


def _run_mcp(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(prog="jsonatapy mcp", description="Run the JSONata MCP server")
    parser.add_argument("--http", action="store_true", help="Serve over HTTP instead of stdio")
    parser.add_argument(
        "--port", type=int, default=None, metavar="N", help="HTTP port (default 8000)"
    )
    args = parser.parse_args(argv)

    try:
        from ._cli.mcp_server import serve
    except ImportError:
        print(
            "error: the 'mcp' extra is not installed. Run:\n"
            '  uvx --from "jsonatapy[mcp]" jsonatapy mcp\n'
            "or: pip install jsonatapy[mcp]",
            file=sys.stderr,
        )
        return 2

    serve(args.http, args.port)
    return 0


def main() -> int:
    argv = sys.argv[1:]
    if argv[:1] == ["mcp"]:
        return _run_mcp(argv[1:])
    return run(argv)


if __name__ == "__main__":
    sys.exit(main())
