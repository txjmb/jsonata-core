"""Lazy Python views (LazyPyDict) behavior tests.

evaluate() converts data lazily by default. `eager_eval` below uses a
pre-converted JsonataData handle (which is always eager) as the reference
point for parity checks against the lazy default.
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
    return jsonatapy.compile(expr).evaluate(data)


def eager_eval(expr, data):
    # Eager reference behavior via the pre-converted data handle.
    return jsonatapy.compile(expr).evaluate_with_data(jsonatapy.JsonataData(data))


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


# Activated in Task 5 (were deferred pending whole-object/whole-stream
# consumer fixes):
#
# - "products^(price).id" (sort by field) using DECORRELATED data (price
#   descending while id is ascending), so a no-op sort would be caught --
#   PRODUCTS itself has price already in ascending insertion order, which
#   would mask a broken sort (see git history for the original repro).
DECORRELATED = {"products": [{"id": i, "price": 100 - i} for i in range(5)]}


def test_lazy_sort_by_field_matches_eager(force_tree_walker):
    expr = "products^(price).id"
    assert lazy_eval(expr, DECORRELATED) == eager_eval(expr, DECORRELATED)


def test_lazy_tuple_stream_matches_eager(force_tree_walker):
    # "products#$i.name" (index binding / tuple stream): each lazy element
    # gets wrapped as {"@": <LazyPyDict>, "__tuple__": true, "$i": ...}; the
    # tuple-unwrap sites must handle a LazyPyDict `@` value, not just Object.
    expr = "products#$i.name"
    assert lazy_eval(expr, PRODUCTS) == eager_eval(expr, PRODUCTS)


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


# ── Task 5: whole-object consumers ──────────────────────────────────────

OBJ = {"a": 1, "b": {"c": 2}, "d": [1, 2]}


@pytest.fixture(params=["vm", "tree"])
def engine(request, monkeypatch):
    if request.param == "tree":
        monkeypatch.setenv("JSONATAPY_FORCE_TREE_WALKER", "1")
    return request.param


@pytest.mark.parametrize(
    "expr,data",
    [
        ("$keys($)", OBJ),
        ("$spread($)", OBJ),
        ("$lookup($, 'a')", OBJ),
        ("$merge([$, {'e': 5}])", OBJ),
        ("$each($, function($v, $k) { $k })", OBJ),
        ("$sift($, function($v) { $v = 1 })", OBJ),
        ("$string($)", OBJ),
        ("$type($)", OBJ),
        ("$boolean($)", OBJ),
        ("$boolean($)", {}),                      # empty dict → false
        ("$exists(b.c)", OBJ),
        ("'a' in $", OBJ),
        ("$ = {'a': 1, 'b': {'c': 2}, 'd': [1, 2]}", OBJ),   # deep equality lazy vs constructed
        ("$distinct([b, b, {'c': 2}])", OBJ),
        ("products^(price)", PRODUCTS),           # specialized sort comparator keys
        ("$sort(products, function($l, $r) { $l.price > $r.price })", PRODUCTS),
        ("$ ~> | b | {'c': 99} |", OBJ),          # transform operator
        ("products#$i.($i & ':' & name)", PRODUCTS),  # tuple stream (# index binding) over lazy elements
        # ── Task 6: whole-suite triage fixes ────────────────────────────
        # Wildcard/descendant steps over ARRAYS of lazy elements (e.g.
        # `Account.Order.Product.*`-shaped paths in the reference suite),
        # not just a single lazy object.
        ("products.*", PRODUCTS),                       # wildcard mapped over lazy array elements
        ("**.name", PRODUCTS),                           # descendant operator recursing through lazy elements
        ("$keys(products)", PRODUCTS),                   # keys() collected across lazy array elements
        ("$lookup(products, 'name')", PRODUCTS),         # lookup() mapped over lazy array elements
        ("$spread(products)", PRODUCTS),                 # spread() mapped over lazy array elements
    ],
)
# NOTE: the `@` tuple-binding operator is NOT implemented in this codebase
# (deferred work) — do not add `@` expressions to these tests.
#
# NOTE: PRODUCTS' `price` field is already ascending in insertion order, so
# the "products^(price)" case above does not by itself discriminate a broken
# sort (a no-op sort coincidentally matches a correct ascending sort here) --
# same masking risk documented for the sort fix generally. The
# merge_sort_specialized/evaluate_sort_term fix is independently verified by
# test_lazy_sort_by_field_matches_eager below, which uses decorrelated data
# (price descending, id ascending) and was confirmed to fail without the fix.
def test_lazy_consumers_match_eager(expr, data, engine):
    assert lazy_eval(expr, data) == eager_eval(expr, data)


# ── Task 7: Pass-through identity, value fidelity, and lazy-error semantics ──

class TestPassThrough:
    def test_filter_returns_original_dict_objects(self):
        data = {"products": [{"id": 1, "big": list(range(100))}, {"id": 2}]}
        expr = jsonatapy.compile("products[id = 1]")
        result = expr.evaluate(data)
        assert result is data["products"][0]          # identity, not a copy

    def test_pass_through_preserves_int_fidelity(self):
        data = {"items": [{"n": 1}]}
        result = jsonatapy.compile("items[n = 1]").evaluate(data)
        assert result is data["items"][0]
        assert isinstance(result["n"], int)

    def test_mutation_visible_between_calls(self):
        data = {"a": 1}
        expr = jsonatapy.compile("a")
        assert expr.evaluate(data) == 1
        data["a"] = 2
        assert expr.evaluate(data) == 2         # no implicit caching


class TestLazyErrors:
    BAD = {"good": 1, "bad": {1, 2, 3}}               # a set is not convertible

    def test_untouched_bad_field_succeeds(self):
        assert jsonatapy.compile("good").evaluate(self.BAD) == 1

    def test_touched_bad_field_raises_typeerror(self):
        with pytest.raises(TypeError):
            jsonatapy.compile("bad").evaluate(self.BAD)

    def test_materializing_bad_object_raises_typeerror(self):
        with pytest.raises(TypeError):
            jsonatapy.compile("$keys($)").evaluate(self.BAD)
