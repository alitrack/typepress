// TypePress — Pure Rust HTML/CSS → PDF engine.
// Uses fulgur (Blitz→Taffy→Krilla) as the rendering backend.
//
// Key additions over vanilla fulgur:
//   - --header / --footer CLI shortcuts (CSS GCPM running elements)
//   - --math auto-detection (katex-rs rendering + KaTeX font loading)
//   - CJK font handling with automatic subsetting

use anyhow::{Context, Result};
use clap::Parser;
use fulgur::asset::AssetBundle;
use fulgur::config::{Margin, PageSize};
use fulgur::engine::Engine;
use regex::Regex;
use std::path::{Path, PathBuf};

mod config;
mod svg;
mod fonts;
mod highlight;
use config::TypePressConfig;

// ── CLI ────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "typepress", version, about = "Pure Rust HTML/CSS → PDF engine")]
struct Cli {
    /// Input HTML file (omit for --stdin)
    input: Option<PathBuf>,

    /// Read HTML from stdin
    #[arg(long)]
    stdin: bool,

    /// Output PDF file path (use "-" for stdout). Required in CLI mode, optional with --config.
    #[arg(short, long)]
    output: Option<PathBuf>,

    // ── Input format ──
    /// Input format: html (default) or md (markdown)
    #[arg(long = "from", default_value = "html")]
    from: String,

    // ── Output format ──
    /// Output format: pdf (default), svg, or png
    #[arg(long = "format", short = 'F', default_value = "pdf")]
    format: String,

    /// Scale factor for PNG output (default: 2.0 for retina)
    #[arg(long, default_value = "2.0")]
    scale: f32,

    // ── Config ──
    /// YAML config file (auto-detects typepress.yaml if omitted)
    #[arg(short = 'c', long)]
    config: Option<PathBuf>,

    // ── Page ──
    /// Page size: A4, Letter, A3, etc.
    #[arg(short, long)]
    size: Option<String>,

    /// Landscape orientation
    #[arg(short, long)]
    landscape: bool,

    /// Page margins in mm (CSS shorthand: "20" or "10 20 30 40")
    #[arg(long)]
    margin: Option<String>,

    // ── Metadata ──
    #[arg(long)]
    title: Option<String>,
    #[arg(long = "author")]
    authors: Vec<String>,
    #[arg(long)]
    language: Option<String>,

    // ── Assets ──
    /// Font files to bundle (TTF/OTF/WOFF2). Repeatable.
    #[arg(long = "font", short = 'f')]
    fonts: Vec<PathBuf>,

    /// CSS files to include. Repeatable.
    #[arg(long = "css")]
    css_files: Vec<PathBuf>,

    // ── Headers & Footers ──
    /// Header text (top-center, every page)
    #[arg(long)]
    header: Option<String>,

    /// Footer text (bottom-center, every page)
    #[arg(long)]
    footer: Option<String>,

    // ── LaTeX Math ──
    /// Auto-detect and load KaTeX math fonts from npm/system paths.
    /// Renders $...$ (inline) and $$...$$ (display) math via katex-rs.
    #[arg(long)]
    math: bool,

    /// Explicit directory containing KaTeX font files (WOFF2/TTF/OTF).
    #[arg(long = "math-dir")]
    math_dir: Option<PathBuf>,

    // ── PDF features ──
    #[arg(long)]
    bookmarks: bool,
    #[arg(long)]
    tagged: bool,
    #[arg(long = "pdf-ua")]
    pdf_ua: bool,
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse_page_size(s: &str) -> PageSize {
    match s.to_uppercase().as_str() {
        "A4" => PageSize::A4,
        "A3" => PageSize::A3,
        "LETTER" => PageSize::LETTER,
        _ => {
            eprintln!("Unknown page size '{s}', defaulting to A4");
            PageSize::A4
        }
    }
}

fn parse_margin(s: &str) -> Margin {
    let values: Vec<f32> = s
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    if values.is_empty() {
        return Margin::default();
    }
    let to_pt = |mm: f32| mm * 72.0 / 25.4;
    match values.as_slice() {
        [all] => Margin::uniform(to_pt(*all)),
        [vert, horiz] => Margin::symmetric(to_pt(*vert), to_pt(*horiz)),
        [top, horiz, bottom] => Margin {
            top: to_pt(*top),
            right: to_pt(*horiz),
            bottom: to_pt(*bottom),
            left: to_pt(*horiz),
        },
        [top, right, bottom, left] => Margin {
            top: to_pt(*top),
            right: to_pt(*right),
            bottom: to_pt(*bottom),
            left: to_pt(*left),
        },
        _ => Margin::default(),
    }
}

// ── Markdown Processing ────────────────────────────────────────────────

fn process_markdown(input: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let options = Options::all();
    let parser = Parser::new_ext(input, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);
    // Fix self-closing tags that Blitz/fulgur doesn't understand
    let html_output = html_output.replace(" />", ">");
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"><style>{DEFAULT_PRINT_CSS}</style></head><body>\n{html_output}\n</body></html>"
    )
}

/// Default CSS for print/document styling injected into Markdown output.
const DEFAULT_PRINT_CSS: &str = r#"
    table { border-collapse: collapse; width: 100%; }
    th { background: #eee; font-weight: bold; }
    td, th { border: 1px solid #999; padding: 4pt 8pt; text-align: left; }
    pre { background: #f5f5f5; border: 1px solid #ddd; padding: 8pt; font-family: monospace; font-size: 9pt; }
    pre code { background: none; padding: 0; }
    code { background: #f0f0f0; padding: 1pt 3pt; }
    blockquote { border-left: 3px solid #ccc; margin: 10pt 0; padding: 4pt 12pt; color: #555; }
    tr { break-inside: avoid; page-break-inside: avoid; }
    thead { display: table-header-group; }
    h2, h3 { break-after: avoid; page-break-after: avoid; }
"#;

// ── Math Processing ────────────────────────────────────────────────────

const ESCAPED_PLACEHOLDER: &str = "\x00TXP_ESC_DOLLAR\x00";

const KATEX_CSS: &str = r#"
.katex-display{display:block;text-align:center;margin:1em 0}
.katex-display>.katex{display:inline-block;text-align:initial}
.katex-inline{display:inline}
.katex{font:normal 1.21em KaTeX_Main,Times New Roman,serif;line-height:1.2;text-indent:0}
.katex .mathrm,.katex .textrm,.katex .text,.katex .textnormal,.katex .textmd{font-family:KaTeX_Main}
.katex .mathit,.katex .textit{font-family:KaTeX_Math;font-style:italic}
.katex .mathbf,.katex .textbf,.katex .textbold{font-family:KaTeX_Main;font-weight:bold}
.katex .amsrm,.katex .mathbb,.katex .textbb{font-family:KaTeX_AMS}
.katex .mathcal,.katex .textcal{font-family:KaTeX_Caligraphic}
.katex .mathfrak,.katex .textfrak{font-family:KaTeX_Fraktur}
.katex .mathtt,.katex .texttt{font-family:KaTeX_Typewriter}
.katex .mathscr,.katex .textscr{font-family:KaTeX_Script}
.katex .mathsf,.katex .textsf{font-family:KaTeX_SansSerif}
.katex .mathnormal{font-family:KaTeX_Math;font-style:italic}
.katex .mainrm{font-family:KaTeX_Main;font-style:normal}
.katex .delimsizing.size1{font-family:KaTeX_Size1}
.katex .delimsizing.size2{font-family:KaTeX_Size2}
.katex .delimsizing.size3{font-family:KaTeX_Size3}
.katex .delimsizing.size4{font-family:KaTeX_Size4}
.katex .op-symbol{font-family:KaTeX_Size1}
.katex .op-symbol.large-op{font-family:KaTeX_Size2}
.katex .accent-body{font-family:KaTeX_Main}
"#;

fn process_math(html: &mut String) -> Result<usize> {
    use katex::{KatexContext, Settings, render_to_string};

    *html = html.replace("\\$", ESCAPED_PLACEHOLDER);

    let display_re = Regex::new(r"\$\$(.+?)\$\$")?;
    let inline_re = Regex::new(r"\$([^$]+?)\$")?;

    let ctx = KatexContext::default();
    let count = &mut 0usize;

    // Display math $$...$$
    while let Some(caps) = display_re.captures(html) {
        let latex = caps.get(1).unwrap().as_str().trim();
        let rendered = render_to_string(
            &ctx,
            latex,
            &Settings {
                display_mode: true,
                ..Default::default()
            },
        )
        .map_err(|e| anyhow::anyhow!("katex error in display math: {e:?}"))?;
        let range = caps.get(0).unwrap().range();
        html.replace_range(
            range,
            &format!("<span class=\"katex-display\">{rendered}</span>"),
        );
        *count += 1;
    }

    // Inline math $...$
    while let Some(caps) = inline_re.captures(html) {
        let latex = caps.get(1).unwrap().as_str().trim();
        let rendered = render_to_string(&ctx, latex, &Settings::default())
            .map_err(|e| anyhow::anyhow!("katex error in inline math: {e:?}"))?;
        let range = caps.get(0).unwrap().range();
        html.replace_range(
            range,
            &format!("<span class=\"katex-inline\">{rendered}</span>"),
        );
        *count += 1;
    }

    *html = html.replace(ESCAPED_PLACEHOLDER, "\\$");

    // Inject KaTeX CSS
    if *count > 0 {
        inject_css(html, KATEX_CSS);
    }

    Ok(*count)
}

// ── Mermaid Processing ─────────────────────────────────────────────────

fn process_mermaid(html: &mut String) -> Result<usize> {
    use mermaid_rs::{EstimatedMeasure, render_diagram};

    let re = Regex::new(r"(?s)```mermaid\n(.*?)```")?;
    let mut count = 0usize;

    // Collect all matches first (avoid borrow issues with mutable html)
    let matches: Vec<_> = re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).unwrap().range(),
                c.get(1).unwrap().as_str().to_string(),
            )
        })
        .collect();

    // Process in reverse order to preserve positions
    for (range, source) in matches.into_iter().rev() {
        let style = mermaid_rs::DiagramStyle::default();
        match render_diagram(&source, &style, &mut EstimatedMeasure) {
            Ok((svg, _w, _h)) => {
                let wrapped = format!(
                    "<div class=\"mermaid-diagram\" style=\"text-align:center;margin:1em 0\">{svg}</div>"
                );
                html.replace_range(range, &wrapped);
                count += 1;
            }
            Err(e) => eprintln!("Warning: mermaid render failed: {e}"),
        }
    }

    Ok(count)
}

// ── Font Scanning ──────────────────────────────────────────────────────

fn scan_font_dir(dir: &Path) -> Vec<PathBuf> {
    let mut fonts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("ttf" | "otf" | "woff2")) {
                fonts.push(path);
            }
        }
        fonts.sort();
    }
    fonts
}

fn auto_detect_katex_fonts() -> Option<PathBuf> {
    // 1. npm global install
    if let Ok(prefix) = std::process::Command::new("npm")
        .args(["config", "get", "prefix"])
        .output()
    {
        let root = String::from_utf8_lossy(&prefix.stdout).trim().to_string();
        let direct = PathBuf::from(&root).join("lib/node_modules/katex/dist/fonts");
        if direct.is_dir() {
            return Some(direct);
        }

        let base = PathBuf::from(&root).join("lib/node_modules");
        if let Some(found) = find_katex_fonts_in(&base, 0, 3) {
            return Some(found);
        }
    }

    // 2. System paths
    for p in &["/usr/share/katex/fonts", "/usr/local/share/katex/fonts"] {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

fn find_katex_fonts_in(dir: &Path, depth: usize, max: usize) -> Option<PathBuf> {
    if depth > max || !dir.is_dir() {
        return None;
    }
    let candidate = dir.join("katex/dist/fonts");
    if candidate.is_dir() {
        return Some(candidate);
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let nested = entry.path().join("node_modules/katex/dist/fonts");
            if nested.is_dir() {
                return Some(nested);
            }
            if entry.path().is_dir() {
                if let Some(found) = find_katex_fonts_in(&entry.path(), depth + 1, max) {
                    return Some(found);
                }
            }
        }
    }
    None
}

// ── Header / Footer ────────────────────────────────────────────────────

fn inject_header_footer(
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

// ── Utility ────────────────────────────────────────────────────────────

fn inject_css(html: &mut String, css: &str) {
    let tag = format!("<style>{css}</style>");
    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, &tag);
    } else if let Some(pos) = html.find("<body") {
        html.insert_str(pos, &format!("{tag}\n"));
    }
}

fn read_input(input: Option<&PathBuf>, stdin: bool) -> Result<String> {
    if stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        Ok(buf)
    } else if let Some(path) = input {
        Ok(std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?)
    } else {
        anyhow::bail!("provide an input HTML file or use --stdin")
    }
}

// ── Multi-Format Output ────────────────────────────────────────────────

fn render_png_from_pdf(pdf_bytes: &[u8], scale: f32) -> Result<Vec<u8>> {
    use tiny_skia::Pixmap;

    let tmp = tempfile::NamedTempFile::new()?;
    let path = tmp.path().to_path_buf();
    std::fs::write(&path, pdf_bytes)?;
    let result = fulgur::inspect::inspect(&path)?;

    let first_page = result.text_items.iter().filter(|t| t.page == 1);
    let w = 595.0 * scale; // A4 width in points
    let h = 842.0 * scale;

    let mut pixmap = Pixmap::new(w as u32, h as u32)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap {w}x{h}"))?;
    pixmap.fill(tiny_skia::Color::WHITE);

    // Simple approach: draw text items as colored rectangles
    // (Full text rendering would need font loading)
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::from_rgba8(0, 0, 0, 255)),
        ..Default::default()
    };
    for item in first_page {
        let rx = item.x * scale;
        let ry = item.y * scale;
        let rw = item.width.max(4.0) * scale;
        let rh = item.height * scale;
        let rect = tiny_skia::Rect::from_xywh(rx, ry, rw, rh)
            .unwrap_or(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap());
        pixmap.fill_rect(rect, &paint, tiny_skia::Transform::default(), None);
    }

    let png_data = pixmap.encode_png()?;
    Ok(png_data)
}

fn render_svg_from_pdf(pdf_bytes: &[u8]) -> Result<String> {
    svg::svg_unicode(pdf_bytes, 1)
}

/// Generate multi-page output filenames from a base path.
/// e.g., "out.svg" → ["out_page1.svg", "out_page2.svg"]
/// If only 1 page, returns just ["out.svg"].
fn page_output_paths(base: &Path, page_count: u32) -> Vec<PathBuf> {
    if page_count <= 1 {
        return vec![base.to_path_buf()];
    }
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    let ext = base.extension().unwrap_or_default().to_string_lossy();
    let parent = base.parent().unwrap_or(Path::new("."));
    (1..=page_count)
        .map(|p| {
            if ext.is_empty() {
                parent.join(format!("{stem}_page{p}"))
            } else {
                parent.join(format!("{stem}_page{p}.{ext}"))
            }
        })
        .collect()
}

fn write_svg_multi(pdf_bytes: &[u8], output: &Path) -> Result<()> {
    let pages = svg::page_count(pdf_bytes)?;
    let paths = page_output_paths(output, pages);
    for (i, path) in paths.iter().enumerate() {
        let svg_content = svg::svg_unicode(pdf_bytes, (i + 1) as u32)?;
        std::fs::write(path, svg_content)?;
        eprintln!("SVG page {} written to {}", i + 1, path.display());
    }
    Ok(())
}

// ── Main ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config: --config <file> or auto-detect typepress.yaml
    let cfg = if let Some(ref path) = cli.config {
        TypePressConfig::from_file(path).ok()
    } else {
        TypePressConfig::auto_detect().map(|(c, _)| c)
    };

    // Merge: CLI args override YAML values.
    let input_file = cli
        .input
        .clone()
        .or_else(|| cfg.as_ref().and_then(|c| c.input.clone()));
    let is_md = cli.from == "md"
        || input_file
            .as_ref()
            .and_then(|p| p.extension())
            .map_or(false, |e| e == "md")
        || cfg.as_ref().and_then(|c| c.from.as_deref()) == Some("md");
    let header = cli
        .header
        .clone()
        .or_else(|| cfg.as_ref().and_then(|c| c.header.clone()));
    let footer = cli
        .footer
        .clone()
        .or_else(|| cfg.as_ref().and_then(|c| c.footer.clone()));

    let base_path = if cli.stdin {
        std::env::current_dir().ok()
    } else {
        input_file.as_ref().and_then(|p| {
            p.canonicalize()
                .ok()
                .and_then(|abs| abs.parent().map(|d| d.to_path_buf()))
                .or_else(|| {
                    p.parent()
                        .map(|d| d.to_path_buf())
                        .filter(|d| !d.as_os_str().is_empty())
                })
                .or_else(|| std::env::current_dir().ok())
        })
    };

    // ── PDF passthrough: if input is already a PDF, just convert format ──
    let is_pdf_input = input_file
        .as_ref()
        .and_then(|p| p.extension())
        .map_or(false, |e| e == "pdf");
    if is_pdf_input && !cli.stdin {
        let pdf_bytes = std::fs::read(input_file.as_ref().unwrap())?;
        let to_stdout = cli.output.as_ref().map_or(false, |o| o.as_os_str() == "-");
        if to_stdout {
            match cli.format.as_str() {
                "svg" => print!("{}", render_svg_from_pdf(&pdf_bytes)?),
                "png" => {
                    use std::io::Write;
                    std::io::stdout().write_all(&render_png_from_pdf(&pdf_bytes, cli.scale)?)?;
                }
                _ => {
                    use std::io::Write;
                    std::io::stdout().write_all(&pdf_bytes)?;
                }
            }
        } else if let Some(ref output) = cli.output {
            match cli.format.as_str() {
                "svg" => write_svg_multi(&pdf_bytes, output)?,
                "png" => {
                    std::fs::write(output, render_png_from_pdf(&pdf_bytes, cli.scale)?)?;
                    eprintln!("PNG written to {}", output.display());
                }
                _ => {
                    std::fs::write(output, &pdf_bytes)?;
                    eprintln!("PDF written to {}", output.display());
                }
            }
        }
        return Ok(());
    }

    let mut html = read_input(input_file.as_ref(), cli.stdin)?;

    // 0a. Process Mermaid diagrams (before markdown→HTML conversion,
    // since mermaid blocks are markdown syntax, not HTML)
    if is_md {
        match process_mermaid(&mut html) {
            Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
            Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
            _ => {}
        }
    }

    // 0b. Convert markdown to HTML
    if is_md {
        html = process_markdown(&html);
    }

    // 1. Inject header/footer
    let header_css = inject_header_footer(&mut html, header.as_deref(), footer.as_deref());

    // 2. Process LaTeX math
    let math_fonts: Vec<PathBuf> = if cli.math || cli.math_dir.is_some() {
        let target = cli.math_dir.or_else(|| {
            if cli.math {
                auto_detect_katex_fonts()
            } else {
                None
            }
        });
        if let Some(ref dir) = target {
            let fonts = scan_font_dir(dir);
            if !fonts.is_empty() {
                eprintln!("Math: {} font(s) from {}", fonts.len(), dir.display());
            }
            fonts
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    match process_math(&mut html) {
        Ok(n) if n > 0 => eprintln!("Rendered {n} math expression(s)"),
        Err(e) => eprintln!("Warning: math processing failed: {e}"),
        _ => {}
    }

    // 2b. Process Mermaid diagrams (HTML input; MD already done in step 0a)
    if !is_md {
        match process_mermaid(&mut html) {
            Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
            Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
            _ => {}
        }
    }

    // 2c. Apply code syntax highlighting (syntect)
    match highlight::highlight_code_blocks(&mut html) {
        Ok(n) if n > 0 => eprintln!("Highlighted {n} code block(s)"),
        Err(e) => eprintln!("Warning: code highlighting failed: {e}"),
        _ => {}
    }

    // 3. Build asset bundle — start with @font-face font resolution
    let mut font_face_paths: Vec<PathBuf> = Vec::new();

    // Parse @font-face from inline styles in the HTML
    for ff in fonts::extract_font_faces_from_html(&html) {
        match fonts::resolve_font_path(&ff.src_url, base_path.as_deref()) {
            Ok(path) => font_face_paths.push(path),
            Err(e) => eprintln!("Warning: @font-face '{}': {e}", ff.family),
        }
    }

    // Parse @font-face from external CSS files
    for css_path in &cli.css_files {
        if let Ok(css_content) = std::fs::read_to_string(css_path) {
            for ff in fonts::parse_font_faces(&css_content) {
                let css_dir = css_path.parent();
                match fonts::resolve_font_path(&ff.src_url, css_dir.or(base_path.as_deref())) {
                    Ok(path) => font_face_paths.push(path),
                    Err(e) => eprintln!(
                        "Warning: @font-face '{}' in {}: {e}",
                        ff.family,
                        css_path.display()
                    ),
                }
            }
        }
    }

    let needs_assets = !cli.fonts.is_empty()
        || !cli.css_files.is_empty()
        || header_css.is_some()
        || !math_fonts.is_empty()
        || !font_face_paths.is_empty();

    let assets = if needs_assets {
        let mut bundle = AssetBundle::new();
        if let Some(ref css) = header_css {
            bundle.add_css(css);
        }
        for f in &cli.fonts {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: font {}: {e}", f.display()));
        }
        for f in &math_fonts {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: math font {}: {e}", f.display()));
        }
        for f in &cli.css_files {
            bundle
                .add_css_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: CSS {}: {e}", f.display()));
        }
        for f in &font_face_paths {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: @font-face font {}: {e}", f.display()));
        }
        Some(bundle)
    } else {
        None
    };

    // 4. Build engine
    let mut builder = Engine::builder();
    if let Some(ref s) = cli.size {
        builder = builder.page_size(parse_page_size(s));
    }
    if cli.landscape {
        builder = builder.landscape(true);
    }
    if let Some(ref m) = cli.margin {
        builder = builder.margin(parse_margin(m));
    }
    if let Some(t) = cli.title {
        builder = builder.title(t);
    }
    if !cli.authors.is_empty() {
        builder = builder.authors(cli.authors);
    }
    if let Some(l) = cli.language {
        builder = builder.lang(l);
    }
    builder = builder
        .bookmarks(cli.bookmarks)
        .tagged(cli.tagged)
        .pdf_ua(cli.pdf_ua);
    if let Some(ref bp) = base_path {
        builder = builder.base_path(bp);
    }
    if let Some(a) = assets {
        builder = builder.assets(a);
    }

    let engine = builder.build();
    let pdf = engine.render_html(&html)?;

    // 5. Route output by format. YAML config triggers multi-format.
    let to_stdout = cli.output.as_ref().map_or(false, |o| o.as_os_str() == "-");

    // Config-driven multi-format output (from YAML output section)
    if let Some(ref oc) = cfg.as_ref().and_then(|c| c.output.as_ref()) {
        if let Some(ref path) = oc.pdf {
            std::fs::write(path, &pdf)?;
            eprintln!("PDF written to {}", path.display());
        }
        if let Some(ref path) = oc.svg {
            write_svg_multi(&pdf, path)?;
        }
        if let Some(ref path) = oc.png {
            std::fs::write(path, render_png_from_pdf(&pdf, cli.scale)?)?;
            eprintln!("PNG written to {}", path.display());
        }
    }

    // CLI-driven output (--format + -o)
    if to_stdout {
        match cli.format.as_str() {
            "svg" => print!("{}", render_svg_from_pdf(&pdf)?),
            "png" => {
                use std::io::Write;
                std::io::stdout().write_all(&render_png_from_pdf(&pdf, cli.scale)?)?;
            }
            _ => {
                use std::io::Write;
                std::io::stdout().write_all(&pdf)?;
            }
        }
    } else if cfg.as_ref().and_then(|c| c.output.as_ref()).is_none() {
        if let Some(ref output) = cli.output {
            match cli.format.as_str() {
                "svg" => write_svg_multi(&pdf, output)?,
                "png" => {
                    std::fs::write(output, render_png_from_pdf(&pdf, cli.scale)?)?;
                    eprintln!("PNG written to {}", output.display());
                }
                _ => {
                    std::fs::write(output, &pdf)?;
                    eprintln!("PDF written to {}", output.display());
                }
            }
        }
    }

    Ok(())
}
