// Fast-iteration mirror of the parent-operator/joins reference-suite cases
// for the %/@ operators, mirroring tests/datetime_picture_suite.rs's
// structure (see that file for the run_case/run_group_file helpers this
// duplicates -- kept as a separate file since this isn't datetime-related).
//
// This is NOT a replacement for `pytest tests/python/test_reference_suite.py`
// (the real gate); it exists so the %/@ ancestry + tuple-stream machinery can
// be iterated on with `cargo test` (seconds) instead of a maturin rebuild +
// pytest cycle per fix. See
// docs/superpowers/specs/2026-07-06-parent-and-focus-binding-operators-design.md.

use jsonata_core::{
    evaluator::{Evaluator, EvaluatorError},
    parser::parse,
    value::JValue,
};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::Path;

/// Mirrors `evaluator_error_to_py` in src/lib.rs: the Python-visible exception
/// message is the raw inner string of the error variant (which carries the
/// leading JSONata error code, e.g. "S0217: ..."), NOT its `Display` impl.
fn error_message(e: &EvaluatorError) -> &str {
    match e {
        EvaluatorError::TypeError(m) => m,
        EvaluatorError::ReferenceError(m) => m,
        EvaluatorError::EvaluationError(m) => m,
    }
}

fn resolve_expr(case: &JsonValue, group_dir: &Path) -> Option<String> {
    if let Some(expr) = case.get("expr").and_then(|e| e.as_str()) {
        return Some(expr.to_string());
    }
    let expr_file = case.get("expr-file").and_then(|e| e.as_str())?;
    fs::read_to_string(group_dir.join(expr_file)).ok()
}

fn resolve_data(case: &JsonValue, dataset_dir: &Path) -> JsonValue {
    if let Some(data) = case.get("data") {
        return data.clone();
    }
    if let Some(dataset) = case.get("dataset").and_then(|d| d.as_str()) {
        let path = dataset_dir.join(format!("{dataset}.json"));
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(parsed) = serde_json::from_str(&content) {
                return parsed;
            }
        }
    }
    JsonValue::Null
}

fn run_case(case: &JsonValue, group_dir: &Path, dataset_dir: &Path) -> Result<(), String> {
    let expr = resolve_expr(case, group_dir).ok_or("missing expr/expr-file")?;
    let data: JValue = resolve_data(case, dataset_dir).into();

    let ast = match parse(&expr) {
        Ok(ast) => ast,
        Err(e) => {
            // A parse error may itself be the expected outcome (code check below).
            if let Some(code) = case.get("code").and_then(|c| c.as_str()) {
                let msg = format!("{e}");
                if msg.contains(code) {
                    return Ok(());
                }
                return Err(format!("expected code {code}, got parse error: {msg}"));
            }
            return Err(format!("parse error: {e}"));
        }
    };
    let mut evaluator = Evaluator::new();
    let result = evaluator.evaluate(&ast, &data);

    if let Some(code) = case.get("code").and_then(|c| c.as_str()) {
        return match &result {
            Err(e) => {
                let msg = error_message(e);
                if msg.starts_with(code) {
                    Ok(())
                } else {
                    Err(format!("expected code {code}, got error: {msg}"))
                }
            }
            Ok(v) => Err(format!("expected error {code}, got result {v:?}")),
        };
    }

    if case.get("undefinedResult").and_then(|b| b.as_bool()) == Some(true) {
        return match result {
            Ok(JValue::Undefined) | Ok(JValue::Null) => Ok(()),
            Ok(other) => Err(format!("expected undefined, got {other:?}")),
            Err(e) => Err(format!("expected undefined, got error: {e}")),
        };
    }

    if let Some(expected) = case.get("result") {
        return match result {
            Ok(v) => {
                let actual = serde_json::to_value(&v)
                    .map_err(|e| format!("failed to serialize result: {e}"))?;
                if &actual == expected {
                    Ok(())
                } else {
                    Err(format!("expected {expected}, got {actual}"))
                }
            }
            Err(e) => Err(format!("expected result {expected}, got error: {e}")),
        };
    }

    Err("test spec has no expected outcome (result, undefinedResult, or code)".to_string())
}

fn run_group_file(path: &Path, group_dir: &Path, dataset_dir: &Path) -> (usize, Vec<String>) {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    let json: JsonValue =
        serde_json::from_str(&content).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"));
    let cases: Vec<&JsonValue> = match &json {
        JsonValue::Array(arr) => arr.iter().collect(),
        obj => vec![obj],
    };

    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        if let Err(msg) = run_case(case, group_dir, dataset_dir) {
            let desc = case
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            failures.push(format!(
                "{}[{i}] ({desc}): {msg}",
                path.file_stem().unwrap().to_string_lossy()
            ));
        }
    }
    (cases.len(), failures)
}

fn groups_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/jsonata-js/test/test-suite/groups")
}

fn dataset_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/jsonata-js/test/test-suite/datasets")
}

#[test]
fn parent_operator() {
    let group_dir = groups_root().join("parent-operator");
    let (total, failures) =
        run_group_file(&group_dir.join("parent.json"), &group_dir, &dataset_dir());
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn joins() {
    let group_dir = groups_root().join("joins");
    let mut all_failures = Vec::new();
    let mut all_total = 0;
    let mut files: Vec<_> = fs::read_dir(&group_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    files.sort();
    for path in files {
        let (total, failures) = run_group_file(&path, &group_dir, &dataset_dir());
        all_total += total;
        all_failures.extend(failures);
    }
    assert!(
        all_failures.is_empty(),
        "{}/{} failed:\n{}",
        all_failures.len(),
        all_total,
        all_failures.join("\n")
    );
}
