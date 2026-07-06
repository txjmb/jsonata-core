# `%` (parent reference) and `@` (focus binding) operators

## Context

This is the deferred Phase 3+4 of
[`docs/superpowers/specs/2026-07-05-reference-suite-coverage-gap-design.md`](2026-07-05-reference-suite-coverage-gap-design.md).
That spec's original estimate treated `%` (28 cases, `parent-operator` group) and `@`
(37 cases, `joins` group) as bounded bug fixes ("some expression shapes don't parse",
"a wrapper object leaks into output"). A pre-implementation investigation (recorded in
memory as `project_parent_and_at_operators.md`) found instead that **both are completely
unimplemented at the lexer/AST level**: `@` causes an immediate lexer error, `%` only ever
tokenizes as binary modulo, and there is no post-parse AST transformation pass of any kind
in this parser (unlike jsonata-js's `processAST`). That finding is why this was split out
into its own combined effort rather than bundled into Phases 0-2 (which shipped as the
`2.1.6` release prep).

Phases 0-2 (reference-suite loader fix, datetime picture-string engine,
`$formatInteger`/`$parseInteger`) are merged to `main`. The `2.1.6` release itself is on
hold at the user's request until this effort (and Phase 5's remaining stragglers) also
land — see Release Line below.

**Reference implementation:** `tests/jsonata-js/src/parser.js` (prefix/infix rules ~L616-847;
the post-parse pass `processAST`/`seekParent`/`pushAncestry`/`resolveAncestry` ~L937-1235)
and `tests/jsonata-js/src/jsonata.js` (`evaluate`'s `case 'parent'` ~L83-85;
`evaluateTupleStep`/`createFrameFromTuple` ~L229-380). All line numbers confirmed by direct
reading during this design's brainstorming session, not secondhand summary.

## Scope

In scope:
- `%` (parent-reference operator), including chained `%.%` (two-or-more levels up),
  working inside plain paths, object-constructor group-by clauses, array constructors,
  predicates/filters, and sort terms.
- `@$var` (focus/positional binding), including chained joins (`Account.Order@$o.Product`)
  and the `S0214`/`S0215`/`S0216` parse-time error cases in `joins/errors.json`.
- Fixing the existing `#$var` (index-binding) tuple-propagation gap as part of the same
  runtime mechanism unification (decided in this design; see "Decisions" below) — `#`'s
  current passing tests must continue to pass unchanged.
- The single unwrap-at-output-boundary fix that closes the "tuple wrapper leaks into
  user-visible output" bug class (latent today for `#`, would otherwise be newly reachable
  for `@`/`%`).

Out of scope:
- Phase 5 (`array-constructor`, `function-distinct`, `flattening` stragglers) — separate,
  untriaged, tracked in the parent spec.
- Any VM/bytecode-compiler support for `%`/`@`/`#` — these fall back to the tree-walker
  only, matching how Phases 1-2 handled uncompilable constructs (e.g. named-variable
  lookups already force tree-walker fallback; see `MEMORY.md`'s Phase 2 compiler notes).

## Decisions (from brainstorming Q&A)

1. **Fix the `#` propagation gap as part of this work**, rather than building `%`/`@` on
   top of a known-buggy mechanism and leaving `#` as-is. The investigation found tuple
   bindings currently propagate through the `data` value itself rather than the real
   variable-scope stack (`self.context`), and only get promoted into `self.context` for
   three specific next-step node types (`Object`/`FunctionApplication`/`Variable`).
2. **Keep the existing `JValue::Object` tuple-wrapper convention** (`__tuple__`/`@`/`$name`
   reserved keys), rather than replacing it with a dedicated non-`JValue` Rust-native
   context structure. This directly mirrors jsonata-js's own design — it also represents
   tuple bindings as a plain object flowing through the pipeline (`{'@': item, ...}`) *and*
   builds a real scope frame from that same object (`createFrameFromTuple`) before
   evaluating each step. Chosen over a bigger refactor (threading a new parameter through
   dozens of evaluator functions) because it's both lower-risk and more faithful to the
   reference implementation.
3. **Aim for full completion of all 69 currently-xfailed cases** (28 `parent-operator` +
   37 `joins` + 4 Phase-5 stragglers are explicitly *not* included in that 69 — Phase 5
   stays separate scope) as the bar for this effort, not partial/best-effort coverage.

## Design

### 1. AST & parser changes

- New leaf node `AstNode::Parent(AncestorSlot)` for `%`, where `AncestorSlot` holds a
  synthetic label (`!0`, `!1`, ...) assigned during the post-parse pass (Section 2) — the
  node itself is inert at parse time, matching jsonata-js's trivial one-line prefix rule.
- **Step-level flags, not wrapping nodes.** `#$i`, `@$v`, and `%`'s ancestor-capture are
  all represented in jsonata-js as flags on the path step itself (`step.index`,
  `step.focus`, `step.ancestor`, all alongside `step.tuple = true`) — not as nodes that
  wrap what they bind. This port adds `focus: Option<String>`, `index_var: Option<String>`,
  `ancestor_label: Option<String>`, and `is_tuple: bool` fields directly on `PathStep`, and
  **retires `AstNode::IndexBind`** in favor of setting `index_var` on the step it applies
  to. This is the most invasive part of the design (it touches `#`'s existing, currently
  passing representation) but is necessary: if `#` keeps a special wrapping node while
  `@`/`%` use step-level flags, the three can't share one unified tuple mechanism, and
  Decision 1's propagation fix can't land as a single change.
- Parser additions: `%` gets a one-line prefix rule (same shape as the existing
  `Token::Star => AstNode::Wildcard` arm in `parse_primary`); `@` gets a one-line infix
  rule validating its RHS is a bare variable reference (else `S0214`). Both produce
  bare/unresolved nodes at this stage — all the real semantic work happens in Section 2,
  matching jsonata-js's own two-pass split.

### 2. Compile-time ancestor-resolution pass

A new pass (new module, e.g. `src/ast_transform.rs`) runs on the freshly-parsed tree
before it reaches the evaluator. It is a faithful port of jsonata-js's
`seekParent`/`pushAncestry`/`resolveAncestry`, adapted to Rust's ownership model: instead
of mutating tree nodes in place (as the JS does), it **consumes the raw tree and rebuilds
an enriched one** with Section 1's new fields filled in — same algorithm, immutable
reconstruction instead of mutate-in-place, the natural Rust adaptation rather than a
design compromise.

For each path, walking backward from the last step: when a `%` (`AstNode::Parent`) is
found, locate the step `level` positions back (chained `%.%` increments `level`) and stamp
*that* step with a synthetic `ancestor_label` and `is_tuple = true` — walking through
parenthesized `AstNode::Block`s (into their last expression, per the
`parent000`-`parent006` test shapes, which deliberately wrap different path segments in
parens to stress exactly this), through predicates and sort terms (mirroring
`pushAncestry`'s predicate/sort-term entry points), and reusing an existing label when the
same step already got one from an earlier `%` reference in the same expression (so
`%.OrderID` and `%.%.\`Account Name\`` sharing an ancestor step share one label, not two).

Errors surface here: `S0217` ("can't derive ancestor") when `%` walks off the front of a
path into something that isn't a name/wildcard/block/path. Note the
`Account.Order.().%` test case (`"undefinedResult": true`, not an error) needs
verification during implementation of exactly where jsonata-js draws this line — `()` is a
syntactically valid empty step, so the ancestor slot likely still resolves at compile time,
just evaluates to undefined at runtime because the slot's bound value was never populated
by an empty step. Confirm against the reference implementation rather than assuming.

### 3. Runtime tuple-binding unification

After Section 1's `IndexBind` migration, a step becomes tuple-producing whenever
`PathStep.is_tuple` is set — by `@`, `#`, *or* a downstream `%` (Section 2) — so a plain
`Account.Order.Product` step with no visible binding syntax can still enter the tuple path
purely because something later needs its ancestor value, exactly matching jsonata-js.

The wrapper stays the existing `JValue::Object` with `__tuple__`/`@`/`$name` keys
(Decision 2), gaining one more reserved-key convention for ancestor labels (`!0`, `!1`,
...). The fix (Decision 1): every tuple step unconditionally builds a real scope frame
from the current tuple dict before evaluating that step's sub-expression — mirroring
`createFrameFromTuple`, called unconditionally in jsonata-js — replacing the current
partial promotion that only covers three next-step node types. `%` then resolves via
ordinary scope lookup of its synthetic label; no special-casing needed at the lookup site
beyond what `$name` resolution already does.

The "leaks into output" bug closes as a side effect of touching every tuple-step call
site: add a single unwrap-at-boundary check (top of `Evaluator::evaluate()` and/or the
`json_to_python` boundary in `src/lib.rs`) that strips a lingering `__tuple__` wrapper
before it reaches the caller.

### 4. Error handling

Four parse/compile-time error codes must match jsonata-js exactly (`joins/errors.json`
asserts on them directly):

| Code | Condition |
|---|---|
| S0214 | `@`'s RHS isn't a bare variable reference |
| S0215 | `@` applied to a step that already has predicates/stages defined |
| S0216 | `@` applied after a sort (`^(...)`) clause |
| S0217 | `%` can't derive an ancestor |

These map onto a coded error variant the same way Phase 1's `D3130`-`D3136` datetime codes
did — message starts with the code, so `test_reference_suite.py`'s `extract_error_code`
picks it up via its `^([TDUS]\d{4}):` prefix match.

### 5. Testing plan

Following the Phase 1/2 pattern: a new cargo integration test file (e.g.
`tests/parent_and_focus_binding_suite.rs`, sibling to `tests/datetime_picture_suite.rs`)
runs every case in `parent-operator/parent.json` and every file under `joins/` through the
full parser+evaluator pipeline, for fast (`cargo test`, seconds) iteration ahead of the
slower `maturin develop` + `pytest` cycle. Given the "full completion" bar (Decision 3),
also add targeted Rust unit tests for the ancestor-resolution pass itself (e.g. asserting
exact label/level bookkeeping for a `%.%` chain through a nested block) — that pass is
intricate enough to want coverage below the end-to-end/black-box level.

## Definition of done

- All 28 `parent-operator` + 37 `joins` xfail entries removed from
  `tests/python/test_reference_suite.py` and passing for real.
- `#$var` (index binding) continues to pass all its existing tests unchanged.
- No wrapper-leak regressions: a bare tuple-producing expression (any of `%`/`@`/`#`) at
  the top level of an expression does not leak `__tuple__`/`@`/`$name`/`!N` keys into
  Python-visible output.
- `cargo test` (including the new integration suite + new unit tests), `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings` all clean.
- `uv run pytest tests/python/test_reference_suite.py` shows only the 4 Phase-5-straggler
  xfails remaining (69 → 4), full suite has no new failures elsewhere.
- README.md / docs/index.md pass-count claims updated to match.

## Release line

Per the user's decision earlier in this session, the `2.1.6` release (Phases 0-2, already
merged and CI-green) is held until this effort and Phase 5 both land, then ships together
in one release rather than as separate point releases. If Phase 5 turns out to be
significantly delayed relative to this effort once both are scoped, revisit whether to
split them into `2.1.6`/`2.1.7` instead — not decided now, since Phase 5 hasn't been
triaged yet.
