//! Path-semantics differential test (issue #184, step 3).
//!
//! Checks that both engines give the value jsonata-js gives for each corpus
//! expression: when a one-item sequence is unwrapped, when nested results are
//! flattened, and how both interact with blocks, constructors, variables,
//! function arguments and map stages. Cases marked `root` run on a
//! top-level array instead of an object.
//!
//! A second test runs generated operator combinations (#212) from
//! `fixtures/path_semantics_combinations.json`; the ones that still differ
//! are listed by issue in `fixtures/path_semantics_combinations_known.json`.
//!
//! Regenerate both fixtures with `node scripts/gen_path_semantics_corpus.js`.
//! Set `PATH_SEMANTICS_VERBOSE=1` to print every divergence.

use jsonata_core::evaluator::Evaluator;
use jsonata_core::value::JValue;
use jsonata_core::Expression;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

mod common;
use common::norm;

/// Cases whose value still differs from jsonata-js. Each entry is
/// `(engine, expression)`. The list must stay exact: a case that starts
/// matching has to be removed from it.
const KNOWN_DIVERGENCES: &[(&str, &str)] = &[
    // jsonata-js 2.3.0 (#838): a step after a group-by applies to the
    // grouped result, `a{k:v}.x` being `(a{k:v}).x`. Plan PR 4.
    ("compiled", "acct.Order.P{N: N}.A"),
    ("tree", "acct.Order.P{N: N}.A"),
    ("compiled", "acct.Order.P{N: N}.Z"),
    ("tree", "acct.Order.P{N: N}.Z"),
    ("compiled", "acct.Order.P{N: nope}.A"),
    ("tree", "acct.Order.P{N: nope}.A"),
    ("compiled", "acct.Order.P{N: N}.%"),
    ("tree", "acct.Order.P{N: N}.%"),
    ("compiled", "acct.Order.P{N: N}[0].N"),
    ("tree", "acct.Order.P{N: N}[0].N"),
    ("compiled", "acct{\"a\": 1}.Order"),
    ("tree", "acct{\"a\": 1}.Order"),
    ("compile", "acct.Order{id: id}.P{N: N}"),
    ("compiled", "acct.Order{\"k\": id}@$o.P"),
    ("tree", "acct.Order{\"k\": id}@$o.P"),
    ("compiled", "acct.Order{\"k\": id}#$i.P"),
    ("tree", "acct.Order{\"k\": id}#$i.P"),
    ("compiled", "acct.Order@$o.P{N: $o.id}.N"),
    ("tree", "acct.Order@$o.P{N: $o.id}.N"),
    ("compiled", "acct.Order.P{N: N}.$uppercase(N)"),
    ("tree", "acct.Order.P{N: N}.$uppercase(N)"),
    ("compiled", "acct.Order{\"k\": id}.P.$string()"),
    ("tree", "acct.Order{\"k\": id}.P.$string()"),
    ("compiled", "acct.Order{\"k\": id}.$count(P)"),
    ("tree", "acct.Order{\"k\": id}.$count(P)"),
    ("compiled", "acct.Order.P{N: N}.function($x) { $x }(1)"),
    ("tree", "acct.Order.P{N: N}.function($x) { $x }(1)"),
    ("compiled", "acct^(id)[0]{\"k\": 1}.Order"),
    ("tree", "acct^(id)[0]{\"k\": 1}.Order"),
    ("compiled", "acct.Order@$o[id = \"zz\"]{\"k\": 1}.id"),
    ("tree", "acct.Order@$o[id = \"zz\"]{\"k\": 1}.id"),
    ("compiled", "acct.Order^(id)[0]{\"k\": id}.P"),
    ("tree", "acct.Order^(id)[0]{\"k\": id}.P"),
    ("compiled", "acct.*{\"k\": 1}.%"),
    ("tree", "acct.*{\"k\": 1}.%"),
    ("compiled", "acct.Order.P.%{\"x\": id}.%"),
    ("tree", "acct.Order.P.%{\"x\": id}.%"),
    // Deliberate: jsonata-js matches a regex on the right of `~>` against
    // its left side converted to a string, so an undefined left side is the
    // text "undefined" and `/n/` matches it. An undefined left side gives
    // undefined here.
    ("compiled", "nope ~> /n/"),
    ("tree", "nope ~> /n/"),
    // Deliberate (#159): `$k` captures `$m` by value when it is defined,
    // while it is still the outer "root"; jsonata-js sees the block's later
    // `$m := 1` through the live frame. Without the outer `$m` nothing is
    // captured and both give 1 (the case before this one in the corpus).
    (
        "compiled",
        "($m := \"root\"; $other := function($f) { $f() }; $outer := function() { ($k := function() { $m }; $m := 1; $other($k)) }; $outer())",
    ),
    (
        "tree",
        "($m := \"root\"; $other := function($f) { $f() }; $outer := function() { ($k := function() { $m }; $m := 1; $other($k)) }; $outer())",
    ),
    // Not yet matched (#191): `$r(4)` nests four sequences of one item.
    // jsonata-js flattens only the outermost when the composition evaluates
    // its first function's call as the argument of its second ("[[[0]]]");
    // here a second level is flattened on the way out of that call
    // ("[[0]]"). Called directly, as `$string($r(4))`, both give "[[[0]]]".
    (
        "compiled",
        "($r := function($n) { $n <= 0 ? 0 : $map([$n], function($i) { $r($i - 1) }) }; ($r ~> $string)(4))",
    ),
    (
        "tree",
        "($r := function($n) { $n <= 0 ? 0 : $map([$n], function($i) { $r($i - 1) }) }; ($r ~> $string)(4))",
    ),
    // Not yet matched (#191): jsonata-js parses `a ?: b` and `a ?? b` as
    // conditions, so `b` is a tail position and a `$eval` there runs where
    // the chain of tail calls began. Here only `?` branches, a block's last
    // expression and a call are tail positions, so it runs in the body's
    // scope. Treating them as tail positions would also take the bodies
    // that end in them off the compiled fast path.
    (
        "compiled",
        "($n := 1; $f := function() { ($n := 2; $x ?: $eval(\"$n\")) }; ($n := 3; $f()))",
    ),
    (
        "tree",
        "($n := 1; $f := function() { ($n := 2; $x ?: $eval(\"$n\")) }; ($n := 3; $f()))",
    ),
    (
        "compiled",
        "($n := 1; $f := function() { ($n := 2; $x ?? $eval(\"$n\")) }; ($n := 3; $f()))",
    ),
    (
        "tree",
        "($n := 1; $f := function() { ($n := 2; $x ?? $eval(\"$n\")) }; ($n := 3; $f()))",
    ),
    // Deliberate (#191, the documented late-binding gap): the composition's
    // `$eval` runs in its defining block, which has finished by the call.
    // jsonata-js keeps that block alive and sees its `$n`; here, as for any
    // function outliving its defining scope, it is gone, and only what the
    // function captured remains. `$eval`'s text names nothing to capture,
    // so it sees the enclosing scope's `$n`.
    (
        "compiled",
        "($n := \"outer\"; $c := ($n := \"def\"; $eval ~> $uppercase); ($n := \"call\"; $c(\"$n\")))",
    ),
    (
        "tree",
        "($n := \"outer\"; $c := ($n := \"def\"; $eval ~> $uppercase); ($n := \"call\"; $c(\"$n\")))",
    ),
];

/// Mirror of `encode` in scripts/gen_path_semantics_corpus.js.
fn encode(v: &JValue) -> Value {
    match v {
        JValue::Undefined => json!({"$undefined": true}),
        JValue::Array(arr) => Value::Array(arr.iter().map(encode).collect()),
        JValue::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), encode(v)))
                .collect::<Map<_, _>>(),
        ),
        JValue::Lambda(_) | JValue::Builtin { .. } | JValue::Regex { .. } => {
            json!({"$function": true})
        }
        other => Value::from(other),
    }
}

/// True for a jsonata-js error code such as `D3137`.
fn is_code(s: &str) -> bool {
    s.len() == 5
        && s.starts_with(|c: char| c.is_ascii_uppercase())
        && s[1..].bytes().all(|b| b.is_ascii_digit())
}

/// The first error code in an error message (`Evaluation error: D3137: ...`).
fn error_code(message: &str) -> Option<&str> {
    message
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| is_code(word))
}

/// Whether an error with `message` is the error a case expects. Some
/// expected errors are jsonata-js crashes with only a message, not a code;
/// any error matches those.
fn error_matches(expected: &Value, message: &str) -> bool {
    match expected.get("$error").and_then(Value::as_str) {
        Some(code) if is_code(code) => error_code(message) == Some(code),
        Some(_) => true,
        None => false,
    }
}

/// Runs each fixture case on both engines and returns the cases whose value
/// differs from jsonata-js, as `(engine, expression)`.
fn divergences(
    fixture: &Value,
    object_data: &JValue,
    root_data: &JValue,
) -> Vec<(&'static str, String)> {
    let verbose = std::env::var_os("PATH_SEMANTICS_VERBOSE").is_some();
    let mut divergences = Vec::new();
    for case in fixture["cases"].as_array().unwrap() {
        let src = case["expression"].as_str().unwrap();
        let expected = norm(&case["result"]);
        let empty_root = JValue::array(Vec::new());
        let data = if case["root"] == "empty" {
            &empty_root
        } else if case["root"] == true {
            root_data
        } else {
            object_data
        };
        let expr = match Expression::compile(src) {
            Ok(expr) => expr,
            Err(e) => {
                if !error_matches(&expected, &e.to_string()) {
                    divergences.push(("compile", src.to_string()));
                    if verbose {
                        println!("compile  {src}\n    expected {expected}\n    error    {e}");
                    }
                }
                continue;
            }
        };
        let compiled = expr.evaluate(data);
        let tree = Evaluator::new().evaluate(expr.ast(), data);
        for (engine, result) in [("compiled", compiled), ("tree", tree)] {
            let matches = match &result {
                Ok(v) => norm(&encode(v)) == expected,
                Err(e) => error_matches(&expected, &e.to_string()),
            };
            if !matches {
                if verbose {
                    let actual = match &result {
                        Ok(v) => encode(v).to_string(),
                        Err(e) => format!("error: {e}"),
                    };
                    println!("{engine:8} {src}\n    expected {expected}\n    actual   {actual}");
                }
                divergences.push((engine, src.to_string()));
            }
        }
    }
    divergences
}

fn assert_known(divergences: Vec<(&str, String)>, known: &[(&str, &str)], list: &str) {
    let known: Vec<(&str, String)> = known.iter().map(|(e, s)| (*e, s.to_string())).collect();
    assert_eq!(
        divergences, known,
        "divergences from jsonata-js changed; update {list} \
         (run with PATH_SEMANTICS_VERBOSE=1 to see each one's values)"
    );
}

#[test]
fn path_semantics_match_jsonata_js() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/path_semantics_differential.json")).unwrap();
    let object_data = JValue::from(fixture["data"].clone());
    let root_data = JValue::from(fixture["root_data"].clone());
    assert_known(
        divergences(&fixture, &object_data, &root_data),
        KNOWN_DIVERGENCES,
        "KNOWN_DIVERGENCES",
    );
}

/// Operator combinations (#212). Cases that still differ are listed by
/// issue in `fixtures/path_semantics_combinations_known.json`. The list holds
/// expressions only, so this checks that each one fails to compile or
/// diverges on both engines: a case fixed or broken on one engine alone
/// fails the test too.
#[test]
fn path_combinations_match_jsonata_js() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/path_semantics_combinations.json")).unwrap();
    let known: Value = serde_json::from_str(include_str!(
        "fixtures/path_semantics_combinations_known.json"
    ))
    .unwrap();
    assert!(
        fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .all(|case| case["root"] != true),
        "the combinations fixture has no root data"
    );
    let data = JValue::from(fixture["data"].clone());

    let mut engines: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    for (engine, src) in divergences(&fixture, &data, &JValue::Undefined) {
        engines.entry(src).or_default().insert(engine);
    }
    let one_engine: Vec<&String> = engines
        .iter()
        .filter(|(_, e)| e.len() == 1 && !e.contains("compile"))
        .map(|(src, _)| src)
        .collect();
    let actual: BTreeSet<&String> = engines.keys().collect();

    let listed: Vec<String> = known["issues"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|issue| issue["expressions"].as_array().unwrap())
        .map(|e| e.as_str().unwrap().to_string())
        .collect();
    let expected: BTreeSet<&String> = listed.iter().collect();
    assert_eq!(
        expected.len(),
        listed.len(),
        "an expression is listed twice"
    );

    let new: Vec<_> = actual.difference(&expected).collect();
    let fixed: Vec<_> = expected.difference(&actual).collect();
    assert!(
        new.is_empty() && fixed.is_empty() && one_engine.is_empty(),
        "combination divergences changed; update \
         fixtures/path_semantics_combinations_known.json\n  new: {new:?}\n  \
         now matching: {fixed:?}\n  diverging on one engine only: {one_engine:?}"
    );
}
