"""Lazy Python views (LazyPyDict) behavior tests.

Written against the temporary JsonataExpression._evaluate_lazy hook while
the lazy path is being built out; Task 8 switches these to evaluate().
"""
import pytest
import jsonatapy


PRODUCTS = {
    "products": [
        {"id": i, "name": f"Product {i}", "price": 10.0 + i * 2.5, "inStock": i % 2 == 0}
        for i in range(10)
    ]
}


def lazy_eval(expr, data):
    return jsonatapy.compile(expr)._evaluate_lazy(data)


def eager_eval(expr, data):
    return jsonatapy.compile(expr).evaluate(data)


@pytest.mark.parametrize(
    "expr",
    [
        "products.price",                      # array field mapping
        "$sum(products.price)",                # fused aggregate
        "$count(products)",                    # array passthrough
    ],
)
def test_lazy_matches_eager_vm(expr):
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


# Deferred to Task 5 (whole-object consumers: object construction over a
# lazy element requires materializing the element as a whole, not just
# field-by-field access):
# - "products.{'n': name, 'p': price}"    # object construction per element


def test_lazy_missing_field_is_undefined():
    # Field-mapping shape (VM-compiled, same route as products.price):
    # a missing key on lazy elements yields undefined -> None.
    assert lazy_eval("products.nosuch", PRODUCTS) is None
    # Control: same shape with a present field returns data, proving the
    # None above means "missing field", not "path unsupported".
    assert lazy_eval("products.price", PRODUCTS) is not None


@pytest.fixture()
def force_tree_walker(monkeypatch):
    monkeypatch.setenv("JSONATAPY_FORCE_TREE_WALKER", "1")


# Deferred to Task 5 (whole-object/whole-stream consumers, not fixable by
# adding field-access arms at sites a-h):
#
# - "products^(price).name" (sort by field). `evaluate_sort_term`'s
#   single-Name-path shortcut does `if let JValue::Object(obj) = &actual_element
#   { obj.get(field) } else { Undefined }`, bypassing paths a-h entirely, so
#   every lazy element's sort key comes back Undefined -> the sort is a no-op.
#   Masked with PRODUCTS because price happens to already be in ascending
#   insertion order there. Proven broken with decorrelated data:
#     data = {"products": [{"id": i, "price": 100 - i} for i in range(5)]}
#     jsonatapy.compile("products^(price).id")._evaluate_lazy(data)  # [0, 1, 2, 3, 4]
#     jsonatapy.compile("products^(price).id").evaluate(data)        # [4, 3, 2, 1, 0]
#
# - "products#$i.name" (index binding / tuple stream). `create_tuple_stream`
#   wraps each lazy element as {"@": <LazyPyDict>, "__tuple__": true, "$i": ...}.
#   The multi-step consumer's existing (unmodified) Object-arm tuple extraction
#   does `if let Some(JValue::Object(inner)) = obj.get("@")`, which doesn't match
#   a LazyPyDict `@` value, so it falls into the "Invalid tuple" `continue` branch
#   and the whole record is silently dropped. Observed: lazy_eval returns None,
#   eager_eval returns the full ['Product 0', ..., 'Product 9'] list.


@pytest.mark.parametrize(
    "expr",
    [
        "products.price",
        "$sum(products.price)",
        "products[price > 20].id",
        "products[0].name",
        "products.name[0]",                    # stage on mapped field
    ],
)
def test_lazy_matches_eager_tree_walker(expr, force_tree_walker):
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


def test_lazy_two_step_var_field(force_tree_walker):
    expr = "($p := products; $p.price)"
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


def test_lazy_dynamic_key_lookup(force_tree_walker):
    # evaluate_path_step (Object, String) arm
    expr = 'products[0].("na" & "me")'
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


def test_lazy_missing_field_tree_walker(force_tree_walker):
    assert lazy_eval("products[0].nosuch", PRODUCTS) == eager_eval(
        "products[0].nosuch", PRODUCTS
    )
