"""Deep non-tail recursion (GitHub issue #200).

jsonata-js has no fixed recursion limit, so ``$f(5000)`` of a non-tail
recursive ``$f`` evaluates there. Evaluation used to stop at 302 levels (about
100 calls) with U1001. It now goes about 10,000 calls deep and raises U1001
beyond that, and must never crash the process, even on a 1MB thread stack
(Windows' default for a main thread). Each case runs in a subprocess on such
a thread, so a native stack overflow fails the test instead of killing pytest.
"""

import json
import subprocess
import sys
import textwrap

import pytest

RECURSION = "($f := function($n){ $n <= 0 ? 0 : 1 + $f($n - 1) }; $f(%d))"
NO_BASE_CASE = "($inf := function($n){$n+$inf($n-1)}; $inf(5))"

_RUNNER = textwrap.dedent("""
    import json, sys, threading
    import jsonatapy

    jsonatapy._set_force_tree_walker(sys.argv[1] == "tree-walker")
    expr, kwargs = sys.argv[2], json.loads(sys.argv[3])
    out = {}

    def run():
        try:
            result = jsonatapy.evaluate(expr, None, **kwargs)
            if isinstance(result, dict) and "v" in result:
                # Report the nested value under "v" by its depth: json.dumps
                # would itself recurse too deep for this small stack.
                v, out["depth"] = result.pop("v"), 0
                while v:
                    v, out["depth"] = v["a"], out["depth"] + 1
            out["ok"] = json.dumps(result)
        except ValueError as e:
            out["error"] = str(e)

    threading.stack_size(1024 * 1024)
    t = threading.Thread(target=run)
    t.start()
    t.join()
    print(json.dumps(out))
    """)


@pytest.fixture(params=["default", "tree-walker"])
def engine(request):
    return request.param


def _evaluate_on_small_stack(engine, expr, **kwargs):
    proc = subprocess.run(
        [sys.executable, "-c", _RUNNER, engine, expr, json.dumps(kwargs)],
        capture_output=True,
        text=True,
        timeout=300,
    )
    assert proc.returncode == 0, f"process crashed ({proc.returncode}): {proc.stderr[-2000:]}"
    return json.loads(proc.stdout)


@pytest.mark.parametrize("n", [150, 1000, 5000, 9000])
def test_non_tail_recursion_evaluates(engine, n):
    assert _evaluate_on_small_stack(engine, RECURSION % n) == {"ok": str(n)}


def test_recursion_through_expression_calls(engine):
    expr = "($f := function($n){ $n <= 0 ? 0 : 1 + ($f)($n - 1) }; ($f)(%d))"
    assert _evaluate_on_small_stack(engine, expr % 9000) == {"ok": "9000"}
    out = _evaluate_on_small_stack(
        engine, "($inf := function($n){ $n + ($inf)($n - 1) }; ($inf)(5))"
    )
    assert out["error"].startswith("U1001")


TAIL_CALLED_CALLBACKS = [
    "($f := function($n){ $n = 0 ? 0 : $reduce([$n, 1], function($a, $b){ $f($a - 1) }) };"
    " $f(5000))",
    "($f := function($n){ $n = 0 ? 'done' : $map([$n], function($x){ $f($x - 1) }) };"
    " $count($f(5000)))",
    "($f := function($n){ $n = 0 ? 'done' : $filter([$n], function($x){ $f($x - 1) }) };"
    " $count($f(5000)))",
    "($f := function($n){ $n = 0 ? 'done' : $each({'k': $n}, function($x){ $f($x - 1) }) };"
    " $count($f(5000)))",
]


@pytest.mark.parametrize("expr", TAIL_CALLED_CALLBACKS)
def test_recursion_through_tail_called_builtin_callback(engine, expr):
    # The callback of a builtin called in tail position is run by the
    # trampoline without an ordinary evaluation step; recursing through it
    # used to overflow the native stack at about 1,200 levels (#191).
    expected = "0" if "$reduce" in expr else "1"
    assert _evaluate_on_small_stack(engine, expr) == {"ok": expected}


def test_unbounded_recursion_through_tail_called_builtin_callback(engine):
    expr = "($f := function($n){ $map([$n], function($x){ $f($x + 1) }) }; $f(0))"
    out = _evaluate_on_small_stack(engine, expr)
    assert out["error"].startswith("U1001")
    out = _evaluate_on_small_stack(engine, expr, timeout=1)
    assert out["error"][:5] in ("U1001", "D1012")


TAIL_CALLED_PARTIALS = [
    # The expression from #255.
    "($f := function($n){ $n <= 0 ? 1 : $f($n-1, ?)() }; $f(2000))",
    "($f := function($n){ $n <= 0 ? 1 : $f($n - 1, ?)() }; $f(9000))",
    "($g := function($n){ $f($n - 1, ?) }; $f := function($n){ $n <= 0 ? 1 : $g($n)() }; $f(5000))",
    "($f := function($n){ $n <= 0 ? 1 :"
    " $reduce(?, function($a, $b){ $f($b) }, 0)([$n - 1]) }; $f(5000))",
    "($f := function($n){ $n <= 0 ? 1 :"
    " ($reduce)([$n - 1], function($a, $b){ $f($b) }, 0) }; $f(5000))",
]


@pytest.mark.parametrize("expr", TAIL_CALLED_PARTIALS)
def test_recursion_through_tail_called_partial_application(engine, expr):
    # A partial application, or a builtin called as a value, in tail position
    # is applied by the trampoline without an ordinary evaluation step;
    # recursing through it used to overflow the native stack at about 2,000
    # levels and crash the process (#255).
    assert _evaluate_on_small_stack(engine, expr) == {"ok": "1"}


def test_unbounded_recursion_through_tail_called_partial_application(engine):
    out = _evaluate_on_small_stack(engine, "($f := function($n){ $f($n + 1, ?)() }; $f(0))")
    assert out["error"].startswith("U1001")


def test_recursion_without_base_case_raises_u1001(engine):
    out = _evaluate_on_small_stack(engine, NO_BASE_CASE)
    assert out["error"].startswith("U1001")


def test_max_stack_depth_raises_d1011(engine):
    out = _evaluate_on_small_stack(engine, RECURSION % 3000, max_stack_depth=5000)
    assert out["error"].startswith("D1011")
    out = _evaluate_on_small_stack(engine, RECURSION % 3000, max_stack_depth=10000)
    assert out == {"ok": "3000"}


def test_deep_value_built_by_recursion_converts_to_python(engine):
    # The result is 5000 dicts deep: converting it to Python objects (and the
    # `=`/`$string` walks inside the expression) must not overflow either.
    expr = (
        "($f := function($n){ $n <= 0 ? {} : {'a': $f($n - 1)} };"
        " $v := $f(5000); {'same': $v = $f(5000), 'len': $length($string($v)), 'v': $v})"
    )
    out = _evaluate_on_small_stack(engine, expr)
    assert out == {"depth": 5000, "ok": '{"same": true, "len": 30002}'}


def test_deeply_nested_expression_inside_recursion(engine):
    inner = "(" * 300 + "$x" + "+0)" * 300
    expr = (
        "($f := function($n){ $n <= 0 ? 0 : 1 + $f($map([$n], function($x){"
        + inner
        + "}) - 1) }; $f(3000))"
    )
    assert _evaluate_on_small_stack(engine, expr) == {"ok": "3000"}


def test_deeply_nested_lambda_body_inside_recursion(engine):
    # Creating a lambda clones its body, which here is 800 levels deep; that
    # clone used to run unguarded on the red zone left by a deep recursion,
    # and crashed once its frame grew.
    inner = "(" * 800 + "$x" + ")" * 800
    expr = (
        "($f := function($n){ $n <= 0 ? 0 : 1 + $f($n - $count($map([$n], function($x){"
        + inner
        + "}))) }; $f(2000))"
    )
    assert _evaluate_on_small_stack(engine, expr) == {"ok": "2000"}


def test_too_deeply_nested_expression_raises_u1002(engine):
    out = _evaluate_on_small_stack(engine, "[" * 5000 + "1" + "]" * 5000)
    assert out["error"].startswith("U1002")


_HOST_RUNNER = textwrap.dedent("""
    import json, sys, threading
    import jsonatapy

    n, stack = int(sys.argv[1]), int(sys.argv[2])
    deep = []
    for _ in range(100_000):
        deep = [deep]

    def h():
        try:
            json.dumps(deep)
            return "no error"
        except RecursionError:
            return "RecursionError"

    # The same probe outside any evaluation, at the top of the main thread.
    print(h(), flush=True)

    def run():
        expr = jsonatapy.compile(
            '($f := function($n){ $n <= 0 ? $h() : $f($n - 1) & "" }; $f(%d))' % n
        )
        expr.register("h", h)
        print(expr.evaluate(None), flush=True)

    if stack:
        threading.stack_size(stack)
        t = threading.Thread(target=run)
        t.start()
        t.join()
    else:
        run()
    """)


@pytest.mark.parametrize(
    "n,stack",
    [(0, 0), (3000, 0), (9000, 0), (50, 1 << 20), (3000, 1 << 20), (9000, 1 << 20)],
)
def test_host_function_recursing_deep_inside_a_recursion(n, stack):
    # A host function called deep inside a recursion runs where the
    # evaluation has grown the stack. CPython's own guard must still turn its
    # runaway C recursion into RecursionError there, rather than the process
    # overflowing the stack. (n=0 on a 1MB thread is left out: there the call
    # is on the caller's own stack, where CPython 3.13 itself overflows.)
    proc = subprocess.run(
        [sys.executable, "-c", _HOST_RUNNER, str(n), str(stack)],
        capture_output=True,
        text=True,
        timeout=300,
    )
    assert proc.returncode == 0, f"process crashed ({proc.returncode}): {proc.stderr[-2000:]}"
    baseline, result = proc.stdout.strip().splitlines()
    if baseline == "no error":
        # This interpreter's C stack guard has more room than the probe needs,
        # so the probe can't reach the guard even outside an evaluation (seen
        # on the actions/setup-python build of CPython 3.15.2 on ubuntu-latest,
        # though not on the python-build-standalone one). The process still must
        # not crash, which is checked above, and the probe must still finish
        # either way.
        assert result in ("no error", "RecursionError")
    else:
        assert result == "RecursionError"


_THREADS_RUNNER = textwrap.dedent("""
    import sys, threading
    import jsonatapy

    sys.setswitchinterval(1e-6)
    barrier = threading.Barrier(4)

    def run():
        expr = jsonatapy.compile(
            "($f := function($n){$n <= 0 ? $h() : 1 + $f($n - 1)}; $f(3000))"
        )
        expr.register("h", lambda: 1)
        barrier.wait()
        expr.evaluate(None)

    threads = [threading.Thread(target=run) for _ in range(4)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    print("done")
    """)


def test_deep_host_calls_from_several_threads_at_once():
    # The first deep host call in a process looks up CPython's stack API by
    # running Python code; threads arriving meanwhile must not deadlock.
    for _ in range(5):
        proc = subprocess.run(
            [sys.executable, "-c", _THREADS_RUNNER],
            capture_output=True,
            text=True,
            timeout=60,
        )
        assert proc.returncode == 0, proc.stderr[-2000:]
        assert proc.stdout.strip() == "done"


@pytest.mark.skipif(sys.platform == "win32", reason="needs resource.setrlimit")
def test_deep_host_call_with_unlimited_stack_limit():
    # With `ulimit -s unlimited` the stack left at entry is effectively
    # unbounded; the segment a deep host call runs on must still be mappable.
    import resource

    def unlimited_stack():
        resource.setrlimit(resource.RLIMIT_STACK, (resource.RLIM_INFINITY, resource.RLIM_INFINITY))

    code = textwrap.dedent("""
        import jsonatapy
        expr = jsonatapy.compile(
            "($f := function($n){$n <= 0 ? 0 : $g($n) + $f($n - 1)}; $f(3000))"
        )
        expr.register("g", lambda n: n)
        print(expr.evaluate(None))
        """)
    _, hard = resource.getrlimit(resource.RLIMIT_STACK)
    if hard != resource.RLIM_INFINITY:
        pytest.skip("the hard stack limit cannot be raised to unlimited")
    proc = subprocess.run(
        [sys.executable, "-c", code],
        capture_output=True,
        text=True,
        timeout=120,
        preexec_fn=unlimited_stack,
    )
    assert proc.returncode == 0, proc.stderr[-2000:]
    assert proc.stdout.strip() == "4501500"


_DROP_RUNNER = textwrap.dedent("""
    import gc, json, sys, threading
    import jsonatapy

    jsonatapy._set_force_tree_walker(sys.argv[1] == "tree-walker")
    expr = sys.argv[2]
    out = {}

    def run():
        result = jsonatapy.evaluate(expr, None)
        # Walk the value instead of json.dumps, which would recurse.
        depth, node = 0, result
        while isinstance(node, (list, dict)) and node:
            node = node[-1] if isinstance(node, list) else node["a"]
            depth += 1
        out["depth"] = depth
        # The Python value is freed on the main thread below: CPython 3.13
        # itself recurses freeing a 100,000-deep dict, and overflows a 1MB
        # thread on macOS arm64 (and a 768KB one on Linux) with no library
        # involved. The value's conversion and the library's own frees still
        # run on this thread.
        out["result"] = result
        del result, node
        # A deep value the evaluation frees before it returns.
        out["count"] = jsonatapy.evaluate("$count([" + expr + "])", None)

    threading.stack_size(1024 * 1024)
    t = threading.Thread(target=run)
    t.start()
    t.join()
    del out["result"]
    gc.collect()
    print(json.dumps(out))
    """)

DEEP_VALUES = [
    "$reduce([1..100000], function($acc, $i){ [[$acc]] }, [])",
    "$reduce([1..100000], function($acc, $i){ {'i': $i, 'a': $acc} }, {})",
    "$reduce([1..100000], function($acc, $i){ $i % 2 ? [{'a': $acc}] : {'a': [$acc]} }, {})",
]


@pytest.mark.parametrize("expr", DEEP_VALUES)
def test_deep_value_returned_to_python_and_dropped(engine, expr):
    # A value 100,000 levels deep used to overflow the native stack when it
    # was freed (#233), killing the process.
    proc = subprocess.run(
        [sys.executable, "-c", _DROP_RUNNER, engine, expr],
        capture_output=True,
        text=True,
        timeout=300,
    )
    assert proc.returncode == 0, f"process crashed ({proc.returncode}): {proc.stderr[-2000:]}"
    out = json.loads(proc.stdout)
    assert out["count"] == 1
    assert out["depth"] >= 100000


_LAZY_INPUT_RUNNER = textwrap.dedent("""
    import json, sys, threading
    import jsonatapy

    jsonatapy._set_force_tree_walker(sys.argv[1] == "tree-walker")
    in_list = sys.argv[2] == "list"
    depth = 100000
    data = {}
    for _ in range(depth):
        data = {"a": [data]} if in_list else {"a": data}
    out = {}

    def run():
        # Reading down the input converts each level to a lazy dict wrapper
        # holding the next: freeing that chain used to recurse (#233).
        out["count"] = jsonatapy.evaluate(
            "$count([$reduce([1..%d], function($x, $i){ $x.a }, $)])" % (depth - 1), data
        )

    threading.stack_size(1024 * 1024)
    t = threading.Thread(target=run)
    t.start()
    t.join()
    print(json.dumps(out))
    """)


@pytest.mark.parametrize("form", ["dict", "list"])
def test_deep_python_input_read_down_is_freed(engine, form):
    proc = subprocess.run(
        [sys.executable, "-c", _LAZY_INPUT_RUNNER, engine, form],
        capture_output=True,
        text=True,
        timeout=300,
    )
    assert proc.returncode == 0, f"process crashed ({proc.returncode}): {proc.stderr[-2000:]}"
    assert json.loads(proc.stdout) == {"count": 1}


TAIL_LOOP = "($loop := function($n){ $n = 0 ? 'done' : $loop($n - 1) }; $loop(%d))"


def test_tail_loop_has_no_iteration_cap(engine):
    # jsonata-js runs a tail loop as long as it keeps calling. It used to stop
    # here at 100,000 iterations with "U1001: ... maximum recursion depth
    # (500) exceeded", a depth nothing enforced (GitHub issue #251).
    assert _evaluate_on_small_stack(engine, TAIL_LOOP % 200000) == {"ok": '"done"'}
    mutual = (
        "($even := function($n){ $n = 0 ? true : $odd($n - 1) };"
        " $odd := function($n){ $n = 0 ? false : $even($n - 1) }; $odd(200001))"
    )
    assert _evaluate_on_small_stack(engine, mutual) == {"ok": "true"}


def test_tail_loop_keeps_guardrails(engine):
    # A depth limit still bounds real recursion but not a tail loop, and a
    # timeout still stops an infinite one.
    out = _evaluate_on_small_stack(engine, TAIL_LOOP % 150000, max_stack_depth=500)
    assert out == {"ok": '"done"'}
    out = _evaluate_on_small_stack(engine, RECURSION % 500, max_stack_depth=500)
    assert out["error"].startswith("D1011")
    out = _evaluate_on_small_stack(engine, "($inf := function(){$inf()}; $inf())", timeout=50)
    assert out["error"].startswith("D1012")


def test_u1001_names_the_real_limit(engine):
    out = _evaluate_on_small_stack(engine, NO_BASE_CASE)
    assert "30000 levels" in out["error"], out
    assert "(500)" not in out["error"]
