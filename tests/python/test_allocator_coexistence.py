"""The bundled mimalloc must not crash next to other native code (GitHub #298).

Wheels are built with the `mimalloc` feature. mimalloc v3 (3.3.x, the
default of `libmimalloc-sys` 0.1.49) had two crashes that the extension
inherited:

- On macOS it keeps its per-thread heap in fixed thread-specific-data slots
  shared by every copy of mimalloc v3 in the process, with no ownership check.
  pyarrow 24.0.0 bundles one too, so importing jsonatapy and then pyarrow made
  the first `compile()` segfault or hang (apache/arrow#50428).
- On every platform, if the first thread to allocate through it exits, the next
  allocation on another thread segfaults in `mi_thread_init`
  (microsoft/mimalloc#1287).

The extension now builds mimalloc v2, which keeps its heap in an ordinary
thread-local variable and has neither problem. Each case runs in a fresh
interpreter, because the crash takes the process down and depends on which
code allocates first.
"""

import subprocess
import sys
import textwrap

import pytest


def _run(code: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, "-c", textwrap.dedent(code)],
        capture_output=True,
        text=True,
        timeout=120,
    )


def test_first_allocating_thread_exits():
    proc = _run("""
        import threading

        def first():
            import jsonatapy
            assert jsonatapy.compile("x").evaluate({"x": [1, 2]}) == [1, 2]

        def second():
            import jsonatapy
            for i in range(1000):
                assert jsonatapy.compile("x.a").evaluate({"x": [{"a": i}] * 50}) == [i] * 50

        for target in (first, second):
            t = threading.Thread(target=target)
            t.start()
            t.join()

        import jsonatapy
        print(jsonatapy.evaluate("$sum([1, 2])", {}))
        """)
    assert proc.returncode == 0, proc.stderr
    assert proc.stdout.strip() == "3"


def test_import_before_pyarrow():
    pytest.importorskip("pyarrow")
    proc = _run("""
        import jsonatapy
        import pyarrow

        print(jsonatapy.compile("$x").evaluate({}, {"x": 1}))
        print(pyarrow.array([1, 2, 3]).to_pylist())
        """)
    assert proc.returncode == 0, proc.stderr
    assert proc.stdout.split() == ["1", "[1,", "2,", "3]"]
