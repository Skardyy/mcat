use std::io::Write;

use assert_cmd::Command;
use base64::{
    Engine,
    engine::general_purpose::{self},
};
use predicates::prelude::*;
use tempfile::Builder;

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

// see https://github.com/Skardyy/mcat/issues/86
#[test]
fn rust_file_piped_is_raw() {
    let mut f = Builder::new().suffix(".rs").tempfile().unwrap();
    f.write_all(b"fn main() { println!(\"Hello\"); }\n")
        .unwrap();

    let output = Command::cargo_bin("mcat")
        .unwrap()
        .arg(f.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"fn main() { println!(\"Hello\"); }\n");
}

#[test]
fn multiple_files_piped_concatenated_raw() {
    let mut a = Builder::new().suffix(".rs").tempfile().unwrap();
    a.write_all(b"fn a() {}\n").unwrap();
    let mut b = Builder::new().suffix(".rs").tempfile().unwrap();
    b.write_all(b"fn b() {}\n").unwrap();

    let output = Command::cargo_bin("mcat")
        .unwrap()
        .arg(a.path())
        .arg(b.path())
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, b"fn a() {}\nfn b() {}\n");
}

// see https://github.com/Skardyy/mcat/issues/91
#[test]
fn stdin_md_with_color_always_flag_is_rendered_not_raw() {
    let output = Command::cargo_bin("mcat")
        .unwrap()
        .arg("-c")
        .write_stdin("# Header\n\nhello world")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(
        output.stdout.contains(&b'\x1b'),
        "expected ANSI escape sequences in colored output"
    );
}
