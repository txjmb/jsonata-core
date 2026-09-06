# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

- **`function` and `λ` are names, not keywords.** They open a lambda only when immediately
  followed by `(`, matching jsonata-js, which has no such keyword and decides lambda-ness from
  the callee of a call. Anywhere else they are ordinary path steps, so `function` now reads a
  field called `function` (and `λ` a field called `λ`) instead of being a parse error.
- **A `/` after a bare `function`/`λ` divides** instead of opening a regex literal. The lexer
  treated the lambda opener as an operator for this purpose, but a lambda's next token is always
  `(` — so a `/` can only follow one of these when it is a name, and a name is a value.
  `function / 2` is now 21, as upstream.
- **A lambda's signature is validated when it is parsed**, not when it is first called — but
  only for the two errors jsonata-js also raises at parse time, `S0401` and `S0402`. Every other
  signature complaint still surfaces at call time as before.

### Deprecated

### Removed

### Fixed

- **`unknown(function)` now reports `T1006`** ("Attempted to invoke a non-function") instead of
  `S0202`. The `function` keyword in argument position used to commit the parser to a lambda,
  which then failed on its shape before anything decided the call target was not a function.
  (reference suite `errors/case005`, [#150](https://github.com/txjmb/jsonata-core/issues/150))
- **A choice group containing a parameterized type — `<(sa<n>)>` — now reports `S0402`**
  ("Choice groups containing parameterized types are not supported") instead of `S0202`. The
  signature parser rejected the shape while still reading characters, so it never got far enough
  to say what it was looking at. `S0401` was carried as text inside an uncoded error and is now
  a first-class variant too.
  (reference suite `function-signatures/case034`,
  [#150](https://github.com/txjmb/jsonata-core/issues/150))

  With these two, the jsonata-js reference suite is at **1686/1686** and
  `KNOWN_DIVERGENCES` is empty.

### Security

## [2.2.8] "Conform-ata" - 2026-08-25

Primarily a conformance release, with documentation work on the Rust crate alongside it. Most
of what follows brings `jsonatapy` closer to the pinned jsonata-js (v2.2.2), and a fair amount
of it **changes the answer** for expressions that already run today. If you are upgrading, the
summary below is the part worth reading; the itemised entries follow.

### What changed, and who it affects

**Boolean coercion of containers — the broadest change.** JSONata has one truthiness rule and
applies it recursively: a container is truthy only if some element is truthy, all the way down.
`jsonatapy` had three rules, and the one every implicit consumer used asked only "is it
non-empty?". A single-element array holding a falsy value — `[0]`, `[""]`, `[false]`, `[null]` —
was therefore truthy where the reference says false. That affects `? :`, `and`, `or`, `$not`,
filter predicates, `$filter`, and any lambda whose result is coerced.

*You are exposed if* your data holds one-element arrays of falsy values (`{"flags": [false]}`,
`{"counts": [0]}`) or you build them (`[expr]`, `expr[]`, `$map`, `$filter`). Ordinary path
expressions are unaffected — a path matching one value unwraps to the scalar, so
`items[v=0].v` was already `0`, not `[0]`.

**`$formatNumber` produces different numbers.** Rounding was half-away-from-zero; the reference
rounds half-to-even. `$formatNumber(12.345, "#,##0.00")` was `"12.35"` and is now `"12.34"`.
Three further defects around empty fractional parts, optional digits and exponents are fixed in
the same pass.

*You are exposed if* you format numbers for display or comparison and depend on the previous
rounding.

**Sequence and null handling in paths.** `*` on a value with no children is `undefined` rather
than `null`; `*` and `#$i` now unwrap a lone result like every other sequence-producing step;
an explicit null is treated as a value by object construction and `[]` rather than
short-circuiting them; and `[]` and `[true]` are finally distinct operators.

*You are exposed if* you rely on the shape (wrapped vs unwrapped) of single-result expressions,
or on `null` where the reference produces `undefined` — the two differ in object construction,
which drops undefined-valued keys but keeps null-valued ones.

**Stricter argument validation.** Six builtins (`$base64encode`, `$base64decode`, `$toMillis`,
`$fromMillis`, `$formatInteger`, `$parseInteger`) had no signature at all and so validated
nothing. They now reject an explicit null with `T0410` where they previously returned `null`,
and they accept the context forms (`str.$base64encode()`) that never worked.

*You are exposed if* you pass a possibly-null value to any of those six and relied on getting
`null` back rather than an error.

**More permissive parsing in two places.** `$base64decode` now accepts what the reference
accepts — partial quanta, characters outside the alphabet, the URL-safe alphabet — instead of
raising. `$parseInteger` returns `NaN` for a value that does not match its picture rather than
raising `D3136`.

*You are exposed if* you depended on those raising to detect bad input.

**Error codes.** This is the largest single change by count. Errors across the evaluator and the
parser reported the wrong JSONata code, or none at all — an error with no code is not something
a caller can branch on. Of the reference suite's 273 code-bearing cases, 107 could not be
verified at 2.2.7 because our error carried no code to compare; that is now 2, and those two are
unreachable rather than unfixed. Parse errors in particular went from uncoded prose to carrying
`S0101`, `S0103`, `S0104`, `S0105`, `S0202`, `S0203`, `S0204`, `S0207`, `S0208`, `S0211`, `S0212`
and `S0401`, so a syntax error is now distinguishable from any other failure.

**Things that used to fail and now work.** Passing a builtin by reference through a variable
(`$f := $uppercase; $map(arr, $f)`) returned empty strings and now works, for every builtin.
`$eval` works as a callback. `$single` is recognised when passed by reference. Twenty-four
builtins that worked in direct calls but raised when passed to `$map`/`$filter` now behave
identically either way.

**Documentation — the Rust crate.** Its examples were marked `rust,ignore`, so nothing verified
they were correct; they are now four compiling doctests. Five rustdoc warnings that rendered as
broken links on docs.rs are gone, the crate header no longer names the wrong crate, and the
module list no longer advertises two private modules while omitting two feature-gated ones. On
the Python side, the package is classified Production/Stable rather than Beta — it still said
Beta at full reference-suite parity. Itemised under Added, Changed and Fixed below.

**Internals with no behavioural intent.** Builtin dispatch is now a single shared
implementation rather than three partial copies, and the differential harness gained the
ability to see distinctions it was previously blind to — `null` vs `undefined`, error codes,
and by-reference dispatch of evaluator-dependent builtins. Each of those blind spots was
hiding real divergences, which is where most of this release came from.

### Compatibility

The differential corpus — over 20,000 comparisons against jsonata-js across two engines and two
input paths — has **no known divergences** for the first time. The one deliberate exception is
base64's character set, documented below.

All 1686 reference-suite cases pass, and each now runs through **both** evaluation engines —
the suite previously ran only whichever the default resolved to, so the tree-walker was
exercised by the differential corpus and nowhere else.

Worth stating plainly rather than quoting the pass count: 43 of its error-expecting cases still
assert only that *something* was raised, because the errors we produce for them carry no JSONata
code to compare, and 15 more specify an error object that is not inspected. That is
[#144](https://github.com/txjmb/jsonata-core/issues/144), and it is mostly parser errors. It is
a gap in what the suite verifies, not a set of known failures — a test now pins the count so it
cannot grow unnoticed.


### Added
- The `jsonata-core` crate documentation now carries four compiling examples — quick start,
  compile-once/evaluate-many, error handling, and host functions via `register_fn`. The
  crate-level example was previously marked `rust,ignore`, so nothing verified it was correct;
  `cargo test --doc` now runs 5 doctests.

### Changed
- The published Python package is now classified `Development Status :: 5 - Production/Stable`.
  It still read `4 - Beta` at full reference-suite parity, and that classifier is the only
  signal the PyPI sidebar and downstream trend trackers read.
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
  (issue [#107](https://github.com/txjmb/jsonata-core/issues/107))

### Deprecated

### Removed

### Fixed
- The last uncoded errors carry their code: calling a non-function is `T1006`, partially
  applying one (a call carrying a `?` placeholder) is `T1008`, assigning to something that is
  not a variable is `S0212`, and a type parameter applied to something other than a function or
  array is `S0401`. Every reference case that names an error code now has that code compared
  against ours, except two that cannot be reached at all — `$encodeUrl` on an unpaired
  surrogate, where the expression never crosses into Rust to be parsed.
  (issue [#144](https://github.com/txjmb/jsonata-core/issues/144))
- Parse errors carry their JSONata code. Unterminated strings are `S0101`, unsupported escapes
  `S0103`, a malformed `\u` escape `S0104`, an unterminated backquoted name `S0105`, a wrong
  token `S0202`, running out of input while expecting one `S0203`, an unknown operator `S0204`,
  an unexpected end of expression `S0207`, and a symbol used where an operand belongs `S0211`.
  Previously all of these were uncoded prose, so callers branching on error codes could not
  distinguish a syntax error from any other failure.
  (issue [#144](https://github.com/txjmb/jsonata-core/issues/144))
- Nine more evaluator errors carry their JSONata code: `$sqrt` of a negative number is `D3060`,
  `$power` overflowing is `D3061`, the single-argument `$sort` on mixed types is `D3070`,
  `$single` matching more than one value is `D3138`, a non-string object key is `T1003`, a
  non-function right side of `~>` is `T2006`, and `$split` given a matcher function that does
  not produce the expected structure is `T1010`. Calling a function without its `$` splits the
  way jsonata-js splits it: a name that *is* a builtin gets `T1005` with the "did you mean
  `$name`?" suggestion, anything else gets `T1006`.
  (issue [#144](https://github.com/txjmb/jsonata-core/issues/144))
- `$decodeUrl` and `$decodeUrlComponent` report `D3140` on malformed input, naming the function
  and quoting the value as jsonata-js does, instead of an uncoded "Invalid percent-encoded URL".
- An expression containing an unpaired surrogate no longer leaks a raw `UnicodeEncodeError`. A
  Python `str` can hold one and a Rust `String` cannot, so such an expression cannot cross the
  boundary to be parsed at all; PyO3's codec error now becomes a `ValueError` naming the
  surrogate and its position, so the library keeps a single error type. (`$encodeUrl` on a lone
  surrogate is `D3140` upstream; there is no point at which we could raise it, since the
  expression never parses.)
- The reference suite compares the `code` of cases that specify an error *object*, not just
  those with a bare `code` field. Nothing about those 15 was checked before.
- `$.7a` now raises `S0201`, not `S0213`. jsonata-js raises `S0213` ("literal value cannot be
  used as a step") from a pass that runs *after* parsing, so an unexpected trailing token fails
  the parse first; we raised it inline, before the trailing token was reached. Our `S0213` was
  already right for `$.7` and `a.7` — only the ordering was wrong. A leftover token now also
  carries `S0201`, the code the reference uses for it, instead of an uncoded "Expected end of
  expression".
- Every reference-suite case now runs through both evaluation engines, and the suite pins how
  many of its error cases it cannot actually verify. At 2.2.7 four cases had the two engines
  raising different errors, each accepted because both were errors and the check was loose.
- The test reporter no longer crashes the run with an `INTERNALERROR` when a non-parametrized
  test fails; it read `item.callspec` unguarded on the failure path.
- The reference-suite harness compares error codes it previously ignored. `extract_error_code`
  was anchored to the start of the message, so any coded error carrying a prefix — `"Runtime
  error: D3030: ..."`, `"Parse error: Invalid syntax: S0209: ..."` — read as *uncoded*, and an
  uncoded error is accepted for any expected code. 36 cases that emit exactly the right code
  were passing without it ever being compared, and one emitting the wrong code passed the same
  way. Now 228 of the 273 code-expecting cases are genuinely compared, up from 191.
  (issue [#144](https://github.com/txjmb/jsonata-core/issues/144))
- Five rustdoc warnings that rendered as broken links on docs.rs are gone. JSONata syntax in
  doc comments — `[expr]`, `|location|update[,delete]|` — was being parsed as intra-doc links,
  and `Rc<str>` as an unclosed HTML tag. `cargo doc` is now warning-free under both the default
  features and `--all-features`.
- The crate's documentation header said `jsonatapy` (the crate is `jsonata-core`), and its
  module list advertised `datetime` and `signature` as public when both are private while
  omitting the feature-gated `lazy` and `capi`.
- A positional predicate that names the same index more than once now repeats the element, as
  jsonata-js does: `nums[[0,0]]` is `[10, 10]`, not `10`. The reference walks the selector array
  and pushes the item on every hit; we used an `any()`-style membership test, which can only ever
  yield each element once. Singleton sequences repeat too — `num[[0,0]]` is `[5, 5]`.
- The tree-walker now recognises an *array* of indices as a positional selector against a scalar.
  It tested only for a single number, so `num[[0]]` was `undefined` and `num[[1]]` was `5` — both
  inverted, because the array fell through to the truthiness branch where an all-falsy container
  is falsy and a non-empty one is truthy. The compiled path was already correct, so the two
  engines disagreed.
- `$eval` can be passed by reference: `$map(["1+1"], $eval)` is `2` rather than an error. It is
  the one evaluator-dependent builtin whose arguments are ordinary values — an expression string
  and an optional focus — so unlike the other nine it runs from evaluated values. Its
  implementation is now shared by the two dispatch paths rather than living only in
  `evaluate_function_call`.
- The other nine evaluator-dependent builtins (`$map`, `$filter`, `$reduce`, `$single`, `$sift`,
  `$each`, `$sort`, `$match`, `$replace`) now raise `T0410` when handed to a higher-order
  function, matching jsonata-js, instead of an uncoded internal error. They still cannot run
  as callbacks — each needs a *function* argument and a callback receives a value and an index,
  which the reference rejects on the signature too — so this is the error code, not the
  behaviour.
- `$single` is recognised as a builtin when passed by reference. The internal name list carried
  `singletonArray`, which is not a JSONata function and appears nowhere in jsonata-js, and
  omitted `single` — so `$map(arr, $single)` raised "Argument 2 must be Function" while direct
  calls worked. (issue [#140](https://github.com/txjmb/jsonata-core/issues/140))
- `$eval(null)` now raises `T0410` rather than returning `null`. The reference's signature is
  `<sx?:x>`, and `s` admits *missing* but not null; `$eval(nothing)` still propagates undefined.
- Truthiness is now one rule, applied recursively, matching `$boolean`. JSONata coerces a
  value to boolean the same way everywhere: a container is truthy only if some element is
  truthy, checked all the way down, so `[0]`, `[[0]]` and `[0,0]` are falsy. `jsonata-core`
  had three rules — `$boolean` (correct), a flat "is it non-empty?" used by every implicit
  consumer, and a third half-recursive one used only by `?:`. `$not([0])` was `false` where
  `$boolean([0])` was already `false`, so `$not` was not computing `!$boolean(x)`. This
  changes the answer for `$not`, `? :`, `and`, `or`, filter predicates, `$filter` and any
  lambda whose result is coerced — 54 measured cases across those seven consumers — whenever
  the value is a container whose contents are all falsy. The `?:`-only rule is deleted rather
  than realigned: with `is_truthy` correct the two agree exactly.
  (issue [#111](https://github.com/txjmb/jsonata-core/issues/111))
- `$formatNumber` now produces the same value as jsonata-js. Four separate defects, all in
  picture analysis and formatting:
  - **Rounding was half-away-from-zero, not half-to-even.** The reference routes this through
    the same helper `$round` uses. `$formatNumber(12.345, "#,##0.00")` was `"12.35"` and is
    `"12.34"`; `$formatNumber(0.25, "0.0")` was `"0.3"` and is `"0.2"`. Our `$round` builtin
    was already correct — `$formatNumber` simply did not use it. This produced wrong numbers
    for ordinary pictures, not just edge cases.
  - **An empty fractional part invented a digit and a separator.** Fractional digit counts now
    come only from the picture, so `"0."` has none: `$formatNumber(1.5, "0.")` was `"1.5"` and
    is `"2"`, and `$formatNumber(1, "0.")` was `"1."` and is `"1"`.
  - **`#` did not suppress a leading zero.** `$formatNumber(0.25, "#.#")` was `"0.3"` and is
    `".2"`, per F&O 4.7.4's adjustments to the minimum digit counts.
  - **The exponent's upper bound was inclusive.** `$formatNumber(1, "#.e0")` was `"0.1e1"` and
    is `"1.0e0"`.

  326 cases across 21 pictures now agree, including negatives, grouping, sub-pictures and
  percent/per-mille. Values at or beyond `1e21` are deliberately excluded: JavaScript switches
  to exponential notation there and the reference formats the resulting text as if it were
  digits, so `$formatNumber(1e21, "#,##0.00")` is `"1e,+21.00"` upstream — a grouping separator
  inside the exponent. (issue [#136](https://github.com/txjmb/jsonata-core/issues/136))
- `$formatNumber` now reports the same picture-string error as jsonata-js. The reference runs
  every sub-picture check in sequence and lets the *last* failure name the error (F&O 4.7.3);
  ours returned on the first, so a picture failing several checks reported the wrong one —
  `"k"` fails both `D3085` and `D3086` and is `D3086`, `"%%"` was `D3082`, `".."` was `D3081`
  and `"e"` was `D3085`. A picture containing no active character at all was also swallowed
  whole as a prefix rather than treated as the active part, which is what stopped `D3086` from
  ever firing for `"k"` or `"%%"`. 29 pictures now agree where 7 diverged.
  (issue [#135](https://github.com/txjmb/jsonata-core/issues/135))
- Error *codes* now match jsonata-js where they did not. The differential harness compared
  only "did it raise", so any error satisfied an expected error — `$replace(1)` raising
  "Argument count mismatch" read as a match for the reference's `T0410`. Comparing codes
  surfaced 112 cases across nine shapes, all now fixed: signature arity failures carry `T0410`;
  `$single` reports `D3138`/`D3139`; `$power` with a missing exponent reports `D3061`;
  `$toMillis` on an unparseable timestamp reports `D3110`; `$millis`/`$now` handed arguments
  by a higher-order function report `T0410`; and `$error`/`$assert` treat an undefined argument
  as the signature says they must — `s`/`b` admit *missing*, so `$error(missing.x)` is
  `D3137` ("no message") and `$assert(missing.x)` is `D3141` (a failed assertion), not type
  errors. (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))
- A builtin bound to a variable now works as a callback: `($f := $uppercase; $map(arr, $f))`
  is `["A", "B"]` rather than `["", ""]`. Binding stores a `JValue::Builtin`, which is not a
  lambda, so `apply_function` fell through to evaluating `$f` as an ordinary variable and the
  callback yielded the function value instead of calling it. Callback arity was resolved from
  the variable's name too, so `$map` handed a one-argument builtin all three of its arguments.
  Both now resolve through the bound builtin, and a host-registered override of the same name
  still suppresses arity truncation. Every builtin was affected, not just the reported
  `$uppercase`; direct invocation (`$f("a")`) always worked, which is why this looked narrow.
  (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))
- `$base64decode` now accepts what jsonata-js accepts. The reference decodes through Node's
  `Buffer`, which ignores characters outside the base64 alphabet, stops at the first padding
  character, takes the URL-safe alphabet alongside the standard one, and drops an incomplete
  trailing quantum — so `$base64decode("a")` is `""`, `$base64decode("YQ")` is `"a"` and
  `$base64decode("!!!!")` is `""`, where a strict decoder rejects all three.
- `$parseInteger` no longer raises `D3136` on a value that does not match its picture. The
  reference runs the picture parse without validating the input — its own source marks that
  path `TODO validate input based on the matcher regex` — so the answer is JavaScript's
  `parseInt`: a leading run of digits, or `NaN` when there is none. `$parseInteger("12a", "000")`
  is `12` and `$parseInteger("abc", "000")` is `NaN`. A malformed *picture* still raises
  `D3130`, and the letters/roman/words parsers are untouched because date-time component
  parsing shares them and needs the error. (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))

- An explicit null now behaves as a value in object construction and in the `[]` array-keep,
  rather than short-circuiting them: `nul{"a": $}` is `{"a": null}` (it was `null`, and
  disagreed with the dotted `nul.{"a": $}`, which was already right), `nul[]` is `[null]`,
  and `nul[][0]` is `[null]`. Four separate sites each carried a pre-`Undefined` `JValue::Null`
  arm that made the operation a no-op. `emptyarr[]` is now `[]` rather than `undefined` — an
  empty array held in a field is a value, unlike an empty result sequence such as `arr.p` over
  `{"arr": []}`, which stays `undefined`.
- `[]` and `[true]` are no longer the same thing. Both parsed to `Predicate(Boolean(true))`,
  but jsonata treats them as different operators: `[]` keeps the result an array while
  `[true]` is an ordinary filter that keeps everything and then unwraps a lone result. One of
  the two therefore had to be wrong for every input — `num[]` and `num[true]` were both `[5]`
  where the reference gives `[5]` and `5`. A dedicated `AstNode::KeepArray`/`Stage::KeepArray`
  marker separates them. (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))
- `#$i` positional binding now unwraps a one-value result, matching every other
  sequence-producing step: `num#$i` is `5` rather than `[5]`, and `deep#$i.$i` is `0` rather
  than `[0]`. The rule holds for every input kind — `arr#$i` and `arrobj#$i` only ever looked
  correct because a multi-element result has no singleton to unwrap, and the filtered form
  `arr#$i[$i=0]` was right for the unrelated reason that a filter predicate already forced
  the unwrap. (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))
- `*` applied to anything without children is now `undefined` rather than `null`, and its
  result unwraps a lone value the way every other sequence-producing step does. jsonata-js
  guards the wildcard with `typeof input === 'object' && input !== null`, so `num.*`,
  `str.*`, `true.*` and the rest all map over nothing; only the `null` case had been fixed
  here, leaving the arm inconsistent with the catch-all beside it, and the two collapse into
  one now that they give the same answer. Separately, `*` was missing from the list of steps
  that produce a query-result sequence — the list `**` and filter predicates are already on —
  so a one-value result stayed wrapped: `deep.*` over `{"a": {"b": 1}}` was `[{"b": 1}]` and
  is now `{"b": 1}`, which also lets `deep.*.*` reach the inner value instead of an array.
  (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))
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
  (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))
- `$sum`, `$max`, `$min` and `$average` over a path of the form `array.field` no longer
  silently skip non-numeric values. The fused aggregate fast path
  (`Evaluator::try_fused_aggregate`) reimplemented aggregate semantics rather than
  delegating to them, and treated a present non-numeric field the same as an absent one.
  With `{"orders": [{"p": 1}, {"p": "free"}]}`, `$sum(orders.p)` returned `1` instead of
  raising `T0412` — a plausible but wrong number rather than an error. The same path also
  returned `0`/`null` for an empty sequence where jsonata-js returns `undefined`. The fast
  path now declines when its assumptions do not hold, so the canonical aggregate produces
  both the error and the empty-sequence semantics. (issue [#97](https://github.com/txjmb/jsonata-core/issues/97))
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
  (issue [#104](https://github.com/txjmb/jsonata-core/issues/104))
- `$trim`, `$merge`, `$reverse`, `$distinct`, `$join` and `$keys` given a missing argument
  now yield `undefined` rather than `null`, matching jsonata-js. This was visible through
  object construction, which drops an undefined-valued key but keeps a null-valued one:
  `{"k": $trim(missing)}` was `{"k": null}` and is now `{}`. For the first five, the two
  evaluation routes previously disagreed — the compiled/VM path returned `null` while the
  tree-walker returned `undefined` — and the shared dispatcher makes them agree. `$keys`
  was wrong on both routes; the tree-walker arm had no `Undefined` case at all, and the new
  shared dispatcher's corpus is what exposed it.
  (issue [#107](https://github.com/txjmb/jsonata-core/issues/107))

### Deliberately not changed
- `$base64encode`/`$base64decode` keep treating their payload as UTF-8, because jsonata's own
  documentation asks for both latin1 *and* UTF-8 and its implementation matches only the first.
  `$base64encode` is documented as latin1 — "all characters in the string are in the 0x00 to
  0xFF range... Unicode characters outside of that range are not supported" — while
  `$base64decode` is documented as "using a UTF-8 Unicode codepage", which its implementation
  does not do. No reading of the reference is self-consistent, and UTF-8 on both sides is the
  half that matches a documented contract exactly while also round-tripping. Above `0xFF`
  nothing is defined at all: `window.btoa` throws `InvalidCharacterError` where Node truncates
  each UTF-16 code unit to a byte, so browser and Node disagree. The cost is real but narrow —
  for `0x80`–`0xFF`, encode has an environment-independent documented answer we do not give
  (`$base64encode("héllo")` is `"aOlsbG8="` upstream, `"aMOpbGxv"` here); matching it would
  require latin1 decode too, which would then contradict the decode docs. A unit test pins our
  choice, with the full reasoning, so it cannot flip unnoticed.
  (issue [#126](https://github.com/txjmb/jsonata-core/issues/126))

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
