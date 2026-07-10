use assert_cmd::Command;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    name: String,
    args: Vec<String>,
    stdin: Option<String>,
    expected_exit: i32,
    expected_stdout: Option<String>,
    expected_stderr_contains: Option<String>,
}

#[test]
fn all_fixtures_pass() {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/study/cli_fixtures.json"
    ))
    .expect("study/cli_fixtures.json must exist");
    let fixtures: Vec<Fixture> =
        serde_json::from_str(&raw).expect("study/cli_fixtures.json must be valid JSON");

    let mut failures = Vec::new();

    for fixture in &fixtures {
        let mut cmd = Command::cargo_bin("jsonata").unwrap();
        cmd.args(&fixture.args);
        if let Some(stdin) = &fixture.stdin {
            cmd.write_stdin(stdin.clone());
        }
        let output = cmd.output().unwrap();

        let actual_exit = output.status.code().unwrap_or(-1);
        if actual_exit != fixture.expected_exit {
            failures.push(format!(
                "{}: expected exit {}, got {}",
                fixture.name, fixture.expected_exit, actual_exit
            ));
            continue;
        }

        if let Some(expected_stdout) = &fixture.expected_stdout {
            let actual_stdout = String::from_utf8_lossy(&output.stdout);
            if &*actual_stdout != expected_stdout {
                failures.push(format!(
                    "{}: expected stdout {:?}, got {:?}",
                    fixture.name, expected_stdout, actual_stdout
                ));
            }
        }

        if let Some(expected_fragment) = &fixture.expected_stderr_contains {
            let actual_stderr = String::from_utf8_lossy(&output.stderr);
            if !actual_stderr.contains(expected_fragment.as_str()) {
                failures.push(format!(
                    "{}: expected stderr to contain {:?}, got {:?}",
                    fixture.name, expected_fragment, actual_stderr
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "fixture failures:\n{}",
        failures.join("\n")
    );
}
