use std::io::Write;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::Builder;

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

#[test]
fn file_md_is_detected_as_markdown() {
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .arg("README.md")
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Markdown"));
}

#[test]
fn stdin_png_detected_as_image() {
    // PNG magic bytes
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin(b"\x89PNG\r\n\x1a\n".as_ref())
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Image"));
}

#[test]
fn stdin_gif_detected_as_gif() {
    // GIF magic bytes
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin(b"GIF89a".as_ref())
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Gif"));
}

#[test]
fn stdin_pdf_detected_as_pdf() {
    // PDF magic bytes
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin(b"%PDF-".as_ref())
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Pdf"));
}

#[test]
fn stdin_jpeg_detected_as_image() {
    // JPEG magic bytes
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin(b"\xff\xd8\xff".as_ref())
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Image"));
}

#[test]
fn stdin_webm_detected_as_video() {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"\x1a\x45\xdf\xa3");
    buf.extend_from_slice(b"\x42\x82\x84webm");
    buf.resize(512, 0);

    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin(buf)
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Video"));
}

#[test]
fn stdout_single_trailing_newline() {
    for args in [vec!["-c"], vec!["-c", "--output", "md"]] {
        let output = Command::cargo_bin("mcat")
            .unwrap()
            .args(&args)
            .write_stdin("# Foo\n")
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.ends_with(b"\n"));
        assert!(!output.stdout.ends_with(b"\n\n"));
    }
}

#[test]
fn mmd_file_with_diagram_is_mermaid() {
    let mut f = Builder::new().suffix(".mmd").tempfile().unwrap();
    f.write_all(b"classDiagram\n  Animal <|-- Dog").unwrap();

    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .arg(f.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Mermaid"));
}

#[test]
fn mmd_file_with_mathpix_content_is_markdown() {
    let mut f = Builder::new().suffix(".mmd").tempfile().unwrap();
    f.write_all(b"# Theorem 1\n\nWe have \\( x^2 + y^2 = z^2 \\).")
        .unwrap();

    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .arg(f.path())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("kind: Markdown")
                .or(predicate::str::contains("kind: PreMarkdown")),
        );
}
