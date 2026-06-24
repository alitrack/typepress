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
pub fn inject_remote_css(html: &mut String) -> Result<usize> {
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
                        eprintln!("Warning: CSS read error for {}: {}", url, e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: CSS download failed for {}: {}", url, e);
            }
        }
    }

    Ok(count)
}

/// Process <img src="https?://..."> tags.
///
/// Downloads each image to a temp directory and replaces the src attribute
/// with a local file:// path that fulgur can load.
///
/// Returns the list of downloaded image paths so they can be cleaned up.
pub fn download_remote_images(html: &mut String) -> Result<(usize, Vec<PathBuf>)> {
    let img_re =
        Regex::new(r#"(?i)<img\b[^>]*?\bsrc\s*=\s*["'](https?://[^"']+)["'][^>]*>"#).unwrap();

    let mut count = 0;
    let mut paths = Vec::new();
    let html_clone = html.clone();

    for cap in img_re.captures_iter(&html_clone) {
        let full_tag = cap.get(0).unwrap().as_str();
        let url = cap.get(1).unwrap().as_str();

        // Skip data: URIs — already embedded
        if url.starts_with("data:") {
            continue;
        }

        match download_to_cache(url, "images") {
            Ok(path) => {
                let file_url = format!("file://{}", path.display());
                let new_tag = full_tag.replace(url, &file_url);
                *html = html.replacen(full_tag, &new_tag, 1);
                eprintln!("Image: downloaded {} → {}", url, path.display());
                paths.push(path);
                count += 1;
            }
            Err(e) => {
                eprintln!("Warning: Image download failed for {}: {}", url, e);
            }
        }
    }

    Ok((count, paths))
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
        let n = inject_remote_css(&mut html).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn test_download_remote_images_noop() {
        let mut html = "<img src=\"data:image/png;base64,xxx\">".to_string();
        let (n, paths) = download_remote_images(&mut html).unwrap();
        assert_eq!(n, 0);
        assert!(paths.is_empty());
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
