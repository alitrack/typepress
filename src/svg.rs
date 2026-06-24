// SVG Unicode rendering — extracts text from PDF with ToUnicode CMap decoding.
//
// fulgur::inspect::inspect() returns raw CID glyph bytes for subset fonts.
// This module parses the PDF's ToUnicode CMaps to recover readable Unicode.
//
// Text position tracking cribbed from fulgur/crates/fulgur/src/inspect.rs
// (MIT-licensed); extended here with CMap-based decoding.

use anyhow::Result;
use lopdf::{Document, Object};
use std::collections::BTreeMap;

// ── ToUnicode CMap ─────────────────────────────────────────────────────

type CidMap = BTreeMap<u16, char>;

/// Whether a CMap uses single-byte CIDs (Type3/COLR fonts with Identity
/// encoding, as opposed to standard CID fonts with u16 CIDs).
#[derive(Clone, Copy, PartialEq)]
enum CidEncoding {
    SingleByte,
    U16,
}

struct FontInfo {
    cmap: CidMap,
    encoding: CidEncoding,
}

/// Parse a ToUnicode CMap stream and return a CID→char mapping.
fn parse_tounicode_cmap(stream_data: &[u8]) -> CidMap {
    let s = String::from_utf8_lossy(stream_data);
    let mut map = CidMap::new();

    // Parse bfchar: <CID> <Unicode>
    // E.g. "beginbfchar\n<0001> <0041>\n<0002> <0042>\nendbfchar"
    if let Some(bfchar) = extract_section(&s, "beginbfchar", "endbfchar") {
        for line in bfchar.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let cid = hex_to_u16(parts[0]);
                let ch = hex_to_char(parts[1]);
                if let (Some(c), Some(ch)) = (cid, ch) {
                    map.insert(c, ch);
                }
            }
        }
    }

    // Parse bfrange: <startCID> <endCID> <startUnicode>
    // E.g. "beginbfrange\n<0003> <0005> <0043>\nendbfrange"
    // This maps CID 0003→0043, 0004→0044, 0005→0045
    if let Some(bfrange) = extract_section(&s, "beginbfrange", "endbfrange") {
        for line in bfrange.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let start_cid = hex_to_u16(parts[0]);
                let end_cid = hex_to_u16(parts[1]);
                let start_uni = hex_to_u32(parts[2]);
                if let (Some(sc), Some(ec), Some(su)) = (start_cid, end_cid, start_uni) {
                    for offset in 0..=(ec - sc) {
                        if let Some(ch) = char::from_u32(su + offset as u32) {
                            map.insert(sc + offset, ch);
                        }
                    }
                }
            }
        }
    }

    map
}

fn extract_section<'a>(text: &'a str, begin: &str, end: &str) -> Option<&'a str> {
    let start = text.find(begin)? + begin.len();
    let end_pos = text[start..].find(end)?;
    Some(&text[start..start + end_pos])
}

fn hex_to_u16(s: &str) -> Option<u16> {
    let s = s.trim_matches(|c| c == '<' || c == '>');
    u16::from_str_radix(s, 16).ok()
}

fn hex_to_u32(s: &str) -> Option<u32> {
    let s = s.trim_matches(|c| c == '<' || c == '>');
    u32::from_str_radix(s, 16).ok()
}

fn hex_to_char(s: &str) -> Option<char> {
    let s = s.trim_matches(|c| c == '<' || c == '>');
    let val = u32::from_str_radix(s, 16).ok()?;

    // Handle UTF-16 surrogate pairs (PDF encodes supplementary-plane
    // characters like U+1F9EC as <D83E DDEC> in bfchar entries)
    if (0xD800..=0xDBFF).contains(&((val >> 16) & 0xFFFF)) {
        let high = (val >> 16) & 0xFFFF;
        let low = val & 0xFFFF;
        if (0xDC00..=0xDFFF).contains(&low) {
            let scalar = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
            return char::from_u32(scalar);
        }
    }

    char::from_u32(val)
}

// ── Font CMap resolution ───────────────────────────────────────────────

/// Build a mapping from font name → (CMap, encoding) by parsing all
/// ToUnicode CMaps and detecting font subtypes (Type3 = single-byte CID).
fn build_font_cmaps(doc: &Document) -> BTreeMap<String, FontInfo> {
    let mut font_info: BTreeMap<String, FontInfo> = BTreeMap::new();

    for (&page_num, &page_id) in &doc.get_pages() {
        let resources = resolve_page_resources(doc, page_id);
        let fonts = match resources
            .as_ref()
            .and_then(|r| r.get(b"Font").ok())
            .and_then(|o| doc.dereference(o).ok())
        {
            Some((_, Object::Dictionary(d))) => d.clone(),
            _ => continue,
        };

        for (name, font_ref) in &fonts {
            let name_str = String::from_utf8_lossy(name).into_owned();
            if font_info.contains_key(&name_str) {
                continue;
            }
            let font_dict = match doc.dereference(font_ref) {
                Ok((_, Object::Dictionary(d))) => d,
                _ => continue,
            };

            // Detect Type3 for single-byte CID encoding
            let is_type3 = font_dict
                .get(b"Subtype")
                .ok()
                .and_then(|s| s.as_name().ok())
                .is_some_and(|n| n == b"Type3");

            // Try ToUnicode CMap
            if let Ok(tu) = font_dict.get(b"ToUnicode")
                && let Ok((_, Object::Stream(stream))) = doc.dereference(tu)
            {
                let cmap = parse_tounicode_cmap(&stream.content);
                if !cmap.is_empty() {
                    font_info.insert(
                        name_str,
                        FontInfo {
                            cmap,
                            encoding: if is_type3 {
                                CidEncoding::SingleByte
                            } else {
                                CidEncoding::U16
                            },
                        },
                    );
                }
            }
        }
        let _ = page_num; // suppress unused warning
    }

    font_info
}

fn resolve_page_resources(doc: &Document, page_id: lopdf::ObjectId) -> Option<lopdf::Dictionary> {
    let mut current_id = page_id;
    loop {
        let dict = match doc.get_object(current_id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return None,
        };
        if let Ok(res) = dict.get(b"Resources")
            && let Ok((_, Object::Dictionary(resources))) = doc.dereference(res)
        {
            return Some(resources.clone());
        }
        match dict.get(b"Parent").and_then(|p| p.as_reference()) {
            Ok(parent_id) if parent_id != current_id => current_id = parent_id,
            _ => return None,
        }
    }
}

// ── Text extraction with CMap decoding ─────────────────────────────────

const IDENTITY: [f32; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

pub struct UnicodeTextItem {
    pub page: u32,
    pub x: f32,
    pub y: f32,
    #[allow(dead_code)]
    pub width: f32,
    #[allow(dead_code)]
    pub height: f32,
    pub text: String,
    pub font: String,
    pub font_size: f32,
}

fn obj_to_f32(obj: &Object) -> f32 {
    match obj {
        Object::Integer(i) => *i as f32,
        Object::Real(f) => *f,
        _ => 0.0,
    }
}

fn obj_as_name_str(obj: &Object) -> Option<&str> {
    obj.as_name().ok().and_then(|b| std::str::from_utf8(b).ok())
}

fn concat_matrix(current: &[f32; 6], new: &[f32; 6]) -> [f32; 6] {
    let (a, b, c, d, e, f) = (new[0], new[1], new[2], new[3], new[4], new[5]);
    let (a2, b2, c2, d2, e2, f2) = (
        current[0], current[1], current[2], current[3], current[4], current[5],
    );
    [
        a * a2 + b * c2,
        a * b2 + b * d2,
        c * a2 + d * c2,
        c * b2 + d * d2,
        e * a2 + f * c2 + e2,
        e * b2 + f * d2 + f2,
    ]
}

/// Decode a PDF text string using the font's CID→Unicode CMap.
///
/// For standard CID fonts, bytes are raw glyph IDs (big-endian u16 pairs).
/// For Type3/COLR fonts with Identity encoding, bytes are single-byte CIDs.
/// Auto-detects interleaved format where fulgur embeds advance adjustments
/// between CID pairs: [CID(2b)][ADJ(2b)][CID(2b)][ADJ(2b)]...
fn decode_with_cmap(bytes: &[u8], info: &FontInfo) -> String {
    let cmap = &info.cmap;
    if cmap.is_empty() {
        return bytes.iter().map(|&b| b as char).collect();
    }

    if info.encoding == CidEncoding::SingleByte {
        return bytes
            .iter()
            .map(|&b| {
                let cid = b as u16;
                cmap.get(&cid).copied().unwrap_or('\u{FFFD}')
            })
            .collect();
    }

    // Standard CID font: u16 big-endian pairs with interleaved detection
    let pairs: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();

    // Auto-detect interleaved format: if all odd-indexed pairs have
    // the same value (consistent advance), skip them as adjustments.
    let interleaved = pairs.len() >= 4 && pairs.iter().skip(1).step_by(2).all(|&v| v == pairs[1]);

    let mut result = String::new();
    for (i, &cid) in pairs.iter().enumerate() {
        if interleaved && i % 2 == 1 {
            continue;
        }
        match cmap.get(&cid) {
            Some(&ch) => result.push(ch),
            None => result.push('\u{FFFD}'),
        }
    }
    result
}

fn estimate_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.5
}

pub fn extract_unicode_text(doc: &Document) -> Result<Vec<UnicodeTextItem>> {
    use lopdf::content::{Content, Operation};
    let font_info = build_font_cmaps(doc);
    let mut items = Vec::new();

    for (&page_num, &page_id) in &doc.get_pages() {
        let content_bytes = match doc.get_page_content(page_id) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let content = match Content::decode(&content_bytes) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut gs_stack: Vec<([f32; 6], String, f32)> =
            vec![(IDENTITY, "unknown".to_string(), 12.0)];
        let mut tm_a: f32 = 1.0;
        let mut tm_b: f32 = 0.0;
        let mut tm_c: f32 = 0.0;
        let mut tm_d: f32 = 1.0;
        let mut tlm_e: f32 = 0.0;
        let mut tlm_f: f32 = 0.0;
        let mut tx: f32 = 0.0;
        let mut ty: f32 = 0.0;
        let mut font_name = String::from("unknown");
        let mut font_size: f32 = 12.0;
        let mut text_leading: f32 = 0.0;

        for Operation { operator, operands } in &content.operations {
            match operator.as_str() {
                "q" => {
                    let top = gs_stack.last().expect("gs_stack non-empty").clone();
                    gs_stack.push(top);
                }
                "Q" if gs_stack.len() > 1 => {
                    gs_stack.pop();
                    let (_, ref saved_font, saved_size) =
                        *gs_stack.last().expect("gs_stack non-empty");
                    font_name = saved_font.clone();
                    font_size = saved_size;
                }
                "cm" if operands.len() == 6 => {
                    let new_m = [
                        obj_to_f32(&operands[0]),
                        obj_to_f32(&operands[1]),
                        obj_to_f32(&operands[2]),
                        obj_to_f32(&operands[3]),
                        obj_to_f32(&operands[4]),
                        obj_to_f32(&operands[5]),
                    ];
                    if let Some(gs) = gs_stack.last_mut() {
                        gs.0 = concat_matrix(&gs.0, &new_m);
                    }
                }
                "Tf" => {
                    if let (Some(name_obj), Some(size)) = (operands.first(), operands.get(1)) {
                        font_name = obj_as_name_str(name_obj).unwrap_or("unknown").to_string();
                        font_size = obj_to_f32(size);
                        if let Some(gs) = gs_stack.last_mut() {
                            gs.1.clone_from(&font_name);
                            gs.2 = font_size;
                        }
                    }
                }
                "TL" if !operands.is_empty() => {
                    text_leading = obj_to_f32(&operands[0]);
                }
                "BT" => {
                    tm_a = 1.0;
                    tm_b = 0.0;
                    tm_c = 0.0;
                    tm_d = 1.0;
                    tlm_e = 0.0;
                    tlm_f = 0.0;
                    let ctm = gs_stack.last().map(|gs| &gs.0).unwrap_or(&IDENTITY);
                    tx = ctm[4];
                    ty = ctm[5];
                }
                "Tm" if operands.len() >= 6 => {
                    tm_a = obj_to_f32(&operands[0]);
                    tm_b = obj_to_f32(&operands[1]);
                    tm_c = obj_to_f32(&operands[2]);
                    tm_d = obj_to_f32(&operands[3]);
                    tlm_e = obj_to_f32(&operands[4]);
                    tlm_f = obj_to_f32(&operands[5]);
                    let ctm = gs_stack.last().map(|gs| &gs.0).unwrap_or(&IDENTITY);
                    tx = ctm[0] * tlm_e + ctm[2] * tlm_f + ctm[4];
                    ty = ctm[1] * tlm_e + ctm[3] * tlm_f + ctm[5];
                }
                "Td" | "TD" if operands.len() >= 2 => {
                    let dx = obj_to_f32(&operands[0]);
                    let dy = obj_to_f32(&operands[1]);
                    if operator == "TD" {
                        text_leading = -dy;
                    }
                    tlm_e += dx * tm_a + dy * tm_c;
                    tlm_f += dx * tm_b + dy * tm_d;
                    let ctm = gs_stack.last().map(|gs| &gs.0).unwrap_or(&IDENTITY);
                    tx = ctm[0] * tlm_e + ctm[2] * tlm_f + ctm[4];
                    ty = ctm[1] * tlm_e + ctm[3] * tlm_f + ctm[5];
                }
                "T*" => {
                    tlm_e += (-text_leading) * tm_c;
                    tlm_f += (-text_leading) * tm_d;
                    let ctm = gs_stack.last().map(|gs| &gs.0).unwrap_or(&IDENTITY);
                    tx = ctm[0] * tlm_e + ctm[2] * tlm_f + ctm[4];
                    ty = ctm[1] * tlm_e + ctm[3] * tlm_f + ctm[5];
                }
                "Tj" => {
                    if let Some(text_obj) = operands.first()
                        && let Ok(bytes) = text_obj.as_str()
                    {
                        let cmap = font_info.get(&font_name);
                        let text = if let Some(c) = cmap {
                            decode_with_cmap(bytes, c)
                        } else {
                            bytes.iter().map(|&b| b as char).collect()
                        };
                        if !text.trim().is_empty() {
                            let w = estimate_width(&text, font_size);
                            items.push(UnicodeTextItem {
                                page: page_num,
                                x: tx,
                                y: ty,
                                width: w,
                                height: font_size,
                                text,
                                font: font_name.clone(),
                                font_size,
                            });
                            tx += w;
                        }
                    }
                }
                "TJ" => {
                    if let Some(array_obj) = operands.first()
                        && let Ok(array) = array_obj.as_array()
                    {
                        let cmap = font_info.get(&font_name);
                        let mut combined = String::new();
                        for elem in array {
                            if let Ok(bytes) = elem.as_str() {
                                if let Some(c) = cmap {
                                    combined.push_str(&decode_with_cmap(bytes, c));
                                } else {
                                    combined.extend(bytes.iter().map(|&b| b as char));
                                }
                            }
                        }
                        if !combined.trim().is_empty() {
                            let w = estimate_width(&combined, font_size);
                            items.push(UnicodeTextItem {
                                page: page_num,
                                x: tx,
                                y: ty,
                                width: w,
                                height: font_size,
                                text: combined,
                                font: font_name.clone(),
                                font_size,
                            });
                            tx += w;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(items)
}

// ── SVG generation ─────────────────────────────────────────────────────

/// Render PDF bytes to SVG with Unicode text (ToUnicode CMap decoding).
pub fn svg_unicode(pdf_bytes: &[u8], page: u32) -> Result<String> {
    let tmp = tempfile::NamedTempFile::new()?;
    let path = tmp.path().to_path_buf();
    std::fs::write(&path, pdf_bytes)?;
    let doc = Document::load(&path).map_err(|e| anyhow::anyhow!("Failed to load PDF: {e}"))?;
    let items = extract_unicode_text(&doc)?;

    // Get page size from the first page's MediaBox
    let w = get_page_width(&doc, page);
    let h = get_page_height(&doc, page);

    let mut svg = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
<rect width="{w}" height="{h}" fill="white"/>
"#
    );

    for item in &items {
        if item.page != page {
            continue;
        }
        let escaped = item
            .text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        svg.push_str(&format!(
            r#"<text x="{x}" y="{y}" font-family="{font}" font-size="{size}" fill="black">{text}</text>
"#,
            x = item.x,
            y = item.y + item.font_size * 0.8,
            font = item.font,
            size = item.font_size,
            text = escaped,
        ));
    }
    svg.push_str("</svg>\n");
    Ok(svg)
}

/// Get the total number of pages in a PDF.
pub fn page_count(pdf_bytes: &[u8]) -> Result<u32> {
    let tmp = tempfile::NamedTempFile::new()?;
    let path = tmp.path().to_path_buf();
    std::fs::write(&path, pdf_bytes)?;
    let doc = Document::load(&path).map_err(|e| anyhow::anyhow!("Failed to load PDF: {e}"))?;
    Ok(doc.get_pages().len() as u32)
}

fn get_page_width(doc: &Document, page_num: u32) -> f32 {
    for (&pn, &page_id) in &doc.get_pages() {
        if pn != page_num {
            continue;
        }
        if let Ok(Object::Dictionary(d)) = doc.get_object(page_id)
            && let Ok(bbox) = d.get(b"MediaBox")
            && let Ok(arr) = bbox.as_array()
            && arr.len() >= 4
        {
            return obj_to_f32(&arr[2]) - obj_to_f32(&arr[0]);
        }
    }
    595.0 // A4 default
}

fn get_page_height(doc: &Document, page_num: u32) -> f32 {
    for (&pn, &page_id) in &doc.get_pages() {
        if pn != page_num {
            continue;
        }
        if let Ok(Object::Dictionary(d)) = doc.get_object(page_id)
            && let Ok(bbox) = d.get(b"MediaBox")
            && let Ok(arr) = bbox.as_array()
            && arr.len() >= 4
        {
            return obj_to_f32(&arr[3]) - obj_to_f32(&arr[1]);
        }
    }
    842.0
}
