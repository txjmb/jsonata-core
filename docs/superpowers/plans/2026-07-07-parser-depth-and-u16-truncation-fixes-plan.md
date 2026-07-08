# Parser Recursion-Depth Guard + Compiler u16/u8 Truncation Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two pre-existing, out-of-scope bugs discovered during the jsonata-js 2.2.1 Phase 2 (guardrails) work and flagged in `docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md`'s Status section: (a) a whole-process SIGSEGV on deeply-nested expressions, root-caused (this session, empirically) to `src/ast_transform.rs`'s unguarded recursive AST walk — **not** the Pratt parser itself, which was the original (imprecise) framing; (b) silent, wrong-answer data corruption in the bytecode compiler wherever a collection's length is cast down to `u16`/`u8` without a bounds check, of which `Instr::MakeArray(u16)` (`src/compiler.rs:328`) was the originally-reported instance but is one of at least 6 structurally-identical sites.

**Architecture:** For (a), retrofit `src/ast_transform.rs`'s mutually-recursive AST-walking functions with the same depth-counter + `stacker::maybe_grow` pattern `src/evaluator.rs`'s `evaluate_internal` already uses, turning a native-stack crash into a graceful parser error. For (b), add length checks at the point each oversized construct would otherwise compile to a truncating bytecode operand, causing compilation to bail out (return `None`) so the always-correct tree-walker handles the construct instead — mirroring how this codebase already treats every other non-compilable construct (wildcards, `ObjectTransform`, `Sort`, etc.).

**Tech Stack:** Rust (`jsonata-core`). No Python-surface changes are anticipated (Python-level behavior — an error instead of a crash for (a), a correct instead of wrong result for (b) — falls out of the Rust fix automatically, same as every other error/correctness fix in this crate).

## Global Constraints

- No behavior change for any expression that doesn't hit these edge cases. Every existing test (`cargo test --all-features`, the 1682-case Python reference suite) must stay green throughout.
- `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check` clean at every commit.
- For (a): the fix must turn the crash into a **graceful, coded error** (not silently truncate/drop part of the expression) — this mirrors the evaluator's own `U1001` philosophy (a hard, Rust-specific safety net, not a jsonata-js behavior to match, since jsonata-js has no equivalent native-stack-overflow failure mode).
- For (b): the fix must **fall back to the tree-walker** (return `None` from the relevant `try_compile_*` function) rather than trying to redesign the bytecode instruction encoding (e.g. widening operands to `u32`) — this is consistent with the existing architecture where compilation is a best-effort fast path and the tree-walker is the always-correct fallback for anything that doesn't fit.
- Every fix must include a test that empirically reproduces the CURRENT bug (crash or wrong-answer) before the fix, and proves it's resolved after — matching this codebase's established practice (e.g. `tests/integration_test.rs::test_deep_recursion_does_not_overflow_native_stack`, which already uses a constrained-stack spawned thread specifically because Windows' default 1MB thread stack is smaller than the CI/dev host's, and reproduces a real historical incident, GitHub issue #34).

## Scope amendment (added after Task 2 landed — read before Task 2b)

Task 2 (ast_transform.rs depth guard) is implemented and reviewed clean against its stated scope. But validating it surfaced two more findings that change this plan's shape:

1. **A residual, real Drop-glue crash.** Task 2's bail-out path avoids native recursive `Drop` at the 3 sites it patched, but the reviewer built a 6-line adversarial input (an array with one branch that trips the depth guard, sitting next to another huge untouched sibling branch still held by an in-progress `for`-loop iterator) that still crashes the process. Patching this site-by-site is whack-a-mole — every early return that abandons a partially-processed tree needs its own iterative teardown, and there is no way to enumerate all such sites with confidence.
2. **A third, more severe, previously-unknown bug**: the raw parser itself (not `ast_transform.rs`) crashes on deeply nested **parenthesized** expressions (`(((...)))`) at a low nesting count in a debug/1MB-stack build. This directly narrows finding #1 below: flat left-associative chains don't recurse in the parser (confirmed, still true), but parenthesized/grouped nesting does recurse in the parser's own `parse_primary`/`parse_expression` — any recursive-descent parser needs real recursion to handle grouping, Pratt or not.

Both findings share one root cause: nothing bounds the DEPTH of the tree as it is being built. `ast_transform.rs`'s guard (Task 2) only catches an over-deep tree after the parser has already fully constructed it — too late to protect the parser's own construction recursion, and too late to avoid needing Drop-glue patches for the tree it was forced to (partially) build while erroring out.

**The fix:** add one depth guard directly in `parser.rs`, at construction time, so an over-deep tree is never built in the first place (Task 2b, below). This:
- Fixes the paren-nesting crash directly (the guard fires during the parser's own recursive descent, before the dangerous depth is reached).
- Makes `ast_transform.rs`'s Task-2 guard pure defense-in-depth: a tree that already passed the parser's own ceiling can still be walked safely by `ast_transform`'s existing guard, but should never be able to approach its ceiling in practice.
- Makes the residual Drop-glue gap unreachable via the only real entry point (`parser::parse`) — there is nothing deep enough left to abandon mid-teardown. This is not a logical proof that recursive `Drop` on `AstNode` is safe in all possible cases (a hypothetical future code path that constructs an `AstNode` programmatically, bypassing the parser, would not be covered) — that residual is accepted as out of scope, parallel to how other Rust-specific implementation details are documented rather than defended against non-parser-driven construction.

This project has no fixed threat model — it's an open-source library usable in any context, including adversarial/untrusted input — so closing the actually-reachable crash paths (via the normal parsing entry point) is worth the extra task, per user confirmation.

Task 2b is inserted between Task 2 and the original Task 3 (now Task 4). Original Tasks 3/4/5 are renumbered 4/5/6 below.

## Root-cause findings from this session's investigation (do not re-derive these — they're already verified)

1. **The crash is NOT in the Pratt parser — for flat left-associative chains specifically.** Empirically confirmed via three isolated probes in a scratch test file (since deleted): (i) `parser::parse()` on a 200,000-term left-nested arithmetic chain (`1+1+1+...+1`) SIGABRTs ("thread has overflowed its stack"); (ii) the SAME input parsed via the raw `Parser::new(...).parse()` entry point (bypassing the post-parse `ast_transform::resolve_ancestry` pass) returns `Ok` cleanly, with the resulting `AstNode` explicitly `mem::forget`'d to also rule out a recursive-`Drop` cause; (iii) manual trace of `parse_expression`'s Pratt-parsing binding powers for `+`/`-` (`Token::Plus | Token::Minus => Some((50, 51))`, i.e. `right_bp = left_bp + 1`) confirms textbook left-associative parsing: each `+`'s recursive right-hand-side call (`parse_expression(51)`) returns almost immediately once it sees the next `+` (`left_bp(50) < min_bp(51)` breaks its inner loop), so the Pratt parser's own recursion depth for a flat left-associative chain stays O(1), not O(n) — confirmed empirically by (ii) succeeding at n=200,000.
   **Amendment (found during Task 2 validation, see "Scope amendment" above): this finding does NOT generalize to parenthesized/grouped nesting** (`(((...)))`). `parse_primary`'s `Token::LeftParen` arm calls `self.parse_expression(0)` for the group's contents, and each layer of parens is one real recursive call — this crashes the raw parser itself at a low nesting count on a 1MB-stack thread, a genuinely different (and more directly reachable) crash than the one this finding originally described. Task 2b's parse-time depth guard closes this too, since it lives in `parse_expression` itself, upstream of both the flat-chain loop and the paren recursion.
2. **The crash IS in `src/ast_transform.rs`.** `pub fn parse()` (`src/parser.rs:1730`) calls the raw Pratt parse, then **unconditionally** pipes the result through `crate::ast_transform::resolve_ancestry(raw_ast)` (`src/parser.rs:1733`) — this always runs, even for expressions using none of `%`/`@`/`#`. `resolve_ancestry` (`src/ast_transform.rs:108`) calls `transform_node` (a whole-tree rebuild, `src/ast_transform.rs:414`, mutually recursive with `transform_children` at `:509`), then a **second**, fully separate whole-tree walk, `substitute_labels` (`:129`). Neither of these (nor the other recursive helpers in this file — `transform_path_steps:789`, `resolve_predicate_slot:884`, `walk_backward:912`, `seek_parent_step:977`, `seek_parent_wrapped:1047`, `migrate_binding_markers:1080`) has any depth counter or `stacker::maybe_grow` call (confirmed: zero matches for `stacker` anywhere in this file), unlike `evaluate_internal` in `src/evaluator.rs`, which has both.
3. **Only tree-shaped (recursively-nested) constructs are at risk, not flat ones.** A `.`-chain path (`a.a.a...a`) is parsed into a single **flat** `Vec<PathStep>` (parser.rs's dot-handling flattens into `steps.push(...)` on the existing `Path`'s step list, not nested `Binary`-style boxing) — so `transform_path_steps` iterates a flat list, it doesn't recurse per step. The vulnerable shape is specifically deep **operator/expression nesting** (arithmetic chains, deeply nested parens/blocks/conditionals), which parser.rs represents as a right-nested chain of `Box`ed `AstNode`s that `transform_node`/`transform_children`/`substitute_labels` then walk recursively, one stack frame per nesting level.
4. **The `MakeArray(u16)` bug is one instance of a 6-site pattern**, all in `src/compiler.rs`, all `<collection>.len() as u16` or `as u8` with no bounds check:
   - `compiler.rs:328` — `Instr::MakeArray(elems.len() as u16)` — **high practical severity**: any literal array with >65,535 elements (or a `$map`/`$sum`-adjacent expression that lowers to this instruction) silently returns a wrong-length, wrong-content result (confirmed empirically: a 100,000-element literal returns length 34,464 = 100,000 mod 65,536).
   - `compiler.rs:341` — `Instr::MakeObject(pairs.len() as u16)` — same class, for object literals with >65,535 key-value pairs. Lower practical likelihood (hand-written/generated object literals rarely approach this) but identical silent-corruption severity.
   - `compiler.rs:355` — `Instr::BlockEnd(n as u16)` — same class, for `(...; ...; ...)` blocks with >65,535 semicolon-separated sub-expressions.
   - `compiler.rs:367` — `arg_count: args.len() as u8` (in `Instr::CallBuiltin`) — same class but a **much lower threshold (256)**. Per this crate's own conventions (`COMPILABLE_BUILTINS`/`max_args` compile-time validation, referenced in project memory), compilable builtins already have small fixed arg-count ceilings, so this specific site may not be practically reachable — verify during implementation rather than assume.
   - `compiler.rs:60`, `:70`, `:80`, `:169` — four **pool-interning** sites (`const_pool.len() as u16`, `string_pool.len() as u16`, `fallback_exprs.len() as u16`, `sub_programs.len() as u16`), each incrementing by one per newly-discovered distinct constant/string/fallback-subexpr/filter-subprogram **during compilation of a single source expression**. Overflowing any of these requires one expression containing tens of thousands of distinct literals/sub-expressions — astronomically unlikely in practice, but the same silent-corruption failure mode if it ever occurs. Lower priority; fix using the same pattern for completeness, not urgency.
5. **The tree-walker has no equivalent length-cast risk** — `JValue::Array(Rc<Vec<JValue>>)`/`JValue::Object(Rc<IndexMap<...>>)` have no fixed-width length field anywhere in the tree-walking evaluator, so falling back to it for oversized constructs is unconditionally correct, not just "less wrong."

---

## Task 1: Map the exact recursive call graph in `src/ast_transform.rs` and confirm a minimal, safe guard-insertion point set

This is a research/verification task with a small code deliverable (no behavior change yet) — it exists because `ast_transform.rs` has ~7 mutually-recursive functions (not evaluator.rs's single clean `evaluate_internal` chokepoint), and guessing which ones need the guard risks silently missing one, exactly the "Task 5 pattern" (a check added at only one of several recursive entry points) that repeatedly bit the guardrails plan this session. Get this right before touching behavior.

**Files:**
- Read: `src/ast_transform.rs` (entire file, 1986 lines — this task requires actually reading it, not sampling)
- Modify: none (this task produces a written call-graph map as its deliverable, appended as a doc comment block at the top of `src/ast_transform.rs`, plus a decision recorded in this task's completion notes — no functional changes)

**Interfaces:**
- Produces: a definitive list of every function in this file that (a) is part of a cycle that recurses on `AstNode`/`PathStep` structure (i.e., its recursion depth scales with the INPUT expression's nesting depth, not a fixed small bound), and (b) therefore needs the depth-guard from Task 2. Task 2 consumes this list directly — do not let Task 2 re-derive it.

- [ ] **Step 1: Read the whole file and build the call graph**

For each of these functions — `resolve_ancestry` (:108), `substitute_labels` (:129), `transform_node` (:414), `transform_children` (:509), `transform_path_steps` (:789), `resolve_predicate_slot` (:884), `walk_backward` (:912), `seek_parent_step` (:977), `seek_parent_wrapped` (:1047), `migrate_binding_markers` (:1080), plus any other top-level `fn`/`pub fn` in the file you find during the read — note: (a) does it call itself directly or indirectly (trace one hop at a time — "calls X, which calls Y, which calls back into this function" counts); (b) if so, does each recursive step consume one level of `AstNode`/`PathStep`/`Stage` nesting (i.e., is recursion depth bounded by input structure depth), or is it bounded by something else entirely (e.g. a fixed-size enum match with no recursive case, or a loop over a `Vec` — NOT a stack-depth risk regardless of the `Vec`'s length, since that's iteration not recursion)?

- [ ] **Step 2: Write the map as a doc comment**

At the top of `src/ast_transform.rs`, after the existing module-level comment (currently lines 1-6), insert a new doc block:
```rust
// Recursion-depth safety (added: see docs/superpowers/plans/2026-07-07-parser-depth-and-u16-truncation-fixes-plan.md):
// The following functions recurse on AstNode/PathStep/Stage structure, with
// recursion depth scaling with the INPUT expression's nesting depth (not a
// fixed bound) - each needs the depth-guard added in Task 2:
//   - <fn name at :line> (recurses via <direct call / via <other fn>>)
//   - <fn name at :line> (...)
//   ...
// Functions confirmed NOT to need guarding (either non-recursive, or their
// only "recursion" is bounded iteration over a Vec, not stack depth):
//   - <fn name at :line> (<why>)
//   ...
```
Fill in the actual list from Step 1's findings — do not leave placeholder text.

- [ ] **Step 3: Sanity-check against the known-good and known-bad repro shapes**

Confirm your list is consistent with: (a) a 200,000-term left-nested arithmetic chain (`1+1+1+...`) crashes (already confirmed this session); (b) a 200,000-step flat dot-path (`a.a.a...a`) does NOT crash via this mechanism (per finding #3 above — if your call-graph analysis suggests `transform_path_steps` DOES recurse per-step rather than iterating a flat `Vec<PathStep>`, that contradicts this session's finding and needs re-checking before proceeding — read `transform_path_steps`'s actual body carefully). Write a quick throwaway test (not committed) reproducing (b) evaluated through the FULL `parser::parse()` path (not the raw `Parser`) at a smaller, fast-to-run depth (e.g. 50,000 steps) to confirm it returns `Ok` rather than hanging/crashing, before deleting the scratch test.

- [ ] **Step 4: Commit**

```bash
git add src/ast_transform.rs
git commit -m "docs: map ast_transform.rs's recursive call graph ahead of adding a depth guard"
```

---

## Task 2: Add a depth-counter + `stacker::maybe_grow` guard to every function Task 1 identified

**Files:**
- Modify: `src/ast_transform.rs` (every function Task 1's map identifies as needing guarding)
- Modify: `src/parser.rs` if a new error variant is needed (check `ParserError`'s existing variants first — `AstTransformError::Coded { code, message }` already maps into `ParserError::Coded` per `src/parser.rs:1733-1736`, so a new `AstTransformError` variant is NOT needed; just pick an appropriate, currently-unused error code string for the new coded error)
- Test: `tests/integration_test.rs`

**Interfaces:**
- Consumes: Task 1's function list.
- Produces: every guarded function gains a `depth: usize` parameter (or equivalent shared counter — see Step 1 for the exact mechanism), threaded through every recursive call Task 1 identified; a new coded error (pick a code — e.g. check `grep -rn '"S0[0-9]' src/` for the next unused `S0`-prefixed slot, or use a clearly Rust-implementation-specific non-`S`-prefixed code analogous to `U1001`, since — like `U1001` — this is a Rust-specific safety net with no jsonata-js equivalent to match numbering against).

- [ ] **Step 1: Decide the threading mechanism**

Given `ast_transform.rs`'s recursion is spread across several mutually-recursive functions (not evaluator.rs's single `evaluate_internal` chokepoint), threading a plain `depth: usize` parameter through every one of Task 1's identified functions and every call between them is the most direct mirror of the existing `evaluate_internal` pattern (increment before recursing, decrement/check after). Do this exactly the way `evaluate_internal` does it — reference `src/evaluator.rs`'s current `evaluate_internal` for the exact pattern (const `RED_ZONE`/`GROW_STACK_SIZE`, `stacker::maybe_grow` wrapping the recursive call, increment-check-decrement around it) — do NOT invent a different mechanism (e.g. a thread-local counter) without a specific reason Task 1 surfaces.

- [ ] **Step 2: Write the failing test FIRST**

In `tests/integration_test.rs`, add (following the existing `test_deep_recursion_does_not_overflow_native_stack` pattern exactly — same 1MB spawned-thread technique, same reasoning about Windows' smaller default stack):
```rust
/// Deeply left-nested arithmetic (e.g. `1+1+1+...`) must hit a graceful
/// depth-guard error in ast_transform's post-parse pass, not crash the
/// whole process. Historical bug: `parser::parse()` unconditionally pipes
/// every parse through `ast_transform::resolve_ancestry`, whose recursive
/// AST walk had no depth guard (found during jsonata-js 2.2.1 Phase 2
/// guardrails work — see docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md).
/// Confirmed empirically (this session) that the raw Pratt parser handles
/// this input fine at n=200,000; the crash is entirely in the post-parse
/// ast_transform pass.
#[test]
fn test_deeply_nested_arithmetic_does_not_overflow_native_stack_at_parse_time() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024) // 1MB, matching Windows' default thread stack
        .spawn(|| {
            let expr_str = format!("({})", vec!["1"; 200_000].join("+"));
            match parser::parse(&expr_str) {
                Ok(_) => "Ok".to_string(),
                Err(e) => format!("Err({e})"),
            }
        })
        .unwrap();

    let outcome = handle
        .join()
        .expect("expression parsing overflowed the native stack instead of returning a graceful error");

    assert!(
        outcome.starts_with("Err"),
        "expected a graceful depth-limit error, got: {outcome}"
    );
}
```
(Fill in the actual expected error-code substring in the assertion once Step 1 picks the code, e.g. `outcome.contains("<CODE>")` in addition to `starts_with("Err")`.)

Also add a companion test confirming REASONABLE nesting still works (pick a depth well below whatever ceiling you land on in Step 3, e.g. 100 or 500 levels) — must return `Ok`, proving the guard doesn't fire on legitimate expressions.

- [ ] **Step 3: Run to verify the first test fails (crashes) before the fix**

Run: `cargo test --test integration_test test_deeply_nested_arithmetic -- --nocapture 2>&1 | tail -20`
Expected: the process aborts with "has overflowed its stack" (matching this session's empirical finding) — this IS the failing state, confirming the test reproduces the real bug before you fix it.

- [ ] **Step 4: Implement the guard**

Apply the threading decided in Step 1 to every function in Task 1's list. Pick a depth ceiling empirically: start with something in the same ballpark as `evaluator.rs`'s `max_recursion_depth = 302` (or higher if `ast_transform`'s per-frame stack usage is smaller — test with `stacker::maybe_grow`'s `GROW_STACK_SIZE` to confirm your chosen ceiling is comfortably reachable within grown-stack budgets on a 1MB-stack thread before erroring). The error should be a coded `AstTransformError` (e.g. `coded("<CODE>", "expression nesting is too deep")`) that propagates through `resolve_ancestry`'s `Result` and out through `parser::parse`'s existing `map_err` (`src/parser.rs:1733-1736`) as a `ParserError::Coded`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test integration_test test_deeply_nested -- --nocapture 2>&1 | tail -20`
Expected: both the crash-repro test (now returns a graceful `Err` instead of crashing) and the reasonable-nesting test (still `Ok`) pass.
Run the full suite: `cargo test --all-features` (must stay at the current full count, all green) and `uv run pytest tests/python/test_reference_suite.py -q` (rebuild first: `env -u VIRTUAL_ENV maturin develop --release --uv`) — expect 1682/1682, no regressions (no reference-suite case is anywhere near this nesting depth).

- [ ] **Step 6: Commit**

```bash
git add src/ast_transform.rs src/parser.rs tests/integration_test.rs
git commit -m "fix(parser): graceful depth-limit error in ast_transform instead of native stack overflow"
```

---

## Task 2b: Add a parse-time nesting-depth guard directly in `parser.rs`

This is the root-cause fix motivated by the "Scope amendment" section above. It supersedes Task 2 as the primary defense (Task 2's `ast_transform.rs` guard stays in place as harmless defense-in-depth — do not remove it or its tests).

**Files:**
- Modify: `src/parser.rs` — add a `depth: usize` field to `struct Parser` (`:671-674`), initialize it to `0` in `Parser::new` (`:677-684`), and guard `parse_expression` (`:1066`)
- Test: `tests/integration_test.rs`

**Interfaces:**
- Consumes: none new.
- Produces: `parser::parse` returns `Err(ParserError::Coded { code: "U1002", .. })` instead of crashing, for BOTH deeply left-nested infix chains AND deeply nested parenthesized/grouped expressions. Reuses the `U1002` code Task 2 already introduced (same conceptual guard — "expression nesting too deep" — just enforced earlier in the pipeline; do not invent a second code for this).

- [ ] **Step 1: Understand why one guard site must cover two different growth patterns**

Read `src/parser.rs:1066-1069` (`parse_expression`'s entry: `let mut lhs = self.parse_primary()?;`) and the loop starting at `:1069` (skim the `Token::Dot`, `Token::At`, `Token::Caret`, and the catch-all binary-operator arm at `:1605-1649` — each one, on the "normal" path, ends by reassigning `lhs` to a NEW node that wraps the OLD `lhs` one level deeper, e.g. `lhs = AstNode::Binary { lhs: Box::new(lhs), .. }` at `:1644-1648`).

This means two structurally different things both need bounding by the SAME counter:
- **Recursive descent** (parens in `parse_primary` calling back into `parse_expression`, unary operands, array/object literal elements, function args, blocks) — depth grows by 1 per actual recursive call, and naturally shrinks back on return, exactly like `evaluate_internal`'s existing guard.
- **Loop-driven left-nesting** (the flat `1+1+1+...` case) — `parse_expression` is NOT called recursively per `+`; the SAME call's `loop { .. }` reassigns `lhs` to a deeper node on every iteration. A guard that only checks depth at function entry would never see this growth, since there's only one outer call.

The fix: increment a shared `self.depth` both at function entry (for recursive descent) AND once per loop iteration whenever `lhs` gets rewrapped (for loop-driven left-nesting), and restore `self.depth` to its value from before this call when the call returns successfully — so depth accumulated by one subtree's parsing doesn't leak into an unrelated sibling subtree parsed afterward (e.g. the second element of `[1+1+1+.., 2+2+2+..]` must not inherit depth left over from the first).

- [ ] **Step 2: Write the failing tests FIRST**

In `tests/integration_test.rs`, add two tests (following the existing 1MB-spawned-thread pattern used by `test_deep_recursion_does_not_overflow_native_stack` and Task 2's `test_deeply_nested_arithmetic_does_not_overflow_native_stack_at_parse_time` — reuse that exact technique):

```rust
/// Deeply PARENTHESIZED nesting (e.g. `((((...))))`) must hit a graceful
/// depth-guard error during parsing itself, not crash the process. This is
/// a DIFFERENT crash than the flat-arithmetic-chain one Task 2 fixed in
/// ast_transform.rs: parens are real recursive descent inside
/// `parse_primary`/`parse_expression` (src/parser.rs), so this crashes
/// BEFORE ast_transform ever runs, and BEFORE ast_transform's own guard
/// gets a chance to protect anything. Found during Task 2's validation
/// (this session) — contradicts the original "crash is not in the parser"
/// framing for this specific shape (see the plan's Scope Amendment).
#[test]
fn test_deeply_nested_parens_does_not_overflow_native_stack_at_parse_time() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024) // 1MB, matching Windows' default thread stack
        .spawn(|| {
            let n = 5_000;
            let expr_str = format!("{}1{}", "(".repeat(n), ")".repeat(n));
            match parser::parse(&expr_str) {
                Ok(_) => "Ok".to_string(),
                Err(e) => format!("Err({e})"),
            }
        })
        .unwrap();

    let outcome = handle
        .join()
        .expect("expression parsing overflowed the native stack instead of returning a graceful error");

    assert!(
        outcome.starts_with("Err") && outcome.contains("U1002"),
        "expected a graceful U1002 depth-limit error, got: {outcome}"
    );
}

/// Reasonable paren nesting must still work (the guard must not fire on
/// legitimate expressions). Pick a depth comfortably below the chosen
/// ceiling (Step 4 picks the ceiling; this must stay well under it).
#[test]
fn test_reasonable_paren_nesting_still_parses() {
    let n = 50;
    let expr_str = format!("{}1{}", "(".repeat(n), ")".repeat(n));
    let result = parser::parse(&expr_str);
    assert!(result.is_ok(), "reasonable paren nesting should parse fine, got: {result:?}");
}

/// The flat left-associative arithmetic chain from Task 2 must now ALSO be
/// caught here, at parse-construction time, independent of ast_transform's
/// guard (which still runs afterward as defense-in-depth, but should never
/// see a tree deep enough to matter now).
#[test]
fn test_deeply_nested_arithmetic_caught_at_parse_construction_time() {
    let expr_str = format!("({})", vec!["1"; 200_000].join("+"));
    let result = parser::parse(&expr_str);
    match result {
        Err(e) => assert!(format!("{e}").contains("U1002"), "expected U1002, got: {e}"),
        Ok(_) => panic!("expected a graceful depth-limit error"),
    }
}

/// Sibling subtrees must not inherit accumulated depth from an earlier
/// sibling: an array of two SHALLOW arithmetic expressions must parse fine
/// even though each element's `parse_expression` call briefly re-enters the
/// loop-driven counter — proves depth is restored to its pre-call value
/// between the two array elements, not left elevated.
#[test]
fn test_sibling_subtrees_do_not_inherit_depth() {
    let shallow = vec!["1"; 20].join("+");
    let expr_str = format!("[{shallow}, {shallow}, {shallow}]");
    let result = parser::parse(&expr_str);
    assert!(result.is_ok(), "shallow siblings should parse fine, got: {result:?}");
}
```

- [ ] **Step 3: Run to verify the paren test fails (crashes) before the fix**

Run: `cargo test --test integration_test test_deeply_nested_parens -- --nocapture 2>&1 | tail -20`
Expected: the process aborts ("has overflowed its stack" or similar) — confirming this reproduces a real, currently-uncaught crash distinct from the one Task 2 fixed.

- [ ] **Step 4: Implement the guard**

Add to `struct Parser` (`:671-674`):
```rust
pub struct Parser {
    lexer: Lexer,
    current_token: Token,
    depth: usize,
}
```
Initialize in `Parser::new` (`:677-684`, add `depth: 0,` to the constructed `Parser { .. }`).

Add constants near the top of `parser.rs` (same values as `evaluate_internal`'s and `ast_transform.rs`'s analogous guards — see `src/evaluator.rs`'s `evaluate_internal` and `src/ast_transform.rs`'s `AST_TRANSFORM_RED_ZONE`/`AST_TRANSFORM_GROW_STACK_SIZE` for precedent):
```rust
const PARSER_RED_ZONE: usize = 128 * 1024;
const PARSER_GROW_STACK_SIZE: usize = 8 * 1024 * 1024;
```
Pick `MAX_PARSE_DEPTH` empirically the same way Task 2 picked `MAX_TRANSFORM_DEPTH`: start at `1000` (matching `ast_transform.rs`'s ceiling, for consistency — a tree that would pass this guard should also comfortably pass `ast_transform`'s), then confirm empirically with a scratch test (deeply nested parens AND a deeply nested flat chain, each at n=999 and n=1001) that a 1MB-stack thread neither overflows below the ceiling nor errors above it with headroom to spare, adjusting down if `stacker::maybe_grow`'s growth isn't enough at 1000 for the paren-recursion case specifically (parens are real recursive descent, unlike the loop-driven chain case, so they cost real stack per level even with growth).

Modify `parse_expression` (`:1066`):
```rust
    fn parse_expression(&mut self, min_bp: u8) -> Result<AstNode, ParserError> {
        let entry_depth = self.depth;
        self.depth += 1;
        if self.depth > MAX_PARSE_DEPTH {
            return Err(ParserError::Coded {
                code: "U1002",
                message: format!(
                    "Stack overflow - maximum expression nesting depth ({}) exceeded while parsing expression",
                    MAX_PARSE_DEPTH
                ),
            });
        }

        const PARSER_RED_ZONE: usize = 128 * 1024;
        const PARSER_GROW_STACK_SIZE: usize = 8 * 1024 * 1024;
        let result = stacker::maybe_grow(PARSER_RED_ZONE, PARSER_GROW_STACK_SIZE, || {
            self.parse_expression_impl(min_bp)
        });

        self.depth = entry_depth;
        result
    }

    fn parse_expression_impl(&mut self, min_bp: u8) -> Result<AstNode, ParserError> {
        let mut lhs = self.parse_primary()?;

        loop {
            // ... existing loop body, UNCHANGED except: every arm that
            // currently ends with `lhs = AstNode::Something { .., lhs: Box::new(lhs), .. }`
            // (or the equivalent `Path`-step-append pattern) gains, immediately
            // before that reassignment, the same depth bump + check:
            //
            //     self.depth += 1;
            //     if self.depth > MAX_PARSE_DEPTH {
            //         return Err(ParserError::Coded {
            //             code: "U1002",
            //             message: format!(
            //                 "Stack overflow - maximum expression nesting depth ({}) exceeded while parsing expression",
            //                 MAX_PARSE_DEPTH
            //             ),
            //         });
            //     }
            //
            // Apply this to EVERY `lhs = ..` reassignment in the loop (Dot/path-step
            // append, ArrayGroup, FunctionApplication, IndexBind, FocusBind, Sort, and
            // the catch-all binary-operator arm) — not just the arithmetic one. Grep
            // the loop body for `lhs = AstNode::` and `lhs = AstNode::Path` to enumerate
            // every site; do not rely on this comment's list being exhaustive.
        }

        // (existing trailing code, if any, unchanged)
        Ok(lhs)
    }
```
Rename the existing loop body into `parse_expression_impl` exactly as shown (a thin wrapper + impl split), so the depth push/pop lives in ONE place (`parse_expression`) while every existing recursive call site in the codebase (which calls `self.parse_expression(..)`, unchanged) automatically goes through the guard without needing to be found and edited individually — recursive descent depth is fully handled by this wrapper. Only the LOOP-driven bumps (inside `parse_expression_impl`, for `lhs` rewrapping) need to be added at each of the enumerated sites, since those don't go through a fresh call to the wrapper.

Because `entry_depth`/`self.depth = entry_depth` in the wrapper resets depth back to what it was before this call on every successful return (regardless of how many loop iterations bumped it in between), a sibling subtree parsed afterward (e.g. the next array element) starts from the same baseline — this is what Step 2's `test_sibling_subtrees_do_not_inherit_depth` proves.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test integration_test test_deeply_nested_parens test_reasonable_paren_nesting test_deeply_nested_arithmetic_caught_at_parse_construction_time test_sibling_subtrees_do_not_inherit_depth -- --nocapture 2>&1 | tail -40`
Expected: all four pass.
Run the full suite: `cargo test --all-features` (must stay green, including Task 2's own tests — `ast_transform.rs`'s guard should still work as defense-in-depth even though it should no longer be reachable via `parser::parse`) and the reference suite: `env -u VIRTUAL_ENV maturin develop --release --uv && uv run pytest tests/python/test_reference_suite.py -q` — expect 1682/1682, no regressions.

- [ ] **Step 6: Commit**

```bash
git add src/parser.rs tests/integration_test.rs
git commit -m "fix(parser): add parse-time nesting-depth guard covering both recursive descent (parens) and loop-driven left-nesting (flat chains)"
```

---

## Task 4: Fix the two high-priority `u16` truncation sites — `MakeArray` and `MakeObject`

**Files:**
- Modify: `src/evaluator.rs` (`try_compile_expr_inner`'s `AstNode::Object`/`AstNode::Array` arms, currently `src/evaluator.rs:476-500` — re-locate by content, not line number, since Tasks 1-2b may have shifted the file slightly)
- Test: `tests/integration_test.rs`

**Interfaces:**
- Consumes: none new.
- Produces: `try_compile_expr_inner` returns `None` (falls back to the tree-walker) for `AstNode::Object`/`AstNode::Array` nodes whose element/pair count exceeds `u16::MAX as usize`, instead of compiling them into a truncating `MakeObject`/`MakeArray` instruction.

- [ ] **Step 1: Write the failing tests**

```rust
/// A literal array with more than u16::MAX (65,535) elements must not
/// silently truncate via Instr::MakeArray's u16 operand - it should either
/// evaluate correctly (falling back to the tree-walker, which has no such
/// limit) or be rejected with a clear error, but never silently return a
/// wrong-length result. Historical bug: compiler.rs:328's
/// `Instr::MakeArray(elems.len() as u16)` truncates silently (confirmed
/// empirically: a 100,000-element literal previously returned length 34,464
/// = 100,000 mod 65,536).
#[test]
fn test_large_literal_array_does_not_silently_truncate() {
    let n = 100_000;
    let expr_str = format!("[{}]", vec!["1"; n].join(","));
    let ast = parse(&expr_str).unwrap();
    let mut evaluator = Evaluator::new();
    let data = JValue::Null;
    let result = evaluator.evaluate(&ast, &data).unwrap();
    match result {
        JValue::Array(arr) => assert_eq!(arr.len(), n, "array literal was silently truncated"),
        other => panic!("expected array, got {other:?}"),
    }
}

/// Same shape for object literals and Instr::MakeObject.
#[test]
fn test_large_literal_object_does_not_silently_truncate() {
    let n = 100_000;
    let pairs: Vec<String> = (0..n).map(|i| format!("\"k{i}\": {i}")).collect();
    let expr_str = format!("{{{}}}", pairs.join(","));
    let ast = parse(&expr_str).unwrap();
    let mut evaluator = Evaluator::new();
    let data = JValue::Null;
    let result = evaluator.evaluate(&ast, &data).unwrap();
    match result {
        JValue::Object(obj) => assert_eq!(obj.len(), n, "object literal was silently truncated"),
        other => panic!("expected object, got {other:?}"),
    }
}
```
Note: constructing a 100,000-element expression STRING might itself risk hitting the parser-depth guard from Task 2 if array/object literal parsing builds a nested structure rather than a flat `Vec` — check `AstNode::Array`/`AstNode::Object`'s actual shape (they should already be flat `Vec<AstNode>`/`Vec<(AstNode,AstNode)>` per the `try_compile_expr_inner` code read this session, lines 476-500, which iterate `pairs`/`elems` as a plain `Vec` — this is NOT the recursive-nesting shape Task 2 guards against, so this should be unaffected, but confirm empirically as part of Step 2).

- [ ] **Step 2: Run tests to verify they fail (or reveal the actual current bug shape)**

Run: `cargo test --test integration_test test_large_literal -- --nocapture 2>&1 | tail -20`
Expected: both FAIL with a wrong `arr.len()`/`obj.len()` (e.g. `34464` instead of `100000`) — confirming the silent-truncation bug as currently observed, not some other error. If either test instead panics/errors for an unrelated reason (e.g. hits Task 2's new depth guard), investigate why before proceeding — that would mean array/object literals aren't as flat as assumed.

- [ ] **Step 3: Implement the guard**

Current (`src/evaluator.rs`, `AstNode::Object`/`AstNode::Array` arms in `try_compile_expr_inner`):
```rust
        // ── Object construction ─────────────────────────────────────────
        AstNode::Object(pairs) => {
            let mut fields = Vec::with_capacity(pairs.len());
            for (key_node, val_node) in pairs {
                // Key must be a string literal
                let key = match key_node {
                    AstNode::String(s) => s.clone(),
                    _ => return None,
                };
                let val = try_compile_expr_inner(val_node, allowed_vars)?;
                fields.push((key, val));
            }
            Some(CompiledExpr::ObjectConstruct(fields))
        }

        // ── Array construction ──────────────────────────────────────────
        AstNode::Array(elems) => {
            let mut compiled = Vec::with_capacity(elems.len());
            for elem in elems {
                // Tag whether the element itself is an array constructor: if so, its
                // array value must be kept nested rather than flattened (tree-walker parity).
                let is_nested = matches!(elem, AstNode::Array(_));
                compiled.push((try_compile_expr_inner(elem, allowed_vars)?, is_nested));
            }
            Some(CompiledExpr::ArrayConstruct(compiled))
        }
```
Replace with:
```rust
        // ── Object construction ─────────────────────────────────────────
        AstNode::Object(pairs) => {
            // Instr::MakeObject's operand is a u16 element count - bail out
            // to the (always-correct, no-limit) tree-walker rather than
            // silently truncating via CompiledExpr::ObjectConstruct here.
            if pairs.len() > u16::MAX as usize {
                return None;
            }
            let mut fields = Vec::with_capacity(pairs.len());
            for (key_node, val_node) in pairs {
                // Key must be a string literal
                let key = match key_node {
                    AstNode::String(s) => s.clone(),
                    _ => return None,
                };
                let val = try_compile_expr_inner(val_node, allowed_vars)?;
                fields.push((key, val));
            }
            Some(CompiledExpr::ObjectConstruct(fields))
        }

        // ── Array construction ──────────────────────────────────────────
        AstNode::Array(elems) => {
            // Instr::MakeArray's operand is a u16 element count - bail out
            // to the (always-correct, no-limit) tree-walker rather than
            // silently truncating via CompiledExpr::ArrayConstruct here.
            if elems.len() > u16::MAX as usize {
                return None;
            }
            let mut compiled = Vec::with_capacity(elems.len());
            for elem in elems {
                // Tag whether the element itself is an array constructor: if so, its
                // array value must be kept nested rather than flattened (tree-walker parity).
                let is_nested = matches!(elem, AstNode::Array(_));
                compiled.push((try_compile_expr_inner(elem, allowed_vars)?, is_nested));
            }
            Some(CompiledExpr::ArrayConstruct(compiled))
        }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test integration_test test_large_literal -- --nocapture 2>&1 | tail -20`
Expected: both PASS (correct length, no truncation — the tree-walker fallback handles them).
Run the full suite: `cargo test --all-features`, and rebuild + reference suite: `env -u VIRTUAL_ENV maturin develop --release --uv && uv run pytest tests/python/test_reference_suite.py -q` — expect 1682/1682.

- [ ] **Step 5: Commit**

```bash
git add src/evaluator.rs tests/integration_test.rs
git commit -m "fix(compiler): fall back to tree-walker instead of silently truncating MakeArray/MakeObject beyond u16::MAX elements"
```

---

## Task 5: Fix the remaining lower-priority truncation sites (`BlockEnd`, `CallBuiltin` arg count, pool-interning)

This task is lower priority than Task 4 (these sites require far more contrived inputs to trigger — tens of thousands of semicolon-separated block expressions, hundreds of arguments to one compiled builtin call, or tens of thousands of distinct literals in one expression) but is the same bug class and should be closed for completeness, using the identical "bail out to tree-walker" pattern.

**Files:**
- Modify: `src/evaluator.rs` (`try_compile_expr_inner`'s `AstNode::Block` arm, currently `src/evaluator.rs:503+` — re-locate by content)
- Modify: `src/compiler.rs` (the 4 pool-interning helper functions at `:57-80` and `:169`, and the `CallBuiltin` emission at `:367` — re-locate by content since Tasks 1-4 may shift line numbers slightly)
- Test: `tests/integration_test.rs`

**Interfaces:**
- Consumes: none new.
- Produces: `try_compile_expr_inner`'s `AstNode::Block` arm returns `None` for blocks with more than `u16::MAX` sub-expressions. The 4 pool-interning functions in `compiler.rs` (`intern_const`/equivalent for `string_pool`/`add_fallback`/the `sub_programs` push site) return a sentinel indicating "pool full, abort this compilation" that propagates up through `BytecodeCompiler::compile` as a compile failure (falling back to the tree-walker) rather than silently wrapping the index — check how `BytecodeCompiler::compile`'s callers currently handle a compilation failure (it likely already returns `Option<BytecodeProgram>` or panics; if it can't currently fail gracefully mid-compilation, that's a larger refactor — investigate and report actual findings rather than assuming, since this determines whether this half of the task is a small change or needs its own sub-design).

- [ ] **Step 1: Investigate whether `BytecodeCompiler::compile` can fail gracefully mid-compilation**

Read `src/compiler.rs`'s `BytecodeCompiler::compile` (entry point) and its callers (`src/evaluator.rs`'s `try_compile_expr`, `src/lib.rs`'s `run_eval`/bench facade). Determine: does anything already handle "compilation started but hit an unrecoverable problem partway through"? If `BytecodeCompiler::compile`'s signature is `fn compile(ce: &CompiledExpr) -> BytecodeProgram` (infallible), making the 4 pool-interning helpers "abort gracefully" requires changing this signature to return `Option<BytecodeProgram>` (or `Result`), which ripples to every caller. If that ripple is large, consider instead: guard at the `CompiledExpr`-construction level (in `evaluator.rs`, similar to Task 4) by pre-counting how many distinct constants/strings/sub-programs a `CompiledExpr` tree would need BEFORE calling `BytecodeCompiler::compile` at all, and bail to `None` there instead — avoids touching `compiler.rs`'s infallible-looking API. Pick whichever approach is actually less invasive after reading the real code; report your reasoning either way.

- [ ] **Step 2: Write failing tests for `AstNode::Block` (the practically-closest-to-reachable of this task's sites)**

```rust
/// A `(...; ...; ...)` block with more than u16::MAX sub-expressions must
/// not silently truncate via Instr::BlockEnd's u16 operand.
#[test]
fn test_large_block_does_not_silently_truncate() {
    let n = 70_000;
    let expr_str = format!("({})", vec!["1"; n].join(";"));
    let ast = parse(&expr_str).unwrap();
    let mut evaluator = Evaluator::new();
    let data = JValue::Null;
    // A block's value is its last expression - correctness here isn't about
    // a returned length, but about not panicking/erroring incorrectly and
    // not silently mis-compiling; a block of `n` literal `1`s should just
    // evaluate to `1` (the last one) without incident.
    let result = evaluator.evaluate(&ast, &data).unwrap();
    assert_eq!(result, JValue::from(1i64));
}
```
Run it first (`cargo test --test integration_test test_large_block -- --nocapture`) to see whether it currently fails, hangs, or passes-by-luck (a block's `BlockEnd` truncation might not be observable via a "does it return the right VALUE" test the way array/object length is, since the bug is about how many stack values get **kept** — check `Instr::BlockEnd`'s actual runtime behavior in `vm.rs` at `n as usize` to determine what a truncated `n` would actually do observably, and adjust this test to actually detect it, e.g. by making each expression in the block produce a DIFFERENT distinguishable value and asserting the last one specifically survives).

- [ ] **Step 3: Implement the `AstNode::Block` guard**

Following the exact same pattern as Task 4, add a `if exprs.len() > u16::MAX as usize { return None; }` guard to `try_compile_expr_inner`'s `AstNode::Block` arm before it attempts to compile.

- [ ] **Step 4: Implement the pool-interning / `CallBuiltin` arg-count guards per Step 1's findings**

Apply whichever approach Step 1 determined was less invasive. Add at minimum a defensive check for `CallBuiltin`'s `arg_count: args.len() as u8` — confirm via the compilable-builtins' `max_args` validation (referenced in project memory/comments near `COMPILABLE_BUILTINS`) whether this is genuinely unreachable; if it's already provably bounded below 256 by existing validation, note that in your report instead of adding a redundant guard, but verify this claim by reading the actual max_args table rather than trusting the memory note blindly.

- [ ] **Step 5: Run tests, verify no regressions**

Run: `cargo test --all-features` (full suite green), `uv run pytest tests/python/test_reference_suite.py -q` (1682/1682, rebuild first).

- [ ] **Step 6: Commit**

```bash
git add src/evaluator.rs src/compiler.rs tests/integration_test.rs
git commit -m "fix(compiler): close remaining u16/u8 truncation sites (BlockEnd, CallBuiltin arg count, pool interning)"
```

---

## Task 6: Final verification and design-spec update

**Files:**
- Modify: `docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md` (the "Two pre-existing bugs" paragraph in the Status section)

**Interfaces:** None — verification and documentation only.

- [ ] **Step 1: Full verification**

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
env -u VIRTUAL_ENV maturin develop --release --uv
uv run pytest tests/python/ -q
ruff check python/ tests/
mypy python/
```
All must be clean. Reference suite must stay 1682/1682.

- [ ] **Step 2: Update the design spec**

Current (`docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md`, the "Two pre-existing bugs" paragraph):
```markdown
Two pre-existing bugs were found along the way. Both are **out of scope for this plan and were
NOT fixed** — they're recorded here for future work, not claimed as resolved:
- The parser has no recursion-depth guard and can crash the whole process (SIGSEGV) on deeply
  left-nested arithmetic expressions (roughly 4000+ terms) — independent of, and prior to, the
  evaluator's own `D1011`/`U1001` guards, which never get a chance to run since the crash happens
  during parsing, before evaluation starts.
- `Instr::MakeArray(u16)` (`src/compiler.rs`) silently produces **wrong, truncated** results — not
  even an error — for JSONata array literals with more than 65,536 elements, because the element
  count is cast to `u16` and overflows silently. This is more severe than the parser crash (silent
  data corruption vs. a loud failure) and should be prioritized in any follow-up. Fixing this would
  also unblock adding a literal-array `D2015` check (`MakeArray`/`ArrayConstruct` are currently
  unguarded — see the "Sequence length → D2015" section below) for full upstream parity, since a
  length cap can't safely be layered on top of a length computation that already overflows.
```
Replace with a summary reflecting the actual fix (fill in the real commit range/PR number once available):
```markdown
Two pre-existing bugs found during Phase 2 are now **fixed**, per
`docs/superpowers/plans/2026-07-07-parser-depth-and-u16-truncation-fixes-plan.md` — and a third,
previously-unknown crash surfaced and was closed along the way:
- The originally-reported crash was root-caused to `src/ast_transform.rs`'s post-parse AST-rebuild
  pass for flat left-nested chains specifically (NOT the Pratt parser's own recursion for that
  shape, which stays O(1) — the original framing above was imprecise for that one case). But
  validating that fix surfaced a **second, more directly reachable parser crash**: deeply
  parenthesized/grouped expressions (`(((...)))`) recurse for real inside `parse_primary`/
  `parse_expression` and crash the raw parser itself, independent of `ast_transform.rs`. Both are
  now closed by a single depth guard added directly in `parser.rs`'s `parse_expression` (bounding
  both loop-driven left-nesting and genuine recursive descent with one shared counter), turning
  either shape into a graceful `U1002` coded error instead of a crash — before `ast_transform.rs`
  ever sees the tree. `ast_transform.rs`'s own depth guard (added first, during this same plan)
  remains in place as defense-in-depth but should no longer be reachable via the normal
  `parser::parse` entry point.
- `Instr::MakeArray`/`MakeObject`/`BlockEnd`'s `u16` operands (and `CallBuiltin`'s `u8` arg count,
  and four pool-interning `u16` casts) are now guarded at compile time: oversized constructs fall
  back to the tree-walker (which has no such limit) instead of silently truncating. This also
  unblocks a follow-up literal-array `D2015` check for full upstream parity, since a length cap can
  now safely be layered on top of a length computation that no longer overflows.
```

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md
git commit -m "docs: mark parser depth-guard and compiler truncation bugs as fixed"
```
