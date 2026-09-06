// Fast-iteration mirror of the Phase 1/2 reference-suite cases (function-fromMillis,
// function-tomillis, function-formatInteger, function-parseInteger) for the shared
// picture-string engine in src/datetime.rs. This is NOT a replacement for
// `pytest tests/python/test_reference_suite.py` (which remains the real gate and covers
// the full 1686-case suite through the actual Python API) -- it exists purely so the
// picture-string engine can be iterated on with `cargo test` (seconds) instead of a
// maturin rebuild + pytest cycle (~2.5 minutes) per fix. See
// docs/superpowers/specs/2026-07-05-reference-suite-coverage-gap-design.md, Phases 1-2.

use jsonata_core::{
    evaluator::{Evaluator, EvaluatorError},
    parser::parse,
    value::JValue,
};
use serde_json::Value as JsonValue;
use std::fs;
use std::path::Path;

/// Mirrors `evaluator_error_to_py` in src/lib.rs: the Python-visible exception message
/// is the raw inner string of the error variant, NOT its `Display` impl (which adds a
/// "Type error: "/"Evaluation error: "/etc. prefix that would shadow the leading JSONata
/// error code, e.g. "D3134: ...", that test_reference_suite.py's extract_error_code()
/// looks for).
fn error_message(e: &EvaluatorError) -> &str {
    match e {
        EvaluatorError::TypeError(m) => m,
        EvaluatorError::ReferenceError(m) => m,
        EvaluatorError::EvaluationError(m) => m,
        #[cfg(feature = "python")]
        EvaluatorError::PyConversionError(m) => m,
    }
}

fn run_case(case: &JsonValue) -> Result<(), String> {
    let expr = case["expr"].as_str().ok_or("missing expr")?;
    let data: JValue = case.get("data").cloned().unwrap_or(JsonValue::Null).into();

    let ast = parse(expr).map_err(|e| format!("parse error: {e}"))?;
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

fn run_group_file(path: &Path) -> (usize, Vec<String>) {
    let content = fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {path:?}: {e}"));
    let json: JsonValue =
        serde_json::from_str(&content).unwrap_or_else(|e| panic!("parsing {path:?}: {e}"));
    let cases: Vec<&JsonValue> = match &json {
        JsonValue::Array(arr) => arr.iter().collect(),
        obj => vec![obj],
    };

    let mut failures = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        if let Err(msg) = run_case(case) {
            let desc = case
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            failures.push(format!(
                "{}[{i}] ({desc}): expr={:?}\n    {msg}",
                path.file_stem().unwrap().to_string_lossy(),
                case.get("expr").and_then(|e| e.as_str()).unwrap_or("")
            ));
        }
    }
    (cases.len(), failures)
}

fn suite_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/jsonata-js/test/test-suite/groups")
}

#[test]
fn from_millis_format_date_time() {
    let (total, failures) =
        run_group_file(&suite_root().join("function-fromMillis/formatDateTime.json"));
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn from_millis_iso_week_date() {
    let (total, failures) =
        run_group_file(&suite_root().join("function-fromMillis/isoWeekDate.json"));
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn to_millis_parse_date_time() {
    let (total, failures) =
        run_group_file(&suite_root().join("function-tomillis/parseDateTime.json"));
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn format_integer() {
    let (total, failures) =
        run_group_file(&suite_root().join("function-formatInteger/formatInteger.json"));
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}

#[test]
fn parse_integer() {
    let (total, failures) =
        run_group_file(&suite_root().join("function-parseInteger/parseInteger.json"));
    assert!(
        failures.is_empty(),
        "{}/{} failed:\n{}",
        failures.len(),
        total,
        failures.join("\n")
    );
}
