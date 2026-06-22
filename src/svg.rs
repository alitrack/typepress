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
    hex_to_u32(s).and_then(char::from_u32)
}

// ── Font CMap resolution ───────────────────────────────────────────────

/// Build a mapping from font name → CID→char map by parsing all ToUnicode
/// CMaps in the PDF's font objects.
fn build_font_cmaps(doc: &Document) -> BTreeMap<String, CidMap> {
    let mut font_cmaps: BTreeMap<String, CidMap> = BTreeMap::new();

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
            if font_cmaps.contains_key(&name_str) {
                continue;
            }
            let font_dict = match doc.dereference(font_ref) {
                Ok((_, Object::Dictionary(d))) => d,
                _ => continue,
            };
            // Try ToUnicode CMap
            if let Ok(tu) = font_dict.get(b"ToUnicode") {
                if let Ok((_, Object::Stream(stream))) = doc.dereference(tu) {
                    let cmap = parse_tounicode_cmap(&stream.content);
                    font_cmaps.insert(name_str, cmap);
                }
            }
        }
        let _ = page_num; // suppress unused warning
    }

    font_cmaps
}

fn resolve_page_resources(doc: &Document, page_id: lopdf::ObjectId) -> Option<lopdf::Dictionary> {
    let mut current_id = page_id;
    loop {
        let dict = match doc.get_object(current_id) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return None,
        };
        if let Ok(res) = dict.get(b"Resources") {
            if let Ok((_, Object::Dictionary(resources))) = doc.dereference(res) {
                return Some(resources.clone());
            }
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
    pub width: f32,
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
/// For CID fonts, the bytes are raw glyph IDs (big-endian u16 pairs).
/// For non-CID fonts, falls back to Latin-1.
fn decode_with_cmap(bytes: &[u8], cmap: &CidMap) -> String {
    if cmap.is_empty() {
        // No CMap → Latin-1 fallback
        return bytes.iter().map(|&b| b as char).collect();
    }
    // CID font: each glyph is a 2-byte big-endian CID
    let mut result = String::new();
    for chunk in bytes.chunks(2) {
        if chunk.len() == 2 {
            let cid = u16::from_be_bytes([chunk[0], chunk[1]]);
            if let Some(&ch) = cmap.get(&cid) {
                result.push(ch);
            } else {
                // Unknown CID → replacement character
                result.push('\u{FFFD}');
            }
        }
    }
    result
}

fn estimate_width(text: &str, font_size: f32) -> f32 {
    text.chars().count() as f32 * font_size * 0.5
}

pub fn extract_unicode_text(doc: &Document) -> Result<Vec<UnicodeTextItem>> {
    use lopdf::content::{Content, Operation};
    let font_cmaps = build_font_cmaps(doc);
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
                    if let Some(text_obj) = operands.first() {
                        if let Ok(bytes) = text_obj.as_str() {
                            let cmap = font_cmaps.get(&font_name);
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
                }
                "TJ" => {
                    if let Some(array_obj) = operands.first() {
                        if let Ok(array) = array_obj.as_array() {
                            let cmap = font_cmaps.get(&font_name);
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

fn get_page_width(doc: &Document, page_num: u32) -> f32 {
    for (&pn, &page_id) in &doc.get_pages() {
        if pn != page_num {
            continue;
        }
        if let Ok(Object::Dictionary(d)) = doc.get_object(page_id) {
            if let Ok(bbox) = d.get(b"MediaBox") {
                if let Ok(arr) = bbox.as_array() {
                    if arr.len() >= 4 {
                        return obj_to_f32(&arr[2]) - obj_to_f32(&arr[0]);
                    }
                }
            }
        }
    }
    595.0 // A4 default
}

fn get_page_height(doc: &Document, page_num: u32) -> f32 {
    for (&pn, &page_id) in &doc.get_pages() {
        if pn != page_num {
            continue;
        }
        if let Ok(Object::Dictionary(d)) = doc.get_object(page_id) {
            if let Ok(bbox) = d.get(b"MediaBox") {
                if let Ok(arr) = bbox.as_array() {
                    if arr.len() >= 4 {
                        return obj_to_f32(&arr[3]) - obj_to_f32(&arr[1]);
                    }
                }
            }
        }
    }
    842.0
}
