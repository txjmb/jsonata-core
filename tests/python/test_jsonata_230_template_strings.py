"""Template string checks from jsonata-js 2.3.0's implementation-tests.js and async-function.js.

Template strings, `«text {expr} text»` and `<<text {expr} text>>`, are new in jsonata-js 2.3.0
(#860). The JSON suite's template-strings group covers most of them; these are the checks
jsonata-js writes in JavaScript, plus the lexer rules the port changed. jsonata-js also checks
the position and token of each error, which this implementation does not report, so only the
codes are compared. Every case runs on the default path and on the forced tree-walker.
"""

import re

import jsonatapy
import pytest


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


def evaluate(expr, data=None, bindings=None):
    return jsonatapy.compile(expr).evaluate(data, bindings)


def error_code(expr, data=None):
    """The JSONata code of the error `expr` raises (a parse error's message has a prefix)."""
    with pytest.raises(ValueError) as info:
        evaluate(expr, data)
    match = re.search(r"\b[A-Z]\d{4}\b", str(info.value))
    return match and match.group(0)


@pytest.mark.parametrize(
    "expr, code",
    [
        # template string: syntax error inside braces
        ("«a {1 +} b»", "S0211"),
        # template string: unclosed function call inside braces
        ("«a {foo(} b»", "S0211"),
        # template string: syntax error inside nested template
        ("«a {«b {1 +}»} c»", "S0211"),
        # template string: syntax error in second interpolation
        ("«a {x} b {y:} c»", "S0202"),
        # template string: unterminated, starting mid-expression
        ("x & «abc", "S0107"),
        # template string: illegal escape
        ("«a\\qb»", "S0103"),
        # template string: unescaped } in the text
        ("«{a}}»", "S0108"),
    ],
)
def test_syntax_errors(engine, expr, code):
    assert error_code(expr) == code


def test_number_formatting_matches_concat(engine):
    result = evaluate("«{-0} {12345678901234567890}»")
    assert result == evaluate('"" & -0 & " " & 12345678901234567890')
    assert result == "0 12345678901234567000"


def test_1000_interpolations_match_the_concat_chain(engine):
    # jsonata-js builds `"" & "x" & a & ...` with 2000 operands to compare against. That
    # chain is deeper than this parser takes (MAX_PARSE_DEPTH, U1002), so it is compared in
    # chunks of 100 parts, which join to the same string.
    template = "«" + "x{a}" * 1000 + "»"
    chunk = '""' + ' & "x" & a' * 100
    expected = "".join(evaluate(chunk, {"a": 1}) for _ in range(10))
    result = evaluate(template, {"a": 1})
    assert result == expected
    assert len(result) == 2000


def test_20000_interpolations_do_not_nest(engine):
    template = "<<" + "x{a}" * 20000 + ">>"
    assert evaluate(template, {"a": 1}) == "x1" * 20000


@pytest.mark.parametrize(
    "expr",
    [
        # evaluation error reports the embedded expression, not an & operator
        "«{1/0}»",
        # evaluation error reports the embedded expression that failed
        "«{1} and {2} and {1/0}»",
        # each part is cast before the next is evaluated, so the D3001 comes first
        '«{1/0}{"a"+1}»',
    ],
)
def test_evaluation_errors(engine, expr):
    assert error_code(expr) == "D3001"


def test_concatenating_a_non_finite_number_is_d3001(engine):
    # Shared with `&` (jsonata-js castString); it used to be an uncoded type error.
    assert error_code('"" & 1/0') == "D3001"


def test_registered_function_inside_a_template(engine):
    # async-function.js "registered async function inside a template string", with a
    # synchronous host function: this implementation has no Promise API.
    expr = jsonatapy.compile("«Hello {$greet(name)}!»")
    expr.register("greet", lambda name: "dear " + name)
    assert expr.evaluate({"name": "Fred"}) == "Hello dear Fred!"


@pytest.mark.parametrize(
    "expr, expected",
    [
        # Text is not lexed as tokens: no regex, comment or name.
        ("«s3://{bucket}/x»", "s3://bk/x"),
        ("«/* not a comment */»", "/* not a comment */"),
        ("«`x`»", "`x`"),
        ("«\n{a}\t»", "\nA\t"),
        # Comments still work inside an embedded expression.
        ("«{/* c */ a}»", "A"),
        # A binding in one part is seen by the next: parts share the template's scope.
        ("«{$x := 2} {$x}»", "2 2"),
        ('«{[1,2]}{ {"a":1} }»', '[1,2]{"a":1}'),
        ("«{function(){1}}»", ""),
        ("«{«{«{a}»}»}»", "A"),
        ("<<{<<{a}>>}>>", "A"),
        ("«<<a>>»", "<<a>>"),
        ("<<«a»>>", "«a»"),
        ("«a\\>b»", "a>b"),
        ("«\\u00e9\\ud83d\\ude00»", "é\U0001f600"),
        # `<<` opens a template wherever an operand starts.
        ("a.<<b>>", "b"),
        ('[1,2,3][<<x>> = "x"]', [1, 2, 3]),
        ('{"a": 1} ~> |$|{"b": <<z>>}|', {"a": 1, "b": "z"}),
        ("a and «x»", True),
        ("«a» & «b»", "ab"),
        ("«x» = «x»", True),
        ("Account.Order[0].«{%.`Account Name`}»", "Firefly"),
    ],
)
def test_template_text_and_nesting(engine, expr, expected):
    data = {
        "a": "A",
        "bucket": "bk",
        "Account": {"Account Name": "Firefly", "Order": [{"OrderID": "o1"}]},
    }
    assert evaluate(expr, data) == expected


@pytest.mark.parametrize(
    "expr, code",
    [
        ("<<a {«b", "S0107"),
        ("«a {x;} b»", "S0202"),
        ("«a {+} b»", "S0211"),
        ("«a{b{c}»", "S0202"),
        ("«a»«b»", "S0201"),
        ("1 + «2»", "T2002"),
        # A trailing backslash is an illegal escape, in a string as in a template.
        ('"abc\\', "S0103"),
        ('"a\\u12"', "S0104"),
    ],
)
def test_template_errors(engine, expr, code):
    assert error_code(expr) == code


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        # A name runs up to whitespace or an operator character, as in jsonata-js, so
        # characters that aren't letters can be part of one.
        ("a€", {"a€": 1}, 1),
        ("foo.bar»", {"foo": {"bar»": 2}}, 2),
        ("$a€ := 3; $a€", None, 3),
    ],
)
def test_names_take_any_non_operator_character(engine, expr, data, expected):
    assert evaluate(f"({expr})" if ":=" in expr else expr, data) == expected


@pytest.mark.parametrize("expr", ["!a", "~a", "a!b"])
def test_bang_and_tilde_alone_are_unknown_operators(engine, expr):
    assert error_code(expr) == "S0204"


@pytest.mark.parametrize(
    "expr, expected",
    [
        # After a binary `*` or `%` an operand starts, so `<<` opens a template and `/` a
        # regex, as jsonata-js's parser lexes them; after the wildcard `*`, `/` divides.
        ("2 * <<a>>", "T2002"),
        ("2 * /a/", "T2002"),
        ("3 % <<a>>", "T2002"),
    ],
)
def test_operand_after_star_and_percent(engine, expr, expected):
    assert error_code(expr, {"a": 4}) == expected


def test_wildcard_then_division(engine):
    assert evaluate("* / 2", {"a": 4}) == 2
