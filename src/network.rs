// Network resource loading — download remote CSS and images.
//
// Two capabilities:
//   1. <link rel="stylesheet" href="https://..."> → download + inject as <style>
//   2. <img src="https://..."> → download → replace src with local file:// path
//
// Motivation: Odoo reports, Bootstrap CDN references, and any HTML that
// includes remote assets need to be fully resolved before entering
// fulgur's rendering pipeline (which has no network access).
//
// We use reqwest::blocking for simplicity — same pattern as fonts.rs.

use crate::diagnostics::Diagnostics;
use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Create a blocking HTTP client with a 30-second timeout.
fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("reqwest client creation should not fail")
}

/// Download a URL to a temp file, with caching.
fn download_to_cache(url: &str, cache_subdir: &str) -> Result<PathBuf> {
    let cache_dir = std::env::temp_dir().join(".typepress").join(cache_subdir);
    std::fs::create_dir_all(&cache_dir)?;

    // Derive filename from URL path
    let parsed = url
        .parse::<reqwest::Url>()
        .with_context(|| format!("Invalid URL: {}", url))?;
    let filename = parsed
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|f| !f.is_empty())
        .unwrap_or("resource");
    // Sanitize filename: keep only safe chars, add .download extension if needed
    let filename = if filename.contains('.') {
        filename.replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-',
            "_",
        )
    } else {
        format!("{}.download", filename)
    };

    let dest = cache_dir.join(&filename);
    if dest.exists() {
        return Ok(dest);
    }

    let response = http_client()
        .get(url)
        .send()
        .with_context(|| format!("Failed to download: {}", url))?;
    let bytes = response
        .bytes()
        .with_context(|| format!("Failed to read body: {}", url))?;
    std::fs::write(&dest, &bytes)
        .with_context(|| format!("Failed to write to {}", dest.display()))?;
    Ok(dest)
}

/// Process <link rel="stylesheet" href="https?://..."> tags.
///
/// Downloads each remote CSS file, injects it as a <style> block, and
/// removes the original <link> tag (it can't be used by fulgur).
pub fn inject_remote_css(html: &mut String, diag: &mut Diagnostics) -> Result<usize> {
    // Match <link> tags that have both rel=stylesheet and href=https?://
    let link_re =
        Regex::new(r#"(?i)<link\b[^>]*\bhref\s*=\s*["'](https?://[^"']+)["'][^>]*>"#).unwrap();

    let mut count = 0;
    let html_clone = html.clone();

    for cap in link_re.captures_iter(&html_clone) {
        let full_tag = cap.get(0).unwrap().as_str();
        let url = cap.get(1).unwrap().as_str();

        // Must have rel=stylesheet (order-independent check)
        if !full_tag.to_lowercase().contains("stylesheet") {
            continue;
        }

        // Skip alternate stylesheets
        if full_tag.to_lowercase().contains("alternate") {
            continue;
        }

        // Skip print-only media queries — they won't render correctly anyway
        if full_tag.to_lowercase().contains("media=\"print\"") {
            continue;
        }

        match download_to_cache(url, "css") {
            Ok(path) => {
                match std::fs::read_to_string(&path) {
                    Ok(css) => {
                        let style_tag =
                            format!("\n<style>\n/* Source: {} */\n{}\n</style>\n", url, css);
                        // Inject before first </head> or at start of <body>
                        if let Some(pos) = html.find("</head>") {
                            html.insert_str(pos, &style_tag);
                        } else if let Some(pos) = html.find("<body") {
                            html.insert_str(pos, &style_tag);
                        } else {
                            html.push_str(&style_tag);
                        }
                        // Remove original <link> tag
                        *html =
                            html.replacen(full_tag, &format!("<!-- downloaded: {} -->", url), 1);
                        count += 1;
                        eprintln!("CSS: downloaded {} → {}", url, path.display());
                    }
                    Err(e) => {
                        diag.push(
                            "TP-1004",
                            format!("failed to read downloaded CSS {url}: {e}"),
                        );
                    }
                }
            }
            Err(e) => {
                diag.push("TP-1004", format!("failed to download CSS {url}: {e}"));
            }
        }
    }

    Ok(count)
}

/// Parse PNG/JPEG/GIF image dimensions from magic bytes.
///
/// fulgur only renders `<img>` tags that carry an explicit size (CSS
/// `width/height` or `width`/`height` attributes) — unsized images are
/// silently dropped. We read the intrinsic size from the image header so
/// the rewritten tag can carry it.
fn image_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() >= 24 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        // PNG: width/height are big-endian u32 at offset 16/20
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((w, h));
    }
    if data.len() >= 10 && &data[..6] == b"GIF87a" || data.len() >= 10 && &data[..6] == b"GIF89a" {
        // GIF: width/height are little-endian u16 at offset 6/8
        let w = u16::from_le_bytes([data[6], data[7]]);
        let h = u16::from_le_bytes([data[8], data[9]]);
        return Some((w as u32, h as u32));
    }
    if data.len() >= 4 && data[0] == 0xFF && data[1] == 0xD8 {
        // JPEG: scan markers for SOF0/SOF1/SOF2 (start of frame)
        let mut i = 2usize;
        while i + 9 < data.len() {
            if data[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            // Standalone markers (RST/DHT/DAC/...) have no length
            if marker == 0xFF || (0xD0..=0xD9).contains(&marker) {
                i += 2;
                continue;
            }
            let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if (0xC0..=0xC3).contains(&marker)
                || (0xC5..=0xC7).contains(&marker)
                || (0xC9..=0xCB).contains(&marker)
                || (0xCD..=0xCF).contains(&marker)
            {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]);
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]);
                return Some((w as u32, h as u32));
            }
            i += 2 + seg_len;
        }
    }
    None
}

/// Process <img src="https?://..."> tags.
///
/// Downloads each image, replaces the src attribute with a bundle-safe
/// name (e.g. `txp-remote-0.png`), and returns the image bytes so the
/// caller can register them in the AssetBundle.
///
/// ⚠️ fulgur's DummyNetProvider ignores `file://` and `http://` img srcs —
/// AssetBundle registration is the ONLY path that renders images.
/// ⚠️ fulgur drops `<img>` tags without an explicit size — we inject
/// `style="width:Wpx;height:Hpx"` from the intrinsic image dimensions.
/// Process `<img src="https?://...">` tags.
///
/// Downloads each image, replaces the src attribute with a bundle-safe
/// name (e.g. `txp-remote-0.png`), injects intrinsic size when the tag
/// carries none, and returns `(bundle_name, bytes)` pairs for the caller
/// to register in the AssetBundle. AssetBundle registration is the ONLY
/// path that renders images; fulgur drops `<img>` tags without an explicit
/// size — we inject `style="width:Wpx;height:Hpx"` from the intrinsic
/// image dimensions.
///
/// Failures are reported through `diag` (TP-1001 download failed,
/// TP-1002 read/zero-byte) and never abort the render.
pub fn download_remote_images(
    html: &mut String,
    diag: &mut Diagnostics,
) -> Result<Vec<(String, Vec<u8>)>> {
    let img_re =
        Regex::new(r#"(?i)<img\b[^>]*?\bsrc\s*=\s*["'](https?://[^"']+)["'][^>]*>"#).unwrap();

    let mut images: Vec<(String, Vec<u8>)> = Vec::new();
    let html_clone = html.clone();

    for cap in img_re.captures_iter(&html_clone) {
        let full_tag = cap.get(0).unwrap().as_str();
        let url = cap.get(1).unwrap().as_str();

        // Skip data: URIs — already embedded
        if url.starts_with("data:") {
            continue;
        }

        match download_to_cache(url, "images") {
            Ok(path) => match std::fs::read(&path) {
                Ok(data) => {
                    if data.is_empty() {
                        diag.push("TP-1002", format!("zero-byte image: {url}"));
                        continue;
                    }
                    let name = format!("txp-remote-{}.png", images.len());
                    let mut new_tag = full_tag.replace(url, &name);
                    // Inject intrinsic size unless the tag already has one
                    // (CSS style width/height or width/height attributes).
                    let has_size = full_tag.contains("width=")
                        || full_tag.contains("width:")
                        || full_tag.contains("height=")
                        || full_tag.contains("height:");
                    if !has_size && let Some((w, h)) = image_dimensions(&data) {
                        let size_style = format!("style=\"width:{w}px;height:{h}px\"");
                        // Insert before the closing '>' of the img tag
                        if let Some(gt) = new_tag.rfind('>') {
                            new_tag.insert_str(gt, &format!(" {size_style}"));
                        }
                    }
                    *html = html.replacen(full_tag, &new_tag, 1);
                    eprintln!("Image: downloaded {} → {name} ({} bytes)", url, data.len());
                    images.push((name, data));
                }
                Err(e) => {
                    diag.push(
                        "TP-1002",
                        format!("failed to read downloaded image {url}: {e}"),
                    );
                }
            },
            Err(e) => {
                diag.push("TP-1001", format!("failed to download image {url}: {e}"));
            }
        }
    }

    Ok(images)
}

/// Process <link rel="stylesheet"> with relative paths (non-http).
///
/// Resolves relative CSS paths against a base directory and injects them
/// as inline <style> blocks. Useful for HTML files that reference local CSS
/// but fulgur processes them as a single string (no file context).
pub fn inject_local_css(html: &mut String, base_path: &Path) -> Result<usize> {
    let link_re = Regex::new(
        r#"(?i)<link\b[^>]*?\brel\s*=\s*["']stylesheet["'][^>]*?\bhref\s*=\s*["']([^"']+\.css)["'][^>]*>"#
    ).unwrap();

    let mut count = 0;
    let html_clone = html.clone();

    for cap in link_re.captures_iter(&html_clone) {
        let full_tag = cap.get(0).unwrap().as_str();
        let href = cap.get(1).unwrap().as_str();

        // Only handle relative/local paths, not http URLs (those go to inject_remote_css)
        if href.starts_with("http://") || href.starts_with("https://") {
            continue;
        }

        let css_path = base_path.join(href);
        match std::fs::read_to_string(&css_path) {
            Ok(css) => {
                let style_tag = format!(
                    "\n<style>\n/* embedded from: {} */\n{}\n</style>\n",
                    href, css
                );
                if let Some(pos) = html.find("</head>") {
                    html.insert_str(pos, &style_tag);
                } else {
                    html.push_str(&style_tag);
                }
                *html = html.replacen(full_tag, &format!("<!-- embedded: {} -->", href), 1);
                count += 1;
                eprintln!("CSS: embedded local {}", css_path.display());
            }
            Err(e) => {
                eprintln!("Warning: CSS file not found {}: {}", css_path.display(), e);
            }
        }
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_remote_css_noop() {
        let mut html = "<html><head></head><body></body></html>".to_string();
        let mut diag = Diagnostics::new();
        let n = inject_remote_css(&mut html, &mut diag).unwrap();
        assert_eq!(n, 0);
        assert!(diag.is_empty());
    }

    #[test]
    fn test_download_remote_images_noop() {
        let mut html = "<img src=\"data:image/png;base64,xxx\">".to_string();
        let mut diag = Diagnostics::new();
        let images = download_remote_images(&mut html, &mut diag).unwrap();
        assert!(images.is_empty());
        assert!(diag.is_empty());
    }

    #[test]
    fn test_image_dimensions_png() {
        // 1x1 PNG
        let png = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00,
        ];
        assert_eq!(image_dimensions(&png), Some((1, 1)));
    }

    #[test]
    fn test_image_dimensions_gif() {
        // 2x3 GIF89a header
        let gif = b"GIF89a\x02\x00\x03\x00\x80\x00\x00";
        assert_eq!(image_dimensions(gif), Some((2, 3)));
    }

    #[test]
    fn test_image_dimensions_jpeg() {
        // JPEG with SOF0: FFD8 ... FF C0 len=11 precision h w ...
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x00, 0x64, 0x00,
            0xC8, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01, 0xFF, 0xD9,
        ];
        // h=0x0064=100, w=0x00C8=200
        assert_eq!(image_dimensions(&jpeg), Some((200, 100)));
    }

    #[test]
    fn test_image_dimensions_unknown() {
        assert_eq!(image_dimensions(b"not an image"), None);
        assert_eq!(image_dimensions(&[]), None);
    }

    #[test]
    fn test_inject_local_css() {
        let dir = std::env::temp_dir();
        let css_path = dir.join("_typepress_test.css");
        std::fs::write(&css_path, "body { color: red; }").unwrap();

        let mut html = format!(
            "<html><head><link rel=\"stylesheet\" href=\"{}\"></head><body></body></html>",
            css_path.file_name().unwrap().to_str().unwrap()
        );
        let n = inject_local_css(&mut html, &dir).unwrap();
        assert!(n > 0);
        assert!(html.contains("body { color: red; }"));

        std::fs::remove_file(&css_path).ok();
    }
}
