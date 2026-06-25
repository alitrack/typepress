// Emoji handling for TypePress.
//
// Krilla supports COLRv1 color emoji via Type3 PDF font embedding
// (since krilla 0.7 / skrifa 0.42). The system "Noto Color Emoji"
// font is typically CBDT bitmap format which Krilla cannot render.
// We auto-download the COLRv1 version from jsDelivr CDN on first use
// (MIT licensed, same as Google's noto-emoji repo).
//
// No font renaming is needed: the system Noto Color Emoji font is
// CBDT bitmap which Krilla rejects, so the AssetBundle COLRv1 version
// (loaded under the same family name) is the only usable match.
//
// Cache: ~/.cache/typepress/fonts/Noto-COLRv1.ttf

use std::path::PathBuf;
use std::time::Duration;

const COLR_FONT_URL: &str =
    "https://cdn.jsdelivr.net/gh/googlefonts/noto-emoji@main/fonts/Noto-COLRv1.ttf";
const COLR_FONT_FILENAME: &str = "Noto-COLRv1.ttf";

/// CSS to inject when COLR font is loaded. Puts the COLR font into the
/// font-family stack right after CJK, before system fallback fonts.
/// The font keeps its original family name "Noto Color Emoji" — we
/// don't rename it because Krilla rejects the system CBDT version
/// anyway, so the AssetBundle COLRv1 font is the only usable match.
pub fn colr_font_face_css() -> &'static str {
    "* { font-family: 'Noto Sans SC', 'Noto Color Emoji', sans-serif !important; }"
}

/// Check if HTML contains emoji characters that need COLR rendering.
pub fn has_emoji(html: &str) -> bool {
    html.chars().any(is_emoji_char)
}

fn is_emoji_char(c: char) -> bool {
    matches!(
        c,
        '\u{1F300}'..='\u{1FAFF}'
            | '\u{2600}'..='\u{27BF}'
            | '\u{2300}'..='\u{23FF}'
    )
}

/// Ensure COLRv1 emoji font is available locally.
/// Downloads on first use; reuses cached copy thereafter.
pub fn ensure_colr_emoji_font() -> Option<PathBuf> {
    let cache_dir = dirs_font_cache();
    let colr_path = cache_dir.join(COLR_FONT_FILENAME);

    if colr_path.exists() && colr_path.metadata().ok().map(|m| m.len()).unwrap_or(0) > 100_000 {
        return Some(colr_path);
    }

    // Download (no renaming needed — COLRv1 format is the only one Krilla can use)
    match download_colr_font(&colr_path) {
        Ok(()) => {
            eprintln!("Emoji: COLRv1 font ready ({})", colr_path.display());
            Some(colr_path)
        }
        Err(e) => {
            eprintln!("Warning: COLR emoji font download failed: {e}");
            eprintln!("  Emoji may render as missing glyphs. Download manually:");
            eprintln!("  curl -o {} {}", colr_path.display(), COLR_FONT_URL);
            None
        }
    }
}

fn dirs_font_cache() -> PathBuf {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));
    home.join(".cache").join("typepress").join("fonts")
}

fn download_colr_font(dest: &PathBuf) -> Result<(), String> {
    let parent = dest.parent().ok_or("no parent dir")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("client: {e}"))?;
    let response = client
        .get(COLR_FONT_URL)
        .send()
        .map_err(|e| format!("download: {e}"))?;
    let bytes = response.bytes().map_err(|e| format!("read: {e}"))?;

    if bytes.len() < 100_000 {
        return Err(format!("font too small: {} bytes", bytes.len()));
    }

    std::fs::write(dest, &bytes).map_err(|e| format!("write: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_has_emoji_detection() {
        assert!(has_emoji("Hello 🧬 World"));
        assert!(has_emoji("🚀 launch"));
        assert!(!has_emoji("Hello World"));
        assert!(!has_emoji("<p>text</p>"));
        assert!(has_emoji("🔍🔧🔮"));
    }
}
