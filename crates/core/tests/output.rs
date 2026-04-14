use assert_cmd::Command;
use base64::{
    Engine,
    engine::general_purpose::{self},
};
use predicates::prelude::*;

#[test]
fn stdin_md_output_is_raw_markdown() {
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--output")
        .arg("md")
        .write_stdin("# Header\n\nhello world")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("# Header"));
}

#[test]
fn stdin_md_output_is_html() {
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--output")
        .arg("html")
        .write_stdin("# Header\n\nhello world")
        .assert()
        .success()
        .stdout(predicate::str::contains("<h1>"))
        .stdout(predicate::str::contains("Header"));
}

#[test]
fn stdin_svg_output_is_image() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#;
    let output = Command::cargo_bin("mcat")
        .unwrap()
        .arg("--output")
        .arg("image")
        .write_stdin(svg)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.starts_with(b"\x89PNG"));
}

#[test]
fn stdin_pdf_output_is_image() {
    let pdf = general_purpose::STANDARD
        .decode(
            "JVBERi0xLjQKMSAwIG9iago8PC9UeXBlIC9DYXRhbG9nCi9QYWdlcyAyIDAgUgo+PgplbmRvYmoK\
         MiAwIG9iago8PC9UeXBlIC9QYWdlcwovS2lkcyBbMyAwIFJdCi9Db3VudCAxCj4+CmVuZG9iagoz\
         IDAgb2JqCjw8L1R5cGUgL1BhZ2UKL1BhcmVudCAyIDAgUgovTWVkaWFCb3ggWzAgMCA1OTUgODQy\
         XQovQ29udGVudHMgNSAwIFIKL1Jlc291cmNlcyA8PC9Qcm9jU2V0IFsvUERGIC9UZXh0XQovRm9u\
         dCA8PC9GMSA0IDAgUj4+Cj4+Cj4+CmVuZG9iago0IDAgb2JqCjw8L1R5cGUgL0ZvbnQKL1N1YnR5\
         cGUgL1R5cGUxCi9OYW1lIC9GMQovQmFzZUZvbnQgL0hlbHZldGljYQovRW5jb2RpbmcgL01hY1Jv\
         bWFuRW5jb2RpbmcKPj4KZW5kb2JqCjUgMCBvYmoKPDwvTGVuZ3RoIDUzCj4+CnN0cmVhbQpCVAov\
         RjEgMjAgVGYKMjIwIDQwMCBUZAooRHVtbXkgUERGKSBUagpFVAplbmRzdHJlYW0KZW5kb2JqCnhy\
         ZWYKMCA2CjAwMDAwMDAwMDAgNjU1MzUgZgowMDAwMDAwMDA5IDAwMDAwIG4KMDAwMDAwMDA2MyAw\
         MDAwMCBuCjAwMDAwMDAxMjQgMDAwMDAgbgowMDAwMDAwMjc3IDAwMDAwIG4KMDAwMDAwMDM5MiAw\
         MDAwMCBuCnRyYWlsZXIKPDwvU2l6ZSA2Ci9Sb290IDEgMCBSCj4+CnN0YXJ0eHJlZgo0OTUKJSVF\
         T0YK",
        )
        .unwrap();

    let output = Command::cargo_bin("mcat")
        .unwrap()
        .arg("--output")
        .arg("image")
        .write_stdin(pdf)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stdout.starts_with(b"\x89PNG"));
}
