# Shared Builtin Dispatch — Stage 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the 53 pure builtins into one shared value-in/value-out dispatcher so a builtin cannot be implemented three times, implemented differently, or reachable from one dispatch path and absent from another.

**Architecture:** A new `src/builtins.rs` owns `dispatch_pure(name, args, context, options)` plus the predicate `is_pure_builtin(name)`. It admits 53 names: the 52 the tree-walker implements without touching the evaluator, plus `not`, which the tree-walker routes through `self.is_truthy` but the compiled path already implements purely. `call_pure_builtin` (VM) is deleted and its 29 arms fold in; `evaluate_function_call` (tree-walker) keeps only its 10 evaluator-dependent arms and delegates the rest; `call_builtin_with_values` (by-reference) is left alone in this stage and picked up in Stage 2. The move is mechanical: the 52 pure arms reach for nothing on `self` except `options`.

**Tech Stack:** Rust (edition 2021), PyO3/maturin for the Python extension, pytest for the differential and reference suites, Node for corpus generation.

**Spec:** `docs/superpowers/specs/2026-08-23-shared-builtin-dispatch-design.md`

## Global Constraints

- **The divergence baseline must stay empty.** `tests/fixtures/fastpath_known_divergences.json` currently has `"divergences": {}`. No task in this plan may add an entry. If a task makes one necessary, stop and report rather than regenerating.
- **Never regenerate the baseline to make a suite green.** `scripts/gen_fastpath_baseline.py` writes whatever currently diverges into the file as `xfail(strict)`. Running it before a suite is green converts real failures into recorded expectations. No task in this plan runs it.
- **Behaviour must not change.** This stage is an extraction. Every suite that passes now must pass after, with identical results.
- **The Python extension must be rebuilt before any pytest run** that follows a Rust change: `maturin develop --release`. Skipping it tests the previously built binary.
- Formatting and lints must be clean: `cargo fmt --check` and `cargo clippy --release --all-targets`.
- Commit messages: imperative mood, `type(scope): subject` (e.g. `refactor(builtins): …`), and end with the two trailers used throughout this repo:
  ```
  Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
  ```

## Full verification command set

Several tasks say "run full verification". That means all of these, all passing:

```bash
cargo fmt --check
cargo clippy --release --all-targets
cargo build --release --features cli --bin jsonata   # cli_fixtures_test needs this binary
cargo test --release
maturin develop --release
uv run pytest tests/python/test_fastpath_differential.py -q   # expect: N passed, 0 xfailed, 0 failed
uv run pytest tests/python/test_reference_suite.py -q          # expect: 1686 passed
uv run pytest tests/python -q -p no:randomly                   # expect: 17436 passed, 4 skipped
```

## Reference: the 53 builtins that move

These move into `dispatch_pure`. Twenty-eight already exist in **both** `call_pure_builtin` and `evaluate_function_call`; twenty-four exist **only** in `evaluate_function_call`; `not` exists in both but differs.

**In both (28):** `string` `length` `uppercase` `lowercase` `number` `sum` `count` `substring` `substringBefore` `substringAfter` `trim` `contains` `split` `join` `max` `min` `average` `abs` `floor` `ceil` `round` `sqrt` `append` `reverse` `distinct` `keys` `merge` `boolean`

**Tree-walker only (24):** `pad` `power` `formatNumber` `formatBase` `formatInteger` `parseInteger` `shuffle` `zip` `exists` `lookup` `spread` `type` `base64encode` `base64decode` `encodeUrlComponent` `decodeUrlComponent` `encodeUrl` `decodeUrl` `error` `assert` `now` `millis` `toMillis` `fromMillis`

**Differs (1):** `not` — VM uses `functions::boolean::boolean` negated; tree-walker uses `self.is_truthy`.

**Staying in `evaluate_function_call` (10):** `map` `filter` `reduce` `single` `sift` `each` `sort` `eval` `match` `replace`

---

### Task 1: Create the module skeleton and its predicate

Establishes `src/builtins.rs` with the predicate and an empty dispatcher, wired into the crate but not yet called by anything. Nothing changes behaviourally; this task exists so later tasks have somewhere to move code to, and so the name list is reviewed once, on its own, rather than buried in a 2000-line move.

**Files:**
- Create: `src/builtins.rs`
- Modify: `src/lib.rs` (module declaration, alongside the existing `mod` lines at 128–141)

**Interfaces:**
- Consumes: `crate::value::JValue`, `crate::evaluator::{EvaluatorError, EvaluatorOptions}`
- Produces:
  - `pub(crate) fn is_pure_builtin(name: &str) -> bool`
  - `pub(crate) fn dispatch_pure(name: &str, args: &[JValue], context: &JValue, options: &EvaluatorOptions) -> Result<JValue, EvaluatorError>`

- [ ] **Step 1: Write the failing test**

Create `src/builtins.rs` containing only the test module for now:

```rust
//! One dispatcher for every builtin that needs nothing but its arguments.
//!
//! A builtin used to be implemented up to three times -- once in the compiled
//! path, once in the tree-walker, once again for by-reference calls -- and the
//! copies drifted. Twenty-four of them existed in exactly one of the three, so
//! `$map(arr, $type)` failed while `$type(x)` worked. This module is the single
//! implementation the three dispatch sites share.
//!
//! What does NOT live here: the ten builtins that need the evaluator itself
//! ($map, $filter, $reduce, $single, $sift, $each, $sort, $eval, $match,
//! $replace). They take AST arguments and call back into evaluation, so they
//! stay in `evaluate_function_call`. That line is a real boundary, not an
//! accident of this refactor -- jsonata-js draws it in the same place.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicate_admits_the_pure_set_and_rejects_the_evaluator_set() {
        for name in [
            "string", "length", "uppercase", "lowercase", "number", "sum", "count",
            "substring", "substringBefore", "substringAfter", "pad", "trim", "contains",
            "split", "join", "max", "min", "average", "abs", "floor", "ceil", "round",
            "sqrt", "power", "formatNumber", "formatBase", "formatInteger", "parseInteger",
            "append", "reverse", "shuffle", "zip", "distinct", "exists", "keys", "lookup",
            "spread", "merge", "boolean", "not", "type", "base64encode", "base64decode",
            "encodeUrlComponent", "decodeUrlComponent", "encodeUrl", "decodeUrl", "error",
            "assert", "now", "millis", "toMillis", "fromMillis",
        ] {
            assert!(is_pure_builtin(name), "{name} should be a pure builtin");
        }

        // These need the evaluator: they take AST arguments and call back into
        // evaluation. Admitting one here would route it into `unreachable!()`.
        for name in [
            "map", "filter", "reduce", "single", "sift", "each", "sort", "eval",
            "match", "replace",
        ] {
            assert!(!is_pure_builtin(name), "{name} needs the evaluator");
        }

        assert!(!is_pure_builtin("nosuchfunction"));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --release builtins::tests 2>&1 | tail -20`
Expected: compile error — `cannot find function 'is_pure_builtin' in this scope`, and `src/builtins.rs` is not yet a module.

- [ ] **Step 3: Declare the module**

In `src/lib.rs`, add alongside the existing module declarations (they are alphabetical-ish; put it after `pub mod ast_transform;`):

```rust
mod builtins;
```

- [ ] **Step 4: Write the minimal implementation**

Add above the `#[cfg(test)]` block in `src/builtins.rs`:

```rust
use crate::evaluator::{EvaluatorError, EvaluatorOptions};
use crate::value::JValue;

/// The builtins `dispatch_pure` can handle: everything that needs only its
/// arguments, the context value, and the evaluation options.
///
/// Paired with `dispatch_pure` the way `is_compilable_builtin` is paired with
/// the compiled path, so the dispatcher's match can keep an `unreachable!()`
/// fallback: a name only reaches it if this predicate admitted it. Adding a
/// name here without adding an arm there is therefore a panic, not a silent
/// wrong answer.
pub(crate) fn is_pure_builtin(name: &str) -> bool {
    matches!(
        name,
        "string" | "length" | "uppercase" | "lowercase" | "number" | "sum" | "count"
            | "substring" | "substringBefore" | "substringAfter" | "pad" | "trim"
            | "contains" | "split" | "join" | "max" | "min" | "average" | "abs"
            | "floor" | "ceil" | "round" | "sqrt" | "power" | "formatNumber"
            | "formatBase" | "formatInteger" | "parseInteger" | "append" | "reverse"
            | "shuffle" | "zip" | "distinct" | "exists" | "keys" | "lookup" | "spread"
            | "merge" | "boolean" | "not" | "type" | "base64encode" | "base64decode"
            | "encodeUrlComponent" | "decodeUrlComponent" | "encodeUrl" | "decodeUrl"
            | "error" | "assert" | "now" | "millis" | "toMillis" | "fromMillis"
    )
}

/// Dispatch a builtin that needs nothing but its arguments.
///
/// `context` is the JSONata context value (`$`) at the call site. It is used
/// for implicit-argument insertion and for the signature's `-` modifier, and
/// jsonata-js passes the same value (`input`) in all three dispatch positions.
pub(crate) fn dispatch_pure(
    name: &str,
    args: &[JValue],
    context: &JValue,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    let _ = (args, context, options);
    unreachable!("dispatch_pure called with non-pure builtin: {}", name)
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --release builtins::tests 2>&1 | grep -E "^test result|^error"`
Expected: `test result: ok. 1 passed`

- [ ] **Step 6: Confirm nothing else broke**

Run: `cargo test --release 2>&1 | grep -E "^test result"`
Expected: every line `ok`, `0 failed`. Dead-code warnings for the two new functions are expected at this point and disappear in Task 3.

- [ ] **Step 7: Commit**

```bash
git add src/builtins.rs src/lib.rs
git commit -m "$(cat <<'EOF'
refactor(builtins): add the module the three dispatch paths will share

Empty dispatcher and its predicate, wired into the crate but not yet
called. Splitting this out means the list of which builtins are pure gets
reviewed on its own rather than buried inside a two-thousand-line move.

The predicate exists so the dispatcher's match can keep an unreachable!()
fallback: a name only arrives if the predicate admitted it, so adding a
name here without an arm there panics loudly instead of answering wrongly.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
EOF
)"
```

---

### Task 2: Move the shared prologue into `dispatch_pure`

Moves the four preparation steps every pure builtin needs — implicit context insertion, lazy normalisation, signature validation, undefined propagation — out of `call_pure_builtin` and into `dispatch_pure`, taking the union of the two paths' context-insertion lists. No arms move yet, so `dispatch_pure` still ends in `unreachable!()` and nothing calls it.

The union matters: the tree-walker's zero-argument list includes `fromMillis` and its missing-first list includes `replace`; the VM's include neither. That is invisible today only because neither is compilable.

**Files:**
- Modify: `src/builtins.rs`
- Read for reference: `src/evaluator.rs` — `call_pure_builtin` prologue (the `args_storage` / `lazy_storage` / `validate_builtin_args` / `propagates_undefined` block), and `evaluate_function_call`'s `context_functions_zero_arg` / `context_functions_missing_first` arrays

**Interfaces:**
- Consumes: `is_pure_builtin`, `dispatch_pure` from Task 1
- Produces: `dispatch_pure` prologue behaviour; no new public names

- [ ] **Step 1: Write the failing test**

Add to `src/builtins.rs`'s `mod tests`:

```rust
    #[test]
    fn context_insertion_covers_both_paths_lists() {
        // The compiled path and the tree-walker kept separate lists of which
        // builtins take the context as an implicit argument, and they
        // disagreed: only the tree-walker had `fromMillis` (zero-arg) and
        // `replace` (missing-first). Merging must take the union, so assert on
        // the member that only one list had.
        let opts = EvaluatorOptions::default();
        // $fromMillis() with a numeric context formats that context.
        let got = dispatch_pure("fromMillis", &[], &JValue::Number(0.0), &opts)
            .expect("fromMillis should read the context");
        assert!(
            matches!(&got, JValue::String(s) if s.starts_with("1970-01-01")),
            "expected an epoch timestamp, got {got:?}"
        );
    }

    #[test]
    fn validation_runs_before_undefined_propagation() {
        // $substring(missing) binds its lone undefined argument to parameter 2
        // and takes parameter 1 from the context, so a non-string context is a
        // T0411 -- not undefined. Propagating first would swallow that, which
        // is the bug #106 fixed in the compiled path.
        let opts = EvaluatorOptions::default();
        let err = dispatch_pure(
            "substring",
            &[JValue::Undefined],
            &JValue::Number(5.0),
            &opts,
        )
        .expect_err("a numeric context cannot satisfy substring's first parameter");
        assert!(
            err.to_string().contains("T0411"),
            "expected T0411, got {err}"
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --release builtins::tests 2>&1 | tail -20`
Expected: both new tests fail by panicking in `unreachable!("dispatch_pure called with non-pure builtin: fromMillis")` / `... substring`.

- [ ] **Step 3: Write the prologue**

Replace `dispatch_pure`'s body in `src/builtins.rs`. Copy the four blocks from `call_pure_builtin` verbatim rather than rewriting them — they carry fixes from #104 and #106 that are easy to lose in a paraphrase. The one deliberate change is the context-insertion lists, which take the union:

```rust
pub(crate) fn dispatch_pure(
    name: &str,
    args: &[JValue],
    context: &JValue,
    options: &EvaluatorOptions,
) -> Result<JValue, EvaluatorError> {
    // 1. Implicit context insertion. The union of what the two paths used to
    //    do separately: only the tree-walker had `fromMillis` here and
    //    `replace` below, which was invisible because neither compiles.
    let args_storage: Vec<JValue>;
    let args: &[JValue] = if args.is_empty() {
        match name {
            "string" => {
                // $string() with a null/undefined context is undefined, not "null".
                if context.is_undefined() || context.is_null() {
                    return Ok(JValue::Undefined);
                }
                args_storage = vec![context.clone()];
                &args_storage
            }
            "number" | "boolean" | "uppercase" | "lowercase" | "fromMillis" => {
                args_storage = vec![context.clone()];
                &args_storage
            }
            _ => args,
        }
    } else if args.len() == 1 {
        match name {
            "substringBefore" | "substringAfter" | "contains" | "split" | "replace" => {
                if matches!(context, JValue::String(_)) {
                    args_storage = std::iter::once(context.clone())
                        .chain(args.iter().cloned())
                        .collect();
                    &args_storage
                } else {
                    args
                }
            }
            _ => args,
        }
    } else {
        args
    };

    // 2. Materialize top-level lazy args so every builtin sees plain Objects.
    #[cfg(feature = "python")]
    let lazy_storage: Vec<JValue>;
    #[cfg(feature = "python")]
    let args: &[JValue] = if args.iter().any(|a| matches!(a, JValue::LazyPyDict(_))) {
        lazy_storage = args
            .iter()
            .map(crate::evaluator::normalize_lazy)
            .collect::<Result<Vec<_>, _>>()?;
        &lazy_storage
    } else {
        args
    };

    // 3. Validate and coerce against the jsonata-js signature.
    let sig_storage: Vec<JValue>;
    let args: &[JValue] = match crate::evaluator::validate_builtin_args(name, args, context)? {
        Some(coerced) => {
            sig_storage = coerced;
            &sig_storage
        }
        None => args,
    };

    // 4. Undefined propagation -- deliberately AFTER validation, which is the
    //    order jsonata-js works in. See the note in `validate_builtin_args`.
    if args.first().is_some_and(JValue::is_undefined) && crate::evaluator::propagates_undefined(name)
    {
        return Ok(JValue::Undefined);
    }

    let _ = args;
    unreachable!("dispatch_pure called with non-pure builtin: {}", name)
}
```

`validate_builtin_args`, `propagates_undefined` and `normalize_lazy` are private to `evaluator.rs` today. Change each from `fn` to `pub(crate) fn` (`normalize_lazy` is already `pub(crate)`).

- [ ] **Step 4: Run the tests to verify they still fail, but differently**

Run: `cargo test --release builtins::tests 2>&1 | tail -20`
Expected: `validation_runs_before_undefined_propagation` now PASSES (validation raises T0411 before the `unreachable!()` is reached). `context_insertion_covers_both_paths_lists` still fails at `unreachable!()`, because `fromMillis` has no arm yet — that arm arrives in Task 4. Leave it failing; note this explicitly when handing the task off for review.

- [ ] **Step 5: Confirm the rest of the suite is untouched**

Run: `cargo test --release 2>&1 | grep -E "^test result"`
Expected: one `FAILED` line for the `builtins` unit tests (the known `fromMillis` case), every other line `ok`. Nothing calls `dispatch_pure` yet, so no behaviour has changed.

- [ ] **Step 6: Commit**

```bash
git add src/builtins.rs src/evaluator.rs
git commit -m "$(cat <<'EOF'
refactor(builtins): move the shared prologue into dispatch_pure

The four steps every pure builtin needs before its own logic runs:
implicit context insertion, lazy materialization, signature validation,
then undefined propagation. Copied rather than paraphrased -- the
ordering carries the #106 fix that validation runs before propagation,
which is what makes $substring(missing) a T0411 instead of undefined.

The context-insertion lists are the one deliberate change: they take the
union. The tree-walker had fromMillis and replace, the compiled path did
not, and that disagreement was invisible only because neither builtin is
compilable, so the compiled path never saw them.

One unit test is left failing on purpose: fromMillis has no arm yet.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
EOF
)"
```

---

### Task 3: Move the 29 compiled-path arms and delete `call_pure_builtin`

Moves `call_pure_builtin`'s match arms into `dispatch_pure` and repoints the compiled path. This is the first task that changes what runs in production, and it is the safest of the three moves: the corpus was measured to route all 29 of these through `call_pure_builtin` already, so any drift shows up immediately.

**Files:**
- Modify: `src/builtins.rs` (add arms)
- Modify: `src/evaluator.rs` (delete `call_pure_builtin`, repoint its callers)

**Interfaces:**
- Consumes: `dispatch_pure` prologue from Task 2
- Produces: `dispatch_pure` handling 29 names; `call_pure_builtin` no longer exists

- [ ] **Step 1: Find every caller**

Run: `grep -n "call_pure_builtin" src/*.rs`
Record the list. Each becomes a `crate::builtins::dispatch_pure(name, args, data, options)` call. Note the argument order: `call_pure_builtin(name, args, data, options)` maps to `dispatch_pure(name, args, context, options)` with `data` as `context`.

- [ ] **Step 2: Move the arms**

Cut every arm from `call_pure_builtin`'s `match name { … }` and paste it into `dispatch_pure`'s match, replacing the `let _ = args; unreachable!(…)` with:

```rust
    match name {
        // ... the 29 arms, verbatim ...

        _ => unreachable!("dispatch_pure called with non-pure builtin: {}", name),
    }
}
```

The arms reference `effective_args`; the prologue in Task 2 shadows the parameter as `args` instead. Rename the arms' `effective_args` to `args` rather than renaming the prologue's binding — the parameter name should match the signature.

Then delete `call_pure_builtin` entirely and repoint every caller found in Step 1.

- [ ] **Step 3: Build and check the unit tests**

Run: `cargo test --release builtins::tests 2>&1 | grep -E "^test result|^error"`
Expected: `context_insertion_covers_both_paths_lists` still fails (`fromMillis` arrives in Task 4). The other two pass. No compile errors.

- [ ] **Step 4: Run full verification**

Run every command in the "Full verification command set" above.
Expected: identical results to before this task — in particular `test_fastpath_differential.py` reports **0 failed, 0 xfailed** and `test_reference_suite.py` reports **1686 passed**. Any differential failure here is real drift between the two implementations, not a flake: read the failure, do not adjust the baseline.

- [ ] **Step 5: Commit**

```bash
git add src/builtins.rs src/evaluator.rs
git commit -m "$(cat <<'EOF'
refactor(builtins): fold the compiled path's arms into dispatch_pure

call_pure_builtin is gone; its twenty-nine arms now live in the shared
dispatcher and the compiled path calls that instead.

This is the safe half of the extraction. Instrumenting call_pure_builtin
and running the differential corpus showed all twenty-nine of these are
genuinely routed through it -- vm_preferred alone does not prove the
compiled path ran, since compilation declines for plenty of shapes -- so
with the divergence baseline empty these two implementations are already
known to agree over 3732 cases across four routes.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
EOF
)"
```

---

### Task 4: Move the 24 tree-walker-only arms

Moves the builtins that exist in exactly one place. These have no compiled-path twin, so nothing is being reconciled — they relocate unchanged. This is the task that makes `$type`, `$zip`, `$spread` and the rest reachable from any dispatch path, which is what Stage 2 needs.

**Files:**
- Modify: `src/builtins.rs` (add 24 arms)
- Modify: `src/evaluator.rs` (delete those 24 arms from `evaluate_function_call`)

**Interfaces:**
- Consumes: `dispatch_pure` from Task 3
- Produces: `dispatch_pure` handling 53 names (29 + 24)

- [ ] **Step 1: Move the arms**

Cut these 24 arms from `evaluate_function_call`'s match and paste them into `dispatch_pure`'s match:

`pad` `power` `formatNumber` `formatBase` `formatInteger` `parseInteger` `shuffle` `zip` `exists` `lookup` `spread` `type` `base64encode` `base64decode` `encodeUrlComponent` `decodeUrlComponent` `encodeUrl` `decodeUrl` `error` `assert` `now` `millis` `toMillis` `fromMillis`

Two mechanical substitutions inside the moved code:
- `evaluated_args` becomes `args`
- `&self.options` becomes `options`; `self.options` becomes `*options`

Do **not** yet delegate from `evaluate_function_call` — that is Step 3. After this step the tree-walker has no arms for these names and would fall through, so the build is expected to be red between Step 1 and Step 3. Do not run the suites in between.

- [ ] **Step 2: Verify the failing unit test now passes**

Run: `cargo test --release builtins::tests 2>&1 | grep -E "^test result|^error"`
Expected: all three tests pass. `context_insertion_covers_both_paths_lists` was failing since Task 2 for want of a `fromMillis` arm; it now has one.

- [ ] **Step 3: Delegate from the tree-walker**

In `evaluate_function_call`, immediately after the existing `validate_builtin_args` call and before the `match name`, insert the delegation. Note that `dispatch_pure` runs its own prologue, so the tree-walker's own prologue steps must be skipped for delegated names — pass the **raw** evaluated arguments, not the already-validated ones.

Restructure so the delegation happens before the tree-walker's prologue:

```rust
        // Everything that needs nothing but its arguments goes to the shared
        // dispatcher, which owns the prologue (context insertion, lazy
        // materialization, validation, undefined propagation) as well as the
        // per-function logic. Only the builtins that call back into evaluation
        // are still handled below.
        if crate::builtins::is_pure_builtin(name) {
            return crate::builtins::dispatch_pure(name, &evaluated_args, data, &self.options);
        }
```

placed directly after the arguments are evaluated into `evaluated_args` and before the tree-walker's context-insertion block. The tree-walker's own context insertion, lazy normalisation and `validate_builtin_args` call then apply only to the 10 evaluator-dependent builtins.

- [ ] **Step 4: Run full verification**

Run every command in the "Full verification command set" above.
Expected: identical results to before this task. Pay attention to `test_reference_suite.py` — these 24 builtins include the date/time and number-formatting functions, which the differential corpus covers only thinly but the reference suite covers heavily.

- [ ] **Step 5: Commit**

```bash
git add src/builtins.rs src/evaluator.rs
git commit -m "$(cat <<'EOF'
refactor(builtins): move the twenty-four tree-walker-only builtins

$type, $zip, $spread, $lookup, $pad, $power, $shuffle, $exists, $error,
$assert, the URL and base64 codecs, the format/parse numerics and the
date/time functions existed in exactly one of the three dispatch paths.
That is why $type(x) works and $map(arr, $type) raises.

Nothing is reconciled here because there was no second implementation to
reconcile against -- these relocate unchanged. The reference suite is the
oracle that matters for this one: the date/time and number-formatting
functions are thin in the differential corpus and heavy in the suite.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
EOF
)"
```

---

### Task 5: Reconcile `$not` and delete the tree-walker's duplicates

Removes the 28 now-dead duplicate arms from `evaluate_function_call` and settles `$not`, the one builtin the two paths implement differently. This is the task that actually collapses the duplication.

`$not` differs: the compiled path is `functions::boolean::boolean(arg)` negated, the tree-walker is `!self.is_truthy(arg)`. Task 4's delegation already routes `$not` to the compiled version, so this task confirms that was right rather than assuming it.

**Files:**
- Modify: `src/evaluator.rs` (delete the 28 duplicate arms plus the `not` arm)

**Interfaces:**
- Consumes: `is_pure_builtin`, `dispatch_pure`
- Produces: `evaluate_function_call` handling only the 10 evaluator-dependent builtins

- [ ] **Step 1: Confirm the arms are unreachable**

Run: `cargo build --release 2>&1 | grep -c "unreachable_patterns\|never read"`

The delegation added in Task 4 returns before the `match name`, so the 28 duplicates plus `not` are dead code. The compiler will not always flag string-match arms as unreachable, so verify by inspection instead: for each of the 29 names, confirm `is_pure_builtin` admits it and therefore the delegation catches it first.

- [ ] **Step 2: Delete the duplicate arms**

Delete these 29 arms from `evaluate_function_call`:

`string` `length` `uppercase` `lowercase` `number` `sum` `count` `substring` `substringBefore` `substringAfter` `trim` `contains` `split` `join` `max` `min` `average` `abs` `floor` `ceil` `round` `sqrt` `append` `reverse` `distinct` `keys` `merge` `boolean` `not`

Also delete the tree-walker's now-unused context-insertion entries for names that no longer reach its prologue: `string`, `number`, `boolean`, `uppercase`, `lowercase`, `fromMillis` from `context_functions_zero_arg`, and `substringBefore`, `substringAfter`, `contains`, `split` from `context_functions_missing_first`. `replace` stays in the latter — it is one of the ten that remain.

The `#[allow(dead_code)]`-free build is the check: if `is_truthy` or another helper becomes unused, remove it rather than annotating it.

- [ ] **Step 3: Write a test pinning `$not`'s reconciled behaviour**

Add to `src/builtins.rs`'s `mod tests`:

```rust
    #[test]
    fn not_uses_the_boolean_coercion_not_raw_truthiness() {
        // The two paths disagreed: the compiled one ran jsonata's $boolean
        // coercion and negated it, the tree-walker negated its own is_truthy.
        // They differ on containers -- $boolean([]) is false and $boolean([0])
        // is false, so $not of each is true. Pin the surviving behaviour.
        let opts = EvaluatorOptions::default();
        let cases = [
            (JValue::array(vec![]), true),
            (JValue::array(vec![JValue::Number(0.0)]), true),
            (JValue::array(vec![JValue::Number(1.0)]), false),
            (JValue::Bool(false), true),
            (JValue::Bool(true), false),
        ];
        for (input, want) in cases {
            let got = dispatch_pure("not", &[input.clone()], &JValue::Undefined, &opts)
                .unwrap_or_else(|e| panic!("$not({input:?}) raised {e}"));
            assert_eq!(got, JValue::Bool(want), "$not({input:?})");
        }
    }
```

- [ ] **Step 4: Run the test**

Run: `cargo test --release builtins::tests 2>&1 | grep -E "^test result|^error"`
Expected: all four tests pass. If `$not` of a container disagrees, stop — the two implementations differ in a way the corpus does not cover, and that needs deciding against jsonata-js rather than picking whichever compiles.

- [ ] **Step 5: Run full verification**

Run every command in the "Full verification command set" above.
Expected: identical results to before this task.

- [ ] **Step 6: Commit**

```bash
git add src/builtins.rs src/evaluator.rs
git commit -m "$(cat <<'EOF'
refactor(builtins): delete the tree-walker's twenty-nine duplicates

The point of the exercise. These arms became dead the moment the tree-walker
started delegating, and removing them takes the count of implementations per
pure builtin from two to one.

$not is the one that needed deciding rather than deleting: the compiled path
ran jsonata's $boolean coercion and negated it, the tree-walker negated its
own is_truthy, and the two differ on containers. The compiled version is the
one that survives, pinned by a test, because $boolean is what the reference
applies.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
EOF
)"
```

---

### Task 6: Confirm the extraction changed nothing, and record it

Re-runs the instrumentation measurement from the design phase against the new structure, so the claim "behaviour-neutral on everything the corpus reaches" is a measurement in the PR rather than an assertion. Then updates the changelog.

**Files:**
- Modify: `CHANGELOG.md` (the `## [Unreleased]` → `### Changed` section)

**Interfaces:**
- Consumes: everything from Tasks 1–5
- Produces: no code interfaces; a verified claim and a changelog entry

- [ ] **Step 1: Measure which builtins the corpus routes through the shared dispatcher**

Temporarily add as the first line of `dispatch_pure`'s body:

```rust
    eprintln!("DISPATCH {}", name);
```

Then:

```bash
maturin develop --release
uv run pytest tests/python/test_fastpath_differential.py -q --capture=no 2>&1 \
  | grep -o 'DISPATCH [a-zA-Z0-9]*' | awk '{print $2}' | sort -u
```

`--capture=no` is required: pytest captures at the file-descriptor level, so a Rust `eprintln!` is swallowed without it and the measurement silently reads as zero.

Record the list. Expect the 29 from before plus whichever of the 24 the corpus reaches.

- [ ] **Step 2: Remove the instrumentation and rebuild**

Delete the `eprintln!` line. Run `maturin develop --release`, then `git diff --quiet src/ && echo clean` to confirm nothing of it survived.

- [ ] **Step 3: Run full verification one final time**

Run every command in the "Full verification command set" above.

- [ ] **Step 4: Add the changelog entry**

In `CHANGELOG.md`, under `## [Unreleased]` → `### Changed`, add:

```markdown
- Every builtin that needs only its arguments is now implemented once, in `src/builtins.rs`,
  and shared by the compiled path and the tree-walker instead of being written out in each.
  Fifty-two builtins were spread across three dispatch sites: twenty-eight were implemented
  twice, twenty-four existed in exactly one, and `$not` was implemented differently in each
  (`$boolean` coercion in one, raw truthiness in the other). No behaviour changes — the
  differential corpus and the 1686-case reference suite produce identical results before and
  after — but a builtin can no longer be correct in one dispatch path and wrong or absent in
  another, which is what let `$type(x)` work while `$map(arr, $type)` raised.
  ([#107](https://github.com/txjmb/jsonata-core/issues/107))
```

- [ ] **Step 5: Commit and open the PR**

```bash
git add CHANGELOG.md
git commit -m "$(cat <<'EOF'
docs(changelog): record the shared builtin dispatcher

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01JEvVbP5MEcZDt7mXv7SxLS
EOF
)"
git push -u origin fix/byref-builtin-dispatch
```

The PR body must state, with the Step 1 measurement as evidence:
- what moved and what stayed, with counts
- that behaviour is unchanged on everything the corpus reaches, and that where the old implementations disagreed in shapes the corpus does *not* reach, the extraction silently picked the compiled path's version — a known limit, stated rather than buried
- that `$not` was the one reconciliation, and why the compiled version won
- that Stage 2 (#107) is still outstanding, and that `$sort`, `$sift` and `$each` remain unreachable by reference because they are evaluator-dependent

---

## Self-Review

**Spec coverage.** Stage 1 of the spec has five requirements. `dispatch_pure` + `is_pure_builtin` in a new module → Task 1. The prologue in the #106 order → Task 2. `call_pure_builtin` deleted, VM repointed → Task 3. The 24 tree-walker-only builtins moved → Task 4. Tree-walker keeps its 10 and the duplicates go → Task 5. The context-insertion union → Task 2 (implemented) and Task 5 (dead entries removed). The spec's testing requirements → the "Full verification command set" plus Task 6's measurement. Stage 2 is deliberately not covered here; it gets its own plan once this lands.

**Placeholders.** None. Every code step carries the actual code, every run step carries the actual command and the expected output.

**Type consistency.** `dispatch_pure(name: &str, args: &[JValue], context: &JValue, options: &EvaluatorOptions) -> Result<JValue, EvaluatorError>` and `is_pure_builtin(name: &str) -> bool` are used identically in Tasks 1–6. The `effective_args` → `args` and `evaluated_args` → `args` renames are stated in the tasks that perform them (3 and 4). The 53-name list in `is_pure_builtin` (Task 1) was checked against ground truth (the tree-walker's pure set unioned with `call_pure_builtin`'s arms) and matches exactly. It accounts for the arms moved in Tasks 3 and 4: 29 + 24 = 53, with `not` inside Task 3's 29 and reconciled in Task 5.

**Known red state.** Task 2 deliberately ends with one failing unit test, and Task 4 is red between Steps 1 and 3. Both are called out where they occur.
