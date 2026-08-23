# Shared builtin dispatch

Covers the extraction of a single pure-builtin dispatcher, and
[#107](https://github.com/txjmb/jsonata-core/issues/107) — by-reference builtin calls —
which lands on top of it.

> **Revised 2026-08-23.** The first version of this document scoped the work as "delegate
> by-reference calls to `call_pure_builtin`" and put unification in *Out of scope*. That was
> wrong, and the measurements in [Three paths, measured](#three-paths-measured) are why: the
> duplication is larger than assumed, and doing #107 alone would mean writing ~21 new arms
> into a copy that unification then deletes. The design below is the revised plan.

## Context

A builtin can reach evaluation two ways. Written out — `$uppercase(name)` — it goes through
`call_pure_builtin` (compiled/VM) or `evaluate_function_call` (tree-walker). Passed *by
reference* — `$map(names, $uppercase)` — it arrives as a `JValue::Builtin` with its arguments
already evaluated and lands in a third function, `call_builtin_with_values`.

That third function is a ~270-line hand-rolled reimplementation with 27 arms, no signature
validation, and a fallback reading `// Add more functions as needed`. A 262-case sweep of
by-reference shapes against the pinned jsonata-js found **52 divergences** in two families:
nine builtins with no arm at all (`$type`, `$zip`, `$spread`, `$distinct`, `$keys`, `$merge`,
`$base64encode`, `$encodeUrl`, `$reverse`), and unvalidated argument types in the arms that
do exist (`$map([null], $uppercase)` is `T0410` in the reference; we return null).

Four of those nine already have correct implementations a few hundred lines up in the same
file. That is the shape of the problem: not "this path forgot to validate", but "there are
three copies and the third is a stub that drifted."

### Three paths, measured

Classifying every arm in `evaluate_function_call`'s 2688-line builtin match by what it
actually reaches for:

| | count | needs |
|---|---|---|
| pure arm-groups | **52** | values + `options` |
| evaluator-dependent | **11** | `apply_function`, `evaluate_internal`, `self.context`, `start_time` |

The eleven are `$map`, `$filter`, `$reduce`, `$single`, `$sift`, `$each`, `$sort`, `$eval`,
`$match`, `$replace` and `$not`.

`call_pure_builtin` has 29 arms. Twenty-eight of them overlap the pure 52; the odd one out is
`$not`, which the VM implements purely (`functions::boolean::boolean`, negated) while the
tree-walker routes it through `self.is_truthy`. So the partition is:

- **28** implemented twice, in `call_pure_builtin` and the tree-walker
- **24** existing only in the tree-walker — `$type`, `$zip`, `$spread`, `$lookup`, `$pad`,
  `$power`, `$shuffle`, `$exists`, `$error`, `$assert`, the four URL codecs, both base64
  codecs, the four `format*`/`parse*` numerics, and the four date/time functions. This is
  precisely the set that fails when passed by reference.
- **1** (`$not`) implemented differently in each

The three paths are therefore not three copies of one thing. They are one genuinely
evaluator-dependent set of 11, and one pure set of 52 that has been copied unevenly across
three sites.

### Why now

`call_pure_builtin` was instrumented and the differential corpus run against it: **29 of 29**
shared builtins are genuinely routed through it, not merely nominally covered. (`vm_preferred`
does not by itself imply the compiled path ran — compilation declines for many shapes — so
this needed measuring rather than assuming.)

With the divergence baseline empty as of [#106](https://github.com/txjmb/jsonata-core/pull/106),
that means the two implementations of all 29 are *proven* to agree across 3732 cases × 4
routes. Merging them is low-risk in a way it was not before #106. That window is the argument
for doing the extraction now rather than adding to the third copy first.

## Design

### Stage 1 — extract `builtins::dispatch_pure`

A new module `src/builtins.rs` owning one function:

```rust
pub(crate) fn dispatch_pure(
    name: &str,
    args: &[JValue],
    context: &JValue,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError>
```

paired with `pub(crate) fn is_pure_builtin(name: &str) -> bool`. It admits **53** names: the
52 above plus `not`, which the tree-walker classifies as evaluator-dependent only because it
reaches for `self.is_truthy`, while the compiled path already implements it purely. The predicate-plus-dispatcher
pairing mirrors the existing `is_compilable_builtin` / `call_pure_builtin` convention rather
than inventing a new one, and lets the match keep its `unreachable!()` fallback — a name can
only arrive here if the predicate admitted it.

Internal order, which is the VM path's order and the one #106 established as correct:

1. implicit context insertion
2. lazy normalisation
3. `validate_builtin_args`
4. the `propagates_undefined` guard — **after** validation, per #106
5. the match

Then:

- `call_pure_builtin` is deleted; the VM calls `dispatch_pure` (`is_compilable_builtin` stays
  as the *compilation* gate, which is a separate question from what dispatch can handle).
- `evaluate_function_call` keeps its 10 remaining evaluator arms and delegates everything
  else. `$not` moves to `dispatch_pure` using the VM's implementation; the corpus exercises
  `$not` against every operand shape in two matrices, so the swap is checked, not hoped.
- `call_builtin_with_values` delegates the 53 and keeps only what it must.

**Unify the context-insertion lists deliberately.** The two paths disagree today: the
tree-walker's zero-argument list includes `fromMillis` and its missing-first list includes
`replace`; the VM's include neither. This is benign only because neither builtin is
compilable, so the VM never sees them. Merging must take the union knowingly, not silently
adopt whichever list is copied first.

**The 24 tree-walker-only builtins move rather than get rewritten.** They are already correct;
extraction relocates them.

### Stage 2 — #107, by-reference dispatch

On top of Stage 1:

**Arity.** `get_callback_param_count` already exists and is already the single place deciding
how many arguments a higher-order function hands its callback (`$map`, `$filter`, `$sift`,
`$each` all consult it). Its `AstNode::Variable` arm currently falls through to `usize::MAX`
for a builtin reference. Add a table lookup before that default.

This is where jsonata-js puts it: `hofFuncArgs` truncates to `getFunctionArity(func)` and
`applyInner` then validates whatever it was handed. Keeping truncation at the HOF call site —
not inside dispatch — also keeps explicit calls honest for free, since
`call_builtin_with_values` is *also* reached by `$f := $substring; $f("abcdef", 1, 2)`, where
truncating would be wrong.

The table cannot be derived from `BUILTIN_SIGNATURES`; for a builtin the reference's arity is
its JavaScript function's parameter count:

| builtin | signature | signature params | `implementation.length` |
|---|---|---|---|
| `$uppercase` | `<s-:s>` | 1 | 1 |
| `$substring` | `<s-nn?:s>` | 3 | 3 |
| `$string` | `<x-b?:s>` | 2 | **1** |

This is why `$map([1,2], $substring)` is `T0410` in jsonata-js while `$map(arr, $uppercase)`
is fine.

**Dispatch.** `call_builtin_with_values` gains a `context: &JValue` parameter (both call sites,
`evaluator.rs:7540` and `:10269`, have one in scope; the reference validates with the call-site
`input` as context) and delegates to `dispatch_pure` for the 53.

**The evaluator-dependent remainder.** Extraction does **not** make this fall out. `$sort`,
`$sift` and `$each` are evaluator-dependent *and* reachable by reference — `$map([[3,1]],
$sort)` has no arm today and still will not after Stage 1. These need explicit handling, and
they are the reason #107 remains real work rather than a consequence of the refactor.

## Testing

The corpus is the acceptance test, as it was for #104.

**Stage 1** claims behaviour-neutrality on everything the corpus reaches — which, per the
measurement above, is all 29 shared builtins across four routes. Where the implementations
disagree in shapes the corpus does *not* reach, extraction silently picks a winner; that is a
known and accepted limit, not an oversight. The reference suite (1686) and the full Python
suite are the second and third nets.

**Stage 2** adds a by-reference matrix to `scripts/gen_fastpath_corpus.js` — builtins crossed
with operand shapes through `$map`, plus a narrower cross through `$filter`, `$sift` and
`$each` — following the existing `BUILTINS_ONE_ARG` / `BUILTINS_SECOND_ARG` matrices rather
than a new mechanism. It also emits `tests/fixtures/builtin_arity.json` from the reference's
own `implementation.length`, with a Rust test asserting the table matches, so a submodule bump
that changes an arity fails CI instead of drifting.

Order is not negotiable, and was the trap in #104:

1. add the matrix, regenerate, **watch it fail** — the baseline is empty, so new divergences
   appear as real failures rather than xfails
2. fix until green
3. only then regenerate the baseline, and `git diff` it to confirm it is still empty

Regenerating first would write the new divergences in as fresh `xfail` entries and turn
everything green while fixing nothing.

Full gate for each stage: the differential suite (four routes, zero xfails), the 1686-case
reference suite, `tests/python` in full, `cargo test --release --features cli`,
`cargo fmt --check`, `cargo clippy`. The benchmark CI's 10%-regression check covers Stage 1's
main performance risk.

## Risks

**Churn in the hottest file.** `src/evaluator.rs` is 13,731 lines and Stage 1 moves ~2000 of
them. The mitigation is that the move is mechanical — the 52 pure arms reach for nothing on
`self` except `options`, verified by census — and that the oracle is at maximum strength right
now.

**Performance.** Dispatch gains one function-call indirection. `hof_result_sequence`,
`check_sequence_length` and `normalize_lazy` are already free `pub(crate)` functions, so no
new borrows are introduced. Benchmark CI is the check.

**Silent winner-picking.** Stated above under Testing; called out in the PR rather than
buried.

**Residual divergences in Stage 2.** If a case cannot be matched because the reference crashes
rather than specifies — as `$substringAfter("xundefinedy", missing.x)` turned out to in #104 —
it goes into the baseline *with a stated reason* and is reported. Growing the baseline is a
last resort and each entry must justify itself.

## Out of scope

Unifying the 11 evaluator-dependent arms as well, by threading an evaluator callback into the
dispatcher. That would remove the pure/impure split entirely, but it couples the shared table
back to the evaluator and is a materially larger design. The pure/impure line is a real
boundary in the reference too, and is worth keeping as the seam.

Expanding `is_compilable_builtin` beyond its current 29. A shared dispatcher plausibly makes
this cheap, and more expressions compiling to bytecode would be a genuine win — but that is a
performance change with its own measurements, and it is not required by either stage here.
