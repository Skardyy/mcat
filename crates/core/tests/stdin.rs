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
    // WebM/EBML magic bytes
    Command::cargo_bin("mcat")
        .unwrap()
        .arg("--testing")
        .write_stdin(b"\x1a\x45\xdf\xa3".as_ref())
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: Video"));
}
