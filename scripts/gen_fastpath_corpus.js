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
const refVersion = require(path.join(__dirname, '..', 'tests', 'jsonata-js', 'package.json')).version;

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
  operands:    { nul: null, arr: [1, 2], obj: { k: 1 }, emptyarr: [], num: 5, str: 'a' },
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
];

const BUILTIN_EXPRESSIONS = [];
for (const fn of BUILTINS_ONE_ARG) {
  for (const o of OPERANDS) BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_one_arg', expr: `$${fn}(${o})` });
}
for (const [fn, first] of BUILTINS_SECOND_ARG) {
  for (const o of OPERANDS) BUILTIN_EXPRESSIONS.push({ fastpath: 'builtin_second_arg', expr: `$${fn}(${first}, ${o})` });
}

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

  const dest = path.join(__dirname, '..', 'tests', 'fixtures', 'fastpath_differential.json');
  // Compact: this file is generated and regenerated, and pretty-printing it
  // pushes the repo past its 500KB per-file CI limit as the corpus grows.
  fs.writeFileSync(dest, JSON.stringify(out) + '\n');
  console.log(`wrote ${cases.length} cases (${errors} expect an error) to ${dest}`);
  console.log(`reference: jsonata-js ${refVersion}`);
}

main();
