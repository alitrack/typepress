// TypePress — Pure Rust HTML/CSS → PDF engine.
// Uses fulgur (Blitz→Taffy→Krilla) as the rendering backend.
//
// main.rs is the CLI thin shell: parse args → build RenderOptions →
// render_document() (src/render.rs) → route output. The full preprocessing
// pipeline lives in render.rs, shared with the HTTP server (src/server.rs).
use anyhow::Result;
use clap::Parser;

use std::path::PathBuf;
mod cli;
mod config;
mod math;
mod render;
mod server;
use config::TypePressConfig;

use cli::{Cli, Command, page_size_mm, read_input};
use render::{RenderOptions, render_document};
use typepress::network::AssetLimits;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // ── Subcommand: HTTP rendering server ──
    if let Some(Command::Serve { .. }) = &cli.command {
        return server::serve(cli.command.unwrap());
    }

    // Structured warnings accumulator — every recoverable failure lands here
    // (see src/diagnostics.rs for warning codes).
    let asset_limits = AssetLimits {
        max_bytes: cli.max_asset_size,
        allow_http: cli.allow_http,
        allowlist: cli.asset_allowlist.clone(),
    };

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
    let resolved_size = cli.resolve_size();
    let resolved_landscape = cli.resolve_landscape();
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

    let content = read_input(input_file.as_ref(), cli.stdin)?;

    // ── Render (full pipeline in render.rs) ──
    let opts = RenderOptions {
        content,
        from: if is_md {
            "md".to_string()
        } else {
            "html".to_string()
        },
        size: resolved_size,
        landscape: resolved_landscape,
        margin: resolved_margin,
        zoom: cli.zoom,
        fit: cli.fit,
        autofit: cli.autofit,
        header,
        footer,
        fonts: cli.fonts.clone(),
        css_files: cli.css_files.clone(),
        images: cli.images.clone(),
        math: cli.math,
        math_dir: cli.math_dir.clone(),
        bookmarks: cli.bookmarks,
        no_outline: cli.no_outline,
        tagged: cli.tagged,
        pdf_ua: cli.pdf_ua,
        no_system_fonts: cli.no_system_fonts,
        title: cli.title.clone(),
        authors: cli.authors.clone(),
        language: cli.language.clone(),
        base_path,
        asset_limits,
        config: cfg.clone(),
    };

    let out = render_document(opts)?;
    let pdf = out.pdf;
    let img_constrained = out.img_constrained;
    let resolved_size = out.resolved_size;
    let resolved_landscape = out.resolved_landscape;
    let effective_zoom = out.effective_zoom;
    let mut diag = out.diagnostics;

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

    // ── Render stats for --json / --hash / --check ──
    if cli.json || cli.hash || cli.check {
        let check_path = pdf_path_for_check.as_deref().unwrap_or_else(|| {
            let tmp = std::env::temp_dir().join("typepress_check.pdf");
            std::fs::write(&tmp, &pdf).ok();
            Box::leak(Box::new(tmp)).as_path()
        });

        let pages = typepress::css_layout::count_pdf_pages(&pdf);
        let bytes = pdf.len() as u64;
        let mut hash_str = None;
        let mut text_items = 0usize;
        let mut images = 0usize;
        let mut title: Option<String> = None;

        if cli.hash {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(&pdf);
            hash_str = Some(format!("{:x}", hasher.finalize()));
        }

        if let Ok(report) = fulgur::inspect::inspect(check_path) {
            text_items = report.text_items.len();
            images = report.images.len();
            title = report.metadata.title.clone();
            if pages > 1 && !cli.fit && !cli.autofit {
                diag.push("TP-1009", "multi-page output (try --fit or --page-size A3)");
            }
            if text_items == 0 && images == 0 {
                diag.push("TP-1010", "no text or images — possible rendering failure");
            }
        }
        let warnings = diag.warnings().to_vec();

        let size_mm =
            page_size_mm(resolved_size.as_deref().unwrap_or("A4")).unwrap_or((210.0, 297.0));
        let (pw, ph) = if resolved_landscape {
            (size_mm.1, size_mm.0)
        } else {
            size_mm
        };

        // --json
        if cli.json {
            let output = serde_json::json!({
                "ok": true,
                "pages": pages,
                "bytes": bytes,
                "page_size": format!("{}x{}mm", pw as u32, ph as u32),
                "landscape": resolved_landscape,
                "text_items": text_items,
                "images": images,
                "images_constrained": img_constrained,
                "hash_sha256": hash_str,
                "title": title,
                "warnings": warnings,
            });
            println!("{}", serde_json::to_string(&output).unwrap());
        }

        // --hash (standalone)
        if cli.hash
            && !cli.json
            && let Some(ref h) = hash_str
        {
            if cli.quiet {
                println!("{h}");
            } else {
                eprintln!("SHA-256: {h}");
            }
        }

        // --check (human-readable)
        if cli.check {
            let zoom_pct = effective_zoom * 100.0;
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
            println!("║  File size:  {:<6} bytes            ", bytes);
            println!("║  Zoom:       {:<5.1}%                 ", zoom_pct);
            println!("║  Text items: {:<4}                   ", text_items);
            println!("║  Images:     {:<4}                   ", images);
            if img_constrained > 0 {
                println!("║  Img scaled: {:<4} to page width     ", img_constrained);
            }
            if let Some(ref t) = title {
                println!("║  Title:      {}", t);
            }
            if let Some(ref h) = hash_str {
                println!("║  SHA-256:    {}…", &h[..32.min(h.len())]);
            }
            println!("╠══════════════════════════════════════╣");
            if warnings.is_empty() {
                println!("║  ✅  No issues detected              ║");
            } else {
                for w in &warnings {
                    let prefix = "║  ⚠  ";
                    let msg = format!("[{}] {}", w.code, w.message);
                    let max_len = 38 - prefix.len();
                    if msg.len() > max_len {
                        println!("{} {}…", prefix, &msg[..max_len - 1]);
                    } else {
                        println!("{}{}", prefix, msg);
                    }
                }
            }
            println!("╚══════════════════════════════════════╝");
        }
    }

    // ── Warning output (non-JSON mode): grep-able, stable codes ──
    if !cli.json && !cli.check && !diag.is_empty() {
        for w in diag.warnings() {
            eprintln!("warning[{}]: {}", w.code, w.message);
        }
    }

    // --strict: any warning → exit code 2 (fatal errors are already exit 1)
    if cli.strict && !diag.is_empty() {
        std::process::exit(2);
    }

    Ok(())
}

#[cfg(test)]
mod preprocess_tests {
    use crate::math::process_math;
    use crate::math::render_math_markup;
    #[cfg(feature = "mermaid-render")]
    use crate::render::process_mermaid;

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
    fn process_mermaid_generates_image() {
        let mut markdown = String::from("```mermaid\ngraph TD\n  A --> B\n```");
        let mut images = Vec::new();
        let rendered = process_mermaid(&mut markdown, &mut images).unwrap();

        assert_eq!(rendered, 1);
        assert!(markdown.contains("<img"), "should embed as img");
        assert!(!images.is_empty(), "should produce PNG bytes");
        assert!(!markdown.contains("mermaid-placeholder"));
    }
}
