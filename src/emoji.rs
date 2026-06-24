// Emoji → image replacement for TypePress.
//
// Motivation: Krilla cannot render CBDT bitmap emoji fonts, and Noto Serif
// CJK SC only covers a subset of Unicode emoji. Emoji characters in the
// supplementary plane (U+1Fxxx) render as tofu (missing-glyph boxes).
//
// Solution: detect emoji in HTML text, render each unique emoji to a small
// PNG via pango-view (which uses the system's color emoji font stack), cache
// the PNGs, and replace emoji characters with <img> tags pointing to the
// cached PNG files.
//
// This is the same approach used by pandoc, mdpdf, and GitHub's Markdown
// renderer. It's the practical standard for PDF engines that lack native
// color emoji support. Long-term roadmap item: native glyph rasterization
// via FreeType+harfbuzz (see typepress-development skill: glyph rasterization).
//
// Cache: /tmp/.typepress/emoji/{codepoint}.png
// Font: system default emoji font (pango-view resolves via fontconfig)

use std::path::PathBuf;
use std::process::Command;

const EMOJI_CACHE_SUBDIR: &str = ".typepress/emoji";
const EMOJI_RENDER_SIZE: &str = "32"; // pango-view font size

// ── Emoji detection ──────────────────────────────────────────────────────

/// Returns true if the character is an emoji that may need replacement.
/// Covers Unicode blocks commonly used for emoji.
fn is_emoji_char(c: char) -> bool {
    matches!(
        c,
        '\u{1F300}'..='\u{1FAFF}' // Misc Symbols, Emoticons, Transport, Supplemental, Extended-A
        | '\u{2600}'..='\u{27BF}'   // Misc Symbols (☀–➿)
        | '\u{2300}'..='\u{23FF}'   // Misc Technical (⌀–⏿)
        | '\u{200D}'                  // Zero-Width Joiner (emoji sequences)
        | '\u{FE0F}'                  // Variation Selector-16 (emoji style)
    )
}

/// Build the filename stem for a sequence of emoji codepoints.
/// Filters out VS16 (U+FE0F) since font rendering doesn't need it.
/// Returns e.g. "1f9ec" for 🧬, "1f469-200d-1f4bb" for 👩‍💻.
fn emoji_filename_stem(chars: &[char]) -> String {
    chars
        .iter()
        .filter(|c| **c != '\u{FE0F}')
        .map(|c| format!("{:x}", *c as u32))
        .collect::<Vec<_>>()
        .join("-")
}

/// Collect a contiguous emoji sequence starting at position `start` in `s`.
/// Includes ZWJ and VS16 modifiers that follow a base emoji.
fn collect_emoji_sequence(s: &str, start: usize) -> &str {
    let mut end = start;
    let mut chars = s[start..].char_indices();

    // Consume first char (must be emoji)
    if let Some((_, c)) = chars.next() {
        end = start + c.len_utf8();
        // Continue if next char is ZWJ or VS16
        for (offset, next_c) in chars {
            if is_emoji_char(next_c) && (next_c == '\u{200D}' || next_c == '\u{FE0F}') {
                end = start + offset + next_c.len_utf8();
            } else {
                // Also include the next char if it's another emoji part of a ZWJ sequence
                if end > start
                    && s[start..end].chars().any(|c| c == '\u{200D}')
                    && is_emoji_char(next_c)
                {
                    end = start + offset + next_c.len_utf8();
                    continue; // check for more ZWJ/VS16
                }
                break;
            }
        }
    }
    &s[start..end]
}

// ── PNG rendering via system emoji font ──────────────────────────────────

pub(crate) fn emoji_cache_dir() -> PathBuf {
    std::env::temp_dir().join(EMOJI_CACHE_SUBDIR)
}

/// Render an emoji character to a PNG file using pango-view.
/// Returns the path to the generated PNG, or None on failure.
///
/// pango-view leverages the system's font stack (fontconfig) for color
/// emoji rendering, which handles CBDT/COLR font formats that Krilla cannot.
pub fn render_emoji_png(codepoint_hex: &str, emoji_text: &str) -> Option<PathBuf> {
    let cache_dir = emoji_cache_dir();
    let path = cache_dir.join(format!("{}.png", codepoint_hex));

    // Cache hit — reuse
    if path.exists() && path.metadata().ok().map(|m| m.len()).unwrap_or(0) > 500 {
        return Some(path);
    }

    // Render: pango-view handles CBDT/COLR color emoji natively
    let result = Command::new("pango-view")
        .args([
            "--font",
            &format!("emoji {EMOJI_RENDER_SIZE}"),
            "--text",
            emoji_text,
            "-q",
            "-o",
            &path.to_string_lossy(),
            "--background",
            "transparent",
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            if path.exists() && path.metadata().ok().map(|m| m.len()).unwrap_or(0) > 500 {
                Some(path)
            } else {
                eprintln!(
                    "Warning: pango-view produced empty output for {} ({})",
                    emoji_text, codepoint_hex
                );
                None
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!(
                "Warning: pango-view failed for {} ({}): {}",
                emoji_text,
                codepoint_hex,
                stderr.trim()
            );
            None
        }
        Err(e) => {
            eprintln!("Warning: pango-view not found or failed: {}", e);
            None
        }
    }
}

/// Type alias for the rendering callback used by `replace_emoji_with_images`.
pub type RenderFn = fn(&str, &str) -> Option<PathBuf>;

// ── Main replacement logic ──────────────────────────────────────────────

/// Scan HTML for emoji characters in text content and replace them with
/// `<img>` tags pointing to PNG files generated by the provided render
/// function.
///
/// Only replaces emoji outside of HTML tags and attributes. Uses proper
/// UTF-8 character iteration (not byte indexing) to handle multibyte emoji.
///
/// Returns the modified HTML and the number of replacements made.
fn replace_emoji_with_renderer(html: &str, render_png: RenderFn) -> (String, usize) {
    let mut result = String::with_capacity(html.len() + html.len() / 4);
    let mut count: usize = 0;
    let mut in_tag = false;

    let _ = std::fs::create_dir_all(emoji_cache_dir());

    let mut chars = html.char_indices().peekable();

    while let Some((pos, c)) = chars.next() {
        if c == '<' {
            in_tag = true;
            result.push(c);
        } else if c == '>' && in_tag {
            in_tag = false;
            result.push(c);
        } else if in_tag {
            result.push(c);
        } else if is_emoji_char(c) && c != '\u{200D}' && c != '\u{FE0F}' {
            // Found base emoji — collect the full sequence
            let emoji_str = collect_emoji_sequence(html, pos);
            let emoji_chars: Vec<char> = emoji_str.chars().collect();
            let stem = emoji_filename_stem(&emoji_chars);

            // Skip already-consumed chars in the iterator
            for _ in 1..emoji_str.chars().count() {
                chars.next();
            }

            // Try to render to PNG
            if let Some(png_path) = render_png(&stem, emoji_str) {
                let path_str = png_path.to_string_lossy();
                if path_str.starts_with("__MARKER__:") {
                    // Krilla cannot render <img> tags — use text markers
                    // that survive the PDF pipeline. Post-processed later
                    // by scripts/overlay-emoji.py into embedded PNGs.
                    result.push_str(&format!("[TPEMOJI:{}]", stem));
                } else {
                    // Image rendering path (for future Krilla versions)
                    let file_url = format!("file://{}", path_str);
                    result.push_str(&format!(
                        "<img src=\"{}\" \
                         style=\"display:inline;width:1em;height:1em;vertical-align:text-bottom\" \
                         alt=\"{}\">",
                        file_url, emoji_str
                    ));
                }
                count += 1;
            } else {
                // Fallback: keep original emoji text
                result.push_str(emoji_str);
            }
        } else {
            result.push(c);
        }
    }

    (result, count)
}

/// Public API: replace emoji in HTML with text markers for post-processing.
///
/// Markers format: "[TPEMOJI:1f9ec]" where the hex part is the
/// codepoint stem (matching the cache filename without .png).
/// PNGs are pre-rendered in the background via pango-view.
///
/// After the PDF is generated, run `scripts/overlay-emoji.py` to replace
/// markers with embedded PNG images.
pub fn replace_emoji_with_images(html: &str) -> (String, usize) {
    replace_emoji_with_renderer(html, |stem, emoji_text| {
        // Pre-render PNG for later overlay
        let _ = render_emoji_png(stem, emoji_text);
        // Signal: use marker, not <img> tag (Krilla cannot render images)
        Some(PathBuf::from(format!("__MARKER__:{}", stem)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock renderer that always fails — tests text processing logic only.
    fn mock_render(_stem: &str, _text: &str) -> Option<PathBuf> {
        None
    }

    /// Mock renderer that returns a fake path — tests <img> tag generation.
    fn mock_render_ok(stem: &str, _text: &str) -> Option<PathBuf> {
        Some(PathBuf::from(format!("/tmp/fake/{}.png", stem)))
    }

    #[test]
    fn test_emoji_filename_stem() {
        assert_eq!(emoji_filename_stem(&['\u{1F9EC}']), "1f9ec");
        assert_eq!(emoji_filename_stem(&['\u{1F680}', '\u{FE0F}']), "1f680");
    }

    #[test]
    fn test_collect_emoji_sequence() {
        assert_eq!(collect_emoji_sequence("🧬abc", 0), "🧬");
        assert_eq!(collect_emoji_sequence("<span>🧬</span>", 6), "🧬");
    }

    #[test]
    fn test_replace_emoji_inline_with_render() {
        let (result, count) = replace_emoji_with_renderer("Hello 🧬 World", mock_render_ok);
        assert!(count >= 1);
        assert!(result.contains("<img src=\"file://"));
        // Emoji preserved in alt attribute
        assert!(result.contains("alt=\"🧬\""));
        // No standalone emoji: count emoji occurrences; should only be inside alt
        let emoji_count = result.matches('🧬').count();
        let alt_count = result.matches("alt=\"🧬\"").count();
        assert_eq!(
            emoji_count, alt_count,
            "emoji should only appear in alt attributes"
        );
    }

    #[test]
    fn test_skip_emoji_in_tags() {
        let html = "<a href='🧬'>link</a>";
        let (result, count) = replace_emoji_with_renderer(html, mock_render_ok);
        assert_eq!(count, 0, "emoji in attribute should not be replaced");
        assert_eq!(result, html);
    }

    #[test]
    fn test_emoji_in_text_node() {
        let html = "<p>Text 🧬 here</p>";
        let (result, count) = replace_emoji_with_renderer(html, mock_render_ok);
        assert!(count >= 1, "text node emoji should be replaced");
        assert!(
            result.contains("alt=\"🧬\""),
            "alt attribute should preserve original emoji"
        );
    }

    #[test]
    fn test_multiple_emoji() {
        let html = "🧬 and 🚀 and 🔮";
        let (result, count) = replace_emoji_with_renderer(html, mock_render_ok);
        assert!(count >= 3, "all base emoji should be replaced");
        // Emoji should only appear in alt attributes, not as text
        for emoji_char in ['🧬', '🚀', '🔮'] {
            let total = result.matches(emoji_char).count();
            let alt_count = result.matches(&format!("alt=\"{}\"", emoji_char)).count();
            assert_eq!(
                total, alt_count,
                "{} should only be in alt attr",
                emoji_char
            );
        }
    }

    #[test]
    fn test_no_emoji_passthrough() {
        let html = "<p>Hello World 123</p>";
        let (result, count) = replace_emoji_with_renderer(html, mock_render_ok);
        assert_eq!(count, 0);
        assert_eq!(result, html);
    }

    #[test]
    fn test_fallback_when_render_fails() {
        let html = "Hello 🧬 World";
        let (result, count) = replace_emoji_with_renderer(html, mock_render);
        assert_eq!(count, 0, "no replacement when render fails");
        assert!(result.contains('🧬'), "original emoji preserved");
    }

    #[test]
    fn test_is_emoji_char_bmp_symbol() {
        assert!(is_emoji_char('\u{2600}')); // ☀
        assert!(!is_emoji_char('A'));
        assert!(!is_emoji_char('中'));
    }
}
