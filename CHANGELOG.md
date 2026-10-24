# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

**Upgrade notes**

- **Breaking for Rust code that builds `EvaluatorOptions` field by field.** `EvaluatorOptions`
  has a new public `cancel` field (see Added), so a struct literal that lists every field
  without `..Default::default()` no longer compiles. Add `cancel: None`, or end the literal with
  `..Default::default()`. Strict semver would call this a major change. It ships in a patch
  release because jsonata-core's version tracks the jsonata-js version it targets (2.2), and
  code that uses `..Default::default()` is unaffected. The Python and C APIs are unchanged.
- **`jsonatapy[mcp]` now needs fastmcp 4.1.2 or later**, which brings `mcp` 2.x (see Changed).

### Added

- **An evaluation can be cancelled from another thread.** `EvaluatorOptions` has a new
  `cancel` field that takes a `CancelToken`. Calling `cancel()` on any clone of the token
  stops the evaluation with `U1003: Evaluation cancelled` wherever `timeout_ms` is checked:
  each node evaluated, each element a higher-order builtin hands its callback, each match
  `$replace` hands its replacement function, and each pass of a tail-call loop. A single
  builtin call over a large input finishes first, as it does under `timeout_ms`, and if it is
  the last step the evaluation returns its result. jsonata-js has no cancellation, so the
  code is in the `U` series. This is a Rust API only. See the upgrade notes above for the
  `EvaluatorOptions` source break.

### Changed

- **The `mcp` extra now needs fastmcp 4.1.2 or later** (was 3.0). fastmcp 4 brings `mcp` 2.x
  and replaces `httpx` with `httpx2`, so an environment that pins `mcp<2` or depends on
  `httpx` alongside `jsonatapy[mcp]` may need updating. The `jsonatapy mcp` server itself is
  unchanged. 4.1.2 is also the first 4.x release with the fix for GHSA-7xgj-8392-5q4r.

### Deprecated

### Removed

### Fixed

- **`timeout_ms` now stops `$map`, `$filter`, `$reduce`, `$single`, `$sift`, `$each` and
  `$sort` when their callback is a builtin, host or partial function, and `$replace` when its
  replacement function is one.** Their loops over such a function had no checkpoint, because
  the call evaluates no node, so `$map(items, $string)` over a large array ran to the end past
  its timeout. A lambda callback was already stopped, since its body is checked like any node.
  An expression with `timeout_ms` set that used to finish late in one of these loops and
  return a result now fails with D1012 instead.

### Security

## [2.2.12] "Gymkata" - 2026-10-20

### Added

### Changed

- **The mimalloc allocator no longer falls into its aligned slow path.** With the `mimalloc`
  feature (the published wheels), every allocation went through `mi_malloc_aligned`. There,
  once the free list of a size class whose block size is not a power of two runs dry, each
  allocation of that size over-allocates from a larger class instead of refilling it (about
  110 extra instructions per allocation), and which sizes are hit depends on what else
  shares their class. The global allocator now calls `mi_malloc` for the alignments mimalloc
  guarantees (up to 16 bytes on 64-bit targets, 8 on 32-bit, for sizes that are a multiple
  of the alignment), which is nearly every Rust allocation, and uses the aligned API for the
  rest. Callgrind on the A/B benchmark expressions: 0.2% to 3.6% fewer instructions on every
  one, both engines (Sort 3.6%, Deep Path 2.8%, Parent 1.9%).
- **An object constructor or `.(expr)` path step is compiled once per evaluation, not each
  time it runs** ([#197](https://github.com/txjmb/jsonata-core/issues/197)). The evaluator
  keeps each step's compiled form, so a step inside a function body, such as `$map(big,
  function($v){ $v.items.{"a": x, "b": y} })`, compiles once instead of once per call (about
  10% fewer instructions for 200 calls), and a step that does not compile is not retried.
  Whether a builtin the step calls is shadowed is still checked every time it runs. Rust API:
  `PathStep` has a new `id: StepId` field, the key for that cache (`StepId::default()` in a
  struct literal, which gives a new id; `PathStep::new` and `with_stages` set it). It is
  ignored by equality and serde, and code that rewrites a step's `node` should give it a new
  id. `AstNode` stays `Send + Sync`.
- **A `$` step over an array known to hold no arrays skips the nested-array scan**
  ([#196](https://github.com/txjmb/jsonata-core/issues/196)). `JArray` gains a private `flat`
  flag, read with `is_flat()` and set with `JArray::new_flat` or `JValue::sequence_flat`, for
  arrays known to hold no array a path step flattens. Input conversion (JSON, `serde_json`,
  Python lists), numeric ranges, path steps that flattened nothing, field lookups over arrays
  and `[...]` constructors over flat parts set it where knowing costs nothing; `make_mut` and
  `with_items` clear it. `$sum(values.$)` over 1,000 numbers runs about 14% fewer instructions
  (callgrind), and results are unchanged, including `n.$` over nested arrays.
- **A filter, sort or group-by right after a sort of a tuple stream sees each tuple as an
  object** ([#214](https://github.com/txjmb/jsonata-core/issues/214)). jsonata-js's sort returns
  a tuple stream's tuples as a plain array, so the stage that follows reads the tuple, not its
  value: `acct.Order.P^(>%.id)[N = "C"]` is undefined (it was `{"N":"C"}`), a second sort such
  as `^(N)` keeps the order, `{N: N}` gives `{}`, and `o.t^(>%.id)^($)` is T2008. This holds
  after a position filter such as `[0]` too. Later path steps see the values as before, and a
  sort used as a value (`$x := acct.Order.P^(>%.id)`, `$string(...)`) gives the values rather
  than the internal tuples.
- **A group-by on a path groups the whole path**
  ([#208](https://github.com/txjmb/jsonata-core/issues/208)). In jsonata-js the `{...}` is
  the path's group, and later `.` steps, calls, filters, sorts and binds extend that same path
  before it is grouped. So `acct.Order.P{N: N}.A` groups `acct.Order.P.A` and gives `{}` (it
  was `"A"`), `acct.Order.P{N: N}[0]` filters step `P` (it was S0209), and a second group-by
  on the same path, as in `acct.Order{id: id}.P{N: N}`, is S0210. A group-by on a block, `$x`
  or `*` still applies to that step alone, and `[]` after it keeps the array instead of
  raising S0209.
- **Rust API: `JValue::Array` holds a `JArray`** instead of an `Rc<Vec<JValue>>` (#184). `JArray`
  dereferences to `Vec<JValue>`, so code that reads arrays is unaffected. Code that builds them
  calls `JValue::array(vec)`, and `JArray::make_mut`, `into_vec` and `ptr_eq` replace
  `Rc::make_mut`, `Rc::try_unwrap` and `Rc::ptr_eq`. `JArray` also carries the `sequence`, `cons`
  and `keep_singleton` flags jsonata-js keeps on arrays, and an `outer_wrapper` flag for the
  top-level input array.
- **Results are unwrapped and flattened by jsonata-js's two rules** (#184). A path step's
  results are flattened one level, except for arrays built by `[...]`, and on the last step a
  single array result is returned as is. After every expression, an empty sequence is
  undefined and a one-item sequence is its item unless `[]` applies. These replace the
  engine's own heuristics, so results now match jsonata-js 2.2.2 in cases such as
  `[{'a':[[1]]}].(a)` (`[[1]]`), `n.$` over nested arrays, `$lookup` and `*` (which flatten
  one level), `x[]@$v` (now accepted) and a range that counts down (`[3..1]`, undefined).
- **A top-level array input is one item to a path**, as jsonata-js's `outerWrapper` makes it,
  while `$`, `*` and an object constructor see the bare array: `{"x": a[0]}` over
  `[{"a":[1,2]},{"a":[3]}]` is `{"x":[1,3]}`. `$eval(expr, array)` wraps its focus the same way.
- **An object constructor groups an array input's items by key**, as jsonata-js
  `evaluateGroupExpression` does: a value is evaluated over its group's items, so
  `[[1,2],[3]].{"x": $}` is `{"x":[1,2,3]}` and over `[4]` it is `{"x":4}`. A group-by's keys
  keep the order they are first produced in.
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

- **Recursion through a tail-called partial application no longer crashes the process**
  ([#255](https://github.com/txjmb/jsonata-core/issues/255)).
  `($f := function($n){ $n <= 0 ? 1 : $f($n-1, ?)() }; $f(2000))` is 1, as in jsonata-js; it
  overflowed the native stack at about 2,000 levels on both engines and killed the Python
  process. A call of a value in tail position that is not a function with tail calls (a
  partial application, also of a builtin such as `$reduce(?, $g, 0)`, or a builtin called as
  a value, `($reduce)(xs, $g, 0)`) was applied by the trampoline outside any evaluation
  step, so a recursion through it bypassed both the native-stack guard and the depth limit.
  It now runs under both: deep recursion evaluates, and with no base case it raises U1001 at
  the 30,000-level limit.
- **A tail-recursive loop no longer stops at 100,000 iterations**
  ([#251](https://github.com/txjmb/jsonata-core/issues/251)). The tail-call trampoline had a
  hardcoded 100,000-iteration cap when no timeout was set, so
  `($loop := function($n){ $n = 0 ? "done" : $loop($n - 1) }; $loop(200000))` raised
  "U1001: Stack overflow - maximum recursion depth (500) exceeded" on both engines, naming a
  depth nothing enforced. jsonata-js has no such cap and returns `"done"`. Tail calls now run
  without any limit: a tail loop keeps one frame, so a million iterations take about 0.4s
  with no memory growth. `max_stack_depth` (D1011) and the 30,000-level U1001 backstop still
  bound non-tail recursion, and a `timeout` (D1012) is what stops a runaway tail loop. The
  U1001 message now names its real limit and follows the wording of the jsonata-js test
  runner's U1001: "Stack overflow error: evaluation nested deeper than 30000 levels. Check for
  non-terminating recursive function. Consider rewriting as tail-recursive."
- **`$eval` with an undefined focus evaluates against the current input**, as in jsonata-js
  ([#247](https://github.com/txjmb/jsonata-core/issues/247)). `$eval("a", $v)` with `$v`
  unbound used the undefined focus as its input and was undefined; it now reads `a` from
  the input, as `$eval("a")` does. A defined focus still replaces the input.
- **A builtin called in a function's tail position returns its result as jsonata-js does**
  ([#250](https://github.com/txjmb/jsonata-core/issues/250)). jsonata-js runs such a call
  from its trampoline, which never unwraps the result, but it was unwrapped here, so an
  empty sequence became undefined: `$map([1,2], function($v){ $keys({}) })` was undefined
  instead of `[[], []]`, and `$keys({"a":1})` there gave `"a"` instead of `["a"]`. A direct
  call such as `$f()` still unwraps it. With this, `$keys` of undefined and `$lookup` over
  an array that matches nothing give an empty sequence, as in jsonata-js; a conditional
  without an else in that position is undefined, not null; and `$sort` tests its
  comparator's result with JavaScript truthiness, so a comparator returning `[]` or `{}`
  swaps. `$split` of undefined is undefined, not null, so
  `$map([1,2], function($v){ $split(nothing, " ") })` is undefined instead of
  `[null, null]` and `[$split(nothing, " ")]` is `[]`; `$sift(nothing, $f)` is undefined
  instead of an error.
- **A builtin called in a function's tail position runs with the input jsonata-js gives it**
  ([#247](https://github.com/txjmb/jsonata-core/issues/247),
  [#250](https://github.com/txjmb/jsonata-core/issues/250)). jsonata-js's trampoline applies
  it with the input of the function's own application: null when the function is a
  callback of a builtin such as `$map` or is applied by `~>` to a non-call, and the call
  site's input for a direct call. It used the function body's input instead, so
  `$map(["a"], function($e){ $string() })` gave the input instead of `"null"`,
  `$map(["a"], function($e){ $eval("a") })` gave 5 instead of undefined, and
  `($f := function(){ $eval("a") }; {"a":7}.$f())` read the outer input instead of
  `{"a":7}`. This affects only what such a builtin reads from the context: `$eval`'s
  focus, and a signature's `-` parameter left without an argument. A builtin that is
  itself a function's tail call applies the functions it is given with the input they were
  defined with, as jsonata-js's trampoline passes them unwrapped:
  `($o := function(){ $map([1], function(){ $eval("a") }) }; $o())` is 5. Known gap
  ([#254](https://github.com/txjmb/jsonata-core/issues/254)): jsonata-js wraps a function
  passed as an argument of a direct call in a closure that applies it with a null input,
  which is not modelled yet, so
  `($h := function($f){ $f("x") }; $h(function($e){ $eval("a", $nope) }))` gives 5 instead
  of undefined.
- **A string literal heading a path reads that field under its predicates, group-by, sort or
  bindings.** `"s"{k: v}.p`, `"s"[0].k`, `"s"#$i.k` and `"s"^(v).k` read field `s`, as in
  jsonata-js, whose `.` makes every string step of a path a name; only a bare `"s".k` did, so
  these read the string itself and gave undefined. `"s"{k: v}.%` is the input (it was S0217).
- **`~>` into a call of an expression's value passes the left side as the first argument.**
  `"x" ~> $uppercase[0]()` is `"X"` and `1 ~> $o[1]()` calls `$o[1]` with 1, as in
  jsonata-js; only a call of a named function got the left side, so these called the function
  without it (T0410) or applied its result (T2006).
- **A called path naming a function without its `$` raises T1005**, as in jsonata-js:
  `uppercase#$i()`, `uppercase[0]()` and `($x := 1; x#$i())` suggest `$uppercase` and `$x`
  instead of raising T1006. As upstream, the hint is given when the path gives nothing and
  its first step names a builtin or a binding with a JavaScript-truthy value, and not when
  that step is a quoted string after `.` (`a."uppercase"[0]()` is T1006).
- **Calling a quoted string step after `.` calls the string, raising T1006.** In jsonata-js,
  `(` takes `"f"` (or `"f"[0]`, `"f"#$i`) as its procedure before `.` makes the path's string
  steps names, so `$o."f"("a")` and `arr."uppercase"()` are T1006; they called the function in
  field `f`, or raised T1005.
- **Calling a step with an index or focus binding calls what the step gives.**
  `($o := [function($x){$x}]; $o#$i(3))` and `$o.g[0]#$i(3)` are 3, as in jsonata-js, where
  `#` and `@` bind tighter than `.` and `(` takes the bound step as its procedure; they were
  parse errors (T1006). A bound last step of a longer path is called in each context item.
  `$o[0]@$v(3)` now raises the S0215 that `$o[0]@$v` does, instead of T1006.
- **A filtered array constructor inside an array constructor keeps its nesting**
  ([#249](https://github.com/txjmb/jsonata-core/issues/249)), as in jsonata-js, which pushes
  a member that is an array constructor whole, with any predicates or `[]` after it.
  `[[0..9][$ % 2 = 0]]` is `[[0,2,4,6,8]]` and `[[1, 2, [3, 4]][-1]]` is `[[3,4]]` (they were
  `[0,2,4,6,8]` and `[3,4]`), and so for `[[1,2][]]`, `[[[1,2]][0]]`, `[1,[2,3][$>0]]` and
  a constructor in an object value; `$count([[1,2][$>0]])` is 1 (it was 2). A member that is
  a path is still spliced in: `[([1,2])[$>0]]`, `[[1,2]#$i]` and `[[3,1,2]^($)]` are flat.
- **A filter on a step that gives nothing is still evaluated once**, as in jsonata-js, whose
  `evaluateFilter` filters `[undefined]` there: `x[$string(1,2,3)]` raises T0410 and
  `x[[a()]]` T1006 when `x` is missing (they gave undefined). The result is still undefined,
  and a filter that selects nothing (`x[false]`, `x[0]`) stops the later ones. As there, a
  sort step's filters do not run when the sort gives nothing (`x^(a)[$error("e")]`), nor do
  those on the call after `~>`. A filter that calls nothing (`x[$ > 5]`) is not run on an
  item without the field, as it cannot raise an error there.
- **Calling a bound literal is the same T1006 as calling the literal.** `"f"@$v()`,
  `a.1@$v()` and `a.true#$i()` raise T1006 while parsing, as `"f"()` and `a.1()` do, ahead of
  a literal-step S0213 or another member's error (`[$string(1,2,3), "f"@$v()]`). A bound
  `.(expr)` step is called as its block: `a.(1)@$v()` is T1006 (it had no error code). A `@`
  on a sort or group-by under a unary minus after `.` comes after that `.`'s S0213
  (`true.-a^(x)@$v` is S0213), as both bind looser than `.` in jsonata-js.
- **A misplaced `@` binding is reported before a literal path step**, as in jsonata-js,
  whose `processAST` checks a `@` step before the `.` around it checks its steps:
  `1[0]@$v.**.q`, `null[0]@$v.$append(a.c.$, n[])` and `1.a[0]@$v` are S0215 and
  `1^(x)@$v.a` is S0216 (they were S0213 since the S0213 check below covered every step).
- **A literal used as a path step is S0213 wherever it is in the path**
  ([#248](https://github.com/txjmb/jsonata-core/issues/248)), as in jsonata-js, which checks
  every step of a path built by `.`. Only a literal right of a dot was rejected, so `null.$`,
  `true.$`, `2#$q.a`, `null@$x.a`, `2[0].a`, `null^(x).a` and `null.(1)` were evaluated, and
  `a.-2` gave `[-2,-2]` (jsonata-js folds `-2` into a number first). A number followed by `.`
  and no digit is now the number and a path step, as in jsonata-js's tokenizer, so `2.a` and
  `1.5.a` are S0213 too (`2.a` was "Invalid number"), and an exponent with no digits (`2e`)
  is S0201. The same check turns a string step into a field name anywhere in the path:
  `"a"[0].b`, `"a".(b)`, `"a".[b]` and `"a"#$i.b` read the field `a` (they used the string).
- **Creating a function with a deeply nested body no longer crashes deep in a recursion.**
  Creating a function clones its body and collects its free variables, two recursive walks
  over the body that ran without the stack guard deep evaluation uses. A body nested a few
  hundred levels deep, created inside a recursion thousands of calls deep, could overrun
  the stack headroom left there and crash the process (a native stack overflow, seen on
  Windows with a 300-level body). Both walks now move to a new stack segment when the
  stack runs low, as the value walkers do.
- **A null sort term raises T2008 whatever the term's form**, as in jsonata-js. A term that is
  not a plain field name (`a.b`, `$.a`, a function call) turned a null result into undefined,
  so `[{"a":{"b":2}},{"a":{"b":null}}]^(a.b)` and `[{"a":2},{"a":null}]^($.a)` returned the
  input unsorted; a plain field such as `^(a)` already raised T2008. The null is now kept, and
  jsonata-js's comparator raises T2008 when it compares it with another defined term. A null
  beside only undefined terms, or in a one-item sort, is never compared, so it does not raise
  and sorts before the undefined ones: `[{},{"a":null}]^($.a)` is `[{"a":null},{}]` (it was
  `[{},{"a":null}]`).
- **Calling an indexed variable calls the function it gives.** `($o := [function(){1}];
  $o[0]())` is 1 and `$string[0](1)` is `"1"`, as in jsonata-js, which evaluates a call's
  procedure expression and calls the result; they were parse errors ("Invalid function name"),
  and T1006 in v2.2.11, so `$exists($o) ? $o[0]() : "none"` failed to compile instead of giving
  `"none"`. An indexed field as a path's last step is called in each context item, as `(` binds
  tighter than `.` in jsonata-js: `$o.g[0]()` gives what `g[0]` returns (it was T1006). An
  index that gives no function is still T1006 (T1008 for a partial application).
- **A group-by on a leading array constructor followed by another step is undefined again**,
  as in jsonata-js and v2.2.11. jsonata-js groups on the constructor node and evaluates that
  step once, so the next step reads the grouped object as an input with no items. Since the
  path-group change for [#208](https://github.com/txjmb/jsonata-core/issues/208) the step ran
  once per item instead: `[1,2]{"k": $}.k` gave `[1,2]` and `[1,2]{$string($): $}.[nothing]`
  gave `[]`. These, `[1,2]{"k": $}.{"z": $}`, `[1,2][]{"k": $}.k` and a group on a block
  starting with a constructor (`([1,2].$){"k": $}.k`) are undefined. `[1,2]{"k":$}` is still
  `{"k":[1,2]}`, `([1,2]){"k":$}.[5]` is still `[5]` and `[1,2]{"k":$}[0]` is still S0209.
- **Freeing a very deeply nested value no longer overflows the stack**
  ([#233](https://github.com/txjmb/jsonata-core/issues/233)). A value was dropped by plain
  recursion, one stack frame per level, so an array or object nested more than about 10,000
  deep (easy to build with `$reduce`, e.g. `$reduce([1..100000], function($a, $i){ {'a': $a}
  }, {})`) crashed the process when it was freed. So did reading down a deep Python input
  (`$reduce([1..99999], function($x, $i){ $x.a }, $)` over `{"a": {"a": ...}}`), as each lazy
  dict wrapper holds the next. The payloads behind an array's and an object's `Rc`, and lazy
  dict wrappers, now check the stack before freeing values that hold containers, and once it
  runs low they take the rest of the value apart on a heap work-list instead of recursing. A
  small payload that frees no container (the common leaf object) skips the check after a
  look at its values; a lazy Python dict wrapper records whether it holds a container, and
  a chain of wrappers read down from one another checks once every 32 levels. Only what
  this drop frees is taken apart; a shared value is just released. Callgrind on the A/B
  benchmark expressions over Python input, against main with the allocator change above
  (instructions per evaluation from the difference between 600 and 100 evaluations): at
  most 0.72% more (the 5-level path lookup on the tree-walker, under 100 instructions),
  0.5% to 0.65% more on the `%`, `#`, `@` and top-rated expressions, and fewer on about half.
  Rust API: `JValue::Object` holds `Rc<JMap>` instead of `Rc<IndexMap<String, JValue>>`, and
  `JArray`'s items sit in a `JItems`. `JMap` dereferences to the `IndexMap`, and `JItems` to
  `Vec<JValue>`; both iterate by reference, and `JMap` collects from `(String, JValue)` pairs.
  `JValue::object` still takes an `IndexMap`. What breaks: `JValue::Object(Rc::new(map))`
  needs `Rc::new(map.into())` or `JValue::object(map)`; `Rc::try_unwrap`, `Rc::make_mut` and
  `Rc::unwrap_or_clone` on an object's payload yield a `JMap` (use `.into_inner()`, or use it
  through deref); `LazyPyDict::to_object()` returns `Rc<JMap>`. Not covered: closures. A
  chain of tens of thousands of closures, each capturing the previous one, is still freed by
  recursion.
- **A function body resolves names where the function is defined, not where it is called**
  ([#191](https://github.com/txjmb/jsonata-core/issues/191)). A binding in the caller's scope
  leaked into the body: `($f := function($x){ $string($x) }; ($string := function($x){"z"};
  $f(1)))` is `"1"` as in jsonata-js (it was `"z"`), and `($g := function(){ $v }; $f :=
  function($v){ $g() }; $f(1))` is undefined (it was `1`). The same holds for transforms,
  partial applications, tail calls, and internal temporaries such as `$__temp_arg_0`, and to a
  closure returned from another function, which resolves through its maker's defining scope and
  never its maker's callers. While the defining scope is still running, a binding made there
  after the function is seen too; once that scope has finished, only the bindings the function
  captured when it was defined remain (jsonata-js keeps the whole scope alive, so
  `($f := ($g := function(){ $y }; $y := 5; $g); $f())` is 5 there and undefined here). A partial
  application such as `$uppercase(?)` now resolves its function when it is made, as jsonata-js
  does: rebinding `$uppercase` afterwards no longer changes it, and partially applying an
  unbound name is T1008 when the partial is made. Every compiled fast
  path now checks builtin shadowing through one rule, so `$map`/`$filter`/`$reduce` callbacks,
  object constructors, predicates and fused aggregates call a builtin only where the
  tree-walker would; a parameter named like a builtin (`function($string){ $string(1) }`) now
  shadows it there too. Calling a name bound to something that is not a function, such as
  `$string := 5; $string(1)`, is T1006 (T1008 when partially applied) instead of calling the
  builtin. Deep recursion no longer slows down with depth when its body calls a builtin, since a
  lookup skips the caller frames. `$eval` runs in its caller's scope without seeing the
  evaluator's temporaries, and a binding it makes lands there, even when it is reached through
  `$map`: `($m := $map; $m(["$w := 3"], $eval); $w)` is 3. A composition `f ~> g` calls g in
  the scope it is called from and f in a call frame of its own below the scope where `~>` was,
  as jsonata-js's `chainAST` does, so `($x := 7; ($string ~> $eval)("$x"))` is 7 (it was
  undefined) and `($c := $eval ~> $string; $c("$z := 5"); $z)` stays undefined. g's call is a
  tail call, so `($inc ~> $loop)($n)` in tail position loops without growing the stack. `$eval`
  as a lambda body's tail call runs where the chain of tail calls began, as jsonata-js applies
  a tail call's thunk: `($v := "root"; $h := function(){ $eval("$v") }; ($v := "caller";
  $h()))` is "caller", while `($r := $eval("$v"); $r)` in the body still reads the body's own
  scope. A tail-call loop reuses its frame for the next pass unless a function that can see
  the frame outlives the pass; functions the pass makes and drops, including ones kept in
  local arrays, objects or other functions, or tail-called, do not count, so such loops run in
  constant memory. A function that does live on, such as one passed to the next pass, keeps
  its frame, so a loop that passes a new closure on each pass still grows with the number of
  passes. That is a cost of lexical scoping, as in jsonata-js: a loop that collects a closure
  on each pass into an accumulator (`$f($k - 1, $append($acc, $c))`) keeps every pass's frame
  with all its bindings, the accumulator so far included, so its memory grows with the square
  of the number of passes (about 1.1 GB at 5,000 passes, where it was 41 MB). A builtin such as
  `$map` run as a tail call applies its function arguments where the chain of tail calls began,
  as jsonata-js does (`$sort` excepted, since jsonata-js fails that call with "comp is not a
  function"); recursion through such a callback, or through a tail-called composition, goes as
  deep as other recursion and is U1001 without a base case, never a native stack overflow.
  Still different from jsonata-js: the right side of `?:` and `??` is not a
  tail position here; a partial application of a user function does not carry the chain of
  tail calls, and a `$eval(?)` partial runs where it is called, not where it was made; function
  arguments of a builtin called outside a tail position are applied where the builtin runs, not
  in jsonata-js's wrapper over the caller's environment; a tail `$eval` takes the call
  site's context as its input; and a composition's first function flattens one more level of
  nested one-item sequences than jsonata-js when its result comes from such a tail-called
  builtin (`($r ~> $string)(4)` with a `$r` nesting `$map` is `"[[0]]"`, `"[[[0]]]"` there).
- **A sort over one item gives that item, not a one-item array**
  ([#221](https://github.com/txjmb/jsonata-core/issues/221)). jsonata-js sorts as a step after
  its input, which flattens the input into a sequence, and returns a sequence of zero or one
  item as is, so it is unwrapped: `acct.Order[1].P^(N)` is `{"N":"C"}` (it was `[{"N":"C"}]`),
  `[{"N":"C"}]^(N)` likewise, and a sort over an empty array is undefined (it was `[]`). A `[]`
  on the sorted path keeps the array (`P[]^(N)`, `P^(N)[]`, `$#$i[]^($)`); a `[]` on a block,
  variable or constructor before the sort (`$x[]^(p)`, `$x[]#$i^(p)`) does not. A sort whose
  input is not a path (a block, literal, function call or array constructor) evaluates it once
  per item of an array input, as a path's first step: over `[{"a":[2,1]},{"a":[3]}]`,
  `(a[0])^($)` is `[2,3]` (it was `2`) and `$count(a[0])^($)` is `2` (it was `1`). A sort of
  objects that only look like internal tuples (`{"@": 2, "__tuple__": true}`) now orders them as data instead of leaving
  them unsorted. A group-by after a sort groups a single array as one item (`z^($){"k": $}` is
  `{"k": [5]}` for `z: [[5]]`), an object constructor inside a `.( … )` block reads `$` as its
  own context (`[1].([[5]].{"v": $})` is `{"v": 5}`), and a field sort term over an array item
  reads its items' fields, so `[[{"p":2},{"p":1}],[{"p":3}]]^(p)` raises T2008 as in jsonata-js.
  The order-by clause sorts with jsonata-js's merge sort and comparator: a missing term sorts
  last in either direction (`[{"p":1},{"q":1}]^(>p)` keeps its order; it put `{"q":1}` first),
  and is checked before the types, so `[{"p":null},{"q":1}]^(p)` keeps its order instead of
  raising T2008. A leading array constructor with a focus or index binding is evaluated once,
  the binding ignored, when a `.` follows it (jsonata-js `consarray`), so `[$]@$w.($)` over
  `[1,2,3]` is `[1,2,3]` and `[a]@$w.{"v": $}` over `[{"a":1},{"a":2}]` is
  `{"v": [{"a":1},{"a":2}]}`.
- **A higher-order function evaluates its callback once, to a function value**
  ([#204](https://github.com/txjmb/jsonata-core/issues/204)). `$map`, `$filter`, `$reduce`,
  `$single`, `$sift`, `$each` and `$sort` evaluate a callback that is not an inline lambda or a
  variable once, then call the function it gives with as many arguments as it takes, as
  jsonata-js's `hofFuncArgs` does. Before, the expression was evaluated again for each item
  with the item as context, so `$map([1,2], ($string))` and `$map([1,2], [$string][0])` gave
  functions instead of `["1","2"]`, `$filter([1,2], (function($v){$v>1}))` was undefined
  instead of 2, and a host function inside the callback ran once per item. `$reduce` raises
  D3050 for any callback taking fewer than two arguments, so `$reduce([1,2], $sum ~> $string)`
  and `$reduce([1,2], $string)` are D3050. The nine higher-order builtins have their
  jsonata-js arities, so `$map([[3,1]], $sort)` hands `$sort` two arguments.
- **A regex literal is a function value**
  ([#205](https://github.com/txjmb/jsonata-core/issues/205)). As in jsonata-js, `/re/` called
  with a string gives its first match as `{match, start, end, groups, next}`, with `start` and
  `end` in UTF-16 code units, `groups` always present and `next()` giving the following match
  (undefined after the last, D1004 for a zero-length one). It can be bound
  (`($f := /a/; $f("xa"))`), passed as a callback (`$filter(["ab","cd"], /b/)`), composed on
  either side of `~>` (`$sort ~> /a/`), and passed to `$match`, `$replace`, `$split` and
  `$contains` from a variable. `$type(/a/)` is `"function"`, `$string(/a/)` is `""`,
  `$boolean(/a/)` is false, and a regex returned to Python is `None`. `"xa" ~> /a/` goes
  through the general `~>` path, so its result has `groups` and `next`. A regex now reads as
  one after `:=`, `(`, `:`, `=`, `?` and the other operators that were read as division
  before (`$string(/a/)` was S0211), `m.next()` calls a field holding a function, and `~>`
  binds at 40 as in jsonata-js, so `"é" & "a" ~> /a/` matches `"éa"`. Deliberate
  divergences: an undefined subject gives undefined (jsonata-js matches the text
  `"undefined"`), and each match's `next()` continues from where that match ended, while
  jsonata-js's continues from wherever the regex last stopped, shared by every match it
  gave: `($m := /a/("xaya"); [$m.next().start, $m.next().start])` is `[3, 3]` here and `[3]`
  in jsonata-js.
- **Non-tail recursion goes about 10,000 calls deep**
  ([#200](https://github.com/txjmb/jsonata-core/issues/200)). Evaluation stopped at 302 levels
  with U1001, so `($f := function($n){ $n <= 0 ? 0 : 1 + $f($n - 1) }; $f(150))` failed where
  jsonata-js gives 150. jsonata-js has no fixed limit; here U1001 now comes at 30,000 levels,
  about 10,000 calls, on both engines. Deep levels run on stack segments allocated as needed,
  so this holds on a 1MB thread stack (Windows' default) too. Converting a deeply nested result
  to and from Python, serializing it, `$string`, `=` and `**` likewise no longer overflow a
  small stack. `max_stack_depth` still raises D1011. A compiled sub-expression nested more than
  128 levels deep now runs on the tree-walker. A Python host function called deep inside a
  recursion gets a stack as large as its caller had, and on Python 3.14 CPython is told where
  that stack is, so its own recursion still ends in `RecursionError` rather than a crash.
- **`$replace` drives a regex or a matcher function as jsonata-js does**
  ([#198](https://github.com/txjmb/jsonata-core/issues/198)). A replacement function's
  `$m.start` and `$m.end` count UTF-16 code units, so `$replace("éab", /a/, function($m)
  {$string($m.start)})` is `"é1b"` (it was `"é2b"`). A zero-length match after the first is
  D1004 with a replacement function too, so `$replace("abc", /x*/, function($m){"-"})` raises
  it (it gave `"-a-b-c-"`), and is still found after the limit is reached. A first match may
  be zero-length, so `$replace("abc", /x*$/, "-")` is `"abc-"` (it was D1004). A custom
  matcher function works as the pattern, with a replacement string or function, and its
  result is checked as `$match` checks it (T1010).
- **`$$name` is the variable named `$name`**
  ([#201](https://github.com/txjmb/jsonata-core/issues/201)). jsonata-js reads everything after
  the leading `$` as the variable name, so `$$` is the variable `$` (the input) and `$$a` is
  the variable `$a`, undefined unless bound. Over `{"a": [1, 2, 3]}`, `$$a` and `$string($$a)`
  are undefined (they were `null` and `"null"`), `($a := 5; $$a)` is undefined, and
  `($$x := 2; $$x * 3)` is 6. `$$` and `$$.a` are unchanged.
- **A lambda called as a path step gets only the arguments written**
  ([#202](https://github.com/txjmb/jsonata-core/issues/202)). The parser no longer passes the
  context item as an implicit first argument, so over `{"a": [1, 2, 3]}`
  `a.function($v) { $v * 2 }()` is undefined (it was `[2, 4, 6]`), as in jsonata-js; pass
  `$` to get the items. A parenthesised step called the same way, as in
  `a.(function($v) { $v * 2 })($)`, now works (it was T1006).
- **Calling an expression applies any function value**, as jsonata-js does. `($uppercase)("x")`
  is `"X"` and `a.($string)()` calls the builtin per item (both were errors). A `?` argument
  partially applies it, so `(function($v, $w) { $v + $w })(?, 1)(5)` is 6, also as the last
  expression of a lambda (`$f := function() { $g(?) }` was an error). Calling a
  non-function, as in `(1)(2)`, raises T1006 (it had no code).
- **`$` may appear inside a name**, as in jsonata-js: `$a$b` is the variable `a$b` and `a$b`
  the field `a$b` (both were parse errors).
- **`@` on a block, `*` or `$` outside a path binds nothing**
  ([#213](https://github.com/txjmb/jsonata-core/issues/213)). jsonata-js only applies a
  binding to a path step, so `(acct.Order.P)@$v` is the three products (it was the input,
  three times), `*@$v[0]` is the first child and `(acct.Order.P){N: N}@$v` groups the
  products. A `.`, sort or `#` after it still makes it a path. `$[]#$i` drops the `[]`, as
  the path `#` builds keeps no singleton array.
- **A `%` filter on a block or `*` outside a path raises S0217**
  ([#211](https://github.com/txjmb/jsonata-core/issues/211)). jsonata-js reports
  `(acct.Order.P)[%.id = "o1"]` and `*[%.id = "o1"]`, which were accepted here. Inside a
  path, as in `(acct.Order.P)[%.id = "o1"].N`, they still work. A `%` on either side of `~>`
  is not passed on, as in jsonata-js, so `%.id ~> $count` is 0 where it raised S0217.
  A filtered group-by under a sort, as in `(acct.Order.P)[%.id = "o1"]{N: N}^(N)`, gives a
  value, and `@` after a filtered group-by, as in `acct.Order.P[0]{N: N}@$v`, raises S0215.
- **A sort over a block's tuple stream unwraps a single or empty result**
  ([#207](https://github.com/txjmb/jsonata-core/issues/207)). `(acct.Order.P[N="C"])^(%.id)`
  is `{"N":"C"}` (it was `[{"N":"C"}]`), and `(acct.Order.P)[N = "Z"]^(>%.id)` is undefined
  (it was `[]`).
- **A `%` looks back through a group-by on a bare `*` or `%`**
  ([#209](https://github.com/txjmb/jsonata-core/issues/209)). `*{N: N}.%` is the whole
  input and `acct.Order.P.(%{"x": id}).%` reaches `acct`, as in jsonata-js, where both raised
  S0217. Inside a block, as in `acct.(*{"k": 1}).%` or `acct.(*).%`, the `%` is undefined.
- **A group-by over an empty tuple stream is `{}`**
  ([#215](https://github.com/txjmb/jsonata-core/issues/215)). `acct.Order.P[%.id = "zz"]{"k": 1}`
  gave `{"k": 1}`. A group-by over an empty plain value still evaluates its pairs once, so
  `nope{"k": $count($)}` stays `{"k": 0}`.
- **`%` inside a group-by no longer raises S0217**
  ([#192](https://github.com/txjmb/jsonata-core/issues/192)). As in jsonata-js, a `%` in a
  group-by's key or value is never resolved and evaluates to undefined, so
  `acct.Order.P{%.id: N}` is `{}` and `acct.Order.P{N: $count(%)}` counts 0. A `%` after a
  group-by, as in `(acct.Order.P{N: N}).%`, seeks through the grouped path. S0217 now uses
  jsonata-js's message: "The object representing the 'parent' cannot be derived from this
  expression".
- **`~>` follows jsonata-js `evaluateApplyExpression`**
  ([#193](https://github.com/txjmb/jsonata-core/issues/193)). The left side is evaluated once
  and passed on even when undefined, so `nope ~> $count()` is `0` and `nope ~> $append([])` is
  `[]` (both used to be undefined). A call on the right gets the left side as its first
  argument. Any other right side is evaluated and must be a function: two functions compose
  wherever they appear, so `arr ~> ($sum ~> $string)` and `$type($sum ~> $string)` now work, and
  `5 ~> nope` raises T2006 instead of giving undefined. A partial application is applied like
  any function, so `"abc" ~> $substring(?, ?)` is `"abc"`. The left side used to be evaluated
  twice, calling a host function on it twice. Two jsonata-js behaviours are deliberately not
  followed: it drops the filter on a call (`arr ~> $map($f)[0]` gives the whole array there),
  and it matches a regex against an undefined left side as the text `"undefined"`.
- **A transform is always a function value that runs when invoked**, as in jsonata-js. It used
  to run at once wherever `$` was bound, so a `~>` into a transform inside another transform's
  update, or `a.(|x|{}|)`, ran it on the wrong input. `$` in a transform's update is now the
  matched object, not the transform's input, and so it is in the delete clause, which is
  evaluated for each matched object as jsonata-js does (it used to be evaluated once, against
  the input). A delete clause that is not a string or an array of strings raises T2012, as
  does one with a non-string item, which used to be skipped, and a transform called with more
  than one argument raises T0410. Like a lambda, it closes over the variables it
  uses, and written directly as a higher-order function's argument it is called, as a
  composition such as `$map(n, $sum ~> $string)` now is.
- **`$sort`, `$single` and the other higher-order builtins can be called by value**:
  `[3,1,2] ~> $sort`, `$f := $single` and `$sort ~> $reverse` used to raise T0410. They now run
  as a direct call does, validating their arguments the same way.
- **A host function named without a call is a function value**, as a builtin is, so
  `5 ~> $dbl`, `$f := $dbl`, `$map(arr, $dbl)` and `$exists($dbl)` work for a function
  registered with `register`. `$exists` on a variable checks its binding first, so a bound
  name that shadows a builtin, such as `($sort := nope; $exists($sort))`, is `false`.
- **A lambda parameter without an argument is undefined**, not `null`, on every path:
  `($f := function($v) { $type($v) }; $f())` used to be `"null"` when the body was simple
  enough to compile, and a tail-recursive lambda bound it as `null` too.
- **`$single`, transforms and partial applications handle undefined as jsonata-js does**
  ([#193](https://github.com/txjmb/jsonata-core/issues/193)). `$single` gives undefined for an
  undefined input and treats `null` as a one-item sequence. A transform gives undefined for an
  undefined input and raises T0410 for one that is not an object or array, however it is
  invoked (`$t(5)` used to return `5`). A placeholder left without an argument is undefined, not
  `null`.
- **`??` propagates errors and coalesces any undefined value**, as jsonata-js does
  ([#195](https://github.com/txjmb/jsonata-core/issues/195)). jsonata-js reads `a ?? b` as
  `$exists(a) ? a : b`. `$error("x") ?? 1` used to swallow the error and give `1`, and the right
  side could then see variables left bound by the failed left side. An undefined left side that
  isn't a path or a variable, such as `$lookup({}, "a") ?? 3`, used to give undefined instead of
  `3`.
- **Each evaluation runs in a scope of its own**, as jsonata-js runs each in a new frame. An
  `Evaluator` reused through the Rust API used to keep what an earlier evaluation bound: a
  top-level `$x := 1`, and on a failed evaluation any scope or temporary `$` an error skipped
  popping. Bindings set on the `Context` beforehand are still visible. `$$` is each evaluation's
  own input; a reused evaluator used to keep answering it from the first.
- **A `.(...)` step's `:=` bindings stay in the step.** `($y := 1; a.($y := 5); $y)` is `1`, as
  in jsonata-js; it used to be `5`. In the Rust AST, a `.(expr)` step is now always
  `FunctionApplication(Block([...]))`, even with one expression, so it can be told apart from a
  `.$fn(...)` step, whose `:=` arguments still bind outside the step.
- **A builtin's arguments are all evaluated.** Two leftover checks returned early: one for an
  unbound variable argument, which skipped the other arguments' errors and signature checks and
  gave `null` rather than undefined (`{"x": $string($nope)}` is now `{}`); and one that
  evaluated a path argument a first time, discarding any error, before evaluating it again.
- **`$match` returns every match**, as jsonata-js does ([#194](https://github.com/txjmb/jsonata-core/issues/194)).
  jsonata-js regexes are always global, so `$match("aba", /a/)` gives both matches, not only
  the first, and the limit argument caps how many are returned. Each match object now always
  has `groups`, and an unmatched capture group in it is undefined, not null. `index` counts
  UTF-16 code units like JavaScript strings, not bytes, so it changes for text before the match
  that isn't ASCII. A negative limit raises `D3040`, and a regex that would match a zero-length
  string again raises `D1004`. A custom matcher's results are checked as jsonata-js checks them,
  so a malformed result raises `T1010`, and a limit of 0 no longer calls the matcher.
- **A fractional limit in `$match` and `$replace` rounds up**, as in jsonata-js, which keeps
  matching while the count is below the limit. `$replace("aaa", "a", "b", 1.5)` is now `"bba"`.
- **Field paths in lambda bodies map over arrays.** `($f := function() { objs.p }; $f())` was
  undefined when `objs` held an array; it is now `[1, 2]` as in jsonata-js.
- **Compiled predicates read the right field from objects with different key orders.** A
  predicate inside a lambda body, such as `xs[a > 2].b`, read fields by the first object's key
  positions and so tested the wrong field on objects ordered differently.
- **Index binding on a sort numbers the sorted items**: `arr^($)#$i` is the sorted array and
  `$i` its positions, where it used to return the input object.
- **Object keys that are null or repeated raise jsonata-js's errors**: a null key is `T1003`
  (it used to be skipped), and two pairs with the same literal key are `D1009` (the later
  value used to win).

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

## [2.2.11] - 2026-10-02

### Added

### Changed
- GitHub Release notes are now the version's `CHANGELOG.md` section (via the new
  `scripts/release-notes.sh`) instead of a list of every commit subject since the previous tag,
  which included merge commits, superseded intermediate steps and unrelated housekeeping. The
  notes also link to the full commit comparison and include the Cargo install line.
- Updated Rust dependencies: `thiserror` 2.0.20 → 2.0.21 and `rand` 0.10.2 → 0.10.3. (#175)

### Deprecated

### Removed

### Fixed
- The release workflow now updates the `jsonata-core = "…"` install snippet in `README.md` when
  it bumps the version. It previously went stale on every release and needed a manual follow-up
  commit. The snippet is updated to 2.2.10 here.
- `$match` now returns undefined (an empty sequence, as in jsonata-js) instead of `null` when a
  regex matches nothing. Since 2.2.8 made `$exists(null)` true, `$exists($match(...))` was true and
  `$count($match(...))` was 1 for a failed match; they are now false and 0, and `[$match(...)]`
  is `[]`. (#176, #177) Thanks to @MonkeyChap for the problem identification and fix.

### Security
- Updated the locked Python dev dependencies `pyjwt` 2.13.0 → 2.15.1 and `urllib3` 2.7.0 → 2.8.0,
  clearing 15 advisories that `pip-audit` reported against `uv.lock` and that were failing the
  Security Scanning workflow. Both are transitive dependencies of optional extras (`fastmcp` and
  the docs tooling), not of the published crate or of the wheel, which has no runtime
  dependencies. This only changes the repository's lockfile; users of the `mcp` extra resolve
  their own versions and should upgrade `pyjwt` to 2.15.1 or later. (#178)

## [2.2.10] - 2026-09-24

### Added
- `SECURITY.md` security policy: security fixes ship as a new patch release of the latest
  minor line (currently 2.2.x) with no backports, and vulnerabilities are reported privately
  through the repository's Security tab rather than in public issues. (#171)
- Benchmark tables gain a per-row **jsonata-core (pure Rust)** column, measured by a new
  `benchmarks/rust` harness that evaluates this project's engine as a Rust library with no
  Python boundary. The column previously labelled `jsonatapy (rust)` is relabelled
  `jsonatapy (json I/O)`: it was always the `evaluate_json` path of the same engine, not a
  separate Rust implementation. (#162)
- The README's performance section now shows an auto-generated chart of the "Realistic
  Workload" benchmarks, regenerated alongside `docs/performance.md` on each release. (#165)
- `CONTRIBUTORS.md`. (#167)

### Changed
- Updated Rust dependencies: `simd-json` 0.18.0 → 0.18.1 (#163), `indexmap` 2.14.0 → 2.14.2
  (#168), and `clap` 4.6.6 → 4.6.7, used only by the `jsonata` CLI (#169).
- CodeQL now runs through GitHub's CodeQL default setup, covering Rust, Python, C/C++,
  JavaScript/TypeScript and GitHub Actions, instead of CodeQL jobs in the Security Scanning
  workflow. (#170, #171)
- The Security Scanning workflow now audits the resolved `uv.lock` with `pip-audit`, not just
  the packages installed on the runner, and fails on a vulnerable locked dependency. It also
  lists `cargo-audit` warnings such as yanked crates, without failing the build on them. (#170)

### Deprecated

### Removed

### Fixed
- The default docs URL (`/stable/`) served the previous release's performance page: the
  release job deployed docs from the release tag, which never contains that release's own
  benchmark numbers. Release docs now take `docs/performance.md` from `main` after the
  benchmark job records it, and the docs workflow can republish the released docs line
  without a new release. (#164)
- Resolved all 18 open CodeQL code-quality alerts in the benchmark and test scripts, and
  implemented the previously stubbed `test_compile_invalid_expression` test. (#170)

### Security
- `Cargo.lock`: `chacha20` 0.10.1 → 0.10.2. 0.10.1 was yanked; it reaches the build through
  `rand` and was compiled into the 2.2.9 Python wheels and CLI binaries. (#170)
- `uv.lock`: `anyio` 4.14.1 → 4.15.1 (CVE-2026-63374, CVE-2026-64847, CVE-2026-63349),
  `cryptography` 49.0.0 → 50.0.1 (PYSEC-2026-3552), and `mkdocs-material` 9.7.6 → 9.7.7
  (PYSEC-2026-3864). These are development and documentation dependencies only; the published
  `jsonatapy` package has no Python dependencies and was not affected. (#170)

## [2.2.9] "Perform-ata" - 2026-08-31

### Changed
- Lambda values now carry their closure directly — `JValue::Lambda(Rc<StoredLambda>)`
  instead of a `lambda_id` name tag resolved through a per-scope side table (#157).
  The `Rc` provides the closure's lifetime, so the side table (`Scope.lambdas`),
  the escape-analysis GC (`extract_lambda_ids`/`collect_lambda_ids` walking every
  block/lambda result, `pop_scope_preserving_lambdas`) and the dual-map handling in
  `unbind`/`clear_current_scope` are gone, along with the O(result size) walk on
  every scope pop. Recursion is late-bound letrec: a closure defined as
  `$f := function ...` records its own name and binds it to itself at each
  invocation, so no `Rc` cycles exist and closures cannot leak (guarded by a
  live-count test). Free-variable capture remains a by-value snapshot at
  definition time, now applied uniformly to lambda-valued variables too.
  Behavioral fixes (all matching jsonata-js, pinned in
  `tests/lambda_closure_suite.rs`): a transform bound to a variable is callable
  and pipeable (`$t := |...|...|; $t(x)`, `x ~> $t`); a partial application
  escaping the scope that defined its target function works; an alias
  (`$g := $f`) survives `$f` being rebound; calling a name rebound to a
  non-function raises T1006 instead of silently invoking the stale lambda; a
  closure captured inside another escaping closure's environment no longer
  dangles. Function equality is now closure identity rather than id-string
  equality. Remaining known divergences from jsonata-js's live-frame capture
  are documented in the same suite's `divergent_from_reference` module.
- Partial application is a typed representation instead of a stringly-typed
  protocol: `StoredLambda` carries an optional `PartialApplication { target,
  bound_args, placeholder_positions, total_args }`, replacing the
  `"__partial_call:name:bool:n"` marker-string body that was parsed at every
  invocation and the `__bound_arg_N` / `__placeholder_positions` /
  `__total_args` / `__partial_target` values smuggled through `captured_env`.
  A user-defined target is a captured closure (`PartialTarget::Lambda`);
  builtins/host functions stay name-resolved at call time
  (`PartialTarget::Named`), preserving call-time shadowing. Partials no longer
  snapshot the entire environment at creation (the old
  `capture_current_environment()` call was only ever a smuggling container),
  and one behavior corner now matches jsonata-js: rebinding a user function
  after partially applying it no longer changes what the partial calls
  (pinned in `tests/lambda_closure_suite.rs` with eight other partial pins).

### Added
- `jsonata_set_limits` on the C ABI: wall-clock timeout (D1012), max AST recursion
  depth (D1011), and max sequence length (D2015) — the same three guardrails the
  Python bindings expose. C embedders previously had no way to bound a runaway
  expression at all (the cleanup review's finding); 0 = unlimited, resettable per
  handle. Covered by the C/C++ smoke tests and a Rust-side capi test.
- `jsonata_core::Expression` — a compile-once API for Rust callers that runs
  compilable expressions on the bytecode VM, exactly like the Python and C
  bindings always have. Until now the VM was unreachable from the pure-Rust
  surface: `Evaluator` always tree-walks, so Rust (and CLI) callers silently
  got the slower path. `Expression::compile(src)?.evaluate(&data)` lowers to
  bytecode lazily and falls back to the tree-walker for non-compilable
  expressions; bindings and host functions still go through `Evaluator`. The
  dispatch now lives in one place (`expression::run_compiled`), shared by the
  Rust API, the Python bindings' `run_eval`, and the C ABI (which previously
  carried a hand-copied duplicate of it), and the CLI uses it when no `--arg`
  bindings are given. The `vm`/`compiler` modules are unconditionally compiled
  again (the cfg gates added earlier in this cycle are gone — the default
  build now has a real consumer).

### Changed
- Python→Rust data conversion — the cost that dominates `evaluate(dict)` on array-heavy
  inputs — is faster on two fronts. `lazy::convert` now dispatches on the exact type object
  and iterates lists via borrowed references instead of pyo3's instance-check chain
  (subclasses still take the original chain, so semantics, error types included, are
  unchanged; pinned by `tests/python/test_convert_fastpath.py`). And the published wheels
  now use mimalloc as the global allocator (new opt-in `mimalloc` cargo feature, enabled in
  `pyproject.toml`), roughly halving the per-list/per-object allocation cost that JValue's
  Rc-per-container representation makes the conversion floor. Measured together on the
  benchmark suite's shapes: "Nested Array Access" (`data[1][1][1][1]`, the one row where
  jsonata-js was still ahead) drops ~35% (12.0µs → 7.8µs on the dev container), the four
  `lazy_check.py` gate rows drop 10–17%, and string-heavy conversion roughly halves. The
  structural fix for the nested-array row — lazy list views — remains deferred (see the
  2026-07-12 lazy-views spec's Limitations).
- The `vm` and `compiler` modules (the bytecode pipeline) are now cfg-gated on the features
  that actually consume them (`python`, `capi`, `bench`) — they were compiled but unreachable
  in a default-feature build, producing 12 permanent dead-code warnings that CI's
  `--all-features` clippy never saw. A default `cargo build` is now warning-clean, and both
  modules carry a header note naming their consumers. List and dict *subclasses* now take the
  boundary converter's fast path too (`PyList_Check`/`PyDict_Check`; iteration was already
  C-level for them, so behavior is unchanged), which let the duplicate slow-path container
  loops be deleted.

### Changed (evaluator internals)
- The vestigial `*_is_explicit_null` plumbing is gone: since the null/undefined split
  (#32) a runtime `JValue::Null` is always an explicit null, yet the flags were still
  computed and threaded through ~20 signatures — nine arithmetic/comparison wrapper
  methods, the `CompiledExpr::ExplicitNull` variant (now `Literal(Null)`), a
  `PushExplicitNull` instruction, and per-instruction bool pairs on the VM's
  arithmetic/comparison opcodes — with both terminal consumers ignoring them (~200
  lines removed). Arithmetic and ordered comparison now have exactly one
  implementation each, shared by the tree-walker, the compiled path, and the VM; as
  part of that, the compiled/VM paths' T2010/T2009 error texts now match the
  tree-walker's richer messages ("Cannot compare X and Y", operator symbol in T2009)
  instead of the generic "Type mismatch in comparison" — same codes, better text, and
  the two engines no longer disagree. `Evaluator::is_truthy` likewise now delegates to
  the shared `compiled_is_truthy` instead of maintaining a twin (#111 had to be fixed
  in both).
- More single-definition consolidation: the two verbatim 45-line signature-error
  translation blocks (direct vs TCO lambda invocation) are one `coerce_lambda_args`
  helper; the five copies of jsonata-js's `hofFuncArgs` argument shaping are two
  helpers (`hof_array_call_args` for $map/$filter/$single, `hof_object_call_args` for
  $sift/$each); the six copy-pasted encoding builtin arms share `unary_string_encoding`.
  `signature.rs` dropped its dead `return_type` field and test-only constructors, and
  a builtin signature that fails to parse now panics at cache build instead of leaving
  that builtin silently unvalidated. `ast_transform.rs`'s 157-line module header — a
  stale task journal whose ~20 line references were all wrong — is a 30-line statement
  of the actual recursion/drop invariants, and `push_ast_node_children`'s leaf arm
  lists every variant explicitly instead of `_ => {}`, so a future `AstNode` variant
  cannot silently opt out of the stack-overflow-on-drop guard.
- Five hand-maintained parallel lists of builtin names (the pure set, the arity table,
  the compilable set, the signature table, and a test's own literal copy) are one
  sorted `builtins::BUILTINS` spec table; `is_pure_builtin`, `is_compilable_builtin`,
  `builtin_arity`, and `signature::builtin_signature` all derive from it, and the
  jsonata-js fixture drift tests still police the data. Adding a builtin is now one
  row plus its dispatch arm. Membership and values are unchanged (the table was
  generated mechanically from the lists it replaces).
- The bare-`AstNode::Name` arm of `evaluate_internal_impl` — a private field-access
  copy that had drifted (kept nulls, never flattened, no tuple or lazy-dict handling)
  and that instrumentation shows no expression in any suite reaches — now delegates
  to `compiled_field_step`, the semantics owner for a single field step.

### Deprecated

### Removed
- Repository hygiene: the checked-in `rustup-init.exe` (12.9 MB Windows installer that
  dominated clone size and permanently tripped the file-size lint), the orphaned root
  `node_modules/jsonata/` copy (~830 KB; nothing referenced it — the benchmark harness
  installs its own under `benchmarks/javascript/`, and the reference implementation is the
  `tests/jsonata-js` submodule), and the stale `src/parser/README.md` (every structural
  claim in it — line counts, ranges, even the crate name in its example — was wrong).
- Dead code that survived the 2.2.8 builtins consolidation: `functions::numeric::{max,
  min, average}` and `functions::object::{keys, lookup}` had no callers and encoded
  pre-#109 semantics the engine no longer has (`Null` instead of `Undefined` on empty
  input, no nested-array recursion) — a trap for anyone fixing a `$max`/`$lookup` bug in
  the wrong place. Their tests, which pinned the wrong behavior, went with them. Also
  removed the unused `datetime::parse_iso8601`. Technically these were `pub` items of the
  crate, so strict semver would call this a breaking change; they were never used by the
  engine itself.

### Fixed
- Two families of tree-walker drift found by unifying the remaining field-extraction
  loops onto `compiled_field_step` (each pinned on BOTH engines in
  `tests/python/test_field_step_and_arraygroup.py` with jsonata-js reference outputs):
  the single-step-Name fast path skipped null-valued fields and returned Null on empty
  (`p` over `[{"p":null},{"p":2}]` was `2` on the tree-walker, `[null,2]` on the VM and
  in the reference — the engines disagreed); and the `.[...]` array-group constructor
  kept undefined elements as null (`foo.blah.[baz]` gave `[[..],[null],[null]]` instead
  of `[[..],[],[]]`, masked downstream by that null-skip), while its empty results
  conflated "constructed empty array over a single value" (kept: `{"a":1}.[b]` is `[]`)
  with "mapped over an empty array" (undefined: `emptyarr.[b]`). Both tree-walker
  Name-step loops (the fast path and the general step loop's inner loop) now delegate
  to `compiled_field_step`, deleting ~100 lines of drifted copies; the reference
  suite's `array-constructor/case013`/`case014` now pass by the correct route. The
  fast path's tuple branch (a third copy, with the same null-skip drift) is deleted
  outright — tuple streams fall through to the general step loop's single tuple
  implementation, and `$#$i.p` over `[{"p":null},{"p":2}]` is now `[null,2]` on both
  engines, matching the reference.
- Errors carry their JSONata spec code at the front of the message again: the
  `FunctionError` wrapping used to bury codes behind prose prefixes ("Runtime
  error: D3030: Cannot convert 'x' to number"), so the C ABI, the Rust CLI, and
  the Python CLI each grew a different classifier and disagreed on whether the
  same error was coded. The inner message now passes through unwrapped
  ("D3030: ..."), `EvaluatorError::code()` / `evaluator::error_code_prefix` is
  the one definition of "coded error" (prefix-anchored), and the C ABI and both
  CLIs classify with it — the same failure now presents identically in every
  binary. The C ABI's mid-string code scan is retired (it existed only to see
  through the now-removed prose prefixes).
- Number stringification now matches jsonata-js exactly (verified against the pinned
  reference with a 307-case randomized differential over scalars, containers,
  prettified output, and `&` concatenation). Three divergences fixed: non-integers
  were truncated to 14 significant digits instead of rounded to 15 (`$string(1/3)`
  was `0.33333333333333`, now `0.333333333333333` — the old formatter counted the
  `0` of a leading `0.` as significant); integer-valued floats above 2^53 printed
  their exact i64 digits instead of the float's shortest round-trip decimal; and
  numbers nested in containers went through serde with different rules again.
  `$string` now stringifies through its own writer implementing the reference's
  replacer (functions → `""`, non-integers → `toPrecision(15)`), and one
  `value::js_number_to_string` defines JS number printing (plain digits in
  [1e-6, 1e21), exponential with `+` outside, `-0` as `0`) for `$string`, concat,
  `Display`, and `$join`. Pinned by `tests/python/test_number_stringification.py`.
- `$var.field` over an array containing nested-array elements now recurses into
  them like jsonata-js's `lookup`: `($v := [[{"p":1}],{"p":2}]; $v.p)` is `[1,2]`
  (previously `2` — the tree-walker's two-step fast path hand-rolled its mapping
  loop and silently skipped non-object elements; it now delegates to the shared
  field step). Found while consolidating the seven divergent field-extraction
  loops the cleanup review flagged; pinned by `tests/python/test_var_field_nested.py`.
- Regex literals now honor the `m` (multiline) flag in `$match` and the `~>`
  chain-pipe, matching `$split`/`$replace` and jsonata-js — the two paths
  previously translated only `i` (see `tests/python/test_regex_flags.py`).

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
