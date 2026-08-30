"""Regex flag translation parity across entry points.

The `m` (multiline) flag was silently dropped by the hand-rolled regex
builders in the `~>` chain-pipe and `$match` paths, while `$split` and
`$replace` translated it correctly via the shared helper — so the same
regex literal behaved differently depending on how it reached the engine.
All four entry points now share `functions::string::build_regex`.
(jsonata-js 2.1.0 accepts only `i` and `m` in regex literals.)
"""

import jsonatapy


def ev(src):
    return jsonatapy.compile(src).evaluate({})


def test_match_multiline():
    assert ev('$match("a\nb", /^b/m).match') == "b"


def test_chain_pipe_multiline():
    assert ev('("a\nb" ~> /^b/m).match') == "b"


def test_replace_multiline():
    assert ev('$replace("a\nb", /^b/m, "Z")') == "a\nZ"


def test_combined_flags():
    assert ev('$match("a\nb", /^B/im).match') == "b"


def test_case_insensitive_still_works_everywhere():
    assert ev('$match("AbC", /b/i).match') == "b"
    assert ev('("AbC" ~> /B/i).match') == "b"
    assert ev('$split("aXbXc", /x/i)') == ["a", "b", "c"]


def test_no_flags_unchanged():
    assert ev('$match("a\nb", /^b/)') is None
