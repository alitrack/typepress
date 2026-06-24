// Emoji handling for TypePress.
//
// Krilla supports COLRv1 color emoji via Type3 PDF font embedding
// (since krilla 0.7 / skrifa 0.42). The system "Noto Color Emoji"
// font is typically CBDT bitmap format which Krilla cannot render.
// We auto-download the COLRv1 version from jsDelivr CDN on first use
// (MIT licensed, same as Google's noto-emoji repo).
//
// Cache: ~/.cache/typepress/fonts/NotoColorEmoji-COLR.ttf

use std::path::PathBuf;
use std::time::Duration;

const COLR_FONT_URL: &str =
    "https://cdn.jsdelivr.net/gh/googlefonts/noto-emoji@main/fonts/Noto-COLRv1.ttf";
const COLR_FONT_FILENAME: &str = "NotoColorEmoji-COLR.ttf";

/// CSS to inject when COLR font is loaded. Puts the COLR font into the
/// font-family stack right after CJK, before system fallback fonts.
/// blitz-html does not support @font-face unicode-range, so we use a
/// direct font-family override on the root element.
pub fn colr_font_face_css() -> &'static str {
    "* { font-family: 'Noto Sans SC', 'Noto Color Emoji COLR', sans-serif !important; }"
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
/// Downloads and renames on first use; reuses cached copy thereafter.
pub fn ensure_colr_emoji_font() -> Option<PathBuf> {
    let cache_dir = dirs_font_cache();
    let colr_path = cache_dir.join(COLR_FONT_FILENAME);

    if colr_path.exists() && colr_path.metadata().ok().map(|m| m.len()).unwrap_or(0) > 100_000 {
        return Some(colr_path);
    }

    // Download and rename
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

    // Download with timeout
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

    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("write: {e}"))?;

    // Rename font family to avoid conflict with system CBDT "Noto Color Emoji"
    rename_colr_font_family(&tmp, "Noto Color Emoji COLR")?;
    std::fs::rename(&tmp, dest).map_err(|e| format!("rename: {e}"))?;

    Ok(())
}

#[cfg(not(test))]
fn rename_colr_font_family(path: &std::path::Path, new_family: &str) -> Result<(), String> {
    use std::process::{Command, Stdio};

    let script = r#"
from fontTools.ttLib import TTFont
import sys
tt = TTFont(sys.argv[1])
for rec in tt['name'].names:
    if rec.nameID in [1, 16, 4, 6]:
        if rec.nameID in [4, 6]:
            rec.string = sys.argv[2]
        else:
            rec.string = sys.argv[2]
tt.save(sys.argv[1])
"#;

    let mut child = Command::new("python3")
        .args(["-c", script, &path.to_string_lossy(), new_family])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("python3: {e}"))?;

    // 10-minute timeout for font subsetting; kill if exceeded
    let (tx, rx) = std::sync::mpsc::channel();
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = tx.send(child.wait());
    });
    let output = match rx.recv_timeout(Duration::from_secs(600)) {
        Ok(Ok(status)) => status,
        Ok(Err(e)) => return Err(format!("wait: {e}")),
        Err(_) => {
            let _ = Command::new("kill").arg(format!("{pid}")).status();
            return Err("python3 timed out after 600s".to_string());
        }
    };
    if !output.success() {
        return Err("fontTools rename failed".into());
    }
    Ok(())
}

#[cfg(test)]
fn rename_colr_font_family(_path: &std::path::Path, _new_family: &str) -> Result<(), String> {
    Ok(()) // no-op in tests
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
