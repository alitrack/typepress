// @font-face CSS support — parse @font-face rules and auto-load web fonts.
//
// Supports:
//   @font-face { font-family: "Name"; src: url("path/to/font.woff2"); }
//   @font-face { font-family: "Name"; src: url("https://example.com/font.woff2"); }
//
// Local paths are resolved relative to the CSS file location or base_path.
// Remote fonts are downloaded and cached in a temp directory.
// All discovered fonts are added to the fulgur AssetBundle.

use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FontFace {
    pub family: String,
    pub src_url: String,
}

/// Parse @font-face rules from CSS content.
pub fn parse_font_faces(css: &str) -> Vec<FontFace> {
    let re = Regex::new(r"(?s)@font-face\s*\{([^}]+)\}").unwrap();
    let family_re = Regex::new(r#"font-family\s*:\s*["']?([^"';]+)["']?"#).unwrap();
    let src_re = Regex::new(r#"src\s*:\s*url\(["']?([^"')]+)["']?\)"#).unwrap();

    let mut faces = Vec::new();
    for caps in re.captures_iter(css) {
        let body = caps.get(1).unwrap().as_str();
        if let (Some(fam), Some(src)) = (
            family_re.captures(body).and_then(|c| c.get(1)),
            src_re.captures(body).and_then(|c| c.get(1)),
        ) {
            faces.push(FontFace {
                family: fam.as_str().trim().to_string(),
                src_url: src.as_str().trim().to_string(),
            });
        }
    }
    faces
}

/// Extract @font-face rules from HTML <style> blocks.
pub fn extract_font_faces_from_html(html: &str) -> Vec<FontFace> {
    let style_re = Regex::new(r"(?s)<style[^>]*>(.*?)</style>").unwrap();
    let mut all = Vec::new();
    for caps in style_re.captures_iter(html) {
        all.extend(parse_font_faces(caps.get(1).unwrap().as_str()));
    }
    all
}

/// Resolve a font URL to a local file path.
///
/// - http(s):// URLs → download to a temp directory
/// - Relative paths → resolve against base_path or cwd
/// - Absolute paths → return as-is
pub fn resolve_font_path(url: &str, base_path: Option<&Path>) -> Result<PathBuf> {
    if url.starts_with("http://") || url.starts_with("https://") {
        download_font(url)
    } else if url.starts_with('/') || (url.len() > 2 && url.as_bytes()[1] == b':') {
        // Absolute path
        let p = PathBuf::from(url);
        if p.exists() {
            Ok(p)
        } else {
            anyhow::bail!("Font file not found: {}", url)
        }
    } else {
        // Relative path — resolve against base_path or cwd
        let base = base_path.unwrap_or_else(|| Path::new("."));
        let resolved = base.join(url);
        if resolved.exists() {
            Ok(resolved)
        } else {
            // Try cwd
            let cwd_resolved = std::env::current_dir()?.join(url);
            if cwd_resolved.exists() {
                Ok(cwd_resolved)
            } else {
                anyhow::bail!(
                    "Font file not found: {} (tried {} and {})",
                    url,
                    resolved.display(),
                    cwd_resolved.display()
                )
            }
        }
    }
}

fn download_font(url: &str) -> Result<PathBuf> {
    let parsed = url
        .parse::<reqwest::Url>()
        .with_context(|| format!("Invalid font URL: {}", url))?;

    // Determine filename from URL path
    let filename = parsed
        .path_segments()
        .and_then(|mut s| s.next_back())
        .unwrap_or("font.ttf");
    let filename = if filename.contains('.') {
        filename.to_string()
    } else {
        format!("{}.ttf", filename)
    };

    let cache_dir = dirs_font_cache()?;
    let dest = cache_dir.join(&filename);

    if dest.exists() {
        return Ok(dest);
    }

    let response =
        reqwest::blocking::get(url).with_context(|| format!("Failed to download font: {}", url))?;
    let bytes = response
        .bytes()
        .with_context(|| format!("Failed to read font body: {}", url))?;
    std::fs::write(&dest, &bytes)
        .with_context(|| format!("Failed to write font to {}", dest.display()))?;

    eprintln!("Font: downloaded {} → {}", url, dest.display());
    Ok(dest)
}

fn dirs_font_cache() -> Result<PathBuf> {
    let dir = dirs_next().unwrap_or_else(|| PathBuf::from("/tmp"));
    let cache = dir.join(".typepress/fonts");
    std::fs::create_dir_all(&cache)?;
    Ok(cache)
}

fn dirs_next() -> Option<PathBuf> {
    std::env::var("XDG_CACHE_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| dirs_sys().map(|d| d.join(".cache")))
}

fn dirs_sys() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

/// Scan a directory for font files and return their paths.
#[allow(dead_code)]
pub fn scan_font_dir(dir: &Path) -> Vec<PathBuf> {
    let mut fonts = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            if matches!(ext.as_deref(), Some("ttf" | "otf" | "woff2" | "ttc")) {
                fonts.push(path);
            }
        }
        fonts.sort();
    }
    fonts
}

/// Discover system font directories for automatic CJK and general font discovery.
/// Returns a list of paths to scan — caller should call scan_font_dir on each.
#[allow(dead_code)]
pub fn system_font_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/share/fonts"));
        paths.push(PathBuf::from("/usr/local/share/fonts"));
        if let Ok(home) = std::env::var("HOME") {
            paths.push(PathBuf::from(&home).join(".local/share/fonts"));
            paths.push(PathBuf::from(&home).join(".fonts"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            paths.push(PathBuf::from(&home).join("Library/Fonts"));
        }
        paths.push(PathBuf::from("/Library/Fonts"));
        paths.push(PathBuf::from("/System/Library/Fonts"));
    }

    #[cfg(target_os = "windows")]
    {
        paths.push(PathBuf::from("C:\\Windows\\Fonts"));
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            paths.push(PathBuf::from(&local).join("Microsoft\\Windows\\Fonts"));
        }
    }

    paths
}
