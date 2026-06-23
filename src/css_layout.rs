// CSS layout preprocessor for TypePress.
//
// Converts CSS Grid and Flexbox layouts to <table>-based layouts
// that taffy (the layout engine inside fulgur) can render correctly.
// Also degrades CSS gradients to solid colors.
//
// This runs BEFORE the HTML enters fulgur's rendering pipeline.

use regex::Regex;

/// Process CSS layout: Grid→Table, Flexbox→Table, Gradient→Solid.
/// Returns the transformed HTML string.
pub fn process_css_layout(html: &str) -> String {
    let mut result = html.to_string();

    // Phase 0: Protect SVG elements from being munged by table conversion
    let (protected, svg_map) = protect_svg_elements(&result);
    result = protected;

    // Phase 1: Gradient → Solid color degradation
    result = degrade_gradients(&result);

    // Phase 2: Grid → Table conversion
    result = convert_grid_to_table(&result);

    // Phase 3: Flexbox → Table conversion
    result = convert_flexbox_to_table(&result);

    // Phase 4: Restore SVG elements
    result = restore_svg_elements(&result, &svg_map);

    result
}

// ── SVG Protection ──────────────────────────────────────────────────────

struct SvgBlock {
    placeholder: String,
    content: String,
}

fn protect_svg_elements(html: &str) -> (String, Vec<SvgBlock>) {
    let mut blocks = Vec::new();
    let mut result = String::with_capacity(html.len());
    let mut i = 0;
    let bytes = html.as_bytes();
    let len = bytes.len();
    let svg_open = b"<svg";
    let svg_close = b"</svg>";

    while i < len {
        if i + 4 < len && &bytes[i..i + 4] == svg_open {
            let start = i;
            let mut depth = 1;
            i += 4;
            // Find matching </svg>
            while i < len && depth > 0 {
                if i + 6 < len && &bytes[i..i + 6] == svg_close {
                    depth -= 1;
                    if depth == 0 {
                        i += 6;
                        break;
                    }
                    i += 6;
                } else if i + 4 < len && &bytes[i..i + 4] == svg_open {
                    depth += 1;
                    i += 4;
                } else {
                    i += 1;
                }
            }
            let content = &html[start..i];
            let placeholder = format!("\x00TXPSVG{}\x00", blocks.len());
            blocks.push(SvgBlock {
                placeholder: placeholder.clone(),
                content: content.to_string(),
            });
            result.push_str(&placeholder);
        } else {
            result.push(html[i..].chars().next().unwrap_or('\0'));
            i += html[i..].chars().next().map_or(1, |c| c.len_utf8());
        }
    }
    (result, blocks)
}

fn restore_svg_elements(html: &str, svg_map: &[SvgBlock]) -> String {
    let mut result = html.to_string();
    for block in svg_map {
        result = result.replace(&block.placeholder, &block.content);
    }
    result
}

// ── Gradient Degradation ────────────────────────────────────────────────

/// Find the matching closing paren position for `linear-gradient(` at `open_pos`.
fn find_gradient_close(html: &str, open_pos: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut depth = 0;
    let mut i = open_pos;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn degrade_gradients(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let bytes = html.as_bytes();
    let mut i = 0;

    // Color regex for extracting first color from gradient args
    let color_re = Regex::new(r#"#[0-9a-fA-F]{3,8}|rgba?\s*\([^)]+\)|hsla?\s*\([^)]+\)"#).unwrap();

    while i < bytes.len() {
        // Look for gradient function calls
        let remaining = &html[i..];
        let lower = remaining.to_lowercase();

        let grad_start = lower
            .find("linear-gradient(")
            .or_else(|| lower.find("radial-gradient("))
            .or_else(|| lower.find("conic-gradient("));

        if let Some(offset) = grad_start {
            // Copy everything before the gradient
            result.push_str(&html[i..i + offset]);

            let func_start = i + offset;
            // Find the `(` after the function name
            let paren_pos = html[func_start..]
                .find('(')
                .map(|p| func_start + p)
                .unwrap_or(func_start);

            if let Some(close_pos) = find_gradient_close(html, paren_pos) {
                let grad_args = &html[paren_pos + 1..close_pos];

                // Extract first color
                let first_color = color_re
                    .find(grad_args)
                    .map(|m| m.as_str())
                    .unwrap_or("inherit");

                // Check what CSS property this gradient was attached to
                // Look backwards for 'background' or just 'color'
                let prefix = if html[..func_start]
                    .trim_end()
                    .ends_with("background-clip: text")
                    || html[..func_start]
                        .trim_end()
                        .ends_with("-webkit-background-clip: text")
                {
                    "color"
                } else {
                    "background"
                };

                result.push_str(&format!("{}: {} /* gradient→solid */", prefix, first_color));
                i = close_pos + 1;
            } else {
                // Unclosed paren — copy as-is
                result.push_str(&html[i..i + offset + "linear-gradient(".len()]);
                i = paren_pos + 1;
            }
        } else {
            // No more gradients
            result.push_str(&html[i..]);
            break;
        }
    }

    // Clean up leftover -webkit-background-clip: text (now useless since gradient removed)
    let clip_re = Regex::new(r"(?i)(?:-webkit-)?background-clip\s*:\s*text\s*;?").unwrap();
    let result = clip_re.replace_all(&result, "").to_string();

    result
}

// ── Grid → Table Conversion ─────────────────────────────────────────────

#[derive(Debug)]
struct GridRule {
    selector: String,
    columns: Vec<String>, // e.g. ["280px", "1fr"]
    gap: Option<String>,
}

fn parse_grid_rules(html: &str) -> Vec<GridRule> {
    let mut rules = Vec::new();

    // Find all <style> blocks
    let style_re = Regex::new(r"(?s)<style[^>]*>(.*?)</style>").unwrap();
    for style_caps in style_re.captures_iter(html) {
        let css = &style_caps[1];

        // Find CSS rules with display: grid
        let rule_re = Regex::new(r"(?s)([^{]+)\s*\{\s*([^}]*)\}").unwrap();
        for rule_caps in rule_re.captures_iter(css) {
            let selectors = rule_caps[1].trim();
            let body = &rule_caps[2];

            if !body.contains("display:grid") && !body.contains("display: grid") {
                continue;
            }

            // Extract grid-template-columns
            let cols_re = Regex::new(r"grid-template-columns\s*:\s*([^;]+)").unwrap();
            let columns: Vec<String> = if let Some(cols_caps) = cols_re.captures(body) {
                cols_caps[1]
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                continue; // Need columns for table conversion
            };

            // Extract gap
            let gap_re = Regex::new(r"gap\s*:\s*([^;]+)").unwrap();
            let gap = gap_re.captures(body).map(|c| c[1].trim().to_string());

            rules.push(GridRule {
                selector: selectors.to_string(),
                columns,
                gap,
            });
        }
    }

    rules
}

fn convert_grid_to_table(html: &str) -> String {
    let grid_rules = parse_grid_rules(html);
    if grid_rules.is_empty() {
        return html.to_string();
    }

    let mut result = html.to_string();

    for rule in &grid_rules {
        // Build column widths
        let col_widths: Vec<String> = rule
            .columns
            .iter()
            .map(|c| {
                if c == "1fr" || c.ends_with("fr") {
                    "auto".to_string()
                } else {
                    c.clone()
                }
            })
            .collect();

        let table_style = if let Some(ref gap) = rule.gap {
            format!("border-collapse:separate;border-spacing:{};width:100%", gap)
        } else {
            "border-collapse:collapse;width:100%".to_string()
        };

        // Match the grid container in HTML
        let class_re = Regex::new(r"\.([\w-]+)").unwrap();
        if let Some(class_caps) = class_re.captures(&rule.selector) {
            let class_name = &class_caps[1];
            result = convert_grid_div_to_table(&result, class_name, &col_widths, &table_style);
        }
    }

    // Strip display:grid from <style> blocks (already converted)
    let grid_display_re = Regex::new(r"(?m)^\s*display\s*:\s*grid\s*;?\s*$").unwrap();
    result = grid_display_re.replace_all(&result, "").to_string();

    // Also strip grid-template-columns and gap (already handled by table)
    let grid_cols_re = Regex::new(r"(?m)^\s*grid-template-columns\s*:[^;]*;\s*$").unwrap();
    result = grid_cols_re.replace_all(&result, "").to_string();

    result
}

fn convert_grid_div_to_table(
    html: &str,
    class_name: &str,
    col_widths: &[String],
    table_style: &str,
) -> String {
    let n_cols = col_widths.len();
    if n_cols == 0 {
        return html.to_string();
    }

    // Build a regex to find <div class="...class_name...">
    let open_re = Regex::new(&format!(
        r#"<div\b[^>]*\bclass\s*=\s*"[^"]*\b{}\b[^"]*"[^>]*>"#,
        regex::escape(class_name)
    ))
    .unwrap();

    let mut result = String::new();

    // Process each match
    let matches: Vec<_> = open_re.find_iter(html).collect();

    if matches.is_empty() {
        return html.to_string();
    }

    let mut offset = 0;
    let html_bytes = html.as_bytes();

    while offset < html_bytes.len() {
        if let Some(m) = open_re.find_at(html, offset) {
            // Copy everything before this match
            result.push_str(&html[offset..m.start()]);

            // Extract the opening tag
            let open_tag = &html[m.start()..m.end()];
            // Copy the opening tag as-is
            result.push_str(open_tag);

            // Now find the matching closing </div>
            let body_start = m.end();
            let close_pos = find_matching_close_div(html, m.start());

            if let Some(body_end) = close_pos {
                // Extract body content and split into direct children
                let body = &html[body_start..body_end];
                let children = extract_direct_children_divs(body);

                // Build table
                result.push('\n');
                result.push_str(&format!(
                    "<table class=\"txp-grid\" style=\"{}\">\n",
                    table_style
                ));

                // Distribute children across columns
                let mut col = 0;
                result.push_str("<tr>\n");
                for child in &children {
                    if col > 0 && col % n_cols == 0 {
                        result.push_str("</tr>\n<tr>\n");
                    }
                    let col_style = if col_widths[col % n_cols] == "auto" {
                        String::new()
                    } else {
                        format!("width:{}", col_widths[col % n_cols])
                    };
                    let td_attrs = if col_style.is_empty() {
                        String::new()
                    } else {
                        format!(" style=\"{}\"", col_style)
                    };
                    result.push_str(&format!("<td{}>{}</td>\n", td_attrs, child));
                    col += 1;
                }
                // Fill remaining cells in row
                while col % n_cols != 0 {
                    result.push_str("<td></td>\n");
                    col += 1;
                }
                result.push_str("</tr>\n");
                result.push_str("</table>\n");

                // Copy closing div
                result.push_str("</div>");

                offset = body_end + 6; // skip past </div>
            } else {
                // No matching close found, copy as-is
                result.push_str(open_tag);
                offset = m.end();
            }
        } else {
            result.push_str(&html[offset..]);
            break;
        }
    }

    result
}

/// Find matching </div> tag given the position of the opening <div>
fn find_matching_close_div(html: &str, open_pos: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut depth = 0;
    let mut i = open_pos;
    let len = bytes.len();

    while i < len {
        if i + 4 <= len && &bytes[i..i + 4] == b"<div" {
            if i + 4 >= len || bytes[i + 4].is_ascii_whitespace() || bytes[i + 4] == b'>' {
                depth += 1;
            }
        }
        if i + 6 <= len && &bytes[i..i + 6] == b"</div>" {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

/// Extract direct child <div> elements from within a container.
/// Naive approach: find top-level <div>...</div> blocks.
fn extract_direct_children_divs(body: &str) -> Vec<String> {
    let mut children = Vec::new();
    let bytes = body.as_bytes();
    let mut i = 0;
    let len = bytes.len();

    while i < len {
        // Skip whitespace
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        // Check for <div or other element starts
        if i + 4 < len && &bytes[i..i + 4] == b"<div" {
            let start = i;
            let mut depth = 0;
            while i < len {
                if i + 4 < len
                    && &bytes[i..i + 4] == b"<div"
                    && (i + 4 >= len || bytes[i + 4].is_ascii_whitespace() || bytes[i + 4] == b'>')
                {
                    depth += 1;
                }
                if i + 6 < len && &bytes[i..i + 6] == b"</div>" {
                    depth -= 1;
                    if depth == 0 {
                        i += 6;
                        break;
                    }
                }
                i += 1;
            }
            children.push(body[start..i].to_string());
        } else if bytes[i] == b'<' {
            // Some other HTML element — skip past it
            while i < len && bytes[i] != b'>' {
                i += 1;
            }
            if i < len {
                i += 1;
            } // skip past >
        } else {
            // Text node
            let start = i;
            while i < len && bytes[i] != b'<' {
                i += 1;
            }
            let text = body[start..i].trim();
            if !text.is_empty() {
                children.push(text.to_string());
            }
        }
    }

    children
}

// ── Flexbox → Table Conversion ──────────────────────────────────────────

struct FlexRule {
    selector: String,
    gap: Option<String>,
    wrap: bool,
    align: Option<String>,
    justify: Option<String>,
}

fn parse_flex_rules(html: &str) -> Vec<FlexRule> {
    let mut rules = Vec::new();

    let style_re = Regex::new(r"(?s)<style[^>]*>(.*?)</style>").unwrap();
    for style_caps in style_re.captures_iter(html) {
        let css = &style_caps[1];

        let rule_re = Regex::new(r"(?s)([^{]+)\s*\{\s*([^}]*)\}").unwrap();
        for rule_caps in rule_re.captures_iter(css) {
            let selectors = rule_caps[1].trim();
            let body = &rule_caps[2];

            let has_flex = Regex::new(r"display\s*:\s*flex").unwrap().is_match(body);
            if !has_flex {
                continue;
            }

            let gap_re = Regex::new(r"gap\s*:\s*([^;]+)").unwrap();
            let gap = gap_re.captures(body).map(|c| c[1].trim().to_string());

            let wrap = body.contains("flex-wrap:wrap") || body.contains("flex-wrap: wrap");

            let align_re = Regex::new(r"align-items\s*:\s*([^;]+)").unwrap();
            let align = align_re.captures(body).map(|c| c[1].trim().to_string());

            let justify_re = Regex::new(r"justify-content\s*:\s*([^;]+)").unwrap();
            let justify = justify_re.captures(body).map(|c| c[1].trim().to_string());

            rules.push(FlexRule {
                selector: selectors.to_string(),
                gap,
                wrap,
                align,
                justify,
            });
        }
    }

    rules
}

fn convert_flexbox_to_table(html: &str) -> String {
    let flex_rules = parse_flex_rules(html);
    if flex_rules.is_empty() {
        return html.to_string();
    }

    let mut result = html.to_string();

    for rule in &flex_rules {
        let class_re = Regex::new(r"\.([\w-]+)").unwrap();
        if let Some(class_caps) = class_re.captures(&rule.selector) {
            let class_name = &class_caps[1];

            let table_style = build_flex_table_style(rule);
            let td_style = build_flex_td_style(rule);

            result =
                convert_flex_div_to_table(&result, class_name, &table_style, &td_style, rule.wrap);
        }
    }

    // Strip display:flex and related properties from <style> blocks
    let flex_display_re = Regex::new(r"(?m)^\s*display\s*:\s*flex\s*;?\s*$").unwrap();
    result = flex_display_re.replace_all(&result, "").to_string();
    let flex_wrap_re = Regex::new(r"(?m)^\s*flex-wrap\s*:[^;]*;\s*$").unwrap();
    result = flex_wrap_re.replace_all(&result, "").to_string();

    result
}

fn build_flex_table_style(rule: &FlexRule) -> String {
    let mut styles = vec!["border-collapse:separate".to_string()];
    if let Some(ref gap) = rule.gap {
        styles.push(format!("border-spacing:{}", gap));
    }
    styles.push("width:100%".to_string());
    if let Some(ref justify) = rule.justify {
        if justify == "center" {
            styles.push("margin:0 auto".to_string());
        }
    }
    styles.join(";")
}

fn build_flex_td_style(rule: &FlexRule) -> String {
    let mut styles = vec!["padding:0".to_string()];
    if let Some(ref align) = rule.align {
        match align.as_str() {
            "center" => styles.push("vertical-align:middle".to_string()),
            "flex-start" | "start" => styles.push("vertical-align:top".to_string()),
            "flex-end" | "end" => styles.push("vertical-align:bottom".to_string()),
            _ => {}
        }
    }
    styles.join(";")
}

fn convert_flex_div_to_table(
    html: &str,
    class_name: &str,
    table_style: &str,
    td_style: &str,
    wrap: bool,
) -> String {
    let open_re = Regex::new(&format!(
        r#"<div\b[^>]*\bclass\s*=\s*"[^"]*\b{}\b[^"]*"[^>]*>"#,
        regex::escape(class_name)
    ))
    .unwrap();

    let mut result = String::new();
    let matches: Vec<_> = open_re.find_iter(html).collect();

    if matches.is_empty() {
        return html.to_string();
    }

    let mut offset = 0;

    while offset < html.len() {
        if let Some(m) = open_re.find_at(html, offset) {
            result.push_str(&html[offset..m.start()]);
            result.push_str(&html[m.start()..m.end()]);

            let body_start = m.end();
            let close_pos = find_matching_close_div(html, m.start());

            if let Some(body_end) = close_pos {
                let body = &html[body_start..body_end];
                let children = extract_direct_children_divs(body);

                result.push('\n');
                result.push_str(&format!(
                    "<table class=\"txp-flex\" style=\"{}\">\n",
                    table_style
                ));

                // For nowrap: single row, one child per cell
                // For wrap: try to intelligently group (default: max 6 per row)
                let cols = if wrap { 6 } else { children.len().max(1) };

                let mut col = 0;
                result.push_str("<tr>\n");
                for child in &children {
                    if col > 0 && col % cols == 0 {
                        result.push_str("</tr>\n<tr>\n");
                    }
                    result.push_str(&format!("<td style=\"{}\">{}</td>\n", td_style, child));
                    col += 1;
                }
                while col % cols != 0 {
                    result.push_str(&format!("<td style=\"{}\"></td>\n", td_style));
                    col += 1;
                }
                result.push_str("</tr>\n");
                result.push_str("</table>\n");

                result.push_str("</div>");
                offset = body_end + 6;
            } else {
                offset = m.end();
            }
        } else {
            result.push_str(&html[offset..]);
            break;
        }
    }

    result
}

/// Uniformly scale all px-based CSS values in HTML to fit content on fewer pages.
/// Returns (new_html, scale_factor).
pub fn scale_css_for_fit(html: &str, scale: f64) -> String {
    let re = Regex::new(r"(\d+(?:\.\d+)?)px").unwrap();
    re.replace_all(html, |caps: &regex::Captures| {
        let val: f64 = caps[1].parse().unwrap_or(0.0);
        let new_val = (val * scale).round().max(1.0) as u64;
        format!("{new_val}px")
    })
    .into_owned()
}

/// Count pages in a PDF byte buffer.
pub fn count_pdf_pages(pdf: &[u8]) -> usize {
    // Count /Type /Page minus /Type /Pages
    let text = String::from_utf8_lossy(pdf);
    let re = regex::Regex::new(r"/Type\s*/Page[^s]").unwrap();
    re.find_iter(&text).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_degrade_gradients_simple() {
        let input = r#"background: linear-gradient(135deg, #0066cc, #7c3aed);"#;
        let output = degrade_gradients(input);
        assert!(output.contains("#0066cc"));
        assert!(!output.contains("linear-gradient"));
    }

    #[test]
    fn test_degrade_gradients_webkit_clip() {
        let input = r#"background: linear-gradient(135deg, var(--blue), var(--purple));
-webkit-background-clip: text;"#;
        let output = degrade_gradients(input);
        assert!(!output.contains("linear-gradient"));
    }

    #[test]
    fn test_protect_svg() {
        let input = r#"<div class="grid"><svg xmlns="http://www.w3.org/2000/svg"><rect x="0" y="0" width="10" height="10"/></svg></div>"#;
        let (protected, map) = protect_svg_elements(input);
        assert!(protected.contains("TXPSVG"));
        assert!(!protected.contains("<svg"));
        let restored = restore_svg_elements(&protected, &map);
        assert_eq!(restored, input);
    }

    #[test]
    fn test_parse_grid_rules() {
        let html = r#"<style>
.main-grid {
    display: grid;
    grid-template-columns: 280px 1fr;
    gap: 24px;
}
</style>"#;
        let rules = parse_grid_rules(html);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].selector.trim(), ".main-grid");
        assert_eq!(rules[0].columns, vec!["280px", "1fr"]);
        assert_eq!(rules[0].gap.as_deref(), Some("24px"));
    }

    #[test]
    fn test_parse_flex_rules() {
        let html = r#"<style>
.arch-nodes {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
}
</style>"#;
        let rules = parse_flex_rules(html);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].wrap);
        assert_eq!(rules[0].gap.as_deref(), Some("8px"));
    }

    #[test]
    fn test_find_matching_close_div() {
        let html = r#"<div class="outer"><div class="inner">hello</div></div>"#;
        let pos = find_matching_close_div(html, 0);
        assert!(pos.is_some());
        assert_eq!(&html[pos.unwrap()..pos.unwrap() + 6], "</div>");
    }

    #[test]
    fn test_full_layout_conversion() {
        let html = include_str!("../templates/agent-knowledge-map.html");
        let output = process_css_layout(html);
        assert!(output.len() > 0);
        assert!(output.matches("<table").count() > 5);
        // No hang, no panic, reasonable table count
    }
}
