# By-reference builtin dispatch

Issue [#107](https://github.com/txjmb/jsonata-core/issues/107). Split out of
[#104](https://github.com/txjmb/jsonata-core/issues/104), whose §4 misdiagnosed this path;
see [#106](https://github.com/txjmb/jsonata-core/pull/106) for what §4's four cases actually
were.

## Context

A builtin can reach evaluation two ways. Written out — `$uppercase(name)` — it is a
`FunctionCall` node and goes through one of the two dispatch paths the engine already
validates: `call_pure_builtin` (compiled/VM) or `evaluate_function_call` (tree-walker).
Passed *by reference* — `$map(names, $uppercase)` — it arrives as a `JValue::Builtin` value
with its arguments already evaluated, and lands in a third function,
`call_builtin_with_values`.

That third function is a ~270-line hand-rolled reimplementation of the builtins. It has 27
arms and ends in:

```rust
// Add more functions as needed
_ => Err(EvaluatorError::ReferenceError(format!(
    "Built-in function {} cannot be called with values directly", name
))),
```

It performs no signature validation — the one path of three that doesn't. A comment at the
site says why it was left that way, citing two behaviours the shared validator does not
model. Both are real; neither is a reason for a third copy of the builtins.

### What is actually broken

A 262-case sweep (≈40 builtins crossed with `[null]`, `[1]`, `["a"]`, `[[1,2]]`,
`[{"k":1}]`, `[true]`, through `$map`/`$filter`/`$sift`/`$each`) against the pinned
jsonata-js found **52 divergences**, in two families:

**Missing arms.** Nine builtins are listed in `is_builtin_function`, so a reference to them
resolves, but have no arm and hit the fallback:

```
$map([1],       $type)      js="number"    ours=raises
$map([[1,2]],   $zip)       js=[[1],[2]]   ours=raises
$map([{"k":1}], $spread)    js=[{"k":1}]   ours=raises
$map([1],       $distinct)  js=1           ours=raises
$map([1],       $keys)      js=[]          ours=raises
$map([{"k":1}], $merge)     js={"k":1}     ours=raises
$map(["a"], $base64encode)  js="YQ=="      ours=raises
$map(["a"],  $encodeUrl)    js="a"         ours=raises
$map([1],       $reverse)   js=[1]         ours=raises
```

**Unvalidated types.** The arms that do exist accept values the signature rejects, mostly
`Null` where the type class is `[sm]`/`[nm]`:

```
$map([null], $uppercase)  js=T0410  ours=null
$map([null], $trim)       js=T0410  ours=null
$map([null], $sum)        js=T0410  ours=null
$map([[1,2]], $length)    js=T0410  ours=2
$map([null], $count)      js=1      ours=0
```

Four of the nine missing builtins — `$distinct`, `$merge`, `$keys`, `$reverse` — already
have correct implementations in `call_pure_builtin`. The by-reference copy has narrower,
buggier versions of arms that exist a few hundred lines up in the same file. That is the
shape of the problem: not "this path forgot to validate", but "there are three copies and
the third is a stub that drifted."

### What the reference does

From `jsonata.js` and `functions.js` in the pinned submodule:

- `applyInner(proc, args, input, environment)` calls
  `validateArguments(proc.signature, args, input)` **unconditionally**. A by-reference call
  is validated exactly like a written-out one, with the call-site `input` as the context
  value for `-` substitution.
- Higher-order functions truncate first. `hofFuncArgs(func, arg1, arg2, arg3)` supplies the
  value, then the index/key, then the container, stopping at `getFunctionArity(func)` —
  which for a builtin is `implementation.length`, the **JavaScript function's own parameter
  count**.

So the two behaviours the site comment flags are both accounted for by putting truncation
where jsonata-js puts it: before the call, not inside it.

Crucially, `implementation.length` is not the signature's parameter count and cannot be
derived from it:

| builtin | signature | signature params | `implementation.length` |
|---|---|---|---|
| `$uppercase` | `<s-:s>` | 1 | 1 |
| `$substring` | `<s-nn?:s>` | 3 | 3 |
| `$string` | `<x-b?:s>` | 2 | **1** |

This is why `$map([1,2], $substring)` is a `T0410` in jsonata-js (three arguments handed to
a three-parameter implementation, and `"naa"` matches no signature) while
`$map(arr, $uppercase)` is fine.

## Design

The guiding constraint is that this bug class was *caused* by having three implementations.
Any fix that leaves three implementations is treating the symptom. So the design is
subtractive: route by-reference calls into a path that already exists and is already
validated, and delete what that makes redundant.

### 1. Arity belongs in `get_callback_param_count`

`Evaluator::get_callback_param_count(&self, func_node: &AstNode) -> usize` already exists and
is already the single place that decides how many arguments a higher-order function hands
its callback. It is consulted by `$map`, `$filter`, `$sift` and `$each`. For a builtin
reference it currently falls through to `usize::MAX`.

Add a generated table and consult it in the `AstNode::Variable` arm, after both lambda
lookups miss and before the `usize::MAX` default:

```rust
if let Some(arity) = builtin_arity(var_name) {
    return arity;
}
```

`usize::MAX` remains the default for genuinely unknown names.

This is the whole of the arity change. Nothing new is invented: the seam is one that the
HOFs already use, and it is the same seam jsonata-js uses.

**Truncation must not move into the dispatch function.** `call_builtin_with_values` is also
reached by `$f := $substring; $f("abcdef", 1, 2)` — an explicit call with a written-out
argument list, where truncating would be wrong. Keeping truncation at the HOF call site
keeps explicit calls honest for free.

### 2. Dispatch delegates to `call_pure_builtin`

`call_pure_builtin(name, args: &[JValue], data: &JValue, options: &EvaluatorOptions)` already
takes values rather than AST, already runs `validate_builtin_args`, and already covers 29
builtins with the correct null/undefined semantics. It is shape-compatible with the
by-reference call site as it stands.

`call_builtin_with_values` gains a `data: &JValue` parameter — both call sites
(`evaluator.rs:7540` and `:10269`) have one in scope — and becomes:

1. host-fn override (unchanged; must stay first so a zero-arg override still works)
2. lazy normalisation (unchanged)
3. **if `is_compilable_builtin(name)` → `call_pure_builtin(name, values, data, &self.options)`**
4. otherwise, the remaining bespoke arms

The gate is `is_compilable_builtin`, which is exactly the predicate defining
`call_pure_builtin`'s coverage — its match ends in `unreachable!()`, so the gate is required,
not merely tidy. The two lists were checked and agree exactly (29 names; nothing in one and
not the other), so the gate cannot route a name into that `unreachable!()`.

Every arm that step 3 covers is then **deleted**, not fixed. That is most of the existing
27, including the four whose correct versions were already sitting in `call_pure_builtin`.
The net change to `call_builtin_with_values` should be strongly negative.

### 3. What is left gets arms

Builtins in neither shared path — `$type`, `$zip`, `$spread`, `$base64encode`, `$encodeUrl`
and any others the matrix surfaces — get arms in the remainder. Which ones, and what they
must return, is decided by the corpus rather than by reading the list: the sweep above is a
probe, not the specification.

Where such an arm would duplicate logic that already exists in `functions::`, it calls that
instead of restating it.

### 4. Corpus and the drift guard

`scripts/gen_fastpath_corpus.js` grows two things.

**A by-reference matrix.** Builtins crossed with operand shapes through `$map`, plus a
narrower cross through `$filter`, `$sift` and `$each` — the four HOFs that consult
`get_callback_param_count`. This follows the existing `BUILTINS_ONE_ARG` /
`BUILTINS_SECOND_ARG` matrices in the same file rather than inventing a new mechanism.

**An arity fixture.** `tests/fixtures/builtin_arity.json`, written from the reference's own
`implementation.length` for every name in its `staticFrame.bind(…)` table:

```json
{ "substring": 3, "uppercase": 1, "string": 1, … }
```

A Rust test asserts `BUILTIN_ARITY` matches that fixture. A submodule bump that changes an
arity then fails CI, instead of silently drifting — which is the exact failure mode that
left this path a stub.

## Testing

The corpus is the acceptance test, as it was for #104. Order matters and is not negotiable:

1. Add the matrix and the arity fixture. Regenerate. **Watch it fail** — the divergence
   baseline is empty as of #106, so new divergences appear as real failures, not xfails.
2. Fix until the suite is green.
3. Only then regenerate the baseline, and `git diff` it to confirm it is still empty.

Regenerating the baseline before the suite is green would write the new divergences in as
fresh `xfail` entries and turn everything green while fixing nothing.

Full gate before the work is called done:

- `tests/python/test_fastpath_differential.py` — every case, four routes, zero xfails
- `tests/python/test_reference_suite.py` — 1686 cases
- `tests/python` in full
- `cargo test --release --features cli`
- `cargo fmt --check`, `cargo clippy`

## Risks

**`call_pure_builtin`'s implicit-context insertion.** It substitutes the context when
`args.is_empty()`. `call_builtin_with_values` guards against empty values before dispatch, so
the branch is unreachable from this path — but the guard is now load-bearing and should be
noted where it sits.

**Arity truncation changing written-out calls.** `get_callback_param_count` is consulted only
by the four HOFs, so an explicit `$f(a, b, c)` is unaffected. This is worth an explicit test
rather than an argument.

**Residual divergences.** If any case cannot be matched — a reference crash rather than a
reference semantic, as `$substringAfter("xundefinedy", missing.x)` turned out to be in #104 —
it goes into the baseline **with a stated reason** and is reported, not buried. Growing the
baseline is a last resort and each entry must justify itself.

## Out of scope

Unifying all three dispatch paths behind a single builtin table. It is the structurally
right end state and would make drift impossible rather than merely detectable, but it
touches the VM's hottest code and is a much larger change. This design reduces three
implementations to two for 29 builtins, which is the part that pays for itself now.
