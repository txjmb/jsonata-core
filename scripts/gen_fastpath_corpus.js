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

async function main() {
  const cases = [];
  let errors = 0;
  for (const { fastpath, expr } of EXPRESSIONS) {
    for (const [dsName, data] of Object.entries(DATASETS)) {
      let expected;
      try {
        const r = await jsonata(expr).evaluate(structuredClone(data));
        expected = r === undefined
          ? { kind: 'undefined' }
          : { kind: 'value', value: r };
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
  const dest = path.join(__dirname, '..', 'tests', 'fixtures', 'fastpath_differential.json');
  fs.writeFileSync(dest, JSON.stringify(out, null, 2) + '\n');
  console.log(`wrote ${cases.length} cases (${errors} expect an error) to ${dest}`);
  console.log(`reference: jsonata-js ${refVersion}`);
}

main();
