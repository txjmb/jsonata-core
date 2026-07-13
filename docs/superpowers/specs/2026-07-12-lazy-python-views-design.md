# Lazy Python Views (`LazyPyDict`) — Design

**Date:** 2026-07-12
**Status:** Approved
**Approach:** "Approach A — lazy Python views + pass-through output" (chosen over projection push-down and a hybrid; see Alternatives)

## Problem

The published benchmarks show jsonatapy losing to jsonata-js on array workloads
(`products.price` 2.3x slower, `Filter by category` 1.7x slower, etc.). Measurement
shows the engine is not the cause:

| Path (`products.price`, 100 objects) | Time/call |
|---|---|
| `evaluate(dict)` — what benchmarks measure | 34.9µs |
| `evaluate_with_data(JsonataData)` — pre-converted | 2.8µs |
| jsonata-js (native JS objects, no conversion) | 14.6µs |

~93% of `evaluate(dict)` time is the eager, recursive Python→`JValue` conversion in
`python_to_json_bound` (src/lib.rs), paid on every call, scaling linearly with data
size (93% share measured at 100, 1k, and 10k objects). The native Rust engine beats
jsonata-js by 7–20x on every losing benchmark row when measured without the Python
boundary (compile-once/parse-once/evaluate-many, same machine). Passing JSON strings
instead of dicts does not help: `evaluate_json` time is dominated by building the
`JValue` tree, not by reading input (simd-json makes only the cheap part cheaper).

The fix must therefore avoid *building the tree*, not accelerate building it.

## Goal

Eliminate most of the per-call conversion tax in `evaluate(dict)` by:

1. **Lazy input:** read Python dict fields on demand; never convert fields the
   expression doesn't touch.
2. **Pass-through output:** when a result value is an untouched input subtree,
   return the original Python object instead of rebuilding it.

## Non-goals

- No implicit cross-call caching of converted data. Every `evaluate()` call sees
  live data; mutation between calls is always visible. (`JsonataData` remains the
  explicit opt-in for amortizing conversion.)
- No engine/VM changes beyond lazy-value handling — the engine already beats JS.
- No lazy wrapper for Python **lists** in v1 (see Limitations).

## Design

### Core data structure

New file `src/lazy.rs`, entirely behind the `python` feature:

```rust
pub struct LazyPyDict {
    obj: Py<PyDict>,                                  // original Python dict
    field_cache: RefCell<HashMap<String, JValue>>,     // converted on first access
    materialized: OnceCell<Rc<IndexMap<String, JValue>>>, // memoized to_object()
}
```

- `get_field(&self, name) -> Result<JValue, LazyError>` — cache hit, else
  `Python::with_gil` → `PyDict.get_item(name)` → convert via `python_to_json_bound`
  (nested dicts become lazy wrappers themselves) → cache. Absent keys cache
  `JValue::Undefined`.
- `to_object(&self) -> Result<Rc<IndexMap<String, JValue>>, LazyError>` — full
  materialization for consumers needing a real object. Iterates the PyDict in
  insertion order, reusing cached conversions. **Skips cache entries that are
  `Undefined`** (absent-field markers must not become object keys). Memoized.
- `py_object(&self) -> &Py<PyDict>` — for output pass-through.

New variant in `src/value.rs`:

```rust
#[cfg(feature = "python")]
LazyPyDict(Rc<lazy::LazyPyDict>),
```

The pure-Rust crate (`jsonata-core` on crates.io, the CLI, `cargo test` without the
`python` feature) compiles the variant away and is unaffected.

### Input path

In `python_to_json_bound` (src/lib.rs):

- `PyDict` arm: wrap in `LazyPyDict` (O(1)) instead of recursively converting.
- `PyList` arm: unchanged shape (eager `Vec`), but dict elements now become lazy
  wrappers — `products` (100 dicts) costs 100 wrapper allocations instead of 100
  full IndexMap conversions.
- `JsonataData::new` keeps **eager deep conversion** (new internal entry point or a
  `lazy: bool` flag). Its contract is "pay conversion once, evaluate many times";
  lazy wrappers would re-read Python objects on every evaluation and hold the GIL
  during evaluation for no benefit.

### Evaluation touch points — choke-point strategy

evaluator.rs has ~101 `JValue::Object(...)` match sites. We do **not** edit them
all. Lazy values are handled where they are *consumed*; everywhere else they flow
opaquely (JValue clone is O(1) either way). Consumption points:

1. **Field-access hot paths** (perf-critical; add a `LazyPyDict` arm calling
   `get_field`):
   - VM: `get_field_cached` / `GetField` / `GetDataField` / `GetVarField`
     (src/vm.rs)
   - Tree-walker: `compiled_field_step`, the single-step and multi-step path
     loops, first-step handling, the 2-step `$var.field` fast path
     (src/evaluator.rs)
2. **Function dispatch normalization** (one edit covers all of functions.rs):
   materialize lazy args to real Objects in `call_pure_builtin` and
   `evaluate_function_call` before dispatch.
3. **Known stragglers** that evaluate their own arguments or poke into objects
   directly (list carried over from the earlier prototype of this design):
   - `$each` / `$sift` (evaluate their object arg outside `evaluated_args`)
   - Sort key extraction: `evaluate_sort_term` single-step path and the
     specialized comparator path
   - Tuple `@` field binding
   - `apply_transform_deep` (materialize at entry, then compare to targets)
   - Deep equality (`=`, `in`, `$distinct`): materialize both sides when either
     is lazy
   - Object construction / merge paths that iterate an existing object's entries
4. **Type predicates:** `is_object()`-style checks, truthiness/boolean coercion of
   objects, and `signature.rs::validate_and_coerce` treat `LazyPyDict` as Object.

A lazy value reaching an unhandled site behaves as "not an object" and produces a
wrong result — caught by the full-suite runs (Testing) rather than silently
tolerated.

### Output path — pass-through

`json_to_python` (src/lib.rs) gets a `LazyPyDict` arm returning a clone of the
original `Py<PyDict>` reference. Filter-shaped queries
(`products[category="Electronics"]`) return the caller's own product dicts; output
conversion becomes near-free. This matches jsonata-js semantics (its results alias
input objects).

### GIL

`evaluate(dict)` already runs under the GIL (PyO3 `#[pymethods]`); lazy accesses
use `Python::with_gil`, which is cheap when the GIL is already held. No evaluation
code gains a `Python` token parameter.

### Error handling

Today, a non-convertible Python value anywhere in the input raises `TypeError` at
call time. With lazy input, conversion errors surface at **first access**:

- `get_field` / `to_object` return a conversion error; a new `EvaluatorError`
  variant carries the message through the engine; the boundary converts it back to
  the same Python `TypeError`.
- Expressions that never touch the bad field now succeed instead of failing —
  strictly more permissive, documented.

## Documented behavior changes

1. **Result aliasing.** Results containing unmodified input subtrees now reference
   the caller's original dicts (no deep copy). Mutating the result mutates the
   input. Matches jsonata-js. Changelog + docs callout. Version: **point release
   only** (2.2.3 → 2.2.4) — this project's version numbering tracks jsonata-js,
   so minor/major bumps happen only when jsonata-js releases one; the aliasing
   change is called out prominently in the changelog instead.
2. **Value fidelity.** Passed-through dicts preserve original Python values exactly
   (`int` stays `int`); converted paths still normalize numbers to float. Net
   improvement; the inconsistency between paths is documented.
3. **Lazy errors.** Conversion `TypeError`s surface only if the offending field is
   touched (same exception type).

## Performance targets

Measured on the dev machine (WSL2), min-of-5-trials methodology, vs jsonata-js on
identical data:

| Case (100 objects) | Today `evaluate(dict)` | Target | jsonata-js |
|---|---|---|---|
| `products.price` | 34.9µs | ≤13.0µs | 14.6µs |
| `products[category="Electronics"]` | 129.3µs | ≤27.6µs | 93.7µs |
| `$sum(products[inStock].price)` | 116.4µs | ≤20.2µs | 67.6µs |
| Complex transformation (dense access) | 160.9µs (wins today) | ≤93.0µs | 415.1µs |

The ecommerce rows convert 9 fields per product (including a nested vendor dict
and a tags list) on every call today; the lazy path touches only the 1–4 fields
each expression references and never converts the rest.

The dense-access row is the guardrail: the earlier prototype regressed dense field
access ~2x. If that reproduces, the contingency is a compile-time field-usage
analysis choosing eager conversion for provably-dense/unanalyzable expressions —
added only if measurement demands it.

The original target column above was a pre-implementation estimate; it was
revised to measured post-implementation reality (Task 9b, selective field
caching in `LazyPyDict::get_field` — cache only heap-typed conversions,
2026-07-13) per user decision 2026-07-13, with the acceptance bar being that
the implementation beats jsonata-js on all four measured rows (met — every
row is comfortably under its jsonata-js column value).

## Testing

- Full reference suite (1682 tests as of 2026-07-12) via `evaluate(dict)` (exercises lazy path on the
  VM engine), **plus a run with `JSONATAPY_FORCE_TREE_WALKER=1`** covering the
  tree-walker engine. (An earlier draft assumed non-empty `bindings` forces the
  tree-walker suite-wide; in reality only 7 suite cases carry non-empty
  bindings, so an explicit env toggle was added in `run_eval`.)
- `cargo test` without the `python` feature — pure-Rust crate unaffected.
- New Python unit tests: absent-field caching; `$keys`/`$spread`/`$merge`/
  `$each`/`$sift`/`$lookup` on lazy values; `$sort` with lazy elements;
  pass-through identity (`result[0] is data["products"][2]`); mutation visible
  between calls; deep equality lazy-vs-eager; lazy conversion error surfacing;
  `Undefined` cache entries excluded from materialization.
- Benchmark suite before/after with min-of-5 methodology; targets above are the
  acceptance bar.

## Limitations

- **Nested pure-array data** (`data[1][1][1][1]` benchmark row) barely improves:
  lists remain eager, so array-of-array structures still convert fully on access.
  A `LazyPyList` would fix it but multiplies the touch surface (Array matching is
  pervasive in the engine); deferred until a real workload needs it.
- Dense-access expressions may regress; bounded by the revised absolute gate
  (≤93µs for the dense-transformation case; see `benchmarks/python/lazy_check.py`).
- Evaluation now reads Python objects mid-flight; code that assumes `JValue` trees
  are GIL-independent after conversion must not be given lazy values
  (`JsonataData` path guarantees this by staying eager).

## Alternatives considered

- **Projection push-down** (analyze expression, convert only referenced fields):
  simpler, never regresses, but cannot help filter-shaped queries — their results
  contain whole input objects, so the projection is "everything". Rejected as a
  standalone; survives as the contingency heuristic for dense access.
- **Implicit identity cache** (pointer-keyed converted-data cache): fastest repeat
  calls but silently stale when the caller mutates the dict between calls.
  Rejected on semantics (user decision).
- **Execution-plan / vectorized engine:** attacks the 7% of runtime that is already
  7–20x faster than jsonata-js natively. Rejected as not the bottleneck; revisit
  only if pre-converted-data workloads on large arrays become a priority.
