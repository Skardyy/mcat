use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn stdin_is_rendered_as_markdown() {
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin("# Header")
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Markdown"));
}
