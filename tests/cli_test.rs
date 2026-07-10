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
