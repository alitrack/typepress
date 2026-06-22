// TypePress CLI integration tests — uses assert_cmd to verify end-to-end behavior.
//
// Tests cover:
//   1. Basic HTML → PDF
//   2. Markdown → PDF with math
//   3. Format flags (--from md, --format svg, --format png)
//   4. Header/footer
//   5. YAML config

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("typepress_cli_{}", name))
}

fn binary() -> Command {
    Command::cargo_bin("typepress").unwrap()
}

// ── Basic CLI tests ──────────────────────────────────────────────────────

#[test]
fn cli_help() {
    let mut cmd = binary();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pure Rust HTML/CSS"))
        .stdout(predicate::str::contains("--from"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--math"));
}

#[test]
fn cli_version() {
    let mut cmd = binary();
    cmd.arg("--version");
    cmd.assert().success();
}

// ── Format tests ─────────────────────────────────────────────────────────

#[test]
fn cli_html_to_pdf() {
    let out = tmp_path("cli_basic.pdf");
    let _ = std::fs::remove_file(&out);

    let mut cmd = binary();
    cmd.args(["--stdin", "-o", out.to_str().unwrap()]);
    cmd.write_stdin("<html><body><h1>Hello</h1></body></html>");
    cmd.assert().success();

    assert!(out.exists());
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_md_to_pdf() {
    let out = tmp_path("cli_md.pdf");
    let _ = std::fs::remove_file(&out);

    let mut cmd = binary();
    cmd.args(["--from", "md", "--stdin", "-o", out.to_str().unwrap()]);
    cmd.write_stdin("# Test\n\nHello world.");
    cmd.assert().success();

    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_md_with_math() {
    let out = tmp_path("cli_math.pdf");
    let _ = std::fs::remove_file(&out);

    let mut cmd = binary();
    cmd.args(["--from", "md", "--math", "--stdin", "-o", out.to_str().unwrap()]);
    cmd.write_stdin("# Math\n\n$E=mc^2$");
    cmd.assert().success();

    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_md_to_svg() {
    let out = tmp_path("cli_svg.svg");
    let _ = std::fs::remove_file(&out);

    let mut cmd = binary();
    cmd.args([
        "--from", "md", "--stdin",
        "--format", "svg",
        "-o", out.to_str().unwrap(),
    ]);
    cmd.write_stdin("# SVG Test\n\nContent.");
    cmd.assert().success();
    cmd.assert().stderr(predicate::str::contains("SVG"));

    assert!(out.exists());
    let svg = std::fs::read_to_string(&out).unwrap();
    assert!(svg.contains("<svg"), "Output should be valid SVG");
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_md_to_png() {
    let out = tmp_path("cli_png.png");
    let _ = std::fs::remove_file(&out);

    let mut cmd = binary();
    cmd.args([
        "--from", "md", "--stdin",
        "--format", "png",
        "--scale", "1.0",
        "-o", out.to_str().unwrap(),
    ]);
    cmd.write_stdin("# PNG Test\n\nOne line.");
    cmd.assert().success();

    assert!(out.exists());
    assert!(out.metadata().unwrap().len() > 100);
    let _ = std::fs::remove_file(&out);
}

// ── Header/footer tests ──────────────────────────────────────────────────

#[test]
fn cli_with_header_footer() {
    let out = tmp_path("cli_hf.pdf");
    let _ = std::fs::remove_file(&out);

    let mut cmd = binary();
    cmd.args([
        "--from", "md", "--stdin",
        "--header", "My Report",
        "--footer", "Page 1",
        "-o", out.to_str().unwrap(),
    ]);
    cmd.write_stdin("# Report\n\nBody.");
    cmd.assert().success();

    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}

// ── Error handling tests ─────────────────────────────────────────────────

#[test]
fn cli_missing_input() {
    let mut cmd = binary();
    cmd.assert().failure();
}

#[test]
fn cli_bad_format() {
    let out = tmp_path("cli_bad.pdf");
    let mut cmd = binary();
    cmd.args(["--format", "xyz", "--stdin", "-o", out.to_str().unwrap()]);
    cmd.write_stdin("test");
    // Unknown format falls back to PDF silently
    cmd.assert().success();
    let _ = std::fs::remove_file(&out);
}

#[test]
fn cli_nonexistent_file() {
    let mut cmd = binary();
    cmd.arg("/nonexistent/path/to/nowhere.html");
    cmd.assert().failure();
}
