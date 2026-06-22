// TypePress library — re-exports for integration tests and potential library usage.

pub mod config;
pub mod css;
pub mod fonts;
pub mod svg;
pub mod highlight;

use anyhow::Result;
use std::path::Path;

// Re-export CSS constants for convenience
pub use css::DEFAULT_PRINT_CSS;

/// Convert Markdown to HTML with GFM extensions and print CSS.
/// Returns a complete HTML document ready for fulgur rendering.
/// Extra CSS is appended to the default print stylesheet (e.g. KATEX_CSS for math).
pub fn markdown_to_html(input: &str) -> String {
    markdown_to_html_with_css(input, "")
}

/// Same as markdown_to_html but with an extra CSS string appended to the stylesheet.
pub fn markdown_to_html_with_css(input: &str, extra_css: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let options = Options::all();
    let parser = Parser::new_ext(input, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    // Fix self-closing tags that Blitz/fulgur doesn't understand
    let html_output = html_output.replace(" />", ">");
    let combined_css = format!("{DEFAULT_PRINT_CSS}{extra_css}");
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><style>{combined_css}</style></head><body>\n{html_output}\n</body></html>"
    )
}

/// Inject header/footer running elements and @page CSS rules.
/// Uses CSS GCPM (Generated Content for Paged Media) running elements:
///   @top-center { content: element(typepress-hdr); }
///   @bottom-center { content: element(typepress-ftr); }
///
/// Returns the @page CSS string that should be added to the AssetBundle.
/// Returns None if neither header nor footer is provided.
pub fn inject_header_footer(
    html: &mut String,
    header: Option<&str>,
    footer: Option<&str>,
) -> Option<String> {
    if header.is_none() && footer.is_none() {
        return None;
    }

    let mut page_css = String::new();
    let mut body_prefix = String::new();
    let mut body_suffix = String::new();

    if let Some(h) = header {
        let escaped = h
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        body_prefix.push_str(&format!(
            "<div style=\"position:running(typepress-hdr)\">{escaped}</div>\n"
        ));
        page_css.push_str(
            "@top-center { content: element(typepress-hdr); font-size: 9pt; color: #555; }\n",
        );
    }
    if let Some(f) = footer {
        let escaped = f
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        body_suffix.push_str(&format!(
            "<div style=\"position:running(typepress-ftr)\">{escaped}</div>\n"
        ));
        page_css.push_str(
            "@bottom-center { content: element(typepress-ftr); font-size: 8pt; color: #888; }\n",
        );
    }

    let css = format!("@page {{ {page_css} }}");

    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, &format!("<style>{css}</style>"));
    } else if let Some(pos) = html.find("<body") {
        html.insert_str(pos, &format!("<style>{css}</style>\n"));
    }

    if let Some(pos) = html.find("<body") {
        let body_end = html[pos..]
            .find('>')
            .map(|p| pos + p + 1)
            .unwrap_or(pos + 5);
        html.insert_str(body_end, &format!("\n{body_prefix}"));
    }
    if let Some(pos) = html.rfind("</body>") {
        html.insert_str(pos, &body_suffix);
    }

    Some(css)
}

/// Render markdown to PDF. This is the core library entry point.
///
/// This function handles the full pipeline: Markdown → HTML (with GFM),
/// header/footer injection, and fulgur rendering.
///
/// For advanced features (math, mermaid, syntax highlighting), use the
/// CLI which orchestrates the complete preprocessing pipeline.
pub fn render_markdown_to_pdf(
    markdown: &str,
    output: &Path,
    fonts: &[std::path::PathBuf],
    css_files: &[std::path::PathBuf],
    header: Option<&str>,
    footer: Option<&str>,
) -> Result<()> {
    use fulgur::asset::AssetBundle;
    use fulgur::engine::Engine;

    // Convert Markdown to HTML
    let mut html = markdown_to_html(markdown);

    // Inject header/footer
    let header_css = inject_header_footer(&mut html, header, footer);

    // Build assets
    let needs_assets = !fonts.is_empty() || !css_files.is_empty() || header_css.is_some();
    let assets = if needs_assets {
        let mut bundle = AssetBundle::new();
        if let Some(ref css) = header_css {
            bundle.add_css(css);
        }
        for f in fonts {
            if let Err(e) = bundle.add_font_file(f) {
                eprintln!("Warning: font {}: {e}", f.display());
            }
        }
        for f in css_files {
            if let Err(e) = bundle.add_css_file(f) {
                eprintln!("Warning: CSS {}: {e}", f.display());
            }
        }
        Some(bundle)
    } else {
        None
    };

    // Build engine
    let mut builder = Engine::builder();
    if let Some(a) = assets {
        builder = builder.assets(a);
    }
    let engine = builder.build();
    let pdf = engine.render_html(&html)?;
    std::fs::write(output, &pdf)?;
    Ok(())
}
