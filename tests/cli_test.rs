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
