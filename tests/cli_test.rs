#![cfg(feature = "cli")]

use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn version_flag_prints_version_and_exits_zero() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--version")
        .assert()
        .success()
        .stdout(contains("jsonata"));
}

#[test]
fn help_flag_lists_known_options() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--compact"))
        .stdout(contains("--raw-output"))
        .stdout(contains("--null-input"))
        .stdout(contains("--from-file"));
}

#[test]
fn evaluates_expression_against_stdin_json() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("name")
        .write_stdin(r#"{"name": "Alice"}"#)
        .assert()
        .success()
        .stdout("\"Alice\"\n");
}

#[test]
fn evaluates_expression_against_file_argument() {
    let dir = std::env::temp_dir();
    let path = dir.join("jsonata_cli_test_input.json");
    std::fs::write(&path, r#"{"name": "Bob"}"#).unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("name")
        .arg(path.to_str().unwrap())
        .assert()
        .success()
        .stdout("\"Bob\"\n");

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn pretty_prints_object_results_by_default() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("{\"x\": a}")
        .write_stdin(r#"{"a": 1}"#)
        .assert()
        .success()
        .stdout(contains("{\n"));
}

#[test]
fn undefined_result_prints_nothing_and_exits_zero() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("nonexistent_field")
        .write_stdin(r#"{"a": 1}"#)
        .assert()
        .success()
        .stdout("");
}

#[test]
fn null_result_prints_literal_null() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("nullField")
        .write_stdin(r#"{"nullField": null}"#)
        .assert()
        .success()
        .stdout("null\n");
}

#[test]
fn multi_document_stdin_is_rejected_not_silently_truncated() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a")
        .write_stdin("{\"a\":1}\n{\"a\":2}\n")
        .assert()
        .code(3)
        .stderr(contains("invalid JSON input"));
}

#[test]
fn compact_flag_produces_single_line_output() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-c")
        .arg("{\"x\": a}")
        .write_stdin(r#"{"a": 1}"#)
        .assert()
        .success()
        .stdout("{\"x\":1}\n");
}

#[test]
fn raw_output_flag_strips_quotes_from_string_results() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-r")
        .arg("name")
        .write_stdin(r#"{"name": "Alice"}"#)
        .assert()
        .success()
        .stdout("Alice\n");
}

#[test]
fn raw_output_flag_does_not_affect_non_string_results() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-r")
        .arg("-c")
        .arg("age")
        .write_stdin(r#"{"age": 30}"#)
        .assert()
        .success()
        .stdout("30\n");
}

#[test]
fn null_input_flag_evaluates_without_reading_stdin() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-n")
        .arg("1 + 1")
        .assert()
        .success()
        .stdout("2\n");
}

#[test]
fn null_input_with_file_argument_is_a_usage_error() {
    let dir = std::env::temp_dir();
    let path = dir.join("jsonata_cli_test_null_conflict.json");
    std::fs::write(&path, r#"{"a": 1}"#).unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-n")
        .arg("1 + 1")
        .arg(path.to_str().unwrap())
        .assert()
        .code(2);

    std::fs::remove_file(&path).unwrap();
}

#[test]
fn from_file_reads_expression_from_a_file() {
    let dir = std::env::temp_dir();
    let expr_path = dir.join("jsonata_cli_test_expr.jsonata");
    std::fs::write(&expr_path, "name").unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-f")
        .arg(expr_path.to_str().unwrap())
        .write_stdin(r#"{"name": "Carol"}"#)
        .assert()
        .success()
        .stdout("\"Carol\"\n");

    std::fs::remove_file(&expr_path).unwrap();
}

#[test]
fn from_file_with_data_file_argument() {
    let dir = std::env::temp_dir();
    let expr_path = dir.join("jsonata_cli_test_expr2.jsonata");
    let data_path = dir.join("jsonata_cli_test_data2.json");
    std::fs::write(&expr_path, "name").unwrap();
    std::fs::write(&data_path, r#"{"name": "Dave"}"#).unwrap();

    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-f")
        .arg(expr_path.to_str().unwrap())
        .arg(data_path.to_str().unwrap())
        .assert()
        .success()
        .stdout("\"Dave\"\n");

    std::fs::remove_file(&expr_path).unwrap();
    std::fs::remove_file(&data_path).unwrap();
}

#[test]
fn from_file_with_nonexistent_expression_file_is_usage_error() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("-f")
        .arg("/nonexistent/path/to/expr.jsonata")
        .write_stdin("{}")
        .assert()
        .code(2)
        .stderr(contains("could not read expression file"));
}

#[test]
fn arg_binds_a_string_variable() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--arg")
        .arg("region=us")
        .arg("$region")
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("\"us\"\n");
}

#[test]
fn argjson_binds_a_json_variable() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--argjson")
        .arg("limit=5")
        .arg("$limit * 2")
        .write_stdin("{}")
        .assert()
        .success()
        .stdout("10\n");
}

#[test]
fn malformed_arg_binding_is_a_usage_error() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--arg")
        .arg("noequalssign")
        .arg("$x")
        .write_stdin("{}")
        .assert()
        .code(2);
}

#[test]
fn evaluation_error_preserves_jsonata_error_code() {
    // NOTE: the brief's original example expression `1 + "a"` was checked
    // against this codebase's actual arithmetic error handling and does
    // NOT produce a coded error here -- `add()` in src/evaluator.rs falls
    // through to an uncoded `format!("Cannot add {:?} and {:?}", ...)` for
    // a Number/String mismatch (pre-existing behavior, unrelated to this
    // task's refactor). `null + 1` (an explicit null literal against the
    // `+` operator) is the expression that actually exercises the T2002
    // coded path in this codebase, so it is used here instead to verify
    // that a coded error passes through un-double-wrapped.
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("null + 1")
        .write_stdin("{}")
        .assert()
        .code(1)
        .stderr(predicates::str::starts_with("T2002:"));
}

#[test]
fn invalid_json_input_exits_three() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a")
        .write_stdin("not json")
        .assert()
        .code(3)
        .stderr(contains("invalid JSON input"));
}

#[test]
fn parse_error_exits_one() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a[")
        .write_stdin("{}")
        .assert()
        .code(1);
}

#[test]
fn missing_expression_argument_exits_two() {
    Command::cargo_bin("jsonata").unwrap().assert().code(2);
}

#[test]
fn nonexistent_input_file_exits_two() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("a")
        .arg("/nonexistent/path/data.json")
        .assert()
        .code(2)
        .stderr(contains("could not read input file"));
}

#[test]
fn unknown_flag_exits_two_via_clap_default() {
    Command::cargo_bin("jsonata")
        .unwrap()
        .arg("--not-a-real-flag")
        .arg("a")
        .assert()
        .code(2);
}
