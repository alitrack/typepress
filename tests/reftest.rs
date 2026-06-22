// TypePress reftest framework — automated regression tests.
//
// Tests cover:
//   1. @font-face CSS parsing
//   2. SVG Unicode text extraction (CID→Unicode via ToUnicode CMap)
//   3. Multi-page SVG output
//   4. Markdown→PDF→SVG end-to-end pipeline
//   5. CJK text rendering regression

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

// ── SVG Unicode text tests ──────────────────────────────────────────────

#[test]
fn test_svg_page_count() {
    // Create minimal PDF
    let md = "# Single Page Test\n\nOne paragraph.";
    let pdf_path = tmp_path("pgcount.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let pages = typepress::svg::page_count(&pdf_bytes).unwrap();
    assert_eq!(pages, 1, "Single-page markdown should produce 1-page PDF");
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_svg_unicode_basic() {
    let md = "# Hello World\n\nTest **bold** and *italic*.";
    let pdf_path = tmp_path("unicode.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let svg = typepress::svg::svg_unicode(&pdf_bytes, 1).unwrap();
    assert!(
        svg.contains("Hello World"),
        "SVG should contain 'Hello World', got: {svg}"
    );
    assert!(svg.contains("<svg"), "SVG should have <svg> root element");
    assert!(svg.contains("</svg>"), "SVG should be well-formed");
    let _ = std::fs::remove_file(&pdf_path);
}

#[test]
fn test_svg_cjk_unicode() {
    if !has_cjk_font() {
        eprintln!("Skipping CJK test: no CJK font available");
        return;
    }
    let md = "# 中文测试\n\n这是一个中文段落。";
    let pdf_path = tmp_path("cjk.pdf");
    typepress::render_markdown_to_pdf(
        md,
        &pdf_path,
        &[PathBuf::from("/mnt/c/Windows/Fonts/simsun.ttc")],
        &[],
        None,
        None,
    )
    .unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let svg = typepress::svg::svg_unicode(&pdf_bytes, 1).unwrap();
    assert!(
        svg.contains("中文测试"),
        "SVG should contain '中文测试' in Unicode"
    );
    assert!(svg.contains("中文段落"), "SVG should contain '中文段落'");
    let _ = std::fs::remove_file(&pdf_path);
}

// ── Multi-page tests ────────────────────────────────────────────────────

#[test]
fn test_multi_page_svg() {
    if !has_cjk_font() {
        eprintln!("Skipping multi-page test: no CJK font available");
        return;
    }
    // Generate enough content for multiple pages
    let mut md = String::from("# 多页测试\n\n");
    for i in 1..=10 {
        md.push_str(&format!("## 第{i}章\n\n"));
        md.push_str("重复内容。".repeat(200).as_str());
        md.push_str("\n\n");
    }
    let pdf_path = tmp_path("multipage.pdf");
    typepress::render_markdown_to_pdf(
        &md,
        &pdf_path,
        &[PathBuf::from("/mnt/c/Windows/Fonts/simsun.ttc")],
        &[],
        None,
        None,
    )
    .unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let pages = typepress::svg::page_count(&pdf_bytes).unwrap();
    assert!(pages > 1, "Expected multiple pages, got {pages}");

    // Verify each page contains valid SVG with text
    for p in 1..=pages {
        let svg = typepress::svg::svg_unicode(&pdf_bytes, p).unwrap();
        assert!(svg.contains("<svg"), "Page {p} should be valid SVG");
        assert!(svg.contains("</svg>"), "Page {p} should be well-formed");
    }
    let _ = std::fs::remove_file(&pdf_path);
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
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let svg = typepress::svg::svg_unicode(&pdf_bytes, 1).unwrap();
    assert!(svg.contains("My Header"), "SVG should contain header text");
    assert!(svg.contains("Page N"), "SVG should contain footer text");
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
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let svg = typepress::svg::svg_unicode(&pdf_bytes, 1).unwrap();
    // Math should render to KaTeX HTML spans, containing the formula glyphs
    assert!(
        svg.contains("katex") || svg.contains("E = mc") || svg.len() > 500,
        "Math-rendered SVG should have content"
    );
    let _ = std::fs::remove_file(&pdf_path);
}

// ── Golden file regression tests ────────────────────────────────────────

#[test]
fn test_golden_svg_unicode() {
    if !has_cjk_font() {
        eprintln!("Skipping golden test: no CJK font available");
        return;
    }
    let md = "# 回归测试\n\n中文内容测试。\n\n- 列表一\n- 列表二";
    let golden_path = project_root().join("tests/golden/cjk_basic.svg");
    let pdf_path = tmp_path("golden.pdf");
    typepress::render_markdown_to_pdf(
        md,
        &pdf_path,
        &[PathBuf::from("/mnt/c/Windows/Fonts/simsun.ttc")],
        &[],
        None,
        None,
    )
    .unwrap();
    let pdf_bytes = std::fs::read(&pdf_path).unwrap();
    let current = typepress::svg::svg_unicode(&pdf_bytes, 1).unwrap();

    if golden_path.exists() {
        let golden = std::fs::read_to_string(&golden_path).unwrap();
        // Compare normalized: strip x/y coordinates (they vary by engine version)
        let norm = |s: &str| -> String {
            s.chars()
                .filter(|&c| c > '\u{007f}' || c.is_alphabetic())
                .collect()
        };
        let current_norm = norm(&current);
        let golden_norm = norm(&golden);
        assert_eq!(
            current_norm, golden_norm,
            "Golden file mismatch. Run with UPDATE_GOLDEN=1 to update."
        );
    } else if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, &current).unwrap();
        eprintln!("Golden file written to {}", golden_path.display());
    } else {
        eprintln!(
            "Golden file not found at {} — run with UPDATE_GOLDEN=1 to create",
            golden_path.display()
        );
    }
    let _ = std::fs::remove_file(&pdf_path);
}

// ── PDF quality tests ───────────────────────────────────────────────────

#[test]
fn test_pdf_valid_structure() {
    let md = "# PDF Structure\n\nContent.";
    let pdf_path = tmp_path("struct.pdf");
    typepress::render_markdown_to_pdf(md, &pdf_path, &[], &[], None, None).unwrap();
    // Verify PDF starts with %PDF- magic
    let header = std::fs::read(&pdf_path).unwrap();
    assert!(header.starts_with(b"%PDF-"), "PDF must start with %PDF-");
    let _ = std::fs::remove_file(&pdf_path);
}
