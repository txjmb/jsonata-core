"""Post or update the Mac Mini benchmark comparison comment on a PR.

Used by .github/workflows/benchmark-compare.yml. The comment is found by a
marker on its first line, so each PR keeps one comparison comment that shows
the latest run instead of a new comment on every push.

Usage: upsert_benchmark_comment.py BODY_FILE
Environment: GH_TOKEN, PR, GITHUB_API_URL, GITHUB_REPOSITORY.
"""

from __future__ import annotations

import json
import os
import sys
import urllib.request

MARKER = "<!-- mac-mini-benchmark-compare -->"


def call(method: str, url: str, payload: dict | None = None) -> object:
    request = urllib.request.Request(
        url,
        data=json.dumps(payload).encode() if payload is not None else None,
        headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
        },
        method=method,
    )
    with urllib.request.urlopen(request) as response:
        return json.loads(response.read() or b"null")


def main() -> None:
    with open(sys.argv[1]) as f:
        body = f.read()
    if MARKER not in body:
        body = f"{MARKER}\n{body}"
    api = os.environ["GITHUB_API_URL"]
    repo = os.environ["GITHUB_REPOSITORY"]
    pr = os.environ["PR"]

    existing = None
    page = 1
    while existing is None:
        comments = call(
            "GET", f"{api}/repos/{repo}/issues/{pr}/comments?per_page=100&page={page}"
        )
        if not comments:
            break
        for comment in comments:
            if comment["body"].startswith(MARKER) and comment["user"]["type"] == "Bot":
                existing = comment
                break
        page += 1

    if existing:
        call("PATCH", existing["url"], {"body": body})
    else:
        call("POST", f"{api}/repos/{repo}/issues/{pr}/comments", {"body": body})


if __name__ == "__main__":
    main()
