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


# Deferred to Task 4 (tree-walker arms):
# These expressions fall back from the VM to the tree-walker and require
# evaluator.rs changes to support lazy values in tree-walker code paths.
# - "products[price > 20].id"             # filter + field (predicate evaluation)
# - "products[0].name"                    # index + field (numeric array indexing)
# - "products.{'n': name, 'p': price}"    # object construction per element


def test_lazy_missing_field_is_undefined():
    # Missing path → None (undefined), same as eager
    assert lazy_eval("products[0].nosuch", PRODUCTS) is None
