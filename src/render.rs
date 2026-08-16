// TypePress render pipeline — shared by CLI (main.rs) and HTTP server (server.rs).
//
// Extracted 2026-08-16 from main.rs (P2 HTTP server prep): the full preprocessing
// chain (Markdown/HTML dual path, math, mermaid, highlight, fonts/emoji, asset
// bundle, image constraint, engine build, autofit/fit) lives here as
// `render_document(RenderOptions) -> RenderOutput`. main.rs keeps CLI parsing,
// config merging and output routing; server.rs builds RenderOptions from JSON.
use anyhow::Result;
use fulgur::asset::AssetBundle;
use fulgur::engine::Engine;
use std::path::{Path, PathBuf};

use crate::cli::{page_size_mm, parse_margin, parse_page_size};
use crate::config::TypePressConfig;
use crate::math::process_math;
use typepress::css::KATEX_CSS;
use typepress::diagnostics::Diagnostics;
use typepress::fonts;
use typepress::network::AssetLimits;
use typepress::{inject_header_footer, markdown_to_html, markdown_to_html_with_css};

/// Everything the render pipeline needs, decoupled from Cli so the HTTP server
/// can construct it from JSON without touching clap.
pub struct RenderOptions {
    pub content: String,
    pub from: String, // "html" | "md"
    pub size: Option<String>,
    pub landscape: bool,
    pub margin: Option<fulgur::config::Margin>,
    pub zoom: f32,
    pub fit: bool,
    pub autofit: bool,
    pub header: Option<String>,
    pub footer: Option<String>,
    pub fonts: Vec<PathBuf>,
    pub css_files: Vec<PathBuf>,
    pub images: Vec<(String, PathBuf)>,
    pub math: bool,
    pub math_dir: Option<PathBuf>,
    pub bookmarks: bool,
    pub no_outline: bool,
    pub tagged: bool,
    pub pdf_ua: bool,
    pub no_system_fonts: bool,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub base_path: Option<PathBuf>,
    pub asset_limits: AssetLimits,
    /// YAML config (typepress.yaml) — merged below CLI level, overridden by
    /// explicit options. `None` skips config merging entirely.
    pub config: Option<TypePressConfig>,
}

pub struct RenderOutput {
    pub pdf: Vec<u8>,
    pub pages: usize,
    pub img_constrained: usize,
    pub effective_zoom: f64,
    pub resolved_size: Option<String>,
    pub resolved_landscape: bool,
    pub diagnostics: Diagnostics,
}

// ── mermaid helpers (feature-gated; identical to the old main.rs versions) ──

#[cfg(feature = "mermaid-render")]
pub(crate) fn detect_mermaid_system_font(prefer_cjk: bool) -> Option<(PathBuf, &'static str)> {
    let cjk_candidates: &[(&str, &str)] = &[
        (
            "WenQuanYi Zen Hei",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        ),
        (
            "WenQuanYi Micro Hei",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ),
        ("Microsoft YaHei", "/mnt/c/Windows/Fonts/msyh.ttc"),
        ("SimSun", "/mnt/c/Windows/Fonts/simsun.ttc"),
    ];
    let latin_candidates: &[(&str, &str)] = &[
        (
            "DejaVu Sans",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ),
        (
            "Liberation Sans",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ),
        (
            "WenQuanYi Zen Hei",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        ),
    ];

    let chains: [&[(&str, &str)]; 2] = if prefer_cjk {
        [cjk_candidates, latin_candidates]
    } else {
        [latin_candidates, cjk_candidates]
    };

    for chain in chains {
        for (family, path) in chain {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some((p, *family));
            }
        }
    }

    None
}

#[cfg(feature = "mermaid-render")]
pub(crate) fn process_mermaid(
    html: &mut String,
    images: &mut Vec<(String, Vec<u8>)>,
) -> Result<usize> {
    use crate::math::escape_html;
    use mermaid_render::{EstimatedMeasure, render_diagram};
    use regex::Regex;

    let re = Regex::new(r"(?s)```mermaid\r?\n(.*?)```")?;
    let mut count = 0usize;

    let matches: Vec<_> = re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).unwrap().range(),
                c.get(1).unwrap().as_str().to_string(),
            )
        })
        .collect();

    for (range, source) in matches.into_iter().rev() {
        let mermaid_font = detect_mermaid_system_font(!source.is_ascii());
        // Color themes: (node_fill, node_stroke, node_text, edge_stroke, edge_text)
        const THEMES: &[(&str, &str, &str, &str, &str)] = &[
            ("#eff6ff", "#3b82f6", "#1e293b", "#64748b", "#475569"), // Blue
            ("#ecfdf5", "#10b981", "#064e3b", "#64748b", "#475569"), // Emerald
            ("#fffbeb", "#f59e0b", "#78350f", "#64748b", "#475569"), // Amber
            ("#fff1f2", "#f43f5e", "#881337", "#64748b", "#475569"), // Rose
            ("#f5f3ff", "#8b5cf6", "#3b0764", "#64748b", "#475569"), // Violet
            ("#ecfeff", "#06b6d4", "#164e63", "#64748b", "#475569"), // Cyan
            ("#fff7ed", "#f97316", "#7c2d12", "#64748b", "#475569"), // Orange
            ("#f0fdfa", "#14b8a6", "#134e4a", "#64748b", "#475569"), // Teal
        ];
        let theme = THEMES[count % THEMES.len()];
        let mut style = mermaid_render::DiagramStyle {
            node_fill: theme.0.into(),
            node_stroke: theme.1.into(),
            node_text: theme.2.into(),
            edge_stroke: theme.3.into(),
            edge_text: theme.4.into(),
            background: "transparent".into(),
            font_family: "sans-serif".into(),
            font_size: 13.0,
        };
        if let Some((_, family)) = mermaid_font.as_ref() {
            style.font_family = (*family).to_string();
        }

        match render_diagram(&source, &style, &mut EstimatedMeasure) {
            Ok((svg, w, h)) => {
                let svg_w = w.max(100.0);
                let svg_h = h.max(100.0);
                match svg_to_png_bytes(&svg, svg_w, svg_h, count) {
                    Ok((name, data)) => {
                        let png_tag = format!(
                            r#"<img src="{name}" width="{svg_w:.0}" height="{svg_h:.0}" style="display:block;margin:1em auto;width:{svg_w:.0}px;height:{svg_h:.0}px" alt="mermaid diagram" />"#
                        );
                        html.replace_range(range, &png_tag);
                        images.push((name, data));
                    }
                    Err(e) => {
                        eprintln!("Warning: mermaid rasterize failed: {e}");
                        let svg_fallback = format!(
                            r#"<div class="txp-mermaid" style="text-align:center;margin:1em 0;width:{svg_w:.0}px;height:{svg_h:.0}px"><svg xmlns="http://www.w3.org/2000/svg" width="{svg_w:.0}" height="{svg_h:.0}" viewBox="0 0 {w:.0} {h:.0}" style="display:block;margin:0 auto">{svg}</svg></div>"#
                        );
                        html.replace_range(range, &svg_fallback);
                    }
                }
                count += 1;
            }
            Err(e) => {
                eprintln!("Warning: mermaid render failed: {e}");
                let fallback = format!(
                    r#"<div class="mermaid-placeholder" style="border:2px dashed #ccc;padding:2em;text-align:center;margin:1em 0;color:#888;font-style:italic">Mermaid render failed: {}</div>"#,
                    escape_html(source.trim())
                );
                html.replace_range(range, &fallback);
            }
        }
    }

    Ok(count)
}

/// Rasterize SVG fragment to PNG bytes for AssetBundle registration.
#[cfg(feature = "mermaid-render")]
fn svg_to_png_bytes(svg_fragment: &str, w: f32, h: f32, index: usize) -> Result<(String, Vec<u8>)> {
    use resvg::usvg;
    use tiny_skia::Pixmap;

    let svg_doc = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">{svg_fragment}</svg>"#
    );
    let mut opts = usvg::Options::default();
    // Load system fonts so resvg can render text in the SVG
    // fontdb is behind Arc, use make_mut to get mutable access
    let fontdb = std::sync::Arc::make_mut(&mut opts.fontdb);
    fontdb.load_system_fonts();
    for dir in &["/usr/share/fonts", "/usr/local/share/fonts"] {
        fontdb.load_fonts_dir(dir);
    }
    let tree = usvg::Tree::from_str(&svg_doc, &opts)?;
    let scale = 2.0;
    let pixmap_w = (w * scale).ceil() as u32;
    let pixmap_h = (h * scale).ceil() as u32;
    let mut pixmap = Pixmap::new(pixmap_w, pixmap_h)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap {pixmap_w}x{pixmap_h}"))?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let name = format!("txp-mermaid-{index}.png");
    Ok((name, pixmap.encode_png()?))
}

// ── math helpers (shared by both pipelines) ──

/// Detect a math-capable system font for KaTeX rendering.
fn detect_math_system_font() -> Option<(PathBuf, String)> {
    // Priority-ordered list of math-capable fonts available on most Linux systems
    let candidates: &[(&str, &str)] = &[
        (
            "DejaVu Serif",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        ),
        (
            "DejaVu Sans",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ),
        (
            "Liberation Serif",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
        ),
        (
            "FreeSerif",
            "/usr/share/fonts/truetype/freefont/FreeSerif.ttf",
        ),
    ];
    for (family, path) in candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some((p, family.to_string()));
        }
    }
    None
}

fn math_font_face_css(font_path: &Path) -> String {
    format!(
        r#"@font-face {{ font-family: 'TypePressMath'; src: url('{}'); }}"#,
        font_path.display()
    )
}

fn detect_emoji_font() -> Option<PathBuf> {
    for path in &[
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "C:\\Windows\\Fonts\\seguiemj.ttf",
    ] {
        let p = std::path::Path::new(path);
        if p.exists() {
            // Skip fonts that exceed the asset size limit
            if let Ok(meta) = std::fs::metadata(p)
                && meta.len() >= 64 * 1024 * 1024
            {
                continue;
            }
            eprintln!("Emoji font: {}", p.display());
            return Some(p.to_path_buf());
        }
    }
    None
}

fn auto_detect_katex_fonts() -> Option<PathBuf> {
    // 1. Common npm global locations (no subprocess, pure path check)
    for npm_root in katex_npm_roots() {
        let direct = npm_root.join("katex/dist/fonts");
        if direct.is_dir() {
            return Some(direct);
        }
        if let Some(found) = find_katex_fonts_in(&npm_root, 0, 3) {
            return Some(found);
        }
    }

    // 2. System paths (Linux, macOS Homebrew)
    for p in &[
        "/usr/share/katex/fonts",
        "/usr/local/share/katex/fonts",
        "/opt/homebrew/share/katex/fonts",
    ] {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

fn katex_npm_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    // npm prefix (env var or standard locations)
    if let Ok(prefix) = std::env::var("npm_config_prefix") {
        roots.push(PathBuf::from(&prefix).join("lib/node_modules"));
    }
    // Unix: $HOME/.npm-global or /usr/local
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(&home).join(".npm-global/lib/node_modules"));
        roots.push(PathBuf::from(&home).join("node_modules"));
    }
    #[cfg(target_os = "linux")]
    roots.push(PathBuf::from("/usr/local/lib/node_modules"));
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/opt/homebrew/lib/node_modules"));
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            roots.push(PathBuf::from(&appdata).join("npm/node_modules"));
        }
    }
    roots
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
            if entry.path().is_dir()
                && let Some(found) = find_katex_fonts_in(&entry.path(), depth + 1, max)
            {
                return Some(found);
            }
        }
    }
    None
}

fn inject_css(html: &mut String, css: &str) {
    let tag = format!("<style>{css}</style>");
    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, &tag);
    } else if let Some(pos) = html.find("<body") {
        html.insert_str(pos, &format!("{tag}\n"));
    }
}

/// Run the full preprocessing + rendering pipeline.
///
/// Mirrors the old main() flow: MD/HTML dual path, math/mermaid/highlight,
/// fonts + emoji, asset bundle, image constraint, engine build, autofit/fit.
/// All recoverable failures land in `diagnostics` (never silent).
pub fn render_document(opts: RenderOptions) -> Result<RenderOutput> {
    let mut diag = Diagnostics::new();
    let mut resolved_size = opts.size.clone();
    let mut resolved_landscape = opts.landscape;
    let resolved_margin = opts.margin;
    let cfg = opts.config.as_ref();
    let is_md = opts.from == "md";

    let mut html = opts.content;
    let header = opts.header;
    let footer = opts.footer;
    let base_path = opts.base_path;

    // ── Math font detection (before any processing) ──
    let math_enabled = opts.math || opts.math_dir.is_some();
    let math_fonts: Vec<PathBuf> = if math_enabled {
        let target = opts.math_dir.or_else(|| {
            if opts.math {
                auto_detect_katex_fonts()
            } else {
                None
            }
        });
        if let Some(ref dir) = target {
            let fonts = fonts::scan_font_dir(dir);
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

    let header_css;

    #[cfg(feature = "mermaid-render")]
    let mut mermaid_images: Vec<(String, Vec<u8>)>;

    let mut remote_images: Vec<(String, Vec<u8>)> = Vec::new();

    if is_md {
        // MD pipeline: Mermaid → Math → Markdown→HTML → Header/Footer → Highlight

        // 0a. Mermaid (raw markdown)
        #[cfg(feature = "mermaid-render")]
        {
            let mut mermaid_vec = Vec::new();
            match process_mermaid(&mut html, &mut mermaid_vec) {
                Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
                Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
                _ => {}
            }
            mermaid_images = mermaid_vec;
        }

        // 0b. Math (raw markdown — pre-empts pulldown-cmark's ENABLE_MATH)
        let math_count = if math_enabled {
            match process_math(&mut html) {
                Ok(n) => {
                    if n > 0 {
                        eprintln!("Rendered {n} math expression(s)");
                    }
                    n
                }
                Err(e) => {
                    eprintln!("Warning: math processing failed: {e}");
                    0
                }
            }
        } else {
            0
        };

        // 0c. Convert markdown to HTML
        html = if math_count > 0 {
            markdown_to_html_with_css(&html, KATEX_CSS)
        } else {
            markdown_to_html(&html)
        };

        // 1. Inject header/footer
        header_css = inject_header_footer(&mut html, header.as_deref(), footer.as_deref());

        // 0d. Download remote images (after markdown→HTML so <img> tags exist)
        match typepress::network::download_remote_images(&mut html, &mut diag, &opts.asset_limits) {
            Ok(imgs) => {
                if !imgs.is_empty() {
                    eprintln!("Downloaded {} remote image(s)", imgs.len());
                    remote_images = imgs;
                }
            }
            Err(e) => eprintln!("Warning: remote images: {e}"),
        }

        // Inject @font-face for math system font (maps 'TypePressMath' to a real font file)
        // This must happen BEFORE extract_font_faces_from_html() so the @font-face rule
        // is picked up and the font file is added to the AssetBundle.
        if math_count > 0 {
            if let Some((math_font_path, family)) = detect_math_system_font() {
                let ff_css = math_font_face_css(&math_font_path);
                inject_css(&mut html, &ff_css);
                eprintln!("Math font: using {family} ({})", math_font_path.display());
            } else {
                eprintln!(
                    "Warning: no math-capable system font found. \
                     Math symbols (∫, ∇, ±, ∂, ∞) may render as empty boxes. \
                     Install DejaVu or Liberation fonts."
                );
            }
        }
    } else {
        // HTML pipeline: CSS Layout → Header/Footer → Math → Mermaid → Highlight

        // 0a. CSS Layout: using native blitz-html 0.3 (flex/grid → taffy natively)
        // (old Grid→Table preprocessing removed — no longer needed)

        // 0b. Network resources: download remote CSS <link> + <img>
        match typepress::network::inject_remote_css(&mut html, &mut diag, &opts.asset_limits) {
            Ok(n) if n > 0 => eprintln!("Downloaded {n} remote CSS file(s)"),
            Ok(_) => {}
            Err(e) => eprintln!("Warning: remote CSS: {e}"),
        }
        if let Some(ref bp) = base_path {
            match typepress::network::inject_local_css(&mut html, bp) {
                Ok(n) if n > 0 => eprintln!("Embedded {n} local CSS file(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("Warning: local CSS: {e}"),
            }
        }
        match typepress::network::download_remote_images(&mut html, &mut diag, &opts.asset_limits) {
            Ok(imgs) => {
                if !imgs.is_empty() {
                    eprintln!("Downloaded {} remote image(s)", imgs.len());
                    remote_images = imgs;
                }
            }
            Err(e) => eprintln!("Warning: remote images: {e}"),
        }

        // 1. Inject header/footer
        header_css = inject_header_footer(&mut html, header.as_deref(), footer.as_deref());

        // 2. Math
        if math_enabled {
            match process_math(&mut html) {
                Ok(n) if n > 0 => eprintln!("Rendered {n} math expression(s)"),
                Err(e) => eprintln!("Warning: math processing failed: {e}"),
                _ => {}
            }
        }

        // 3. Mermaid
        #[cfg(feature = "mermaid-render")]
        {
            let mut mermaid_vec = Vec::new();
            match process_mermaid(&mut html, &mut mermaid_vec) {
                Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
                Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
                _ => {}
            }
            mermaid_images = mermaid_vec;
        }
    }

    // 4. Apply code syntax highlighting (syntect)
    match typepress::highlight::highlight_code_blocks(&mut html) {
        Ok(n) if n > 0 => eprintln!("Highlighted {n} code block(s)"),
        Err(e) => eprintln!("Warning: code highlighting failed: {e}"),
        _ => {}
    }

    // 3. Build asset bundle — start with @font-face font resolution
    // Parse @font-face from inline styles in the HTML

    let mut font_face_paths: Vec<PathBuf> = Vec::new();
    // Emoji font fallback: register system emoji font for glyphs missing
    // from Noto Serif CJK (👦👧👩🛠 etc). Note: Krilla does not support
    // color bitmap fonts, so color emoji glyphs render as monochrome outlines.
    // Skip fonts exceeding the asset size limit (e.g. Apple Color Emoji ~192MB).
    if let Some(emoji_path) = detect_emoji_font() {
        font_face_paths.push(emoji_path);
    }
    // COLRv1 emoji font: auto-download for native color emoji rendering
    // (krilla supports COLR via Type3 PDF font embedding since v0.7;
    //  CBDT bitmap fonts are NOT supported — we use the COLRv1 version)
    #[allow(clippy::collapsible_if)]
    if typepress::emoji::has_emoji(&html) {
        if let Some(colr_path) = typepress::emoji::ensure_colr_emoji_font(&opts.asset_limits) {
            if !font_face_paths.iter().any(|p| p == &colr_path) {
                font_face_paths.push(colr_path);
            }
        } else {
            diag.push(
                "TP-1005",
                "failed to download COLRv1 emoji font — emoji may render as boxes",
            );
        }
    }
    for ff in fonts::extract_font_faces_from_html(&html) {
        match fonts::resolve_font_path(&ff.src_url, base_path.as_deref(), &opts.asset_limits) {
            Ok(path) => font_face_paths.push(path),
            Err(e) => diag.push("TP-1005", format!("@font-face '{}': {e}", ff.family)),
        }
    }

    // Parse @font-face from external CSS files
    for css_path in &opts.css_files {
        if let Ok(css_content) = std::fs::read_to_string(css_path) {
            for ff in fonts::parse_font_faces(&css_content) {
                let css_dir = css_path.parent();
                match fonts::resolve_font_path(
                    &ff.src_url,
                    css_dir.or(base_path.as_deref()),
                    &opts.asset_limits,
                ) {
                    Ok(path) => font_face_paths.push(path),
                    Err(e) => diag.push(
                        "TP-1005",
                        format!("@font-face '{}' in {}: {e}", ff.family, css_path.display()),
                    ),
                }
            }
        }
    }

    let needs_assets = !opts.fonts.is_empty()
        || !opts.css_files.is_empty()
        || header_css.is_some()
        || !math_fonts.is_empty()
        || !font_face_paths.is_empty()
        || !opts.images.is_empty()
        || !remote_images.is_empty()
        || {
            #[cfg(feature = "mermaid-render")]
            {
                !mermaid_images.is_empty()
            }
            #[cfg(not(feature = "mermaid-render"))]
            {
                false
            }
        };

    let assets = if needs_assets {
        let mut bundle = AssetBundle::new();
        if let Some(ref css) = header_css {
            bundle.add_css(css);
        }
        for f in &opts.fonts {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: font {}: {e}", f.display()));
        }
        for f in &math_fonts {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: math font {}: {e}", f.display()));
        }
        for f in &opts.css_files {
            bundle
                .add_css_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: CSS {}: {e}", f.display()));
        }
        for f in &font_face_paths {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: @font-face font {}: {e}", f.display()));
        }
        #[cfg(feature = "mermaid-render")]
        {
            for (name, data) in mermaid_images.drain(..) {
                bundle.add_image(name, data);
            }
        }
        // Remote <img src="https://..."> downloads
        for (name, data) in remote_images.drain(..) {
            bundle.add_image(name, data);
        }
        // CLI -i / --image flag images
        for (name, path) in &opts.images {
            if let Ok(data) = std::fs::read(path) {
                bundle.add_image(name, data);
            } else {
                eprintln!(
                    "Warning: cannot read image {} ({}): file not found",
                    name,
                    path.display()
                );
            }
        }
        Some(bundle)
    } else {
        None
    };

    // ── Image width constraint: scale images that exceed page content width ──
    let margin_mm: f64 = resolved_margin
        .map(|m| (m.left as f64 + m.right as f64) / 72.0 * 25.4)
        .unwrap_or(20.0); // default ~10mm each side
    let size_mm = page_size_mm(resolved_size.as_deref().unwrap_or("A4")).unwrap_or((210.0, 297.0));
    let page_w_mm = if resolved_landscape {
        size_mm.1
    } else {
        size_mm.0
    };
    let content_w_mm = page_w_mm - margin_mm;
    let content_w_pt = content_w_mm * 72.0 / 25.4;

    let (constrained_html, img_constrained, img_warnings) =
        typepress::css_layout::constrain_images_to_page(&html, content_w_pt, &mut diag);
    let mut html = if img_constrained > 0 {
        eprintln!(
            "Constrained {img_constrained} image(s) to page width ({content_w_mm:.0}mm / {content_w_pt:.0}pt)"
        );
        for w in &img_warnings {
            eprintln!("  {w}");
        }
        constrained_html
    } else {
        html
    };

    // 4. Build engine
    let mut builder = Engine::builder();

    // ── Merge YAML config (explicit options override YAML) ──
    if opts.no_system_fonts {
        builder = builder.system_fonts(false);
    }
    if let Some(c) = cfg {
        if let Some(ref pc) = c.page {
            if resolved_size.is_none()
                && let Some(ref size) = pc.size
            {
                builder = builder.page_size(parse_page_size(size));
            }
            if !resolved_landscape && let Some(ls) = pc.landscape {
                builder = builder.landscape(ls);
            }
            if resolved_margin.is_none()
                && let Some(ref margin) = pc.margin
            {
                builder = builder.margin(parse_margin(margin));
            }
        }
        if let Some(ref mc) = c.metadata {
            if opts.title.is_none()
                && let Some(ref title) = mc.title
            {
                builder = builder.title(title.clone());
            }
            if opts.authors.is_empty() && !mc.author.is_empty() {
                builder = builder.authors(mc.author.clone());
            }
            if opts.language.is_none()
                && let Some(ref lang) = mc.language
            {
                builder = builder.lang(lang.clone());
            }
        }
        if let Some(ref pdf_cfg) = c.pdf {
            if !opts.bookmarks
                && let Some(bm) = pdf_cfg.bookmarks
            {
                builder = builder.bookmarks(bm);
            }
            if !opts.tagged
                && let Some(tg) = pdf_cfg.tagged
            {
                builder = builder.tagged(tg);
            }
            if !opts.pdf_ua
                && let Some(ua) = pdf_cfg.pdf_ua
            {
                builder = builder.pdf_ua(ua);
            }
        }
    }

    // ── Explicit options (override YAML) ──
    if let Some(ref s) = resolved_size {
        // Custom page-size: "W H" or standard "A4"
        builder = builder.page_size(parse_page_size(s));
    }
    let landscape = resolved_landscape;
    if landscape {
        builder = builder.landscape(true);
    }
    if let Some(m) = resolved_margin {
        builder = builder.margin(m);
    }
    // --zoom: scale all CSS px values so layout engine sees reduced content height.
    // Uses the same approach as --fit (CSS px scaling) instead of CSS transform,
    // because transform: scale() is purely visual — Taffy doesn't see it for pagination.
    if (opts.zoom - 1.0).abs() > f32::EPSILON {
        html = typepress::css_layout::scale_css_for_fit(&html, opts.zoom as f64);
    }
    // --no-outline: invert bookmarks default
    let bookmarks = if opts.no_outline {
        false
    } else {
        opts.bookmarks
    };
    let cli_title = opts.title.clone();
    let cli_authors = opts.authors.clone();
    let cli_language = opts.language.clone();
    if let Some(t) = cli_title.clone() {
        builder = builder.title(t);
    }
    if !cli_authors.is_empty() {
        builder = builder.authors(cli_authors.clone());
    }
    if let Some(l) = cli_language.clone() {
        builder = builder.lang(l);
    }
    builder = builder
        .bookmarks(bookmarks)
        .tagged(opts.tagged)
        .pdf_ua(opts.pdf_ua);
    if let Some(ref bp) = base_path {
        builder = builder.base_path(bp);
    }
    if let Some(a) = assets.clone() {
        builder = builder.assets(a);
    }

    let engine = builder.build();
    let mut pdf = engine.render(&html)?;
    let mut effective_zoom = opts.zoom as f64;

    // --autofit: try increasingly larger page sizes + orientations,
    // pick the combination that yields the highest zoom on a single page.
    if opts.autofit {
        let pages = typepress::css_layout::count_pdf_pages(&pdf);
        if pages > 1 {
            let base_size = resolved_size.as_deref().unwrap_or("A4");
            let mut candidates: Vec<(&str, bool)> = vec![(base_size, false), (base_size, true)];
            // Try one step larger if still not fitting
            if base_size == "A4" {
                candidates.push(("A3", false));
                candidates.push(("A3", true));
            } else if base_size == "A3" {
                candidates.push(("A2", true));
            }
            let margin = resolved_margin;
            let sys_fonts = !opts.no_system_fonts;
            let bp = base_path.clone();
            let ast = assets.clone();
            let mut best: Option<(String, f64, bool, Vec<u8>)> = None;

            for &(size_name, ls) in &candidates {
                let mut eb = Engine::builder();
                if !sys_fonts {
                    eb = eb.system_fonts(false);
                }
                eb = eb.page_size(parse_page_size(size_name));
                if ls {
                    eb = eb.landscape(true);
                }
                if let Some(m) = margin {
                    eb = eb.margin(m);
                }
                if let Some(ref bp) = bp {
                    eb = eb.base_path(bp.clone());
                }
                if let Some(ref a) = ast {
                    eb = eb.assets(a.clone());
                }
                let candidate_engine = eb.build();
                let candidate_pdf = candidate_engine.render(&html)?;
                let candidate_pages = typepress::css_layout::count_pdf_pages(&candidate_pdf);

                let zoom = if candidate_pages <= 1 {
                    1.0
                } else {
                    // Binary search fit
                    let mut lo = 0.0_f64;
                    let mut hi = 1.0_f64;
                    for _ in 0..12 {
                        let mid = (lo + hi) / 2.0;
                        let scaled = typepress::css_layout::scale_css_for_fit(&html, mid);
                        let p = candidate_engine.render(&scaled)?;
                        if typepress::css_layout::count_pdf_pages(&p) <= 1 {
                            lo = mid;
                        } else {
                            hi = mid;
                        }
                    }
                    lo * 0.995
                };

                let is_better = match &best {
                    None => true,
                    Some((_, best_zoom, _, _)) => zoom > *best_zoom,
                };
                if is_better {
                    best = Some((
                        size_name.to_string(),
                        zoom,
                        ls,
                        if zoom >= 0.999 {
                            candidate_pdf
                        } else {
                            Vec::new()
                        },
                    ));
                }
            }

            if let Some((ref size_name, zoom, ls, ref cached_pdf)) = best {
                // Rebuild final engine with winning config
                let mut eb = Engine::builder();
                if !opts.no_system_fonts {
                    eb = eb.system_fonts(true);
                } else {
                    eb = eb.system_fonts(false);
                }
                eb = eb.page_size(parse_page_size(size_name));
                if ls {
                    eb = eb.landscape(true);
                }
                if let Some(m) = resolved_margin {
                    eb = eb.margin(m);
                }
                // Re-apply full metadata config
                if let Some(ref t) = cli_title {
                    eb = eb.title(t.clone());
                }
                if !cli_authors.is_empty() {
                    eb = eb.authors(cli_authors.clone());
                }
                if let Some(ref l) = cli_language {
                    eb = eb.lang(l.clone());
                }
                eb = eb
                    .bookmarks(bookmarks)
                    .tagged(opts.tagged)
                    .pdf_ua(opts.pdf_ua);
                if let Some(ref bp) = base_path {
                    eb = eb.base_path(bp.clone());
                }
                if let Some(a) = assets.clone() {
                    eb = eb.assets(a);
                }
                let final_engine = eb.build();

                if zoom >= 0.999 {
                    pdf = cached_pdf.clone();
                } else {
                    // Apply zoom scaling and re-render
                    let scaled_html = typepress::css_layout::scale_css_for_fit(&html, zoom);
                    pdf = final_engine.render(&scaled_html)?;
                }

                eprintln!(
                    "Autofit: {} {} → 1 page at {:.1}% zoom",
                    size_name,
                    if ls { "landscape" } else { "portrait" },
                    zoom * 100.0
                );
                effective_zoom = zoom;
                resolved_size = Some(size_name.clone());
                resolved_landscape = ls;
            }
        }
    }

    // --fit: if multi-page, scale CSS uniformly and re-render to one page
    // Uses binary search to find maximum zoom that still fits on one page,
    // instead of the naive 0.95/pages formula that wastes whitespace.
    if opts.fit && !opts.autofit {
        let pages = typepress::css_layout::count_pdf_pages(&pdf);
        if pages > 1 {
            // Binary search: find max zoom ∈ [0, 1] producing exactly 1 page
            let mut lo = 0.0_f64;
            let mut hi = 1.0_f64;
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                let scaled = typepress::css_layout::scale_css_for_fit(&html, mid);
                let p = engine.render(&scaled)?;
                if typepress::css_layout::count_pdf_pages(&p) <= 1 {
                    lo = mid; // fits → try larger
                } else {
                    hi = mid; // too much → shrink
                }
            }
            // Width cap: guard against horizontal overflow that fulgur silently clips.
            // count_pdf_pages can only detect vertical overflow (page breaks), not
            // horizontal overflow — content extending beyond page width is clipped.
            if let Some(html_max_w) = typepress::css_layout::max_explicit_width_px(&html) {
                let page_dim = page_size_mm(resolved_size.as_deref().unwrap_or("A4"))
                    .unwrap_or((210.0, 297.0));
                let (pw, _ph) = if resolved_landscape {
                    (page_dim.1, page_dim.0)
                } else {
                    page_dim
                };
                let margin_mm = resolved_margin
                    .map(|m| (m.left as f64) / 72.0 * 25.4) // pt → mm
                    .unwrap_or(20.0);
                let content_px = (pw - 2.0 * margin_mm) * 96.0 / 25.4;
                let safe_scale = content_px / html_max_w;
                if lo > safe_scale {
                    eprintln!(
                        "  Width cap: {:.1}% → {:.1}% ({}px max-width > {}px page content)",
                        lo * 100.0,
                        safe_scale * 100.0,
                        html_max_w,
                        content_px as u32,
                    );
                    lo = safe_scale;
                }
            }
            // 0.5% initial safety margin; post-validate loop below may shrink further
            let mut scale = lo * 0.995;
            for retry in 0..8 {
                let scaled_html = typepress::css_layout::scale_css_for_fit(&html, scale);
                let p = engine.render(&scaled_html)?;
                if typepress::css_layout::count_pdf_pages(&p) <= 1 {
                    pdf = p;
                    break;
                }
                scale *= 0.97; // shrink 3% per retry to avoid overflow
                if retry == 0 {
                    eprintln!(
                        "  Content overflow at {:.1}%, retrying at {:.1}%…",
                        lo * 0.995 * 100.0,
                        scale * 100.0,
                    );
                }
            }
            eprintln!(
                "Fitting {pages} pages → 1 page (max zoom {:.1}%)",
                scale * 100.0
            );
            effective_zoom = scale;
            let new_pages = typepress::css_layout::count_pdf_pages(&pdf);
            eprintln!(" → {new_pages} page(s) after fitting");
        }
    }

    let pages = typepress::css_layout::count_pdf_pages(&pdf);

    Ok(RenderOutput {
        pdf,
        pages,
        img_constrained,
        effective_zoom,
        resolved_size,
        resolved_landscape,
        diagnostics: diag,
    })
}
