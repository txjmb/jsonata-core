// Generates the fast-path differential corpus.
//
// The reference suite has ~1686 cases but only ONE of them is shaped like
// `$agg(array.field)` -- the exact AST shape the fused aggregate fast path
// requires -- and that one declines the fast path. Issue #97 lived in that
// blind spot: a wrong answer that no suite case could reach.
//
// This corpus targets the optimisations themselves. It crosses expressions
// that trigger a known fast path with payloads chosen to break the
// assumptions those fast paths make (non-numeric where a number is expected,
// empty sequences, missing fields, nulls, nested arrays, non-object
// elements). Expected results come from the pinned jsonata-js in
// tests/jsonata-js, so the corpus tracks the same reference the conformance
// suite does.
//
// Usage: node scripts/gen_fastpath_corpus.js
// Writes: tests/fixtures/fastpath_differential.json

const fs = require('fs');
const path = require('path');
const jsonata = require(path.join(__dirname, '..', 'tests', 'jsonata-js', 'src', 'jsonata.js'));
const jsFunctions = require(path.join(__dirname, '..', 'tests', 'jsonata-js', 'src', 'functions.js'));
const jsDatetime = require(path.join(__dirname, '..', 'tests', 'jsonata-js', 'src', 'datetime.js'));
const refVersion = require(path.join(__dirname, '..', 'tests', 'jsonata-js', 'package.json')).version;

// Arity of every builtin that a higher-order function can pass a callback
// by reference (`$map(arr, $uppercase)`). jsonata-js truncates the arguments
// it hands the callback to the *JavaScript function's parameter count*
// (`hofFuncArgs` -> `getFunctionArity` -> `implementation.length`), not to
// the JSONata signature -- `$string` has two signature parameters but arity
// 1, and `$substring` has arity 3, which is why `$map([1,2], $substring)` is
// a T0410 in the reference (arity 3 > the 1 argument $map supplies).
//
// `now` and `millis` are inline closures bound in jsonata.js's `evaluate`
// (tests/jsonata-js/src/jsonata.js:2140 and :2143), not entries in
// functions.js, so their arity is hard-coded here rather than read off a
// `.length`.
const FUNCTIONS_JS_ARITY_NAMES = [
  'abs', 'append', 'assert', 'average', 'base64decode', 'base64encode',
  'boolean', 'ceil', 'contains', 'count', 'decodeUrl', 'decodeUrlComponent',
  'distinct', 'encodeUrl', 'encodeUrlComponent', 'error', 'exists', 'floor',
  'formatBase', 'formatNumber', 'join', 'keys', 'length', 'lookup',
  'lowercase', 'max', 'merge', 'min', 'not', 'number', 'pad', 'power',
  'reverse', 'round', 'shuffle', 'split', 'spread', 'sqrt', 'string',
  'substring', 'substringAfter', 'substringBefore', 'sum', 'trim', 'type',
  'uppercase', 'zip',
];
const DATETIME_JS_ARITY_NAMES = ['toMillis', 'fromMillis', 'formatInteger', 'parseInteger'];

function buildBuiltinArity() {
  const arity = {};
  for (const name of FUNCTIONS_JS_ARITY_NAMES) arity[name] = jsFunctions[name].length;
  for (const name of DATETIME_JS_ARITY_NAMES) arity[name] = jsDatetime[name].length;
  arity.now = 2; // jsonata.js:2140 -- function(picture, timezone)
  arity.millis = 0; // jsonata.js:2143 -- function()
  return arity;
}

// The reference's signature strings live only as literals in its
// `staticFrame.bind(name, defineFunction(impl, '<sig>'))` calls -- nothing
// exports them -- so they are read out of the source. `$now` and `$millis`
// bind to `environment`, not `staticFrame`, and are deliberately absent.
//
// The count assertion is the point: if a future submodule bump changes how
// builtins are registered, this must fail loudly rather than quietly return a
// short list that then reads as "jsonatapy has every signature".
function buildBuiltinSignatures() {
  const source = fs.readFileSync(
    path.join(__dirname, '..', 'tests', 'jsonata-js', 'src', 'jsonata.js'),
    'utf8'
  );
  const bindCount = (source.match(/staticFrame\.bind\(/g) || []).length;
  const pattern =
    /staticFrame\.bind\('([A-Za-z0-9]+)', *defineFunction\([A-Za-z0-9._]+, *'([^']+)'\)\)/g;
  const sigs = {};
  for (const m of source.matchAll(pattern)) sigs[m[1]] = m[2];
  const found = Object.keys(sigs).length;
  if (found !== bindCount) {
    throw new Error(
      `signature extraction found ${found} of ${bindCount} staticFrame.bind calls -- ` +
        'the registration shape in jsonata.js changed; update the pattern above'
    );
  }
  const sorted = {};
  for (const k of Object.keys(sigs).sort()) sorted[k] = sigs[k];
  return sorted;
}

// Payloads. Each keeps the same field names so one expression can be run
// against all of them; the shapes differ in the ways fast paths care about.
const DATASETS = {
  nums:        { arr: [{ p: 1 }, { p: 2 }, { p: 3 }], obj: { a: 1 }, empty: [] },
  mixed:       { arr: [{ p: 1 }, { p: 'free' }], obj: { a: 1 }, empty: [] },
  all_strings: { arr: [{ p: 'a' }, { p: 'b' }], obj: { a: 1 }, empty: [] },
  with_null:   { arr: [{ p: 1 }, { p: null }], obj: { a: 1 }, empty: [] },
  with_bool:   { arr: [{ p: 1 }, { p: true }], obj: { a: 1 }, empty: [] },
  nested_arr:  { arr: [{ p: [1, 2] }, { p: 3 }], obj: { a: 1 }, empty: [] },
  missing:     { arr: [{ p: 1 }, { q: 9 }], obj: { a: 1 }, empty: [] },
  non_object:  { arr: [1, { p: 2 }], obj: { a: 1 }, empty: [] },
  singleton:   { arr: [{ p: 'free' }], obj: { a: 1 }, empty: [] },
  empty_arr:   { arr: [], obj: { a: 1 }, empty: [] },
  obj_first:   { arr: { p: 1 }, obj: { a: 1 }, empty: [] },
  deep:        { arr: [{ p: { q: 1 } }, { p: { q: 2 } }], obj: { a: 1 }, empty: [] },
  // Operand fixtures for the operator matrix below. Each field supplies one
  // *scalar* operand shape; the matrix crosses them so every binary operator is
  // exercised against every value kind on both sides.
  //
  // `obj.nul` is a second, nested explicit null (distinct from the top-level
  // `nul`), needed by the null-context path probes below: `obj.nul.{}` pins
  // the guard that fires on a *step result* being null, not just on a bare
  // field reference being null.
  // `tf`/`ff`/`zero`/`estr` are the remaining scalar kinds, `deep` a
  // nested object, `one` a single-element array and `arrobj` an array of
  // objects. The path-operator matrix below crosses every one of these
  // against `.*`, `.**` and friends: those operators branch on the *kind*
  // of value they are applied to, so a fixture per kind is what makes the
  // coverage systematic rather than anecdotal.
  operands:    { nul: null, arr: [1, 2], obj: { k: 1, nul: null }, emptyarr: [], num: 5, str: 'a',
                 tf: true, ff: false, zero: 0, estr: '', deep: { a: { b: 1 } }, one: [7],
                 arrobj: [{ x: 1 }, { y: 2 }] },
};

const EXPRESSIONS = [];
const add = (fastpath, expr) => EXPRESSIONS.push({ fastpath, expr });

// -- Fused aggregates: $agg(arr.field), the issue #97 family ----------------
for (const agg of ['sum', 'max', 'min', 'average']) {
  add('fused_aggregate', `$${agg}(arr.p)`);
  add('fused_aggregate', `$${agg}(arr[p > 1].p)`);
  add('fused_aggregate', `$${agg}(arr[p].p)`);
  add('fused_aggregate', `$${agg}(empty.p)`);
  add('fused_aggregate', `$${agg}(nope.p)`);
  add('fused_aggregate', `$${agg}(arr.q)`);
  add('fused_aggregate', `$${agg}(arr.p.q)`);
  // Non-fused controls: same function, shapes the fast path declines.
  add('aggregate_control', `$${agg}([1, 2])`);
  add('aggregate_control', `$${agg}(obj.a)`);
}

// -- Two-step path fast path ------------------------------------------------
for (const e of ['arr.p', 'obj.a', 'arr[0].p', 'nope.p', 'arr.p[0]', 'arr.p[-1]', 'empty.p']) {
  add('two_step_path', e);
}

// -- Compiled filter predicates --------------------------------------------
for (const e of ['arr[p > 1]', 'arr[p = 1]', 'arr[p]', 'arr[p != null]', 'arr[$$.obj.a = 1].p']) {
  add('compiled_filter', e);
}

// -- Higher-order function fast paths --------------------------------------
add('hof_map', '$map(arr, function($v) { $v.p })');
add('hof_map', '$map(arr.p, function($v) { $v * 2 })');
add('hof_filter', '$filter(arr, function($v) { $v.p > 1 })');
add('hof_reduce', '$reduce(arr.p, function($a, $b) { $a + $b })');
add('hof_reduce', '$reduce(arr.p, function($a, $b) { $a + $b }, 100)');

// -- Specialized sort comparator -------------------------------------------
add('specialized_sort', '$sort(arr, function($l, $r) { $l.p > $r.p }).p');
add('specialized_sort', '$sort(arr, function($l, $r) { $l.p < $r.p }).p');

// -- Object / array construction -------------------------------------------
add('construct', 'arr.{"k": p}');
add('construct', '[arr.p]');
add('construct', 'arr.p[]');

// -- Arithmetic / comparison explicit-null flags ---------------------------
for (const e of ['arr.p + 1', 'arr.p = 1', 'arr.p < 2', 'arr.p & "x"']) {
  add('compiled_arith', e);
}

// -- Operator matrix -------------------------------------------------------
//
// The fast-path corpus above puts *sequences* on either side of an operator,
// because that is what path expressions produce. That left scalar operands
// untested, and every bug found in that gap belonged to the same family:
// arithmetic, comparison, equality and concatenation each treated an explicit
// null as though it were undefined. `null & "x"` returned "x" instead of
// "nullx" and survived a corpus that reported zero divergences.
//
// So cross every binary operator with every value kind on both sides. 12
// operands x 12 x 15 operators, plus the unary forms.
const OPERANDS = [
  '1',          // number
  '0',          // number, falsy
  '"s"',        // string
  '""',         // string, falsy
  'true',
  'false',
  'null',       // literal null
  'nul',        // null arriving from data, not a literal
  'missing.x',  // undefined
  'arr',        // array
  'obj',        // object
  'emptyarr',   // empty array
];

const BINARY_OPS = [
  '+', '-', '*', '/', '%', '&',
  '=', '!=', '<', '<=', '>', '>=',
  'and', 'or', 'in',
];

for (const op of BINARY_OPS) {
  for (const lhs of OPERANDS) {
    for (const rhs of OPERANDS) {
      add('operator_matrix', `${lhs} ${op} ${rhs}`);
    }
  }
}
for (const x of OPERANDS) {
  add('operator_unary', `-(${x})`);
  add('operator_unary', `$not(${x})`);
}

// -- Builtin matrix --------------------------------------------------------
//
// Same reasoning as the operator matrix, one layer up. A probe of 20 builtins
// against these operands found divergences in 15% of cases, in two families:
// an explicit null treated as undefined (`$count(null)` is 1, not 0;
// `$abs(null)` raises), and a scalar rejected where JSONata coerces it to a
// singleton sequence (`$reverse(1)` is `[1]`). Both are what jsonata-js's
// signature types `l`/`m` and `a` already specify, so the matrix maps how far
// the gap reaches.
//
// Names lifted from the reference's own `staticFrame.bind(... defineFunction)`
// table so the set is its 55, not a guess.
const BUILTINS_ONE_ARG = [
  'sum', 'count', 'max', 'min', 'average', 'string', 'lowercase', 'uppercase',
  'length', 'trim', 'number', 'floor', 'ceil', 'round', 'abs', 'sqrt',
  'boolean', 'not', 'keys', 'exists', 'spread', 'merge', 'reverse', 'type',
  'sort', 'distinct', 'single', 'zip', 'error', 'encodeUrl', 'encodeUrlComponent',
  'decodeUrl', 'decodeUrlComponent', 'substring', 'substringBefore',
  'substringAfter', 'pad', 'contains', 'replace', 'split', 'join', 'match',
  'formatNumber', 'formatBase', 'power', 'lookup', 'append', 'assert',
  'map', 'filter', 'reduce', 'sift', 'each',
];
// `random` and `shuffle` are non-deterministic and cannot be compared.

// Second-argument probes: a well-formed first argument, the matrix in slot two.
const BUILTINS_SECOND_ARG = [
  ['lookup', 'obj'],
  ['append', 'arr'],
  ['power', '2'],
  ['round', '2.5'],
  ['formatBase', '255'],
  ['pad', '"a"'],
  ['substringBefore', '"abc"'],
  ['substringAfter', '"abc"'],
  ['contains', '"abc"'],
  ['split', '"a,b"'],
  ['join', '["a","b"]'],
  ['substring', '"abcdef"'],
  // The four multi-parameter builtins from #126 group 2. Their second
  // parameter was guarded by hand before they had a signature, so the
  // matrix is what makes deleting those guards provable rather than
  // assumed.
  ['toMillis', '"2018-01-01"'],
  ['fromMillis', '0'],
  ['formatInteger', '1'],
  ['parseInteger', '"1"'],
];

const BUILTIN_EXPRESSIONS = [];
for (const fn of BUILTINS_ONE_ARG) {
  for (const o of OPERANDS) BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_one_arg', expr: `$${fn}(${o})` });
}
for (const [fn, first] of BUILTINS_SECOND_ARG) {
  for (const o of OPERANDS) BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_second_arg', expr: `$${fn}(${first}, ${o})` });
}

// -- By-reference matrix: builtins passed as a bare callback to
// $map/$filter/$sift/$each/$single/$sort, e.g. `$map(arr, $uppercase)`. This
// is a THIRD dispatch path (`call_builtin_with_values`) that neither
// BUILTINS_ONE_ARG/SECOND_ARG (which only exercise explicit argument lists)
// nor the hand probes below (a handful of specific shapes) cover
// systematically -- issue #107 stage 2. `now` (embeds a live timestamp) and
// `shuffle` (randomised output) are excluded for the same non-determinism
// reason BUILTINS_ONE_ARG excludes them.
//
// `$single` and `$sort` were missing from this matrix even though they
// dispatch through the same by-reference path as $filter/$sift/$each --
// `$single`'s own HOF call site was the one spot that never consulted
// callback arity at all (hard-coded a 3-arg call), so this matrix could not
// have caught that regression. Keep both here going forward.
const BUILTINS_BY_REFERENCE = Object.keys(buildBuiltinArity()).filter(
  (name) => name !== 'now' && name !== 'shuffle'
);

// Array shapes crossed with every by-reference builtin through $map: the
// empty array, the real two-element numeric array, and single-element
// arrays built from each scalar/container operand -- covering every operand
// *kind* the way BUILTINS_ONE_ARG does for explicit calls, but through the
// untruncated, unvalidated-before-stage-2 by-reference path.
const MAP_ARRAY_SHAPES = ['emptyarr', 'arr', '[nul]', '[num]', '[str]', '[obj]', '[arr]'];
for (const fn of BUILTINS_BY_REFERENCE) {
  for (const shape of MAP_ARRAY_SHAPES) {
    BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_by_reference', expr: `$map(${shape}, $${fn})` });
  }
  // Narrower cross for the other five HOFs that consult
  // get_callback_param_count (or, for $sort, dispatch a builtin comparator
  // through the same by-reference path even though it always calls it with
  // exactly 2 args) -- one representative operand each, matching
  // BUILTINS_SECOND_ARG's "narrower" convention.
  BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_by_reference', expr: `$filter(arr, $${fn})` });
  BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_by_reference', expr: `$sift(obj, $${fn})` });
  BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_by_reference', expr: `$each(obj, $${fn})` });
  // `$single` errors for any predicate over a 2-element array (0 or 2
  // matches, never exactly 1), and the harness only asserts error *kind*,
  // not code/message -- so a 2-element array here would make every case
  // error both before and after a callback-arity regression, blind to the
  // very bug this line exists to catch. `[obj]` is a single-element array,
  // so a truthy predicate produces a real value to compare byte-for-byte.
  BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_by_reference', expr: `$single([obj], $${fn})` });
  BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_by_reference', expr: `$sort(arr, $${fn})` });
}

// Hand-written probes for shapes neither matrix can reach. Both cross one
// varying operand against a fixed partner, which leaves out three-argument
// calls, second arguments that interact with the *content* of the first, and
// builtins passed by reference to a higher-order function -- the last of
// which is a whole dispatch path (`call_builtin_with_values`) that an
// explicit argument list never reaches.
const BUILTIN_PROBES = [
  // An undefined start is NaN in JavaScript's arithmetic, so the presence of
  // the length argument decides the answer: absent, the whole string;
  // present, empty. "Treat undefined as 0" gets the second one wrong.
  '$substring("abcdef", missing.x, 2)',
  '$substring(missing.x, 1)',
  // JavaScript stringifies an undefined search term to "undefined" rather
  // than treating it as absent, which is observable when the subject
  // contains that text.
  '$substringBefore("xundefinedy", missing.x)',
  // No $substringAfter twin: with a separator it finds, the reference then
  // dereferences `chars.length` on the undefined separator and dies of a raw
  // JavaScript TypeError. The generator records that as `kind: 'error'`, the
  // same shape a real JSONata error gets, so the expectation would read as
  // "jsonata-core must crash here too" -- a limitation of the generator, not
  // a conformance target.
  '$lookup({"undefined": 7}, missing.x)',
  '$pad("a", missing.x, "-")',
  // $spread builds a sequence, so a one-entry result unwraps and an empty
  // one is undefined -- including when the entry comes from inside an array.
  '$spread([obj])',
  '$spread([{}])',
  '$spread({})',
  // The one-argument form of $sift takes its object from the context, which
  // is the case that stops `$sift(obj)` from simply being rejected on arity.
  '$sift(function($v, $k) { $k = "num" })',
  '$sift(obj, function($v) { true })',
  '$sift(obj, function($v) { false })',
  // $each builds a sequence too, and it drops only undefined results -- an
  // explicit null is a result like any other.
  '$each(obj, function($v) { $v })',
  '$each({"a": null}, function($v) { $v })',
  '$each({"a": null, "b": 1}, function($v) { $v })',
  '$each({}, $string)',
  // Builtins passed by reference. A higher-order function hands its callback
  // a fixed number of arguments regardless of what the builtin declares, and
  // a context-capable parameter makes the declared count look like zero.
  '$map(["a","b"], $uppercase)',
  '$map([1,2], $string)',
  '$map(["a","b"], $trim)',
  '$map([1,2], $sqrt)',
  '$map([" a ", "b"], $length)',
  '$filter([1,0,2], $boolean)',
  '$sift(obj, $boolean)',
  '$each(obj, $string)',
  '$map([[1,2],[3]], $count)',
  // Object construction is the only common construct that distinguishes
  // `null` from `undefined` through the public API: a null-valued key is
  // kept, an undefined-valued key is dropped. Evaluating `$f(missing.x)`
  // directly hides that distinction -- both a JS `undefined` and an engine
  // `Null` returned at the top level end up looking like the same missing
  // result once compared. Crossing a missing operand with object
  // construction is what caught issue #107's five-builtin regression
  // (`$trim`, `$merge`, `$reverse`, `$distinct`, `$join` all started
  // returning `{"k": null}` instead of `{}`), and is why it survived a
  // 14928-case differential corpus that never wrapped a result this way.
  '{"k": $trim(missing.x)}',
  '{"k": $merge(missing.x)}',
  '{"k": $reverse(missing.x)}',
  '{"k": $distinct(missing.x)}',
  '{"k": $join(missing.x)}',
  // Already-correct builtins, kept alongside the five above as a control
  // group: if these ever start failing, the bug is in the probe or the
  // harness, not in one specific builtin's undefined handling.
  '{"k": $uppercase(missing.x)}',
  '{"k": $string(missing.x)}',
  '{"k": $count(missing.x)}',
  '{"k": $sum(missing.x)}',
  '{"k": $keys(missing.x)}',
  '{"k": $spread(missing.x)}',
  // Zero-argument $string against an explicit-null context (issue #110):
  // jsonata-js's `x` signature type admits null unwrapped, so `string(null)`
  // runs and returns "null" -- unlike a genuinely undefined context, which
  // stays undefined. `$f(missing.x)` above can't reach this: it puts null on
  // the *argument*, not the context.
  'nul.$string()',
  '{"k": nul.$string()}',
  // The null-context guard's exception is `AstNode::FunctionApplication`,
  // which covers parenthesised block steps (`.(expr)`) as well as `.$fn()`
  // calls -- so a block step run on an explicit null must behave the same
  // way a function-application step does. `nul.($*2)` is the sharpest case:
  // it used to return `null` and now raises T2001 (can't multiply null),
  // which is the guard's largest observable behaviour change and previously
  // had no test pinning it.
  'nul.(1)',
  'nul.($*2)',
  // Null-context path/pipe probes (issues #114, #116). Two sites in the
  // tree-walker short-circuit on an explicit `null` as though it meant
  // "no value" -- the `~>` operator's LHS check, and the path-step loop's
  // current-value guard -- when jsonata-js treats null as an ordinary value
  // that flows into each step/function like any other. Undefined genuinely
  // is absence and must keep short-circuiting; only the null half of each
  // guard is wrong.
  //
  // These specifically need `dataset: 'operands'` (the only dataset with a
  // literal `nul: null` field) run through the `json` entry route: the
  // `dict` route collapses jsonatapy's Undefined and explicit Null to the
  // same Python `None`, so it cannot tell "the guard short-circuited"
  // (wrong) from "the guard ran and correctly produced null/undefined"
  // (right) -- see #109. Mixed with these are same-shaped cases that must
  // NOT change: `missing.x` (genuinely undefined, not null) must keep
  // short-circuiting, and a handful of already-correct null-context shapes
  // serve as a control group the way the builtin-undefined probes above do.
  'nul ~> $string()',
  'nul ~> $uppercase()',
  'nul ~> |$|{}|',
  'missing.x ~> $string()',
  'nul.{}',
  'nul.{"a":1}',
  'nul.*',
  'nul.()',
  'obj.nul.{}',
  'nul.{}.a',
  'nul.foo',
  'nul[0]',
  'nul[true]',
  'nul.**',
  'nul.a.b',
  'nul.$keys()',
  'nul.$type()',
  '[nul].{}',
  'missing.x.{}',
  'missing.x.*',
  // `nul.(foo.bar)` reaches a THIRD null-context site: the block step binds
  // `$` to the (null) current value and evaluates its inner expression as a
  // fresh path whose `data` argument is that null -- a multi-step path's
  // first-step handling has its own `JValue::Null` short-circuit, separate
  // from the loop guard the other cases above exercise (`nul.(foo)` is only
  // one step and hits an unrelated len==1 fast path, so it doesn't pin this).
  'nul.(foo.bar)',
  '{"k": nul.(foo.bar)}',
  // Two non-null fixes rode in on this branch alongside the null-context
  // work, and no probe above -- or in the reference suite -- pins either:
  // `**` singleton unwrapping (`a.**` over `{"a":5}` is `5`, not `[5]`) and
  // the `~> |...|` transform operator's argument type check (a non-object,
  // non-array LHS is T0410, not a silent passthrough).
  'num.**',
  'obj.**',
  'num ~> |$|{}|',
  'obj.k ~> |$|{}|',
  'obj ~> |$|{"z":9}|',
  // Six builtins carried no entry in BUILTIN_SIGNATURES, so
  // `validate_builtin_args` returned `Ok(None)` and *nothing* checked their
  // arguments -- an explicit null sailed through to the implementation
  // instead of raising T0410 (#126 group 2). Restoring the reference's own
  // signature is the fix, but a signature does more than reject nulls: its
  // `-` marker switches on context substitution, and the padding it adds
  // moves what the `propagates_undefined` guard reads. So the null forms
  // below are the target, and the undefined, zero-argument and arity forms
  // are regression guards for behaviour that is already correct.
  '$base64encode(nul)',
  '$base64decode(nul)',
  '$toMillis(nul)',
  '$fromMillis(nul)',
  '$formatInteger(nul, "0")',
  '$parseInteger(nul, "0")',
  '$base64encode(missing.x)',
  '$base64decode(missing.x)',
  '$toMillis(missing.x)',
  '$fromMillis(missing.x)',
  '$formatInteger(missing.x, "0")',
  '$parseInteger(missing.x, "0")',
  // Context substitution through the `-` marker: a context of the right type
  // fills the first parameter, one of the wrong type is T0411, and no
  // context at all (the top-level object) is T0411 too.
  'str.$base64encode()',
  '("YQ==").$base64decode()',
  'num.$fromMillis()',
  'num.$formatInteger("0")',
  'str.$parseInteger("0")',
  'num.$base64encode()',
  'str.$fromMillis()',
  'str.$toMillis()',
  '$base64encode()',
  '$toMillis()',
  // $formatInteger and $parseInteger declare a *required* second parameter,
  // so the one-argument forms are errors -- T0410 for the missing picture,
  // T0411 when the substituted context takes the first slot.
  '$formatInteger(1)',
  '$parseInteger("1")',
  // $fromMillis is the only one of the six with a *third* parameter, and the
  // second-argument matrix cannot reach it. Its `<n-s?s?:s>` timezone slot
  // rejects a null and trims a trailing undefined, which is what makes
  // dropping the arm's hand-rolled `Some(JValue::Null) => None` safe.
  '$fromMillis(0, "[Y0001]", null)',
  '$fromMillis(0, "[Y0001]", nul)',
  '$fromMillis(0, "[Y0001]", missing.x)',
  '$fromMillis(0, "[Y0001]", 1)',
];
// -- Path operators crossed with every value kind ---------------------------
// `*` and `**` dispatch on the kind of value the step is applied to, and the
// arms that decide "this kind has no children" are the ones that historically
// answered `null` where jsonata-js answers `undefined` (#126 group 1). Crossing
// the operators with the operand fixtures covers every arm rather than the
// handful a hand probe would reach.
const PATH_OPERANDS = [
  'nul', 'arr', 'obj', 'emptyarr', 'num', 'str', 'tf', 'ff', 'zero', 'estr',
  'deep', 'one', 'arrobj',
];
for (const o of PATH_OPERANDS) {
  for (const op of ['*', '**']) {
    BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: `${o}.${op}` });
  }
  // Chained, to pin singleton unwrapping: a one-value result unwraps, so
  // `deep.*` is the inner object rather than a one-element array, and the
  // second step then has an object to work on.
  BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: `${o}.*.*` });
}
BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: '*' });
BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: '**' });

// Positional binding. `#$i` turns its step into a tuple stream, and the same
// singleton rule applies to the result: a one-value stream unwraps, so
// `num#$i` is `5` rather than `[5]`. The bare and `.$i` forms cover the value
// and the index; the filtered form is the control, since a filter predicate
// already forces the unwrap and so was never wrong.
for (const o of PATH_OPERANDS) {
  for (const form of ['#$i', '#$i.$i', '#$i[$i=0]']) {
    BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: `${o}${form}` });
  }
}
BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: 'arrobj#$i.{"i":$i,"x":x}' });
BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: 'arr#$i^($i)' });
BUILTIN_EXPRESSIONS.push({ fastpath: 'path_operator', expr: 'arr#$i[0]' });

for (const expr of BUILTIN_PROBES) BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_probe', expr });

async function main() {
  const cases = [];
  let errors = 0;
  for (const { fastpath, expr } of EXPRESSIONS) {
    // The operator matrix supplies its own operands; running it against every
    // payload would multiply the corpus without adding information.
    const datasets = fastpath.startsWith('operator_')
      ? [['operands', DATASETS.operands]]
      : Object.entries(DATASETS).filter(([n]) => n !== 'operands');
    for (const [dsName, data] of datasets) {
      let expected;
      try {
        const r = await jsonata(expr).evaluate(structuredClone(data));
        if (r === undefined) {
          expected = { kind: 'undefined' };
        } else if (typeof r === 'number' && !Number.isFinite(r)) {
          // Infinity and NaN both serialise to the JSON text "null", so
          // recording them as a value would assert the wrong expectation --
          // `1/0` is Infinity in jsonata-js, not null. Record the kind instead.
          expected = { kind: 'nonfinite', value: Number.isNaN(r) ? 'nan' : (r > 0 ? 'inf' : '-inf') };
        } else {
          expected = { kind: 'value', value: r };
        }
      } catch (e) {
        // Record only that it errored, plus the code for diagnostics. Error
        // *codes* are deliberately not asserted: jsonata-core's messages do
        // not carry codes uniformly yet, so asserting them would swamp this
        // harness with pre-existing message-format noise rather than the
        // value-vs-error divergences it exists to catch.
        expected = { kind: 'error', code: e.code || null };
        errors++;
      }
      cases.push({ fastpath, expr, dataset: dsName, expected });
    }
  }
  const out = {
    _comment: 'GENERATED by scripts/gen_fastpath_corpus.js -- do not edit by hand.',
    reference: `jsonata-js ${refVersion} (tests/jsonata-js submodule)`,
    datasets: DATASETS,
    cases,
  };
  // Builtin cases go to their own fixture: one combined file crosses the
  // repo's 500KB per-file CI limit.
  const builtinCases = [];
  for (const { fastpath, expr } of BUILTIN_EXPRESSIONS) {
    let expected;
    try {
      const r = await jsonata(expr).evaluate(structuredClone(DATASETS.operands));
      if (r === undefined) {
        expected = { kind: 'undefined' };
      } else if (typeof r === 'number' && !Number.isFinite(r)) {
        expected = { kind: 'nonfinite', value: Number.isNaN(r) ? 'nan' : (r > 0 ? 'inf' : '-inf') };
      } else {
        expected = { kind: 'value', value: r };
      }
    } catch (e) {
      expected = { kind: 'error', code: e.code || null };
    }
    builtinCases.push({ fastpath, expr, dataset: 'operands', expected });
  }
  fs.writeFileSync(
    path.join(__dirname, '..', 'tests', 'fixtures', 'builtin_differential.json'),
    JSON.stringify({
      _comment: 'GENERATED by scripts/gen_fastpath_corpus.js -- do not edit by hand.',
      reference: `jsonata-js ${refVersion} (tests/jsonata-js submodule)`,
      datasets: { operands: DATASETS.operands },
      cases: builtinCases,
    }) + '\n'
  );
  console.log(`wrote ${builtinCases.length} builtin cases`);

  const arityDest = path.join(__dirname, '..', 'tests', 'fixtures', 'builtin_arity.json');
  const arity = buildBuiltinArity();
  const sortedArity = {};
  for (const k of Object.keys(arity).sort()) sortedArity[k] = arity[k];
  fs.writeFileSync(arityDest, JSON.stringify(sortedArity) + '\n');
  console.log(`wrote ${Object.keys(sortedArity).length} builtin arities to ${arityDest}`);

  const sigDest = path.join(__dirname, '..', 'tests', 'fixtures', 'builtin_signatures.json');
  const sigs = buildBuiltinSignatures();
  fs.writeFileSync(sigDest, JSON.stringify(sigs, null, 2) + '\n');
  console.log(`wrote ${Object.keys(sigs).length} builtin signatures to ${sigDest}`);

  const dest = path.join(__dirname, '..', 'tests', 'fixtures', 'fastpath_differential.json');
  // Compact: this file is generated and regenerated, and pretty-printing it
  // pushes the repo past its 500KB per-file CI limit as the corpus grows.
  fs.writeFileSync(dest, JSON.stringify(out) + '\n');
  console.log(`wrote ${cases.length} cases (${errors} expect an error) to ${dest}`);
  console.log(`reference: jsonata-js ${refVersion}`);
}

main();
