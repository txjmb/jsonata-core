"""Nested-array elements under a variable-rooted field path.

The tree-walker's two-step `$var.field` fast path hand-rolled its mapping
loop and silently skipped elements that were themselves arrays, where
jsonata-js's `lookup` recurses into them — so
`($v := [[{"p":1}],{"p":2}]; $v.p)` returned 2 instead of [1,2]. The fast
path now delegates to the shared field step. Reference outputs below were
taken from jsonata-js 2.1.0.
"""

import jsonatapy


def ev(src, data=None):
    return jsonatapy.compile(src).evaluate(data if data is not None else {})


def test_nested_array_elements_are_recursed():
    assert ev('($v := [[{"p":1}],{"p":2}]; $v.p)') == [1, 2]


def test_singleton_result_unwraps():
    assert ev('($v := [[{"p":1}]]; $v.p)') == 1


def test_null_fields_are_kept():
    assert ev('($v := [{"p":null},{"p":2}]; $v.p)') == [None, 2]


def test_inside_hof_lambda():
    assert ev('$map([1], function($i) { ($w := [[{"p":1}],{"p":2}]; $w.p) })') == [1, 2]


def test_absent_field_is_undefined():
    assert ev('($v := [{"q": 9}]; $v.p)') is None


def test_variable_bound_from_data():
    assert ev("($x := v; $x.p)", {"v": [[{"p": 1}], {"p": 2}]}) == [1, 2]
