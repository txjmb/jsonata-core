# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed
- Every builtin that needs only its arguments is now implemented once, in `src/builtins.rs`,
  and shared by the compiled path and the tree-walker instead of being written out in each.
  Fifty-three builtins were spread across two dispatch sites: twenty-nine were implemented
  twice, and twenty-four existed in exactly one, which is why `$type(x)` worked while
  `$map(arr, $type)` raised. The differential corpus and the 1686-case reference suite
  confirm the extraction preserves *value* and *error* behaviour for every builtin other
  than the six listed under Fixed below. Both harnesses collapse `null` and `undefined` to
  Python `None`, so that axis is confirmed only where the corpus's new object-construction
  probes cover it directly (an undefined-valued key is dropped, a null-valued one is kept).
  `$string()` against an explicit-null context is a further route disagreement at the
  pre-branch baseline: the compiled path answered `undefined`, the tree-walker answered
  `null`. The shared dispatcher answers `null` on both. `evaluate_function_call` drops from
  2688 lines to 1266.
  ([#107](https://github.com/txjmb/jsonata-core/issues/107))

### Deprecated

### Removed

### Fixed
- Six builtins — `$base64encode`, `$base64decode`, `$toMillis`, `$fromMillis`,
  `$formatInteger` and `$parseInteger` — now validate their arguments. They had no entry in
  the builtin signature table, and a missing entry is not a weaker check but no check at
  all: validation returns early for a name it does not know, so these six fell back to
  hand-rolled arity and type guards. An explicit null returned `null` where jsonata-js
  raises `T0410` (`$base64encode(null)`), the `-` context marker never fired
  (`str.$base64encode()` was `null`, not `"YQ=="`), `$formatInteger`'s context form was
  rejected as an arity error, and `$base64encode`/`$base64decode` raised a type error on a
  missing argument instead of propagating `undefined`. The table had drifted to a strict
  *subset* of jsonata-js's own — 55 of its 63 entries, every one byte-identical, with eight
  simply absent — which is why nothing caught it; a new test compares the two tables in
  both directions against a generated fixture, so a submodule bump that adds or changes a
  signature now fails CI. The two entries still absent are deliberate and named in that
  test: `$clone` is not implemented here at all, and `$eval` needs the evaluator.
  ([#126](https://github.com/txjmb/jsonata-core/issues/126))
- `$sum`, `$max`, `$min` and `$average` over a path of the form `array.field` no longer
  silently skip non-numeric values. The fused aggregate fast path
  (`Evaluator::try_fused_aggregate`) reimplemented aggregate semantics rather than
  delegating to them, and treated a present non-numeric field the same as an absent one.
  With `{"orders": [{"p": 1}, {"p": "free"}]}`, `$sum(orders.p)` returned `1` instead of
  raising `T0412` — a plausible but wrong number rather than an error. The same path also
  returned `0`/`null` for an empty sequence where jsonata-js returns `undefined`. The fast
  path now declines when its assumptions do not hold, so the canonical aggregate produces
  both the error and the empty-sequence semantics. ([#97](https://github.com/txjmb/jsonata-core/issues/97))
- Builtin argument handling now matches jsonata-js for a missing argument in a *required*
  slot. The reference validates a call and then hands the arguments to the function body
  unchanged, so an undefined that the signature admitted reaches a JavaScript expression
  and JavaScript's coercion supplies the answer: `$substring("abcdef", missing.x)` is
  `"abcdef"` (and `$substring("abcdef", missing.x, 2)` is `""`, because the undefined start
  makes the end `NaN`), `$pad("a", missing.x)` is `"a"`, and `$substringBefore` and
  `$lookup` stringify the missing argument to the literal `"undefined"` rather than
  treating it as absent. `$trim(missing.x)` propagates undefined instead of raising.
- `$substring`, `$substringBefore` and `$substringAfter` given a single missing argument
  now raise `T0411` on both engines. The compiled path applied its undefined-propagation
  shortcut before signature validation; jsonata-js validates first, and for these three
  the lone undefined argument binds to parameter 2 while parameter 1 comes from the
  context. The tree-walker never had the shortcut, so the two engines disagreed.
- `$spread` and `$each` now follow jsonata-js's sequence rules: `$spread({"k": 1})` is the
  object rather than `[{"k": 1}]`, and `$spread([])`, `$spread({})` and `$each({}, fn)` are
  `undefined` rather than empty containers. `$spread`'s *array* branch is deliberately
  exempt — the reference folds it with `concat`, which drops the sequence flag — so
  `$spread([{"k": 1}])` stays wrapped. `$each` also no longer drops explicit nulls from its
  results.
- `$sift(obj)` now raises instead of returning `undefined`. The one-argument form is
  `$sift(function)`, with the object taken from the context.
  ([#104](https://github.com/txjmb/jsonata-core/issues/104))
- `$trim`, `$merge`, `$reverse`, `$distinct`, `$join` and `$keys` given a missing argument
  now yield `undefined` rather than `null`, matching jsonata-js. This was visible through
  object construction, which drops an undefined-valued key but keeps a null-valued one:
  `{"k": $trim(missing)}` was `{"k": null}` and is now `{}`. For the first five, the two
  evaluation routes previously disagreed — the compiled/VM path returned `null` while the
  tree-walker returned `undefined` — and the shared dispatcher makes them agree. `$keys`
  was wrong on both routes; the tree-walker arm had no `Undefined` case at all, and the new
  shared dispatcher's corpus is what exposed it.
  ([#107](https://github.com/txjmb/jsonata-core/issues/107))

### Security

## [2.2.7] - 2026-07-21

### Added

### Changed
- Dependabot no longer auto-bumps the `tests/jsonata-js` reference submodule (removed the
  `gitsubmodule` ecosystem entry). Reference-suite updates are handled by the
  `sync-jsonata.yml` workflow, which runs the conformance suite against each new jsonata-js
  release and opens a tracking issue (or a clean PR) — avoiding context-free failing bump
  PRs like #82.

### Deprecated

### Removed

### Fixed
- Ensures compliance with the **jsonata-js 2.2.2** reference test suite (reference submodule
  bumped to `6c7e95f`); the full reference suite — 1686 cases — passes. Three behavior
  changes were required to match jsonata-js 2.2.2:
  - `$contains(str, token)` now returns `undefined` when either argument is undefined,
    instead of raising a type error (jsonata-js #809).
  - `$each(obj, fn)` now returns `undefined` when its first argument is undefined, instead
    of raising `each() first argument must be an object`.
  - An object constructor (group-by) applied to an empty or undefined sequence now yields
    an empty object `{}` instead of `undefined` (jsonata-js #817, "correctly handle empty
    joins"); `null` input still returns `null` and non-empty grouping is unchanged.

### Security

## [2.2.6] - 2026-07-20

### Added
- Host-callable custom functions (Rust core): `Evaluator::register_fn` and
  `Evaluator::register_fn_override` let a host register native functions callable
  from an expression as `$name(...)` — the equivalent of jsonata-js's
  `registerFunction`. Functions are plain closures
  (`Fn(&[JValue]) -> Result<JValue, EvaluatorError>`) and resolve after the
  expression's own bindings/lambdas and before built-ins. `register_fn` rejects
  collisions with built-ins; `register_fn_override` allows deliberately replacing
  the impure built-ins (`$now`, `$millis`, `$random`, `$eval`) for determinism
  injection or sandboxing. Evaluation stays synchronous. See
  `examples/host_functions.rs` and the Rust crate docs.
- Host-callable custom functions (Python binding): `JsonataExpression.register(name,
  func)` and `.register_override(name, func)` expose the above to Python. The callable
  receives already-evaluated positional arguments and must return a JSON-compatible
  value synchronously; an `async def` (coroutine) is rejected at call time with
  guidance to await I/O outside jsonata and pass results via `bindings`. Collision and
  compilable-builtin-override rules are validated at `register()` time.
- Host-callable custom functions (C ABI): `jsonata_register_fn(expr, name, fn, user_data)`
  and `jsonata_register_fn_override(...)` expose the feature to C and any language with C
  interop. The callback receives its arguments as a JSON array string and returns a JSON
  result string (jsonata copies it; the host retains ownership), or NULL to signal an error.
  See `bindings/c/jsonata.h`, `bindings/c/README.md`, and `bindings/c/examples/smoke.c`.

### Changed

### Deprecated

### Removed

### Fixed
- Path expressions no longer drop explicit `null` values from query-result sequences.
  `evaluate_path`'s array-mapping fast path predates the null/undefined migration in #32
  and skipped both, so `arr.p` over `[{"p": 1}, {"p": null}]` returned `1` instead of
  `[1, null]`. Only an *absent* field is undefined and drops out; a present `null` is a
  value and stays. Fixed for both the JSON and Python-dict (lazy view) routes. This
  corrects everything downstream of such a sequence — `$count`, array construction,
  comparison and arithmetic operands, and the fused aggregates, which now raise `T0412`
  on a null element rather than silently summing around it.
  ([#98](https://github.com/txjmb/jsonata-core/issues/98), root cause 1)
- Filter predicates now unwrap a single-element result, so `arr[p = 1]` is `{"p": 1}` rather
  than `[{"p": 1}]`. The tree-walker decided this from `step.stages` alone, but `arr[p = 1]`
  parses its predicate as a `Predicate` step *node* with empty stages, so no filter written
  that way was ever recognised as an array operation. Numeric-literal predicates are excluded:
  those are index access and already return the selected element, so counting them would
  unwrap twice and turn `a[0]` over `[[5]]` into `5`.
  ([#98](https://github.com/txjmb/jsonata-core/issues/98), root cause 2)
- Numeric filter predicates now select by position instead of being treated as truthy. In
  JSONata `arr[p]` keeps an element only when `p` equals that element's own index; negative
  values count from the end and fractional values floor, and an array of numbers is a set of
  such indices. The tree-walker previously evaluated the predicate against the whole array
  and treated a numeric result as a multi-index selector, so `arr[p]` over
  `[{"p": 1}, {"p": 2}]` returned both elements instead of nothing. Fixed for standalone
  predicates; filters in *stage* position (`a.b[-1]`, which maps the index over each
  extracted sub-array) keep their existing semantics.
  ([#98](https://github.com/txjmb/jsonata-core/issues/98), root cause 3)
- The compiled path and bytecode VM now apply the same index rule. `CompiledStep` records
  whether its filter came from a standalone `Predicate` step or a `Stage::Filter`, a
  distinction the compiler previously discarded on the stated assumption that "both
  encodings have identical runtime semantics" -- true for boolean predicates, false for
  numeric ones. ([#98](https://github.com/txjmb/jsonata-core/issues/98), root cause 3)
- A predicate applied to a non-array value now treats it as the singleton sequence it is:
  index 0, length 1. `arr[p]` over `{"p": 1}` is undefined (1 does not match index 0) and
  `arr[-1]` wraps to the value itself. A string predicate on an object is no longer computed
  property access -- `o["a"]` keeps the object because a non-empty string is truthy, matching
  jsonata-js, rather than looking up the key.
  ([#98](https://github.com/txjmb/jsonata-core/issues/98), root cause 4)
- A non-empty Python `dict` is no longer falsy on the bytecode VM and compiled paths.
  Dicts cross the boundary as a lazy view rather than a materialised object, and
  `compiled_is_truthy` had no arm for that variant, so it fell through to its catch-all and
  returned `false` for every one. This affected any truthiness context on the compiled path
  -- `o ? a : b`, `and`/`or`, `$boolean`, `$not`, filter predicates -- and only when data was
  passed as a dict, so the same expression over an equivalent JSON string was correct. The
  tree-walker was unaffected. ([#98](https://github.com/txjmb/jsonata-core/issues/98))
- Ordered comparisons (`<`, `<=`, `>`, `>=`) against an undefined operand now return undefined
  instead of raising `T2010`, and an explicit `null` operand now raises `T2010` instead of
  returning null. `ordered_compare` predated the null/undefined split and matched only on
  `JValue::Null`, so a real `Undefined` reached its catch-all. Rewritten to jsonata-js's rule:
  only numbers, strings and undefined are comparable; an undefined operand yields undefined;
  otherwise a type mismatch is `T2009`.
- An unbound variable (`$x`) now evaluates to undefined rather than null, so `3 > $x` is
  undefined, `{"a": $x}` drops the key, and `$not($x)` is undefined -- all matching jsonata-js.
  The surrounding comment already described these as the intended results; only the value was
  wrong. ([#98](https://github.com/txjmb/jsonata-core/issues/98), root cause 5)
- Explicit nulls now survive a stage filter (`arr.p[-1]`, `arr.p[0]`, `arr.p[]`). The
  tuple/stage branch of `evaluate_path` mapped an absent field to `JValue::Null` and then
  skipped every null, dropping present nulls alongside genuinely missing fields -- the same
  pre-migration pattern already fixed in the no-stages fast path, in three more places
  (the object arm, the tuple arm and the lazy-dict arm).
  ([#98](https://github.com/txjmb/jsonata-core/issues/98))
- `arr.p[-1]` now takes the last element of each extracted group on the bytecode VM, matching
  the tree-walker. Numeric-literal predicates are index access and are deliberately left to
  the tree-walker, but the guard tested only for `AstNode::Number` -- `[-1]` parses as a
  *negation* of a literal, slipped through, and compiled to a plain truthy constant that kept
  every element. ([#98](https://github.com/txjmb/jsonata-core/issues/98))
- Arithmetic on an explicit `null` now raises instead of silently producing null, including
  when the null arrives at runtime rather than as a literal -- `$map([1, null], function($v)
  { $v * 2 })` raises `T2001` where it previously returned `[2, null]`. Only *undefined*
  propagates. Error codes now match jsonata-js: a bad left operand is `T2001` and a bad right
  operand is `T2002` (previously `T2002` for both), and each defined operand is type-checked
  before undefined propagation, so `false + $x` raises rather than returning undefined. The
  five tree-walker operators now delegate to the same shared implementation as the compiled
  path and VM instead of each carrying its own copy of the null handling.
  ([#98](https://github.com/txjmb/jsonata-core/issues/98))
- Ordered comparisons inside filters and sort comparators now reject uncomparable operands.
  `compiled_ordered_cmp` was the un-migrated twin of `Evaluator::ordered_compare`: it still
  conflated `JValue::Null` with `JValue::Undefined`, so `arr[p > 1]` over
  `[{"p": 1}, {"p": null}]` silently returned undefined where jsonata-js raises `T2010`.
  Rewritten to the same rule -- only numbers, strings and undefined are comparable; an
  undefined operand yields undefined; a type mismatch is `T2009`.
- `$sort` comparators of the form `function($l, $r) { $l.f > $r.f }` no longer sort inputs
  that jsonata-js rejects. The specialized Schwartzian-transform fast path collapsed every
  non-numeric, non-string key into "missing" and treated mixed types as "keep original
  order"; it now declines those inputs so the general comparator raises `T2010`/`T2009`.
  Absent keys are still undefined and still sort last on the fast path.
  ([#102](https://github.com/txjmb/jsonata-core/issues/102), cluster A)
- `$map` and `$filter` now return sequences rather than arrays: a single result unwraps to
  that result and an empty result is undefined, so `$map(arr, function($v){$v.p})` over
  `[{"p": "free"}]` is `"free"` rather than `["free"]`, and over `[]` is undefined rather
  than `[]`. `$map` also accepts a non-array argument as the singleton sequence containing
  it, which `$filter` already did.
- Field access on a lambda parameter (`$v.p`, `$l.rating`) now yields undefined for a missing
  field instead of null. The `$var.field` fast path used by sort and higher-order-function
  bodies predates the null/undefined split, which is why `$map` produced `[1, null]` where
  jsonata-js drops the undefined, and why `$filter` raised `T2010` comparing what was really
  a missing field. ([#102](https://github.com/txjmb/jsonata-core/issues/102), cluster B)
- `!=` against an undefined operand is now `false`, matching `=`. jsonata-js returns false
  for both when either side is undefined -- `!=` is not the negation of `=` there -- so
  `arr[p != null]` no longer keeps elements whose `p` is missing.
- Object construction as a path step now follows sequence semantics: `arr.{"k": p}` over a
  single element is the object rather than a one-element array, and over a non-array value it
  builds from that value instead of from the root document (previously `{}`).
  ([#102](https://github.com/txjmb/jsonata-core/issues/102), cluster C)
- `&` now stringifies an explicit `null` as `"null"`, matching `$string(null)`, and treats
  only an *undefined* operand as the empty string. `null & "x"` was `"x"` and is now
  `"nullx"`; `missing.x & "x"` is still `"x"`.
- `in` is membership again, not array filtering. An array on the left made `evaluate_binary_op`
  treat the expression as `array[predicate]`, so `arr in 1` evaluated as `arr[1]` and returned
  an element. It now follows jsonata-js: an undefined operand on either side gives `false`, a
  non-array right side is wrapped, and membership is decided with `===` -- primitives by value,
  composites by identity. `obj in [obj]` is true, `obj in [{"k": 1}]` is false, and an object
  on the right is no longer treated as key-containment (`"k" in obj` is `false`).
- Division and modulo by zero no longer raise. jsonata-js checks operands, never results:
  `1/0` is `Infinity` and `0/0` is `NaN`, and the `D1001` appears when such a value is used as
  an operand (`1/(10e300 * 10e100)`) or serialised inside a composite
  (`$string({"inf": 1/0})`). The multiply overflow check moved from the result to the operands
  to match. JSON cannot spell Infinity, so the JSON-returning APIs give `null` for it, exactly
  as JavaScript's `JSON.stringify` does.
- Unary negation of an explicit `null` now raises `D1002` instead of returning null; only
  *undefined* propagates.

### Security

## [2.2.5] - 2026-07-14

### Added
- C API (`capi` cargo feature): use the engine from C, C++, or any language with C interop.
  Eight functions, JSON text in/out (`jsonata_compile`, `jsonata_evaluate`, `jsonata_bind_var`,
  `jsonata_free_expr`, `jsonata_free_string`, `jsonata_last_error_message`,
  `jsonata_last_error_code`, `jsonata_version`), thread-local error slot, and engine panics
  caught at the boundary instead of aborting the host process. Ships with a hand-written
  header (`bindings/c/jsonata.h`), build/link/CMake documentation (`bindings/c/README.md`),
  and a CI-gated smoke test compiled as both C and C++. Build with
  `cargo build --release --features capi`.

### Fixed
- Performance regression in v2.2.4 on small/fast expressions (issue #74): the
  `JSONATAPY_FORCE_TREE_WALKER` test toggle read the environment variable on every
  evaluation (both `evaluate()` and `evaluate_json()`), costing ~100-200ns per call —
  10-30% of a sub-microsecond expression. The toggle is now a process-wide atomic seeded
  from the environment once at import (whole-process forcing works unchanged) and flippable
  via a private test hook. Small-expression benchmarks recover 5-16%; the remaining few
  percent vs v2.2.3 on tiny payloads is the documented cost of lazy conversion (which makes
  realistic workloads up to 48% faster, see 2.2.4 notes).

### Changed
- Benchmark tooling: PR benchmark comments and release regression issues now state their
  comparison baseline explicitly (which commit/release, recorded when, on which runner),
  and the vs-jsonata-js comparison is labeled as such.

## [2.2.4] - 2026-07-13

### Added
- `JSONATAPY_FORCE_TREE_WALKER=1` environment variable (testing/debugging): forces every
  evaluation through the tree-walking evaluator, bypassing the default lazy/VM-preferred path.

### Changed
- `evaluate(dict)` now converts Python data lazily by default. Previously every call eagerly
  converted the entire input `dict` (and nested structures) to the internal value tree before
  evaluation began; now only the fields an expression actually touches are converted, and
  untouched input subtrees pass straight through to the output unchanged. Measured on the dev
  machine (min-of-5, vs. the prior eager `evaluate(dict)`):
  - `products.price` (100 objects): 34.9µs → 11.3µs (3.1x)
  - Filter by category (100 products, 9 fields each): 129.3µs → 23.4µs (5.5x)
  - `$sum(products[inStock].price)`: 116.4µs → 17.1µs (6.8x)
  - Complex dense transformation: 160.9µs → 79µs (2.0x)

  Every measured row now beats jsonata-js on identical data and machine (14.6/93.7/67.6/415.1µs
  respectively). See `benchmarks/python/lazy_check.py` for the reproducible gate.
- **Behavior change:** results containing an unmodified input subtree now reference the
  caller's *original* Python `dict`/`list` objects (result aliasing, matching jsonata-js),
  rather than a fresh copy. Mutating such a result mutates the corresponding input — copy
  explicitly (e.g. `copy.deepcopy`) first if you plan to mutate. Passed-through values also keep
  their exact Python type (an `int` field the expression never reads stays an `int`); fields the
  expression does touch still round-trip through the engine's number representation (whole
  values come back as Python `int`), as before.
- **Behavior change:** an unconvertible input value (e.g. a `set`) now raises `TypeError` only
  when the expression actually touches it, instead of eagerly for the whole input at the start
  of `evaluate()`. Any Python exception raised while lazily reading a field (e.g. `OverflowError`
  on an integer too large to represent) is likewise normalized to `TypeError` at this boundary,
  rather than propagating as its original exception type.

### Deprecated

### Removed

### Fixed

### Security

## [2.2.3] - 2026-07-12

### Added
- `evaluate_json_or_none()`'s `json_str` parameter now accepts `None` (in addition to a JSON
  string), binding the top-level context (`$`) to a true JSONata `Undefined` rather than an
  explicit `null`. (#68)
- `docs/cli.md`: a full command-line reference for both `jsonata` (Rust) and `jsonatapy`
  (Python), covering flags, input resolution, output/exit-code semantics, and the MCP
  subcommand — previously undocumented outside internal spec/plan files. (#68)

### Changed
- Release CLI binary archives are now named `jsonata-v<version>-<target>.tar.gz`/`.zip`
  (previously `jsonata-<target>.tar.gz`/`.zip` with no version embedded).

### Deprecated

### Removed

### Fixed
- The Python CLI's `-n`/`--null-input` now binds `$` to a true `Undefined`, matching the Rust
  CLI exactly (`jsonatapy -n '$'` now prints nothing, as `jsonata -n '$'` already did). It
  previously passed an explicit JSON `null` context instead, observable only for expressions
  referencing `$` directly. (#68)
- Release workflow: `publish-pypi` and `publish-crates` now require `build-cli-binaries` to
  succeed first. Previously the two registry publishes were independent of the CLI binary
  build, so a CLI build failure (untested in a real release prior to this) would have shipped
  a version to PyPI/crates.io with no CLI binaries attached and no way to reuse that version
  number.

### Security

## [2.2.2] - 2026-07-09

### Added
- Documented `JsonataData`, `evaluate_with_data`, and `evaluate_data_to_json` in `docs/api.md`
  (previously absent from the API reference entirely, despite being the "3-15x faster"
  pre-converted-data path highlighted in the README's Performance section). Added a cross-link
  from `docs/rust-crate.md` to the full auto-generated API reference on docs.rs. (#65)

### Changed
- Updated Rust dependencies to the latest versions compatible with existing (unpinned) semver
  requirements: `simd-json` 0.17.0 → 0.17.2 plus 6 transitive patch bumps. No security advisories
  found before or after (`cargo deny check`). (#65)

### Deprecated

### Removed

### Fixed
- SIMD-accelerated JSON parsing (`simd-json` feature, on by default) was consistently *slower*
  than the plain `serde_json` fallback for most payload sizes (up to 29% slower at 180KB), the
  opposite of its intent — caused by allocating fresh internal scratch buffers on every single
  parse call. Fixed by reusing a thread-local scratch buffer across calls; SIMD parsing now beats
  `serde_json` consistently (up to +22% faster) instead of losing at 3 of 4 tested sizes.
  Also corrected the README's "(optional feature)" wording for SIMD, which implied opt-in when
  it's actually enabled by default, including in published wheels. (#65)

### Security

## [2.2.1] - 2026-07-08

> Same code as [2.1.7](#217---2026-07-08) below, renumbered. This project's release versions
> track the jsonata-js major/minor version they target (patch numbers are independent — see
> README). `2.1.7` incorrectly continued the old `2.1.x` patch series even though this release's
> guardrails feature and signature-engine fixes target jsonata-js `2.2.0`/`2.2.1`; `2.1.7` is
> superseded immediately by this release and should not be used.

### Added
- Guardrails: `timeout` (ms, error code `D1012`), `max_stack_depth` (error code `D1011`), and
  `max_sequence_length` (error code `D2015`) keyword arguments on `compile()` and every
  `evaluate*()` call, enforced consistently across all three execution engines (tree-walker,
  compiled-expression fast path, bytecode VM). All default to `None` (unlimited) — no behavior
  change unless configured. See [Guardrails](docs/api.md#guardrails). (jsonata-js 2.2.1 Phase 2, #56)
- Documented the guardrails API in `docs/api.md`, `docs/usage.md`, and `docs/error-handling.md`
  (previously shipped with accurate Python docstrings but no user-facing docs), and corrected
  `docs/migration-from-js.md`'s stale claim that Python had no built-in timeout support.

### Fixed
- A deeply-nested expression (arithmetic chains, parenthesized/grouped expressions) no longer
  crashes the whole process (previously a native stack overflow) — now raises a graceful `U1002`
  error instead, via a depth guard in the parser and a second, defense-in-depth guard in the
  post-parse AST pass.
- `Instr::MakeArray`/`MakeObject`/`BlockEnd`'s bytecode operands (and `CallBuiltin`'s argument
  count, and internal constant-pool bookkeeping) no longer silently produce wrong, truncated
  results for oversized literals/blocks/calls (e.g. array literals with more than 65,535
  elements) — such cases now fall back to the always-correct tree-walker instead.
- `ast_transform.rs`'s depth-guard error messages no longer imply `%`/`@`/`#` ancestor-operator
  usage (e.g. "...while resolving ancestor/path metadata") when the guard fires for any
  sufficiently-nested expression, including plain arithmetic.
- Release workflow now fails loudly on a fresh dispatch when the target version tag already
  exists at a different commit, instead of silently reusing the wrong commit (#53).

## [2.1.7] - 2026-07-08

**Superseded by [2.2.1](#221---2026-07-08) above, published the same day.** This version was
numbered following a simple patch-increment from `2.1.6` rather than this project's actual
versioning policy (track jsonata-js's major/minor). It is fully functional and was not yanked —
package registries don't allow deleting a published version — but `2.2.1` is the version that
should be used going forward.

### Added
- Guardrails: `timeout` (ms, error code `D1012`), `max_stack_depth` (error code `D1011`), and
  `max_sequence_length` (error code `D2015`) keyword arguments on `compile()` and every
  `evaluate*()` call, enforced consistently across all three execution engines (tree-walker,
  compiled-expression fast path, bytecode VM). All default to `None` (unlimited) — no behavior
  change unless configured. See [Guardrails](docs/api.md#guardrails). (jsonata-js 2.2.1 Phase 2, #56)
- Documented the guardrails API in `docs/api.md`, `docs/usage.md`, and `docs/error-handling.md`
  (previously shipped with accurate Python docstrings but no user-facing docs), and corrected
  `docs/migration-from-js.md`'s stale claim that Python had no built-in timeout support.

### Fixed
- A deeply-nested expression (arithmetic chains, parenthesized/grouped expressions) no longer
  crashes the whole process (previously a native stack overflow) — now raises a graceful `U1002`
  error instead, via a depth guard in the parser and a second, defense-in-depth guard in the
  post-parse AST pass.
- `Instr::MakeArray`/`MakeObject`/`BlockEnd`'s bytecode operands (and `CallBuiltin`'s argument
  count, and internal constant-pool bookkeeping) no longer silently produce wrong, truncated
  results for oversized literals/blocks/calls (e.g. array literals with more than 65,535
  elements) — such cases now fall back to the always-correct tree-walker instead.
- `ast_transform.rs`'s depth-guard error messages no longer imply `%`/`@`/`#` ancestor-operator
  usage (e.g. "...while resolving ancestor/path metadata") when the guard fires for any
  sufficiently-nested expression, including plain arithmetic.
- Release workflow now fails loudly on a fresh dispatch when the target version tag already
  exists at a different commit, instead of silently reusing the wrong commit (#53).

## [2.1.6] - 2026-07-07

### Added
- `%` (parent-reference) and `@`/`#` (focus/index binding) operators.
- jsonata-js 2.2.1 Phase 1: signature engine rewrite adding `+`/`-` arity support (#36).
- Versioned documentation via `mike`, fixing a gh-pages deploy race (#39).

### Fixed
- Reference test-suite coverage gaps: loader fix, datetime picture-strings,
  `formatInteger`/`parseInteger` (Phases 0-2), and array-constructor/`distinct` stragglers
  (Phase 5, #44).
- `release.yml` never actually built macOS wheels, due to a broken `actions/setup-python`
  invocation (#41).
- Several benchmark accuracy/fairness corrections (await `jsonata-js` calls properly, substantiate
  pre-converted-data speedup claims, use clean CI-sourced numbers, use jsonata-python's `Context`
  for fair repeated-eval timing) (#45, #46, #49, #50).

### Changed
- macOS temporarily dropped from the main release matrix pending a self-hosted runner fix (#45,
  #47) — later restored via a self-hosted Mac Mini runner.

## [2.1.5] - 2026-07-04

### Fixed
- Native stack overflow on deep recursion — replaced with a graceful, coded error (fixes #34).
- Lambda IDs are now generated from a monotonic counter instead of an AST pointer address, fixing
  a ~0.5%-frequency wrong-closure bug from id aliasing across recursive/repeated evaluation
  (fixes #35).
- Tree-walker missing-path/field access now correctly produces `Undefined` instead of `Null` in
  the ~20 sites that predated the `Null`/`Undefined` distinction (fixes #32).
- CI repairs following the default-branch rename to `main`; patched `RUSTSEC-2026-0097`.

### Changed
- Bumped `pyo3` to 0.29 and `rand` to 0.10.

## [2.1.4] - 2026-03-22

### Added
- `cargo publish` step in the release workflow.

### Fixed
- Release workflow is now idempotent for version-bump and tag-creation steps.
- Corrected an incorrect expected value in the `substring` test suite.

## [2.1.3] - 2026-03-22

> Versions 2.1.1 and 2.1.2 were bumped internally but never published as standalone tagged
> releases — their changes are folded into this entry, the next version actually released.

### Added
- Bytecode VM (`compiler.rs` + `vm.rs`, "Phase 4") restored and wired into the Python execution
  path, with Criterion benchmarks comparing it against the tree-walker.
- `pyo3` made an optional dependency; Rust crate renamed to `jsonata-core` and published to
  crates.io independently of the `jsonatapy` PyPI package.

### Fixed
- PyO3 0.28 compatibility (`PyObject` → `Py<PyAny>`).
- Clippy deprecations and `cargo-deny` license-check failures.
- Upgraded Pillow (dev/docs dependency) to 12.1.1 for a CVE fix.

### Changed
- Multiple benchmark documentation and accuracy corrections.

## [2.1.0] - 2026-02-08

### Added
- Initial public release: Rust-based JSONata implementation targeting jsonata-js v2.1.0 semantics.
- Full jsonata-js v2.1.0 reference test-suite compatibility.
- Python bindings (PyO3), published as `jsonatapy` on PyPI.
- Rust core, published as `jsonata-core` on crates.io.

---

## Reference Implementation Tracking

This project tracks the [jsonata-js](https://github.com/jsonata-js/jsonata) reference implementation.

**Current status:** Full test-suite compatibility with jsonata-js v2.1.0 (+3 commits). jsonata-js
v2.2.1 compatibility work is in progress — Phase 1 (signature engine, `+`/`-` arity support) and
Phase 2 (resource guardrails) are done; see
`docs/superpowers/specs/2026-07-04-jsonata-2.2.1-design.md` for the authoritative, up-to-date
status of this effort.

### Version History
- Target tracking v2.1.0 - Project initialization (2025-01-17)
