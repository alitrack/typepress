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

// ── CLI ────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "typepress", version, about = "Pure Rust HTML/CSS → PDF engine")]
struct Cli {
    /// Input HTML file (omit for --stdin)
    input: Option<PathBuf>,

    /// Read HTML from stdin
    #[arg(long)]
    stdin: bool,

    /// Output PDF file path (use "-" for stdout)
    #[arg(short, long)]
    output: PathBuf,

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
    let matches: Vec<_> = re.captures_iter(html).map(|c| {
        (c.get(0).unwrap().range(), c.get(1).unwrap().as_str().to_string())
    }).collect();

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

// ── Main ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

    let base_path = if cli.stdin {
        std::env::current_dir().ok()
    } else {
        cli.input.as_ref().and_then(|p| {
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

    let mut html = read_input(cli.input.as_ref(), cli.stdin)?;

    // 1. Inject header/footer
    let header_css = inject_header_footer(&mut html, cli.header.as_deref(), cli.footer.as_deref());

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

    // 2b. Process Mermaid diagrams
    match process_mermaid(&mut html) {
        Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
        Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
        _ => {}
    }

    // 3. Build asset bundle
    let needs_assets = !cli.fonts.is_empty()
        || !cli.css_files.is_empty()
        || header_css.is_some()
        || !math_fonts.is_empty();

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

    // 5. Write output
    let to_stdout = cli.output.as_os_str() == "-";
    if to_stdout {
        use std::io::Write;
        std::io::stdout().write_all(&pdf)?;
    } else {
        std::fs::write(&cli.output, &pdf)?;
        eprintln!("PDF written to {}", cli.output.display());
    }

    Ok(())
}
