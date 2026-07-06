# Reference-suite coverage gap: loader bug hiding 408 test cases

## Context

The README and `docs/index.md` claim `1258/1258` JSONata reference tests passing ("Full
JSONata 2.1.0 compatibility"). That number is stale (the suite currently runs `1274/1274`) and,
more importantly, **it has never been the full reference suite**. `load_test_cases()` in
`tests/python/test_reference_suite.py` only globs `group_dir.glob("case*.json")`. The reference
suite (submodule `tests/jsonata-js`, currently pinned at `v2.2.1`, commit `06fc08c`) also ships
test-spec files under other names — descriptive names (`parent.json`, `formatInteger.json`,
`employee-map-reduce.json`), `issueNNN.json` files, etc. — none of which match `case*.json` and
so have **never been loaded or run**, going back to whenever this harness was first written. This
predates and is unrelated to the 2.2.0/2.2.1 version bump that prompted this investigation.

Count: **20 files / 408 test cases** (some files are JSON arrays of multiple cases) are silently
skipped. Total real reference-suite size is `1274 + 408 = 1682` cases; current true coverage is
`1274/1682 = 75.7%`.

Running all 408 previously-skipped cases directly against jsonatapy (ad hoc script, not yet
committed — see Phase 0) gives **83/408 passing (20%), 325 failing**. This is the actual finding:
not "we're missing a few 2.2.0 cases" but "~325 real reference-suite cases have been silently
failing this whole time with nobody noticing," including two categories of outright crash (Rust
panics), a completely unimplemented function pair, and an unimplemented operator.

### Is this about the 2.2.0/2.2.1 bump specifically?

No — checked and ruled out. `git diff v2.1.0 v2.2.1 -- test/test-suite/` (inside the submodule)
shows 19 new files, all added to already-existing group directories, no new categories. 17 of the
19 use `case*.json` naming and are already included in the current (passing) 1274. The other 2
(`function-formatNumber/issue785.json`, `issue786.json`) fall into the same pre-existing glob bug
— but both pass cleanly when run manually, so the 2.2.x-specific additions are not the problem.
**All 20 never-globbed files, and all 325 real failures, predate 2.1.0** (verified via
`git log --follow --diff-filter=A` inside the submodule — the oldest of these files date to
2018–2019).

### Failure breakdown (325 failures, by group)

Of the 408 previously-skipped cases, 31 already pass cleanly and aren't shown below (the 2
`function-formatNumber` issue files, all 19 cases in `comparison-operators/deep-equals.json`, and
all 4 cases in `literals/array-inputs.json`). The remaining 377 cases, across the 9 groups below,
include both the 325 failures and 52 more passing cases mixed in with them:

| Group | Failing / Total | Root cause |
|---|---|---|
| `function-fromMillis` | 87 / 90 | 68× `$fromMillis(millis, picture)` 2-arg form rejected outright by an arity check (`"fromMillis() requires exactly 1 argument"` — the picture-string formatting form was never implemented); 19× Rust panic (see below) |
| `function-formatInteger` | 63 / 65 | `$formatInteger` is **entirely unimplemented** — "Unknown function: formatInteger" on every case. It's listed as a recognized builtin name at `src/evaluator.rs:8989` but has no dispatch arm anywhere in `evaluator.rs`. |
| `function-parseInteger` | 60 / 61 | `$parseInteger` is **entirely unimplemented** — same pattern, "Unknown function: parseInteger", same line 8989 listing with no dispatch arm. |
| `function-tomillis` | 46 / 47 | 30× `"Parse error: input contains invalid characters"` (picture-string date parsing rejects valid inputs); 13× Rust panic (see below); 2× wrong numeric result; 1× wrong undefined/0 handling |
| `joins` | 37 / 43 | `@` positional-binding/tuple-stream operator: some expression shapes fail to parse (`"Unexpected token: @"`, e.g. `Employee@$e.(Contact)[...]`); others parse but **leak the internal tuple-wrapper object** into the final result instead of collapsing it (e.g. expected `[3, 1, 4]`, got `[{'@': 3, '$pos': 0, '__tuple__': True}, ...]`) |
| `parent-operator` | 28 / 44 | `%` parent-reference operator (e.g. `%.OrderID`) — lexer only ever treats `Token::Percent` as binary modulo (`src/parser.rs:602-604`, `712`, `1543`); there is no prefix/primary-position production that turns a leading `%` into a parent-context reference, so every such expression fails to parse |
| `array-constructor` | 2 / 5 | not yet triaged |
| `function-distinct` | 1 / 8 | not yet triaged |
| `flattening` | 1 / 14 | not yet triaged |

**Rust panics** (both in `src/datetime.rs::parse_with_components`, ~line 291): `chars[pos..end]`
is sliced without checking `pos <= chars.len()` first. When a picture string has more components
than the input string has characters for, `pos` can advance past `chars.len()`, `end` gets
clamped to `chars.len()` by the `.min()`, and `pos > end` panics the slice
(`"slice index starts at 4 but ends at 3"` and similar). This is a real crash (PyO3 converts it to
a Python `PanicException`, which is **not** a subclass of `Exception` — it only inherits
`BaseException`, so naive `except Exception` handling in calling code won't catch it) reachable
from ordinary reference-suite input, not a contrived edge case.

## Scope

This is its own multi-phase effort, separate from any other in-flight work. Each phase is its own
branch/PR. Do this in a fresh session — the investigation above is already captured here, so the
new session shouldn't need to re-derive it, just verify and build on it.

- **Phase 0 — stop the bleeding.** Fix the loader (`tests/python/test_reference_suite.py`) to
  discover all 20 non-`case*.json` files (including `expr-file` support, which the runner already
  has — only the glob is the problem). Do **not** land this alone with 325 new red tests: either
  fix the quick/contained bugs in the same PR (the two panics, at minimum — small, well-scoped)
  and `xfail` the rest with tracking references to Phases 1-3 below, or land the loader fix and
  the xfails together as one atomic change. Also correct the false "1258/1258 full compliance" /
  "Full JSONata 2.1.0 compatibility" claims in `README.md` (lines ~105, ~163) and `docs/index.md`
  (line ~57) to state the real, current numbers — this doesn't need to wait for later phases.
- **Phase 1 — datetime picture-string engine.** Fix the two panics (bounds check in
  `parse_with_components`) and the `$fromMillis`/`$toMillis` picture-string gaps (2/3-arg
  `fromMillis`, and the `$toMillis` "invalid characters" parse failures). Given the size (87 + 46
  = 133 cases, the single largest chunk), triage each unique failing case individually — some of
  the `$toMillis` mismatches may be genuine spec edge cases (e.g. component-order/BCE-year
  handling) rather than one shared bug.

  **Done (2026-07-05):** ported jsonata-js's `datetime.js` picture-string engine wholesale
  into `src/datetime.rs` (shared integer-format machinery + date/time component analysis,
  formatting, and parsing). 127 of the 133 candidate cases were real (87 fromMillis + 40
  tomillis; 6 turned out to already pass pre-Phase-1 and weren't xfailed). 1490/1682 passing,
  192 xfailed remaining.

  **Known limitation (not fixed, intentionally out of scope):** jsonata-js pins a single
  `environment.timestamp` per top-level `evaluate()` call so that `$now()` and a picture
  string's "default unspecified date/time parts to now" behavior see the same instant. This
  Rust port does not pin anything — `$now()` and the parse-time "now" default each call
  `Utc::now()` independently. The one reference-suite case that depends on this
  (`function-tomillis/parseDateTime`, "time only defaults to todays date") compares only the
  *date* portion of two independent `Utc::now()` reads, so it can only flake across the
  midnight-UTC boundary — reproducible only by chance, not by any input in this repo. If a
  CI run ever fails this specific case with no code change nearby, this is why; fixing it
  would mean threading a fixed "now" timestamp through `Evaluator` (a cross-cutting change,
  not scoped to this effort).
- **Phase 2 — `$formatInteger`/`$parseInteger`.** Implement both (they're inverses and likely
  share a picture-string mini-language, per jsonata-js's `functions.js`/spec: roman numerals,
  ordinal letters, grouping separators, etc.). 123 cases combined.
- **Phase 3 — `%` parent operator.** Add prefix/primary-position parsing for `Token::Percent` so
  it produces a parent-context-reference AST node (distinct from its existing infix-modulo use),
  and wire evaluator support for resolving it against the enclosing path context. 28 cases.
- **Phase 4 — `@` binding / tuple-stream correctness.** Two sub-issues to separate: (a) parser
  gaps where certain `@`-expression shapes don't parse at all, vs (b) an evaluator bug where
  internal tuple-wrapper objects (`{"@": ..., "$pos": ..., "__tuple__": true}`) leak into
  user-visible output instead of being unwrapped — compare against jsonata-js's `parser.js`/
  `jsonata.js` tuple-stream handling (`evaluateStep`/sort/group code) to find where the existing,
  fairly extensive tuple-handling code in `evaluator.rs` (grep `"@"` — dozens of sites) misses the
  unwrap step. 37 cases.

  **Correction (2026-07-05, before starting Phase 3):** the above two paragraphs understate the
  work by roughly an order of magnitude. A pre-implementation investigation (grep-verified)
  found that **both features are completely unimplemented at the lexer/AST level**, not
  "partially working with some gaps":
  - `@` is not tokenized at all — the lexer has no `'@'` arm and hits the catch-all
    `UnexpectedToken` error on any input containing it. There is no `AstNode` variant for
    focus/positional binding (the only existing binding node is `AstNode::IndexBind` for `#$var`).
  - `%` only ever tokenizes as binary modulo (confirmed at `src/parser.rs:602-604`, `712`,
    `1543` as the original spec said) — there is no prefix-position handling anywhere in
    `parse_primary`, and no `AstNode::Parent`/ancestor-slot concept exists.
  - Our parser is a single-pass Pratt/recursive-descent parser with **no post-parse AST
    transformation pass** analogous to jsonata-js's `processAST`/`seekParent`/`resolveAncestry`
    (`parser.js:937-1030`) — that machinery (compile-time "ancestor slot" assignment walking
    back through path steps, blocks, predicates, and sort terms) would need to be built from
    scratch, not adapted from an existing pass.
  - The runtime tuple-wrapper convention that DOES exist (`__tuple__`/`@`/`$name` keys on an
    `IndexMap`-backed `JValue::Object`, ~20 call sites in `evaluator.rs`) currently backs only
    `#$var` (index binding). It propagates bindings by re-threading extra keys through the
    `data` value itself rather than the real variable-scope stack (`self.context`), only
    partially promotes them into `self.context` for three next-step node types, and has no
    single "unwrap tuple wrapper before returning to caller" choke point — several of these are
    themselves latent correctness gaps (e.g. a bare tuple-producing expression with no further
    field access can leak the wrapper object straight into Python-visible output today, for the
    one binding operator — `#` — that already works).
  - `%` cannot simply piggyback on this wrapper either: the parent-operator test suite requires
    `%` to work in plain paths with no `@`/`#` present at all (e.g. `Account.Order.Product.{
    'Order': %.OrderID }` has no tuple binding anywhere), so `%` needs its own ancestor-tracking
    side channel independent of the `@`/`#` wrapper, while also needing to compose with it for
    the handful of test cases that use both together (`library.loans@$L.books@$B[...].{ ...,
    'parent': $keys(%) }`).

  **Decision:** given this is a ground-up, two-feature addition sharing fragile core-parser/
  evaluator territory (flagged in project memory as a known-fragile area), rather than a
  bounded bug fix, Phases 3 and 4 are combined into one future effort and deferred to their own
  dedicated session/branch/PR — consistent with this spec's own "each phase is its own
  branch/PR" rule, which anticipated needing to split out exactly this kind of unexpectedly
  large phase. Phases 0-2 ship now as their own release; Phases 3+4 (combined) and 5 remain
  open follow-up work with no landing date attached.
- **Phase 5 — remaining stragglers.** `array-constructor` (2), `function-distinct` (1),
  `flattening` (1) — **DONE (2026-07-06)**. Three distinct root causes, all in
  `src/evaluator.rs`: (1) the array-constructor step (`.[a,b]`) didn't implement
  jsonata-js's `evaluateStep` special case where, when it's the path's last step and
  maps over exactly one input item, the constructed sub-array becomes the whole path
  result directly rather than being wrapped in an extra outer array (fixed by tracking
  `is_last_step` in the path-step loop); (2) the compiled/VM fast path
  (`try_compile_path`) folded the explicit `[]` keep-array marker
  (`Predicate(Boolean(true))`) into an ordinary boolean filter, discarding its
  keep-singleton semantics that the tree-walker's `evaluate_predicate` already handled
  correctly — fixed by bailing out to the tree-walker for that marker, matching the
  existing numeric-predicate bailout; (3) `$distinct` threw on non-array input instead
  of jsonata-js's `functions.js` behavior of returning non-array (and length-<=1 array)
  input unchanged. All three verified against real jsonata-js and fixed at both
  builtin-dispatch call sites (compiled and tree-walker). 1682/1682 passing, zero
  xfails remain in the suite.

Out of scope: the jsonata-js 2.2.1 signature-engine spec's Phase 2
(`docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md`) — porting jsonata-js's new
`timeout`/`stack`/`sequence` guardrail *options*. That spec's Phase 1 (submodule bump to v2.2.1,
signature `+` support) is already merged (`bbce6e7`); Phase 2 is **deliberately deferred** —
decided 2026-07-05: since it's a pure API addition and the only reason a `2.2.0` jsonatapy release
would ever make sense, it's on hold indefinitely in favor of this effort, and this effort does not
depend on it landing either way (or ever).

### Release line

None of these bugs are 2.2-specific — every one of them (missing `$parseInteger`, the datetime
panics, etc.) has been present and silently unexercised in every published jsonatapy version,
including the current `2.1.5` on PyPI/crates.io. **Decided:** these fixes ship as ordinary `2.1.x`
patch releases (`2.1.6`, `2.1.7`, ...) as each phase below lands — not held for a `2.2.0` cut,
which is deferred (see above) and unrelated regardless.

## Verification

- After Phase 0: `pytest tests/python/test_reference_suite.py` collects `1682` cases total (not
  `1274`); passing count + xfail count accounts for all of them (no silently-dropped cases, no
  unmarked failures).
- After each subsequent phase: the corresponding xfail markers are removed and those cases move
  to genuinely passing; `cargo test` and `cargo clippy -- -D warnings` stay clean.
- Final definition of done: `1682/1682` passing, zero xfails left from this effort, and
  README/docs/performance claims match the real, current total.

## Reproduction

`tests/python/check_skipped_reference_cases.py` (committed alongside this spec) loads every
non-`case*.json` file under `tests/jsonata-js/test/test-suite/groups/`, runs each spec the same
way `test_reference_suite.py` does (including `expr-file` and `dataset` resolution), and catches
`BaseException` (not just `Exception`) so a `PanicException` doesn't kill the whole sweep. Run it
directly: `python tests/python/check_skipped_reference_cases.py`. It is not a pytest file (no
`test_` prefix, not collected by CI) — it's a scratch tool for this investigation. Re-run it
before starting Phase 0 to confirm the 83/408 baseline still holds, since the codebase will have
moved on since this spec was written; once the loader is fixed and cases are folded into the real
suite (with `xfail` markers for what's not yet fixed), this script has served its purpose and can
be deleted.
