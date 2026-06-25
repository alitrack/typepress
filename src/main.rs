// TypePress — Pure Rust HTML/CSS → PDF engine.
// Uses fulgur (Blitz→Taffy→Krilla) as the rendering backend.
//
// Key additions over vanilla fulgur:
//   - --header / --footer CLI shortcuts (CSS GCPM running elements)
//   - --math auto-detection (katex-rs rendering + KaTeX font loading)
//   - CJK font handling with automatic subsetting
use anyhow::Result;
use clap::Parser;
use fulgur::asset::AssetBundle;

use fulgur::engine::Engine;
use std::path::{Path, PathBuf};
mod config;
mod fonts;
use config::TypePressConfig;
use typepress::css::KATEX_CSS;
use typepress::{inject_header_footer, markdown_to_html};
mod cli;
mod math;

use cli::{Cli, page_size_mm, parse_margin, parse_page_size, read_input};
use math::process_math;

#[cfg(feature = "mermaid-render")]
fn detect_mermaid_system_font(prefer_cjk: bool) -> Option<(PathBuf, &'static str)> {
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
fn process_mermaid(html: &mut String) -> Result<usize> {
    use math::escape_html;
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
        let mermaid_font = detect_mermaid_system_font(source.chars().any(|c| !c.is_ascii()));
        let mut style = mermaid_render::DiagramStyle::default();
        if let Some((_, family)) = mermaid_font.as_ref() {
            style.font_family = (*family).to_string();
        }

        match render_diagram(&source, &style, &mut EstimatedMeasure) {
            Ok((svg, w, h)) => {
                let svg_w = w.max(100.0);
                let svg_h = h.max(100.0);
                let svg_doc = format!(
                    r#"<div class="txp-mermaid" style="text-align:center;margin:1em 0"><svg xmlns="http://www.w3.org/2000/svg" width="{svg_w}" height="{svg_h}" viewBox="0 0 {svg_w} {svg_h}" style="display:block;margin:0 auto">{svg}</svg></div>"#
                );
                html.replace_range(range, &svg_doc);
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

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Load config: --config <file> or auto-detect typepress.yaml
    let cfg = if let Some(ref path) = cli.config {
        TypePressConfig::from_file(path)
            .map_err(|e| eprintln!("Warning: failed to load config {}: {e}", path.display()))
            .ok()
    } else {
        TypePressConfig::auto_detect().map(|(c, _)| c)
    };

    // Merge: CLI args override YAML values.
    // Resolve page settings early (before cli partial-moves)
    let mut resolved_size = cli.resolve_size();
    let mut resolved_landscape = cli.resolve_landscape();
    let resolved_margin = cli.resolve_margin();
    let input_file = cli
        .input
        .clone()
        .or_else(|| cfg.as_ref().and_then(|c| c.input.clone()));
    let is_md = cli.from == "md"
        || input_file
            .as_ref()
            .and_then(|p| p.extension())
            .is_some_and(|e| e == "md")
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
        .is_some_and(|e| e == "pdf");
    if is_pdf_input && !cli.stdin {
        let pdf_bytes = std::fs::read(input_file.as_ref().unwrap())?;
        let to_stdout = cli.output.as_ref().is_some_and(|o| o.as_os_str() == "-");
        if to_stdout {
            use std::io::Write;
            std::io::stdout().write_all(&pdf_bytes)?;
        } else if let Some(ref output) = cli.output {
            std::fs::write(output, &pdf_bytes)?;
            eprintln!("PDF written to {}", output.display());
        }
        return Ok(());
    }

    let mut html = read_input(input_file.as_ref(), cli.stdin)?;

    // ── Math font detection (before any processing) ──
    let math_enabled = cli.math || cli.math_dir.is_some();
    let math_fonts: Vec<PathBuf> = if math_enabled {
        let target = cli.math_dir.or_else(|| {
            if cli.math {
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

    if is_md {
        // MD pipeline: Mermaid → Math → Markdown→HTML → Header/Footer → Highlight

        // 0a. Mermaid (raw markdown)
        #[cfg(feature = "mermaid-render")]
        match process_mermaid(&mut html) {
            Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
            Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
            _ => {}
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
            typepress::markdown_to_html_with_css(&html, KATEX_CSS)
        } else {
            markdown_to_html(&html)
        };

        // 1. Inject header/footer
        header_css = inject_header_footer(&mut html, header.as_deref(), footer.as_deref());

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
        match typepress::network::inject_remote_css(&mut html) {
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
        match typepress::network::download_remote_images(&mut html) {
            Ok((n, _)) if n > 0 => eprintln!("Downloaded {n} remote image(s)"),
            Ok(_) => {}
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
        match process_mermaid(&mut html) {
            Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
            Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
            _ => {}
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
    if let Some(emoji_path) = detect_emoji_font() {
        font_face_paths.push(emoji_path);
    }
    // COLRv1 emoji font: auto-download for native color emoji rendering
    // (krilla supports COLR via Type3 PDF font embedding since v0.7;
    //  CBDT bitmap fonts are NOT supported — we use the COLRv1 version)
    #[allow(clippy::collapsible_if)]
    if typepress::emoji::has_emoji(&html) {
        if let Some(colr_path) = typepress::emoji::ensure_colr_emoji_font() {
            if !font_face_paths.iter().any(|p| p == &colr_path) {
                font_face_paths.push(colr_path);
                // Inject CSS @font-face to force parley to use COLR font
                // for emoji codepoints (otherwise Unifont/CJK fonts take priority)
                let css = typepress::emoji::colr_font_face_css();
                inject_css(&mut html, css);
            }
        }
    }
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

    // ── Merge YAML config (CLI args override YAML) ──
    if cli.no_system_fonts {
        builder = builder.system_fonts(false);
    }
    if let Some(ref c) = cfg {
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
            if cli.title.is_none()
                && let Some(ref title) = mc.title
            {
                builder = builder.title(title.clone());
            }
            if cli.authors.is_empty() && !mc.author.is_empty() {
                builder = builder.authors(mc.author.clone());
            }
            if cli.language.is_none()
                && let Some(ref lang) = mc.language
            {
                builder = builder.lang(lang.clone());
            }
        }
        if let Some(ref pdf_cfg) = c.pdf {
            if !cli.bookmarks
                && let Some(bm) = pdf_cfg.bookmarks
            {
                builder = builder.bookmarks(bm);
            }
            if !cli.tagged
                && let Some(tg) = pdf_cfg.tagged
            {
                builder = builder.tagged(tg);
            }
            if !cli.pdf_ua
                && let Some(ua) = pdf_cfg.pdf_ua
            {
                builder = builder.pdf_ua(ua);
            }
        }
    }

    // ── CLI args (override YAML or set directly) ──
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
    if (cli.zoom - 1.0).abs() > f32::EPSILON {
        html = typepress::css_layout::scale_css_for_fit(&html, cli.zoom as f64);
    }
    // --no-outline: invert bookmarks default
    let bookmarks = if cli.no_outline { false } else { cli.bookmarks };
    let cli_title = cli.title.clone();
    let cli_authors = cli.authors.clone();
    let cli_language = cli.language.clone();
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
        .tagged(cli.tagged)
        .pdf_ua(cli.pdf_ua);
    if let Some(ref bp) = base_path {
        builder = builder.base_path(bp);
    }
    if let Some(a) = assets.clone() {
        builder = builder.assets(a);
    }

    let engine = builder.build();
    let mut pdf = engine.render_html(&html)?;
    let mut effective_zoom = cli.zoom as f64;

    // --autofit: try increasingly larger page sizes + orientations,
    // pick the combination that yields the highest zoom on a single page.
    if cli.autofit {
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
            let sys_fonts = !cli.no_system_fonts;
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
                let candidate_pdf = candidate_engine.render_html(&html)?;
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
                        let p = candidate_engine.render_html(&scaled)?;
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
                if !cli.no_system_fonts {
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
                    .tagged(cli.tagged)
                    .pdf_ua(cli.pdf_ua);
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
                    pdf = final_engine.render_html(&scaled_html)?;
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
    if cli.fit && !cli.autofit {
        let pages = typepress::css_layout::count_pdf_pages(&pdf);
        if pages > 1 {
            // Binary search: find max zoom ∈ [0, 1] producing exactly 1 page
            let mut lo = 0.0_f64;
            let mut hi = 1.0_f64;
            for _ in 0..12 {
                let mid = (lo + hi) / 2.0;
                let scaled = typepress::css_layout::scale_css_for_fit(&html, mid);
                let p = engine.render_html(&scaled)?;
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
                let p = engine.render_html(&scaled_html)?;
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

    // 5. Route output by format. YAML config triggers multi-format.
    let to_stdout = cli.output.as_ref().is_some_and(|o| o.as_os_str() == "-");
    let mut pdf_path_for_check: Option<PathBuf> = None;

    // Config-driven output (from YAML output section)
    if let Some(ref path) = cfg
        .as_ref()
        .and_then(|c| c.output.as_ref())
        .and_then(|oc| oc.pdf.as_ref())
    {
        std::fs::write(path, &pdf)?;
        eprintln!("PDF written to {}", path.display());
        pdf_path_for_check = Some(path.to_path_buf());
    }
    // CLI-driven output
    if to_stdout {
        use std::io::Write;
        std::io::stdout().write_all(&pdf)?;
    } else if let Some(ref output) = cli.output {
        // Check if YAML already handles PDF
        let yaml_has_pdf = cfg
            .as_ref()
            .and_then(|c| c.output.as_ref())
            .is_some_and(|oc| oc.pdf.is_some());
        if !yaml_has_pdf {
            std::fs::write(output, &pdf)?;
            eprintln!("PDF written to {}", output.display());
            pdf_path_for_check = Some(output.clone());
        }
    }

    // --check: diagnostic report
    if cli.check {
        let check_path = pdf_path_for_check.as_deref().unwrap_or_else(|| {
            // No file output (e.g. stdout or YAML-only), write to temp
            let tmp = std::env::temp_dir().join("typepress_check.pdf");
            std::fs::write(&tmp, &pdf).ok();
            // Leak the PathBuf to get &Path lifetime — tiny, one-shot allocation
            Box::leak(Box::new(tmp)).as_path()
        });
        match fulgur::inspect::inspect(check_path) {
            Ok(report) => {
                let size_mm = page_size_mm(resolved_size.as_deref().unwrap_or("A4"))
                    .unwrap_or((210.0, 297.0));
                let (pw, ph) = if resolved_landscape {
                    (size_mm.1, size_mm.0)
                } else {
                    size_mm
                };
                let zoom_pct = effective_zoom * 100.0;
                let pages = report.pages;

                println!();
                println!("╔══════════════════════════════════════╗");
                println!("║  TypePress Diagnostic Report         ║");
                println!("╠══════════════════════════════════════╣");
                println!(
                    "║  Page size:  {:>4.0}×{:<4.0} mm ({})",
                    pw,
                    ph,
                    if resolved_landscape {
                        "landscape"
                    } else {
                        "portrait"
                    }
                );
                println!("║  Pages:      {:<4}                   ", pages);
                println!("║  Zoom:       {:<5.1}%                 ", zoom_pct);
                println!(
                    "║  Text items: {:<4}                   ",
                    report.text_items.len()
                );
                println!(
                    "║  Images:     {:<4}                   ",
                    report.images.len()
                );
                if let Some(ref t) = report.metadata.title {
                    println!("║  Title:      {}", t);
                }
                println!("╠══════════════════════════════════════╣");

                let mut warnings: Vec<&str> = Vec::new();
                if pages > 1 && !cli.fit {
                    warnings.push("multi-page output (try --fit or --page-size A3)");
                }
                if zoom_pct > 0.1 && zoom_pct < 60.0 {
                    warnings.push("zoom < 60% — text may be hard to read");
                }
                if report.text_items.is_empty() && report.images.is_empty() {
                    warnings.push("no text or images — possible rendering failure");
                }
                if pages > 1 && cli.fit {
                    warnings.push("--fit could not reduce to 1 page (try larger page)");
                }

                if warnings.is_empty() {
                    println!("║  ✅  No issues detected              ║");
                } else {
                    for w in &warnings {
                        // Truncate long warnings to fit the box width (36 chars minus prefix)
                        let prefix = "║  ⚠  ";
                        let max_len = 38 - prefix.len();
                        if w.len() > max_len {
                            println!("{} {}…", prefix, &w[..max_len - 1]);
                        } else {
                            println!("{}{}", prefix, w);
                        }
                    }
                }
                println!("╚══════════════════════════════════════╝");
            }
            Err(e) => {
                eprintln!("Check: failed to inspect PDF: {e}");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod preprocess_tests {
    use crate::math::process_math;
    use crate::math::render_math_markup;

    #[test]
    fn render_math_markup_preserves_structured_layout() {
        let inline = render_math_markup(r"E = mc^2", false).unwrap();
        assert!(inline.contains("mc²"));

        let inline_sub = render_math_markup(r"x_1 + x_2", false).unwrap();
        assert!(inline_sub.contains("x₁"));
        assert!(inline_sub.contains("x₂"));

        let limits = render_math_markup(r"\int_0^\infty x_i^2 dx", false).unwrap();
        assert!(limits.contains("txp-op-limits"));
        assert!(limits.contains("txp-op-over"));
        assert!(limits.contains("txp-op-under"));
        assert!(limits.contains("xᵢ²"));

        let fraction = render_math_markup(r"\frac{1}{2}", true).unwrap();
        assert!(fraction.contains("txp-frac"));
        assert!(fraction.contains("txp-frac-num"));
        assert!(fraction.contains("txp-frac-den"));

        let radical = render_math_markup(r"\sqrt{2}", false).unwrap();
        assert!(radical.contains("txp-sqrt"));
        assert!(radical.contains("txp-sqrt-glyph"));
    }

    #[test]
    fn process_math_keeps_going_after_invalid_expression() {
        let mut markdown = String::from("Good $E = mc^2$ bad $$\\badcommand$$ still $x_1$.");
        let rendered = process_math(&mut markdown).unwrap();

        assert_eq!(
            rendered, 2,
            "only valid expressions should count as rendered"
        );
        assert!(markdown.contains("mc²"));
        assert!(markdown.contains("txp-math-error"));
        assert!(markdown.contains("x₁"));
    }

    #[test]
    #[cfg(feature = "mermaid-render")]
    fn process_mermaid_generates_inline_svg() {
        let mut markdown = String::from("```mermaid\ngraph TD\n  A --> B\n```");
        let rendered = process_mermaid(&mut markdown).unwrap();

        assert_eq!(rendered, 1);
        assert!(markdown.contains("<svg"));
        assert!(markdown.contains("viewBox="));
        assert!(markdown.contains("A"));
        assert!(!markdown.contains("mermaid-placeholder"));
    }
}
