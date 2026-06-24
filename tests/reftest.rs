// TypePress reftest framework — automated regression tests.
//
// Tests cover:
//   1. @font-face CSS parsing
//   2. Markdown→PDF end-to-end pipeline
//   3. HTML pipeline (CSS/AssetBundle)
//   4. Negative / error-path tests
//   5. PDF structural quality

use std::path::PathBuf;

// ── Test helpers ────────────────────────────────────────────────────────

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn tmp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("typepress_reftest_{}", name))
}

fn has_cjk_font() -> bool {
    PathBuf::from("/mnt/c/Windows/Fonts/simsun.ttc").exists()
        || PathBuf::from("/usr/share/fonts").is_dir()
}

// ── @font-face parsing tests ────────────────────────────────────────────

#[test]
fn test_parse_font_face_basic() {
    let css = r#"
        @font-face {
            font-family: "MyFont";
            src: url("fonts/MyFont.woff2");
        }
    "#;
    let faces = typepress::fonts::parse_font_faces(css);
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "MyFont");
    assert_eq!(faces[0].src_url, "fonts/MyFont.woff2");
}

#[test]
fn test_parse_font_face_multiple() {
    let css = r#"
        @font-face { font-family: "A"; src: url("a.ttf"); }
        @font-face { font-family: "B"; src: url("b.woff2"); }
    "#;
    let faces = typepress::fonts::parse_font_faces(css);
    assert_eq!(faces.len(), 2);
    assert_eq!(faces[0].family, "A");
    assert_eq!(faces[1].family, "B");
}

#[test]
fn test_parse_font_face_no_fonts() {
    let css = "body { color: red; }";
    let faces = typepress::fonts::parse_font_faces(css);
    assert!(faces.is_empty());
}

#[test]
fn test_extract_font_faces_from_html() {
    let html = r#"<html><head><style>
        @font-face { font-family: "WebFont"; src: url("web.woff2"); }
    </style></head><body></body></html>"#;
    let faces = typepress::fonts::extract_font_faces_from_html(html);
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "WebFont");
}

// ── End-to-end pipeline tests ───────────────────────────────────────────

#[test]
fn test_e2e_markdown_to_pdf() {
    let md = "# Test\n\nContent here.";
    let pdf_path = tmp_path("e2e.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists(), "PDF should exist");
    assert!(
        pdf_path.metadata().unwrap().len() > 100,
        "PDF should have content"
    );
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_e2e_with_header_footer() {
    let md = "# Doc\n\nBody.\n\nMore body.";
    let pdf_path = tmp_path("hf.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], Some("My Header"), Some("Page N"))
        .unwrap();
    assert!(pdf_path.exists());
    assert!(pdf_path.metadata().unwrap().len() > 100);
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_e2e_mermaid_processing() {
    let md = "# Diagram\n\n```mermaid\ngraph TD\n  A-->B\n```\n\nText after.";
    let pdf_path = tmp_path("mermaid.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_e2e_math_rendering() {
    let md = "# Math\n\nEinstein: $E = mc^2$\n\nDisplay: $$\\int_0^\\infty e^{-x^2} dx$$";
    let pdf_path = tmp_path("math.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists());
    assert!(pdf_path.metadata().unwrap().len() > 100);
    let _ = std::fs::remove_file(&pdf_path);
}

// ── HTML pipeline tests ─────────────────────────────────────────────────

#[test]
fn test_html_direct_input() {
    let html = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body><h1>Direct HTML</h1><p>Rendered from raw HTML input.</p></body></html>";
    let pdf_path = tmp_path("html_direct.pdf");
    use fulgur::engine::Engine;
    let engine = Engine::builder().build();
    let pdf = engine.render_html(html).unwrap();
    std::fs::write(&pdf_path, &pdf).unwrap();
    assert!(pdf_path.exists());
    assert!(pdf.starts_with(b"%PDF-"));
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_html_with_custom_css() {
    let html = "<!DOCTYPE html><html><head><meta charset=\"utf-8\"></head><body><h1>Styled</h1><p class=\"red\">Red text via CSS.</p></body></html>";
    let pdf_path = tmp_path("html_css.pdf");
    use fulgur::asset::AssetBundle;
    use fulgur::engine::Engine;
    let mut assets = AssetBundle::new();
    assets.add_css(".red { color: red; font-weight: bold; }");
    let engine = Engine::builder().assets(assets).build();
    let pdf = engine.render_html(html).unwrap();
    std::fs::write(&pdf_path, &pdf).unwrap();
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

// ── Negative / error-path tests ─────────────────────────────────────────

#[test]
fn test_negative_empty_markdown() {
    let pdf_path = tmp_path("empty.pdf");
    let result = typepress::render_markdown_to_pdf("", &pdf_path, &[], &[], None, None);
    assert!(
        result.is_ok(),
        "Empty markdown should succeed: {:?}",
        result.err()
    );
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_negative_invalid_html_no_panic() {
    let md = "unmatched <b> tags and <broken stuff";
    let pdf_path = tmp_path("invalid.pdf");
    let result = typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None);
    assert!(
        result.is_ok(),
        "Invalid HTML should not crash: {:?}",
        result.err()
    );
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_negative_multiline_math_edge_cases() {
    let md = "$$\n\\begin{aligned}\nx &= 1 \\\\\ny &= 2\n\\end{aligned}\n$$\n\n$$\n\n\n$$\n\nText after empty math.";
    let pdf_path = tmp_path("math_edge.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_negative_special_characters() {
    let md =
        "# Special Chars\n\n& < > \" ' \\n\\n\\t tab\\r\\n\\nUnicode: Café • ★ λ σ\\n\\nEmoji: 🙂";
    let pdf_path = tmp_path("special.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_negative_very_long_content() {
    let mut md = String::from("# Long Document\n\n");
    for i in 0..50 {
        md.push_str(&format!("## Section {}\n\n", i));
        md.push_str(&"Long paragraph with lots of text. ".repeat(50));
        md.push_str("\n\n");
    }
    let pdf_path = tmp_path("long.pdf");
    typepress::render_markdown_to_pdf(&md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_highlight_empty_code_blocks() {
    let md = "# Code\n\n```python\n\n```\n\n```rust\n// only a comment\n```";
    let pdf_path = tmp_path("empty_code.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    assert!(pdf_path.exists());
    let _ = std::fs::remove_file(&pdf_path);
}

// ── PDF quality tests ───────────────────────────────────────────────────

#[test]
fn test_pdf_valid_structure() {
    let md = "# PDF Structure\n\nContent.";
    let pdf_path = tmp_path("struct.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    let header = std::fs::read(&pdf_path).unwrap();
    assert!(header.starts_with(b"%PDF-"), "PDF must start with %PDF-");
    let _ = std::fs::remove_file(&pdf_path);
}
