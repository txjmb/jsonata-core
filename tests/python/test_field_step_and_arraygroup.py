"""Field-step and array-group conformance, pinned against jsonata-js.

Two families of tree-walker drift fixed while unifying the field-extraction
loops onto `compiled_field_step` (all expected values below were produced by
the pinned jsonata-js submodule, and every case is asserted on BOTH engines):

1. The single-step-Name fast path skipped null-valued fields and returned
   Null for an empty result, so `p` over [{"p":null},{"p":2}] was 2 on the
   tree-walker but [null,2] on the VM and in the reference.
2. The `.[...]` array-group constructor kept undefined elements as null
   (`foo.blah.[baz]` gave [[..],[null],[null]] instead of [[..],[],[]]),
   masked downstream by the null-skip in (1); and its empty results confused
   "constructed empty array over a single value" (kept: `{"a":1}.[b]` is [])
   with "mapped over an empty array" (undefined: `emptyarr.[b]`).
"""

import jsonatapy
import jsonatapy._jsonatapy as _impl
import pytest


@pytest.fixture(params=[False, True], ids=["vm", "tree"])
def engine(request):
    _impl._set_force_tree_walker(request.param)
    yield
    _impl._set_force_tree_walker(False)


def ev(src, data):
    return jsonatapy.compile(src).evaluate(data)


def test_name_step_keeps_nulls(engine):
    assert ev("p", [{"p": None}, {"p": 2}]) == [None, 2]
    assert ev("p", [{"p": None}, {"p": None}]) == [None, None]
    assert ev("p", [{"p": None}]) is None  # reference: null (unwrapped singleton)


def test_name_step_empty_is_undefined(engine):
    assert ev("p", [{"q": 1}]) is None  # reference: undefined


def test_name_step_flattens_and_recurses(engine):
    assert ev("p", [{"p": [1, 2]}, {"p": 3}]) == [1, 2, 3]


def test_array_group_drops_undefined_elements(engine):
    data = {
        "foo": {
            "blah": [
                {"baz": {"fud": "hello"}},
                {"buz": {"fud": "world"}},
                {"bazz": "gotcha"},
            ]
        }
    }
    assert ev("foo.blah.[baz]", data) == [[{"fud": "hello"}], [], []]
    assert ev("foo.blah.[baz].fud", data) == "hello"
    assert ev("foo.blah.[baz, buz].fud", data) == ["hello", "world"]


def test_array_group_over_single_value_keeps_empty_array(engine):
    assert ev('{"a":1}.[b]', {}) == []
    assert ev("foo.[b]", {"foo": {"bar": 1}}) == []


def test_array_group_mapped_over_empty_array_is_undefined(engine):
    assert ev("emptyarr.[b]", {"emptyarr": []}) is None
    assert ev("nothing.[b]", {}) is None


def test_array_group_singleton_mapping_keeps_group(engine):
    # jsonata-js: the lone constructed group IS the result — and it is NOT
    # singleton-unwrapped further ([3] stays [3]).
    assert ev("$.[value]", [{"value": 3}]) == [3]
    assert ev("$.[value, eps]", [{"value": 3, "eps": 9}]) == [3, 9]
    assert ev("blah.[baz]", {"blah": [{"baz": 1}]}) == [1]
    assert ev("blah.[baz]", {"blah": [{"baz": 1}, {"baz": 2}]}) == [[1], [2]]


def test_tuple_stream_field_extraction_keeps_nulls(engine):
    # The fast path's tuple branch used to skip nulls and return Null on
    # empty; tuple streams now go through the general loop's single tuple
    # implementation. Reference outputs from jsonata-js.
    assert ev("$#$i.p", [{"p": None}, {"p": 2}]) == [None, 2]
    assert ev("$#$i.p", [{"q": 1}]) is None
    assert ev("arr#$i.p", {"arr": [{"p": 1}, {"p": [2, 3]}]}) == [1, 2, 3]
    assert ev("($#$i.p)[$i=0]", [{"p": None}, {"p": 2}]) is None
