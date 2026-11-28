"""Ports of the checks jsonata-js writes in JavaScript rather than in its JSON suite.

The checks come from `implementation-tests.js`, `async-function.js` and
`parser-pluggable-regex.js` in jsonata-js 2.3.0. `test_implementation_parity.py` maps each one
to the test here that covers it, or says why it doesn't apply. jsonata-js registers host
functions with `registerFunction` or `assign`; the Python equivalent is `register` and
`register_override`. jsonata-js also checks the position and token of each error, which this
implementation does not report, so only the codes are compared. Every case runs on the
default path and on the forced tree-walker.
"""

import re
import time

import jsonatapy
import pytest


@pytest.fixture(params=[False, True], ids=["vm_preferred", "forced_tree_walker"])
def engine(request):
    jsonatapy._set_force_tree_walker(request.param)
    yield request.param
    jsonatapy._set_force_tree_walker(False)


TESTDATA1 = {
    "foo": {
        "bar": 42,
        "blah": [{"baz": {"fud": "hello"}}, {"baz": {"fud": "world"}}, {"bazz": "gotcha"}],
        "blah.baz": "here",
    },
    "bar": 98,
}


def _product(name, product_id, sku, colour, size, weight, price, quantity):
    width, height, depth = size
    return {
        "Product Name": name,
        "ProductID": product_id,
        "SKU": sku,
        "Description": {
            "Colour": colour,
            "Width": width,
            "Height": height,
            "Depth": depth,
            "Weight": weight,
        },
        "Price": price,
        "Quantity": quantity,
    }


TESTDATA2 = {
    "Account": {
        "Account Name": "Firefly",
        "Order": [
            {
                "OrderID": "order103",
                "Product": [
                    _product(
                        "Bowler Hat",
                        858383,
                        "0406654608",
                        "Purple",
                        (300, 200, 210),
                        0.75,
                        34.45,
                        2,
                    ),
                    _product(
                        "Trilby hat", 858236, "0406634348", "Orange", (300, 200, 210), 0.6, 21.67, 1
                    ),
                ],
            },
            {
                "OrderID": "order104",
                "Product": [
                    _product(
                        "Bowler Hat", 858383, "040657863", "Purple", (300, 200, 210), 0.75, 34.45, 4
                    ),
                    _product("Cloak", 345664, "0406654603", "Black", (30, 20, 210), 2.0, 107.99, 1),
                ],
            },
        ],
    }
}


def compile_with(expr, register=None, override=None):
    compiled = jsonatapy.compile(expr)
    for name, func in (register or {}).items():
        compiled.register(name, func)
    for name, func in (override or {}).items():
        compiled.register_override(name, func)
    return compiled


def evaluate(expr, data=None, bindings=None, **kwargs):
    return jsonatapy.compile(expr).evaluate(data, bindings, **kwargs)


def error_code(call):
    """The JSONata code of the error `call()` raises (a parse error's message has a prefix)."""
    with pytest.raises(ValueError) as info:
        call()
    match = re.search(r"\b[A-Z]\d{4}\b", str(info.value))
    return match and match.group(0)


# --- Functions with side-effects ---


def test_millis_returns_milliseconds_since_the_epoch(engine):
    # 27 Sep 2016, first commit to JSONata
    assert evaluate("$millis()", TESTDATA2) > 1474934400


@pytest.mark.parametrize("function", ["$millis()", "$now()"])
def test_timestamp_is_the_same_within_an_expression(engine, function):
    expr = f'{{"now": {function}, "delay": $sum([1..10000]), "later": {function}}}.(now = later)'
    assert evaluate(expr, TESTDATA2) is True


@pytest.mark.parametrize("function", ["$millis()", "$now()"])
def test_timestamp_differs_between_evaluations(engine, function):
    expr = jsonatapy.compile(function)
    first = expr.evaluate(TESTDATA2)
    time.sleep(0.002)
    assert expr.evaluate(TESTDATA2) != first


@pytest.mark.parametrize(
    "expr, pattern",
    [
        # follows this pattern - "2017-05-09T10:10:16.918Z"
        ("$now()", r"\d\d\d\d-\d\d-\d\dT\d\d:\d\d:\d\d.\d\d\dZ"),
        # follows this pattern - "10:23am GMT+00:00"
        ("$now('[h]:[M01][P] [z]')", r"\d?\d:\d\d[ap]m GMT\+00:00"),
        ("$now('[h]:[M01][P] [z]', '-0500')", r"\d?\d:\d\d[ap]m GMT-05:00"),
    ],
)
def test_now_formats(engine, expr, pattern):
    assert re.fullmatch(pattern, evaluate(expr, TESTDATA2))


def test_overlapping_evaluations_keep_their_own_timestamp(engine):
    # jsonata-js suspends the first evaluation on a Promise and runs a second one meanwhile.
    # This implementation has no async API, so the host function runs the second evaluation
    # itself, on the same thread, once the clock has moved past the first one's timestamp.
    expr = jsonatapy.compile(
        "($m := $millis(); $n := $now(); $pause($m); [$m = $millis(), $n = $now()])"
    )
    seen = []

    def pause(millis):
        seen.append(millis)
        if len(seen) == 1:
            while int(time.time() * 1000) <= millis:
                time.sleep(0.001)
            assert expr.evaluate(TESTDATA2) == [True, True]
        return True

    expr.register("pause", pause)
    assert expr.evaluate(TESTDATA2) == [True, True]
    assert len(seen) == 2 and seen[1] > seen[0]


def test_random(engine):
    result = evaluate("$random()")
    assert isinstance(result, float) and 0 <= result < 1
    assert evaluate("$random() = $random()") is False
    assert evaluate("($f := $random; $f() < 1)") is True
    # Its signature `<:n>` takes no argument, and a callback is always handed one.
    assert error_code(lambda: evaluate("$map([1], $random)")) == "T0410"
    assert compile_with("$random()", override={"random": lambda: 0.5}).evaluate(None) == 0.5


# --- Tests that rely on JavaScript-style object traversal ---


def test_object_traversal_order(engine):
    # Python dicts keep insertion order, as JavaScript objects do for string keys.
    assert evaluate("foo.*[0]", TESTDATA1) == 42
    assert evaluate("**[2]", TESTDATA2) == "Firefly"


# --- Tests that use the $clone() function ---


@pytest.mark.parametrize(
    "expr, expected",
    [
        ("$clone(foo)", None),
        ("$clone({})", {}),
        ('$clone({"a": 1})', {"a": 1}),
        # jsonata-js clones through JSON: `JSON.parse($string(arg))`.
        ('$clone([1, {"f": function(){1}}])', [1, {"f": ""}]),
        ('$map([{"a": 1}], $clone)', {"a": 1}),
    ],
)
def test_clone(engine, expr, expected):
    assert evaluate(expr, TESTDATA2) == expected


def test_clone_needs_an_object_or_array(engine):
    assert error_code(lambda: evaluate("$clone(1)")) == "T0410"


def test_transform_ignores_a_clone_bound_in_the_expression(engine):
    # jsonata-js #859: a transform copies with the built-in $clone, whatever the expression
    # binds to the name.
    result = evaluate('( $clone := 5; $ ~> |Account.Order.Product|{"blah":"foo"}| )', TESTDATA2)
    assert result["Account"]["Order"][0]["Product"][0]["blah"] == "foo"


def test_transform_uses_a_host_clone(engine):
    import copy

    calls = []
    expr = compile_with(
        'Account ~> |Order|{"Product":"blah"},nomatch|',
        override={"clone": lambda obj: calls.append(obj) or copy.deepcopy(obj)},
    )
    assert expr.evaluate(TESTDATA2) == {
        "Account Name": "Firefly",
        "Order": [
            {"OrderID": "order103", "Product": "blah"},
            {"OrderID": "order104", "Product": "blah"},
        ],
    }
    assert len(calls) == 1


def test_transform_with_a_host_clone_that_is_not_a_function(engine):
    # A transform uses the clone the host provides, so if that is not a function, it can't
    # make a copy to work on.
    expr = '( $ ~> |Account.Order.Product|{"blah":"foo"}| )'
    assert error_code(lambda: evaluate(expr, TESTDATA2, {"clone": 5})) == "T2013"


def test_transform_with_an_identity_host_clone_still_copies(engine):
    # Deviation: jsonata-js then changes the input itself, and `$$.a` is part of the copy.
    # Here the clone's result is copied again, so the input never changes, and `$$.a` is
    # not in that copy (D1014).
    data = {"a": {"b": 1}}
    bindings = {"clone": lambda obj: obj}
    assert evaluate('$ ~> | a | {"b": 2} |', data, bindings) == {"a": {"b": 2}}
    assert data == {"a": {"b": 1}}
    assert error_code(lambda: evaluate('$ ~> | $$.a | {"b": 2} |', data, bindings)) == "D1014"


# --- Tests that bind Javascript functions ---


def test_override_now(engine):
    expr = compile_with("$now()", override={"now": lambda: "time for tea"})
    assert expr.evaluate(TESTDATA2) == "time for tea"


def test_host_function_exception_message_is_kept(engine):
    # jsonata-js rethrows the host's own exception. Here it becomes a ValueError that keeps
    # the exception's type and message.
    class DOMException(Exception):  # noqa: N818 - the name jsonata-js's check uses
        pass

    def throw():
        raise DOMException("Here is my message")

    with pytest.raises(ValueError, match="DOMException: Here is my message"):
        compile_with("$throwDomEx()", {"throwDomEx": throw}).evaluate({})


@pytest.mark.parametrize(
    "expr, func, expected",
    [
        # jsonata-js registers this one with the signature <n:n>; `register` takes none.
        ("$map([1,4,9,16], $squareroot)", __import__("math").sqrt, [1, 2, 3, 4]),
        ("$map([1,4,9,16], $squareroot)", lambda num: num**0.5, [1, 2, 3, 4]),
        ("$filter([1,4,9,16], $even)", lambda num: num % 2 == 0, [4, 16]),
        (
            "$sift({'one': 1, 'four': 4, 'nine': 9, 'sixteen': 16}, $even)",
            lambda num: num % 2 == 0,
            {"four": 4, "sixteen": 16},
        ),
        (
            "$each({'one': 1, 'four': 4, 'nine': 9, 'sixteen': 16}, $squareroot)",
            lambda num: num**0.5,
            [1, 2, 3, 4],
        ),
    ],
)
def test_host_function_as_a_callback(engine, expr, func, expected):
    # A callback gets only as many of (value, index, array) as it declares, as jsonata-js
    # reads a JavaScript function's `length`: here one, so `math.sqrt` is not handed three.
    name = re.search(r"\$(squareroot|even)\)", expr).group(1)
    assert compile_with(expr, {name: func}).evaluate(TESTDATA2) == expected


@pytest.mark.parametrize(
    "func, expected",
    [
        (lambda a, b, c: [a, b, c], [[1, 0, [1, 2]], [2, 1, [1, 2]]]),
        (lambda a, b: [a, b], [[1, 0], [2, 1]]),
        # Only parameters before the first default count, as for a JavaScript function.
        (lambda a, b=None: [a, b], [[1, None], [2, None]]),
        (lambda *args: list(args), [[1], [2]]),
    ],
)
def test_host_callback_arity_follows_its_parameters(engine, func, expected):
    assert compile_with("$map([1, 2], $f)", {"f": func}).evaluate(None) == expected


def test_host_callback_arity_for_reduce(engine):
    # $reduce needs a callback of at least two parameters (D3050), as in jsonata-js.
    one = compile_with("$reduce([1, 2], $f)", {"f": lambda a: a})
    assert error_code(lambda: one.evaluate(None)) == "D3050"
    two = compile_with("$reduce([1, 2, 3], $f)", {"f": lambda a, b: a + b})
    assert two.evaluate(None) == 6


def test_partially_apply_host_function(engine):
    expr = compile_with(
        '($firstn := $substr(?, 0, ?); $first5 := $firstn(?, 5); $first5("Hello World"))',
        {"substr": lambda s, start, length: s[start : start + length]},
    )
    assert expr.evaluate(TESTDATA2) == "Hello"


def repeating_letters(char, repeat):
    """A custom matcher for `repeat` contiguous occurrences of `char`."""
    chars = char * repeat

    def match(string, offset=0):
        pos = string.find(chars, offset or 0)
        if pos == -1:
            return None
        return {
            "match": chars,
            "start": pos,
            "end": pos + len(chars),
            "groups": [],
            "next": lambda: match(string, pos + len(chars)),
        }

    return match


WELSH = "LLANFAIRPWLLGWYNGYLLGOGERYCHWYRNDROBWLLLLANTYSILIOGOGOGOCH"


@pytest.mark.parametrize(
    "expr, expected",
    [
        (
            f"$match('{WELSH}', $repeatingLetters('L', 2))",
            [
                {"match": "LL", "index": 0, "groups": []},
                {"match": "LL", "index": 10, "groups": []},
                {"match": "LL", "index": 18, "groups": []},
                {"match": "LL", "index": 37, "groups": []},
                {"match": "LL", "index": 39, "groups": []},
            ],
        ),
        (
            f"$split('{WELSH}', $repeatingLetters('L', 2))",
            ["", "ANFAIRPW", "GWYNGY", "GOGERYCHWYRNDROBW", "", "ANTYSILIOGOGOGOCH"],
        ),
        (
            f"$replace('{WELSH}', $repeatingLetters('L', 2), 'Ỻ')",
            "ỺANFAIRPWỺGWYNGYỺGOGERYCHWYRNDROBWỺỺANTYSILIOGOGOGOCH",
        ),
        (f"$contains('{WELSH}', $repeatingLetters('L', 4))", True),
        # A Python matcher can only return None for no match.
        (f"$contains('{WELSH}', $repeatingLetters('Q', 4))", False),
        # A JSONata matcher's null is a match, as jsonata-js tests for undefined only.
        ("$contains('abc', function($s){ null })", True),
        ("$contains('abc', function($s){ undefined })", False),
    ],
)
def test_custom_matcher(engine, expr, expected):
    expr = compile_with(expr, {"repeatingLetters": repeating_letters})
    assert expr.evaluate(None) == expected


def my_func(arr, fn):
    return 2 * fn(arr)


@pytest.mark.parametrize(
    "expr, extra, expected",
    [
        ("$myfunc([1,2,3], $sum)", {}, 12),
        ("$myfunc([1,2,3], λ($arr) { $arr[1] + $arr[2] })", {}, 10),
        ("$myfunc([1,2,3], $myfunc2)", {"myfunc2": lambda arr: 2 * arr[1]}, 8),
    ],
)
def test_host_function_calls_a_function_argument(engine, expr, extra, expected):
    expr = compile_with(expr, {"myfunc": my_func, **extra})
    assert expr.evaluate(None) == expected


def test_host_function_returns_a_function(engine):
    expr = compile_with(
        """(
          $startsWithHello := $startsWith("Hello");
          [$startsWithHello("Hello, Bob"), $startsWithHello("Goodbye, Bill")]
        )""",
        {"startsWith": lambda prefix: lambda arg: arg.startswith(prefix)},
    )
    assert expr.evaluate(None) == [True, False]


def counter(count):
    return {"value": count, "inc": lambda: counter(count + 1)}


@pytest.mark.parametrize(
    "expr, expected",
    [
        ("$counter(5).value", 5),
        ("$counter(0).inc().value", 1),
        ("$counter(0).inc().inc().inc().inc().value", 4),
        ("$counter(3).value + 5", 8),
        ("$counter(0).inc().inc().inc().inc().value + 3", 7),
        ("$counter($counter(3).inc().inc().value).inc().value", 6),
        ("$counter(3).inc().value.$string()", "4"),
        ("«count: {$counter(1).inc().value}, then {$counter(5).value}»", "count: 2, then 5"),
    ],
)
def test_functions_in_bindings_and_results(engine, expr, expected):
    # async-function.js binds `counter`, whose result holds a method, and awaits each
    # Promise it returns; here the same functions are synchronous.
    assert evaluate(expr, {}, {"counter": counter}) == expected


def test_calling_a_missing_method_of_a_host_result(engine):
    expr = "$counter(2).inc().foo().value"
    assert error_code(lambda: evaluate(expr, {}, {"counter": counter})) == "T1006"


# --- Tests that are specific to a Javascript runtime ---


@pytest.mark.parametrize(
    "expr, expected",
    [
        ('/ab/ ("ab")', {"match": "ab", "start": 0, "end": 2, "groups": []}),
        ("/ab/ ()", None),
        ('/ab+/ ("ababbabbcc")', {"match": "ab", "start": 0, "end": 2, "groups": []}),
        ('/a(b+)/ ("ababbabbcc")', {"match": "ab", "start": 0, "end": 2, "groups": ["b"]}),
        (
            '/a(b+)/ ("ababbabbcc").next()',
            {"match": "abb", "start": 2, "end": 5, "groups": ["bb"]},
        ),
        (
            '/a(b+)/ ("ababbabbcc").next().next()',
            {"match": "abb", "start": 5, "end": 8, "groups": ["bb"]},
        ),
        ('/a(b+)/ ("ababbabbcc").next().next().next()', None),
        ('/a(b+)/i ("Ababbabbcc")', {"match": "Ab", "start": 0, "end": 2, "groups": ["b"]}),
    ],
)
def test_regex_called_as_a_function(engine, expr, expected):
    # jsonata-js compares JSON.stringify output, which drops the `next` function. A function
    # comes out to Python as None, so the key is dropped here instead.
    result = evaluate(expr)
    if isinstance(result, dict):
        assert result.pop("next") is None
    assert result == expected


@pytest.mark.parametrize(
    "expr, code",
    [("//", "S0301"), ("/", "S0302"), ("/\\/", "S0302"), ("/\\\\\\/", "S0302")],
)
def test_empty_regex(engine, expr, code):
    assert error_code(lambda: evaluate(expr)) == code


@pytest.mark.parametrize(
    "expr, expected",
    [
        ('$match("test escape \\\\", /\\\\/)', {"match": "\\", "index": 12, "groups": []}),
        (
            '$match("ababbabbcc",/ab/)',
            [
                {"match": "ab", "index": 0, "groups": []},
                {"match": "ab", "index": 2, "groups": []},
                {"match": "ab", "index": 5, "groups": []},
            ],
        ),
        (
            '$match("ababbabbcc",/a(b+)/)',
            [
                {"match": "ab", "index": 0, "groups": ["b"]},
                {"match": "abb", "index": 2, "groups": ["bb"]},
                {"match": "abb", "index": 5, "groups": ["bb"]},
            ],
        ),
        ('$match("ababbabbcc",/a(b+)/, 1)', {"match": "ab", "index": 0, "groups": ["b"]}),
        ('$match("ababbabbcc",/a(b+)/, 0)', None),
        ("$match(nothing,/a(xb+)/)", None),
        ('$match("ababbabbcc",/a(xb+)/)', None),
    ],
)
def test_match(engine, expr, expected):
    assert evaluate(expr) == expected


@pytest.mark.parametrize(
    "expr, code",
    [
        ('$match("a, b, c, d", /ab/, -3)', "D3040"),
        ('$match("a, b, c, d", /ab/, null)', "T0410"),
        ('$match("a, b, c, d", /ab/, "2")', "T0410"),
        ('$match("a, b, c, d", "ab")', "T0410"),
        ('$match("a, b, c, d", true)', "T0410"),
        ("$match(12345, 3)", "T0410"),
        ("$match(12345)", "T0410"),
    ],
)
def test_match_errors(engine, expr, code):
    assert error_code(lambda: evaluate(expr)) == code


POC_859 = """(
  $r := $clone({"a":0,"constructor":0});
  $clone := function($i){$i};
  $r ~> | $ | {"__proto__":/x/} |;
  $each($r, function($v,$k){
    $k="a" ? ($r ~> | $ | {},"constructor" |; 0)
           : $v("return 'Your Node.js version is ' + process.version ")()
  })[1]
)"""


@pytest.mark.parametrize(
    "expr, data, expected",
    [
        # Input here has no prototype: these names are ordinary keys, absent unless present.
        ("foo.__proto__", {"foo": {"bar": "baz"}}, None),
        ('{} ~> | __proto__ | {"is_admin": true} |', None, {}),
        ('{} ~> | constructor | {"is_admin": true} |', None, {}),
        ("plain.constructor", {"plain": {"constructor": 1}}, 1),
        ('{} ~> | $ | {"__proto__": {"injected": true}} |', {}, {"__proto__": {"injected": True}}),
    ],
)
def test_prototype_names_are_ordinary_keys(engine, expr, data, expected):
    assert evaluate(expr, data) == expected


@pytest.mark.parametrize(
    "expr",
    [
        "foo.toString()",
        '{} ~> | __lookupGetter__("__proto__")() | {"is_admin": true} |',
        """(
            __lookupSetter__('__proto__')(constructor);
            __defineGetter__('l', constructor('return 42'));
            valueOf().l
        )""",
        # jsonata-js #859: either an error or a value that isn't the code's output string.
        POC_859,
    ],
)
def test_prototype_methods_are_not_functions(engine, expr):
    assert error_code(lambda: evaluate(expr, {"foo": {"bar": "baz"}})) == "T1006"


@pytest.mark.parametrize(
    "expr, data, bindings",
    [
        ('{} ~> | $string | {"implementation": null} |', None, None),
        ('($f := function() { 1 }; {} ~> | $f | {"x": 1} |)', None, None),
        ('{} ~> | $settings | {"limit": 0} |', {}, {"settings": {"limit": 10}}),
        ('$ ~> | $$.foo | {"x": 1} |', {"foo": {"a": 1}}, None),
        (
            '($clone := function($o) { $o }; $ ~> | [a, $settings] | {"changed": true} |)',
            {"a": {"b": 1}},
            {"settings": {"limit": 10}},
        ),
    ],
)
def test_transform_matches_only_the_copy(engine, expr, data, bindings):
    assert error_code(lambda: evaluate(expr, data, bindings)) == "D1014"
    if bindings:
        assert bindings == {"settings": {"limit": 10}}


def test_transform_cannot_match_a_host_function(engine):
    expr = compile_with('{} ~> | $fn | {"x": 1} |', {"fn": lambda: 1})
    assert error_code(lambda: expr.evaluate({})) == "D1014"


def test_transform_matches_values_in_the_copy_that_are_not_objects(engine):
    assert evaluate('{"a": "text"} ~> | a | {} |') == {"a": "text"}


# --- Test that yield platform specific results ---


def test_sqrt_rounding(engine):
    assert evaluate("$sqrt(10) * $sqrt(10)") == 10.000000000000002


# --- Sorting large arrays ---

LARGE_INPUT = {"items": [{"id": index, "v": (index * 7919) % 100000} for index in range(100000)]}


@pytest.mark.parametrize(
    "expr",
    ["$sort(items.v)", "$sort(items, function($l, $r) { $l.v > $r.v }).v", "items^(v).v"],
)
def test_sort_large_arrays(engine, expr):
    result = evaluate(expr, LARGE_INPUT)
    assert len(result) == 100000
    assert result == sorted(result)


# --- Tests that include infinite recursion ---


def ackermann(m, n):
    return f"""(
        $ack := function($m, $n) {{
            $m = 0 ? $n + 1 :
            $n = 0 ? $ack($m - 1, 1) :
            $ack($m - 1, $ack($m, $n - 1))
        }};
        $ack({m}, {n})
    )"""


@pytest.mark.parametrize(
    "expr, options, code",
    [
        (
            "($inf := function($n){$n+$inf($n-1)};  $inf(5))",
            {"timeout": 1000, "max_stack_depth": 300},
            "D1011",
        ),
        (
            "( $inf := function(){$inf()}; $inf())",
            {"timeout": 1000, "max_stack_depth": 500},
            "D1012",
        ),
        ("( $inf := function(){$inf()}; $inf())", {"timeout": 1000}, "D1012"),
        (ackermann(4, 4), {"max_stack_depth": 500}, "D1011"),
        ("[0..1001]", {"max_sequence_length": 1000}, "D2015"),
        # The left of `~>` is evaluated before `count()` is found to be missing its `$`.
        ("[0..100].([0..100]) ~> count()", {"max_sequence_length": 1000}, "D2015"),
        ("$append([0..600], [0..600]) ~> $count()", {"max_sequence_length": 1000}, "D2015"),
    ],
)
def test_guardrails(engine, expr, options, code):
    assert error_code(lambda: evaluate(expr, **options)) == code


def test_ackermann_completes_for_small_parameters(engine):
    assert evaluate(ackermann(3, 4), timeout=1000, max_stack_depth=500) == 125


def test_left_of_apply_is_evaluated_before_a_missing_sigil(engine):
    assert error_code(lambda: evaluate('$error("x") ~> count()')) == "D3137"
    assert error_code(lambda: evaluate('"a" ~> count()')) == "T1005"


# --- Tests that the descendant operator is consistent with the single wildcard ---


@pytest.mark.parametrize(
    "expr, expected",
    [
        ("($f := function(){1}; [[$f].**])", []),
        ("[[$sum].**]", []),
        ('($f := function(){1}; $count({"a": {"b": "x"}, "fn": $f}.**))', 3),
        ('($f := function(){1}; [{"a": {"b": "x"}, "fn": $f}.**[$type($) = "function"]])', []),
    ],
)
def test_descendants_skip_functions(engine, expr, expected):
    assert evaluate(expr) == expected


# --- Tests that interpreter internals are not reachable from an expression ---


@pytest.mark.parametrize(
    "expr",
    [
        '($f := function(){1}; {} ~> | $ | $f, "_jsonata_lambda" |)',
        '{} ~> | $ | $lookup, "_jsonata_function" |',
    ],
)
def test_transform_rejects_a_function_as_the_update(engine, expr):
    assert error_code(lambda: evaluate(expr, {})) == "T2011"


# --- async-function.js ---

DOWNLOADS = {
    "downloads": [
        {"downloads": 1, "day": "2016-09-01"},
        {"downloads": 2, "day": "2016-09-02"},
        {"downloads": 3, "day": "2016-09-03"},
        {"downloads": 1453, "day": "2017-03-10"},
        {"downloads": 1194, "day": "2017-03-11"},
        {"downloads": 988, "day": "2017-03-12"},
    ]
}


def test_host_function_result_is_queried(engine):
    # jsonata-js fetches npm download counts over HTTP; a canned response stands in for it.
    expr = compile_with(
        '$httpget("https://api.npmjs.org/downloads/range/2016-09-01:2017-03-31/jsonata")'
        ".downloads{ $substring(day, 0, 7): $sum(downloads) }",
        {"httpget": lambda url: DOWNLOADS},
    )
    assert expr.evaluate(None) == {"2016-09": 6, "2017-03": 3635}


@pytest.mark.parametrize(
    "expr",
    [
        '5 + $httpget("htttttps://api.npmjs.org/downloads/range/2016-09-01:2017-03-31/jsonata")',
        '$httpget("htttttps://api.npmjs.org/downloads/range/2016-09-01:2017-03-31/jsonata")'
        ".downloads{ $substring(day, 0, 7): $sum(downloads) }",
    ],
)
def test_host_function_error_is_raised(engine, expr):
    def httpget(url):
        raise OSError(f"Invalid protocol: {url.split(':')[0]}")

    with pytest.raises(ValueError, match="Invalid protocol: htttttps"):
        compile_with(expr, {"httpget": httpget}).evaluate(None)


@pytest.mark.parametrize("value", [None, "hello"])
def test_return_values(engine, value):
    assert evaluate("value", {"value": value}) == value


def test_missing_value_is_none(engine):
    assert evaluate("value", {}) is None


def test_one_expression_evaluated_twice(engine):
    expr = jsonatapy.compile("{'1':'goat','2': 'cheese'} ~> $lookup($string(payload))")
    assert [expr.evaluate({"payload": 1}, {}), expr.evaluate({"payload": 2}, {})] == [
        "goat",
        "cheese",
    ]


# --- parser-pluggable-regex.js ---


def test_match_with_the_built_in_regex(engine):
    assert evaluate('$match("ababbabbcc",/a(b+)/, 1)') == {
        "match": "ab",
        "index": 0,
        "groups": ["b"],
    }
