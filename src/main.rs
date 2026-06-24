// TypePress — Pure Rust HTML/CSS → PDF engine.
// Uses fulgur (Blitz→Taffy→Krilla) as the rendering backend.
//
// Key additions over vanilla fulgur:
//   - --header / --footer CLI shortcuts (CSS GCPM running elements)
//   - --math auto-detection (katex-rs rendering + KaTeX font loading)
//   - CJK font handling with automatic subsetting

use anyhow::{Context, Result};
use clap::Parser;
use fulgur::asset::AssetBundle;
use fulgur::config::{Margin, PageSize};
use fulgur::engine::Engine;
use regex::Regex;
use std::path::{Path, PathBuf};

mod config;
mod fonts;
mod svg;
use config::TypePressConfig;
use typepress::css::KATEX_CSS;
use typepress::{inject_header_footer, markdown_to_html};

// ── CLI ────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "typepress",
    version,
    about = "Pure Rust HTML/CSS → PDF engine\n\nwkhtmltopdf compatible — use as a drop-in replacement."
)]
struct Cli {
    /// Input HTML file (omit for --stdin)
    input: Option<PathBuf>,
    /// Read HTML from stdin
    #[arg(long)]
    stdin: bool,
    /// Output PDF file path (use "-" for stdout). Required in CLI mode, optional with --config.
    #[arg(short, long)]
    output: Option<PathBuf>,
    // Input format
    #[arg(long = "from", default_value = "html")]
    from: String,
    // Output format
    #[arg(long = "format", short = 'F', default_value = "pdf")]
    format: String,
    #[arg(long, default_value = "2.0")]
    scale: f32,
    // Config
    #[arg(short = 'c', long)]
    config: Option<PathBuf>,
    // Page: size
    #[arg(short = 's', long, alias = "page-size")]
    size: Option<String>,
    #[arg(long = "page-width", conflicts_with = "size")]
    page_width: Option<String>,
    #[arg(long = "page-height", conflicts_with = "size")]
    page_height: Option<String>,
    #[arg(short = 'l', long)]
    landscape: bool,
    #[arg(short = 'O', long)]
    orientation: Option<String>,
    // Page: margins (wkhtmltopdf compat)
    #[arg(long = "margin")]
    margin: Option<String>,
    #[arg(short = 'T', long = "margin-top")]
    margin_top: Option<String>,
    #[arg(short = 'B', long = "margin-bottom")]
    margin_bottom: Option<String>,
    #[arg(short = 'L', long = "margin-left")]
    margin_left: Option<String>,
    #[arg(short = 'R', long = "margin-right")]
    margin_right: Option<String>,
    // Zoom
    #[arg(long = "zoom", default_value = "1.0")]
    zoom: f32,
    #[arg(long)]
    fit: bool,
    // Metadata
    #[arg(long)]
    title: Option<String>,
    #[arg(long = "author")]
    authors: Vec<String>,
    #[arg(long)]
    language: Option<String>,
    // Assets
    #[arg(long = "font", short = 'f')]
    fonts: Vec<PathBuf>,
    #[arg(long = "css", alias = "user-style-sheet")]
    css_files: Vec<PathBuf>,
    // Headers & Footers
    #[arg(long = "header", alias = "header-html")]
    header: Option<String>,
    #[arg(long = "footer", alias = "footer-html")]
    footer: Option<String>,
    // wkhtmltopdf compat no-ops
    #[arg(long, hide = true)]
    encoding: Option<String>,
    #[arg(long = "no-outline")]
    no_outline: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
    // Math
    #[arg(long)]
    math: bool,
    #[arg(long = "math-dir")]
    math_dir: Option<PathBuf>,
    // PDF features
    #[arg(long)]
    bookmarks: bool,
    #[arg(long)]
    tagged: bool,
    #[arg(long = "pdf-ua")]
    pdf_ua: bool,
    #[arg(long = "no-system-fonts")]
    no_system_fonts: bool,
}

impl Cli {
    fn resolve_margin(&self) -> Option<Margin> {
        let has_side = self.margin_top.is_some()
            || self.margin_bottom.is_some()
            || self.margin_left.is_some()
            || self.margin_right.is_some();
        if has_side {
            let top = self
                .margin_top
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(20.0);
            let bottom = self
                .margin_bottom
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(20.0);
            let left = self
                .margin_left
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(10.0);
            let right = self
                .margin_right
                .as_deref()
                .and_then(|s| parse_length_mm(s).ok())
                .unwrap_or(10.0);
            let to_pt = |mm: f32| mm * 72.0 / 25.4;
            return Some(Margin {
                top: to_pt(top),
                bottom: to_pt(bottom),
                left: to_pt(left),
                right: to_pt(right),
            });
        }
        self.margin.as_deref().map(parse_margin)
    }
    fn resolve_landscape(&self) -> bool {
        if let Some(ref o) = self.orientation {
            o.eq_ignore_ascii_case("landscape")
        } else {
            self.landscape
        }
    }
    fn resolve_size(&self) -> Option<String> {
        if let (Some(w), Some(h)) = (self.page_width.as_ref(), self.page_height.as_ref()) {
            Some(format!("{} {}", w, h))
        } else {
            self.size.clone()
        }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

fn parse_page_size(s: &str) -> PageSize {
    match s.to_uppercase().as_str() {
        "A4" => PageSize::A4,
        "A3" => PageSize::A3,
        "LETTER" => PageSize::LETTER,
        _ => {
            // Try custom WxH in mm: "594x420"
            if let Some((w, h)) = s.split_once('x')
                && let (Ok(w), Ok(h)) = (w.trim().parse::<f32>(), h.trim().parse::<f32>())
                && w > 0.0
                && h > 0.0
            {
                return PageSize::custom(w, h);
            }
            eprintln!("Unknown page size '{s}', defaulting to A4");
            PageSize::A4
        }
    }
}

fn parse_margin(s: &str) -> Margin {
    let values: Vec<std::result::Result<f32, _>> =
        s.split_whitespace().map(parse_length_mm).collect();
    if values.is_empty() || values.iter().all(|v| v.is_err()) {
        return Margin::default();
    }
    let values: Vec<f32> = values.into_iter().map(|v| v.unwrap_or(20.0)).collect();
    let to_pt = |mm: f32| mm * 72.0 / 25.4;
    match values.as_slice() {
        [all] => Margin::uniform(to_pt(*all)),
        [vert, horiz] => Margin::symmetric(to_pt(*vert), to_pt(*horiz)),
        [top, horiz, bottom] => Margin {
            top: to_pt(*top),
            right: to_pt(*horiz),
            bottom: to_pt(*bottom),
            left: to_pt(*horiz),
        },
        [top, right, bottom, left] => Margin {
            top: to_pt(*top),
            right: to_pt(*right),
            bottom: to_pt(*bottom),
            left: to_pt(*left),
        },
        _ => Margin::default(),
    }
}

/// Parse a CSS length to millimeters. Supports mm, cm, in, pt, px suffixes.
/// Plain numbers are treated as mm.
fn parse_length_mm(s: &str) -> std::result::Result<f32, &'static str> {
    let s = s.trim();
    if let Some(val) = s.strip_suffix("mm") {
        return val.trim().parse::<f32>().map_err(|_| "invalid mm");
    }
    if let Some(val) = s.strip_suffix("cm") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 10.0)
            .map_err(|_| "invalid cm");
    }
    if let Some(val) = s.strip_suffix("in") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 25.4)
            .map_err(|_| "invalid in");
    }
    if let Some(val) = s.strip_suffix("pt") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 25.4 / 72.0)
            .map_err(|_| "invalid pt");
    }
    if let Some(val) = s.strip_suffix("px") {
        return val
            .trim()
            .parse::<f32>()
            .map(|v| v * 25.4 / 96.0)
            .map_err(|_| "invalid px");
    }
    // Plain number → treat as mm
    s.parse::<f32>().map_err(|_| "invalid number")
}

// ── Markdown Processing ────────────────────────────────────────────────

// ── Math Processing ────────────────────────────────────────────────────

const ESCAPED_PLACEHOLDER: &str = "\x00TXP_ESC_DOLLAR\x00";

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn html_dom_children_to_math_html(children: &[katex::dom_tree::HtmlDomNode]) -> String {
    let mut html = String::new();
    for child in children {
        html.push_str(&html_dom_to_math_html(child));
    }
    html
}

fn html_dom_to_math_html(node: &katex::dom_tree::HtmlDomNode) -> String {
    use katex::dom_tree::HtmlDomNode;

    match node {
        HtmlDomNode::MathML(math) => math_node_to_html(math),
        HtmlDomNode::DomSpan(span) => html_dom_children_to_math_html(&span.children),
        HtmlDomNode::Fragment(fragment) => html_dom_children_to_math_html(&fragment.children),
        HtmlDomNode::Symbol(symbol) => escape_html(&symbol.text),
        _ => String::new(),
    }
}

fn math_children_to_plain_text(children: &[katex::mathml_tree::MathDomNode]) -> Option<String> {
    let mut text = String::new();
    for child in children {
        text.push_str(&math_dom_to_plain_text(child)?);
    }
    Some(text)
}

fn math_children_to_html(children: &[katex::mathml_tree::MathDomNode]) -> String {
    let mut html = String::new();
    for child in children {
        html.push_str(&math_dom_to_html(child));
    }
    html
}

fn math_dom_to_html(node: &katex::mathml_tree::MathDomNode) -> String {
    use katex::mathml_tree::MathDomNode;

    match node {
        MathDomNode::Math(math) => math_node_to_html(math),
        MathDomNode::Text(text) => escape_html(&text.text),
        MathDomNode::Space(space) => escape_html(space.character.as_deref().unwrap_or(" ")),
        MathDomNode::Fragment(fragment) => math_children_to_html(&fragment.children),
    }
}

fn math_dom_to_plain_text(node: &katex::mathml_tree::MathDomNode) -> Option<String> {
    use katex::mathml_tree::MathDomNode;

    match node {
        MathDomNode::Math(math) => math_node_to_plain_text(math),
        MathDomNode::Text(text) => Some(text.text.clone()),
        MathDomNode::Space(space) => Some(space.character.as_deref().unwrap_or(" ").to_string()),
        MathDomNode::Fragment(fragment) => math_children_to_plain_text(&fragment.children),
    }
}

fn math_child_html(node: &katex::mathml_tree::MathNode, index: usize) -> String {
    node.children
        .get(index)
        .map(math_dom_to_html)
        .unwrap_or_default()
}

fn math_child_plain_text(node: &katex::mathml_tree::MathNode, index: usize) -> Option<String> {
    node.children.get(index).and_then(math_dom_to_plain_text)
}

fn unicode_superscript_char(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        '+' => '⁺',
        '-' => '⁻',
        '=' => '⁼',
        '(' => '⁽',
        ')' => '⁾',
        'a' => 'ᵃ',
        'b' => 'ᵇ',
        'c' => 'ᶜ',
        'd' => 'ᵈ',
        'e' => 'ᵉ',
        'f' => 'ᶠ',
        'g' => 'ᵍ',
        'h' => 'ʰ',
        'i' => 'ⁱ',
        'j' => 'ʲ',
        'k' => 'ᵏ',
        'l' => 'ˡ',
        'm' => 'ᵐ',
        'n' => 'ⁿ',
        'o' => 'ᵒ',
        'p' => 'ᵖ',
        'r' => 'ʳ',
        's' => 'ˢ',
        't' => 'ᵗ',
        'u' => 'ᵘ',
        'v' => 'ᵛ',
        'w' => 'ʷ',
        'x' => 'ˣ',
        'y' => 'ʸ',
        'z' => 'ᶻ',
        'β' => 'ᵝ',
        'γ' => 'ᵞ',
        'δ' => 'ᵟ',
        'θ' => 'ᶿ',
        'φ' => 'ᵠ',
        'χ' => 'ᵡ',
        _ => return None,
    })
}

fn unicode_subscript_char(ch: char) -> Option<char> {
    Some(match ch {
        '0' => '₀',
        '1' => '₁',
        '2' => '₂',
        '3' => '₃',
        '4' => '₄',
        '5' => '₅',
        '6' => '₆',
        '7' => '₇',
        '8' => '₈',
        '9' => '₉',
        '+' => '₊',
        '-' => '₋',
        '=' => '₌',
        '(' => '₍',
        ')' => '₎',
        'a' => 'ₐ',
        'e' => 'ₑ',
        'h' => 'ₕ',
        'i' => 'ᵢ',
        'j' => 'ⱼ',
        'k' => 'ₖ',
        'l' => 'ₗ',
        'm' => 'ₘ',
        'n' => 'ₙ',
        'o' => 'ₒ',
        'p' => 'ₚ',
        'r' => 'ᵣ',
        's' => 'ₛ',
        't' => 'ₜ',
        'u' => 'ᵤ',
        'v' => 'ᵥ',
        'x' => 'ₓ',
        'β' => 'ᵦ',
        'γ' => 'ᵧ',
        'ρ' => 'ᵨ',
        'φ' => 'ᵩ',
        'χ' => 'ᵪ',
        _ => return None,
    })
}

fn unicode_superscript(text: &str) -> Option<String> {
    let mut rendered = String::new();
    for ch in text.chars() {
        rendered.push(unicode_superscript_char(ch)?);
    }
    Some(rendered)
}

fn unicode_subscript(text: &str) -> Option<String> {
    let mut rendered = String::new();
    for ch in text.chars() {
        rendered.push(unicode_subscript_char(ch)?);
    }
    Some(rendered)
}

fn render_superscript_text(text: &str) -> String {
    if let Some(mapped) = unicode_superscript(text) {
        mapped
    } else {
        format!(
            r#"<span class="txp-script-sup">{}</span>"#,
            escape_html(text)
        )
    }
}

fn render_script_stack_fallback(base: &str, over: Option<&str>, under: Option<&str>) -> String {
    let mut stack = String::new();
    if let Some(over) = over {
        stack.push_str(&format!(r#"<span class="txp-script-over">{over}</span>"#));
    }
    if let Some(under) = under {
        stack.push_str(&format!(r#"<span class="txp-script-under">{under}</span>"#));
    }
    format!(
        r#"<span class="txp-script-pair"><span class="txp-script-base">{base}</span><span class="txp-script-stack">{stack}</span></span>"#
    )
}

fn is_large_operator_text(text: &str) -> bool {
    matches!(
        text.trim(),
        "∫" | "∮"
            | "∯"
            | "∰"
            | "∑"
            | "∏"
            | "⋂"
            | "⋃"
            | "⋁"
            | "⋀"
            | "⨀"
            | "⨁"
            | "⨂"
            | "⨆"
    )
}

fn render_large_operator_limits(base: &str, over: Option<&str>, under: Option<&str>) -> String {
    let over_html = over
        .map(|value| format!(r#"<span class="txp-op-over">{value}</span>"#))
        .unwrap_or_default();
    let under_html = under
        .map(|value| format!(r#"<span class="txp-op-under">{value}</span>"#))
        .unwrap_or_default();
    format!(
        r#"<span class="txp-op-limits">{over_html}<span class="txp-op-base">{base}</span>{under_html}</span>"#
    )
}

fn math_node_to_plain_text(node: &katex::mathml_tree::MathNode) -> Option<String> {
    use katex::mathml_tree::{MathDomNode, MathNodeType};

    match node.node_type {
        MathNodeType::Math
        | MathNodeType::Mrow
        | MathNodeType::Mstyle
        | MathNodeType::Mpadded
        | MathNodeType::Mphantom
        | MathNodeType::Mi
        | MathNodeType::Mn
        | MathNodeType::Mo
        | MathNodeType::Mtext => math_children_to_plain_text(&node.children),
        MathNodeType::Semantics => {
            let mut text = String::new();
            for child in &node.children {
                match child {
                    MathDomNode::Math(math) if math.node_type == MathNodeType::Annotation => {}
                    _ => text.push_str(&math_dom_to_plain_text(child)?),
                }
            }
            Some(text)
        }
        MathNodeType::Annotation => Some(String::new()),
        MathNodeType::Mspace => Some(" ".to_string()),
        MathNodeType::Menclose => math_children_to_plain_text(&node.children),
        MathNodeType::Mglyph => node.attributes.get("alt").cloned(),
        _ => None,
    }
}

fn math_node_to_html(node: &katex::mathml_tree::MathNode) -> String {
    use katex::mathml_tree::{MathDomNode, MathNodeType};

    match node.node_type {
        MathNodeType::Math
        | MathNodeType::Mrow
        | MathNodeType::Mstyle
        | MathNodeType::Mpadded
        | MathNodeType::Mphantom => math_children_to_html(&node.children),
        MathNodeType::Semantics => {
            let mut html = String::new();
            for child in &node.children {
                match child {
                    MathDomNode::Math(math) if math.node_type == MathNodeType::Annotation => {}
                    _ => html.push_str(&math_dom_to_html(child)),
                }
            }
            html
        }
        MathNodeType::Annotation => String::new(),
        MathNodeType::Mi | MathNodeType::Mn | MathNodeType::Mo | MathNodeType::Mtext => {
            math_children_to_html(&node.children)
        }
        MathNodeType::Mspace => " ".to_string(),
        MathNodeType::Msup => {
            let base = math_child_html(node, 0);
            let sup = math_child_html(node, 1);
            if let Some(base_text) = math_child_plain_text(node, 0)
                && is_large_operator_text(&base_text)
            {
                return render_large_operator_limits(&base, Some(&sup), None);
            }
            if let Some(sup_text) = math_child_plain_text(node, 1) {
                format!("{base}{}", render_superscript_text(&sup_text))
            } else {
                render_script_stack_fallback(&base, Some(&sup), None)
            }
        }
        MathNodeType::Msub => {
            let base = math_child_html(node, 0);
            let sub = math_child_html(node, 1);
            if let Some(base_text) = math_child_plain_text(node, 0)
                && is_large_operator_text(&base_text)
            {
                return render_large_operator_limits(&base, None, Some(&sub));
            }
            if let Some(sub_text) =
                math_child_plain_text(node, 1).and_then(|text| unicode_subscript(&text))
            {
                format!("{base}{sub_text}")
            } else {
                render_script_stack_fallback(&base, None, Some(&sub))
            }
        }
        MathNodeType::Msubsup => {
            let base = math_child_html(node, 0);
            let sub = math_child_html(node, 1);
            let sup = math_child_html(node, 2);
            if let Some(base_text) = math_child_plain_text(node, 0)
                && is_large_operator_text(&base_text)
            {
                return render_large_operator_limits(&base, Some(&sup), Some(&sub));
            }
            let sub_text = math_child_plain_text(node, 1).and_then(|text| unicode_subscript(&text));
            let sup_text = math_child_plain_text(node, 2);
            if let Some(sub_text) = sub_text {
                if let Some(sup_text) = sup_text {
                    format!("{base}{sub_text}{}", render_superscript_text(&sup_text))
                } else {
                    render_script_stack_fallback(&base, Some(&sup), Some(&sub))
                }
            } else {
                render_script_stack_fallback(&base, Some(&sup), Some(&sub))
            }
        }
        MathNodeType::Mfrac => {
            let numerator = math_child_html(node, 0);
            let denominator = math_child_html(node, 1);
            format!(
                r#"<span class="txp-frac"><span class="txp-frac-num">{numerator}</span><span class="txp-frac-den">{denominator}</span></span>"#
            )
        }
        MathNodeType::Msqrt => {
            let radicand = math_children_to_html(&node.children);
            format!(
                r#"<span class="txp-sqrt"><span class="txp-sqrt-glyph">√</span><span class="txp-sqrt-body">{radicand}</span></span>"#
            )
        }
        MathNodeType::Mroot => {
            let body = math_child_html(node, 0);
            let index = math_child_html(node, 1);
            format!(
                r#"<span class="txp-root"><sup>{index}</sup><span class="txp-sqrt"><span class="txp-sqrt-glyph">√</span><span class="txp-sqrt-body">{body}</span></span></span>"#
            )
        }
        MathNodeType::Mover => {
            let base = math_child_html(node, 0);
            let over = math_child_html(node, 1);
            format!(
                r#"<span class="txp-overunder"><span class="txp-script-over">{over}</span><span class="txp-overunder-base">{base}</span></span>"#
            )
        }
        MathNodeType::Munder => {
            let base = math_child_html(node, 0);
            let under = math_child_html(node, 1);
            format!(
                r#"<span class="txp-overunder"><span class="txp-overunder-base">{base}</span><span class="txp-script-under">{under}</span></span>"#
            )
        }
        MathNodeType::Munderover => {
            let base = math_child_html(node, 0);
            let under = math_child_html(node, 1);
            let over = math_child_html(node, 2);
            format!(
                r#"<span class="txp-overunder"><span class="txp-script-over">{over}</span><span class="txp-overunder-base">{base}</span><span class="txp-script-under">{under}</span></span>"#
            )
        }
        MathNodeType::Mtable => {
            let mut rows = String::new();
            for child in &node.children {
                rows.push_str(&math_dom_to_html(child));
            }
            format!(r#"<table class="txp-matrix"><tbody>{rows}</tbody></table>"#)
        }
        MathNodeType::Mtr | MathNodeType::Mlabeledtr => {
            let mut cells = String::new();
            for child in &node.children {
                match child {
                    MathDomNode::Math(math) if math.node_type == MathNodeType::Mtd => {
                        cells.push_str(&math_dom_to_html(child));
                    }
                    _ => {
                        cells.push_str(&format!("<td>{}</td>", math_dom_to_html(child)));
                    }
                }
            }
            format!("<tr>{cells}</tr>")
        }
        MathNodeType::Mtd => {
            let inner = math_children_to_html(&node.children);
            format!("<td>{inner}</td>")
        }
        MathNodeType::Menclose => {
            let inner = math_children_to_html(&node.children);
            let notation = node
                .attributes
                .get("notation")
                .map(String::as_str)
                .unwrap_or_default();
            if notation.contains("box") {
                format!(r#"<span class="txp-menclose-box">{inner}</span>"#)
            } else {
                inner
            }
        }
        MathNodeType::Mglyph => node
            .attributes
            .get("alt")
            .map(|text| escape_html(text))
            .unwrap_or_default(),
    }
}

fn render_math_markup(latex: &str, display_mode: bool) -> Result<String> {
    use katex::types::OutputFormat;
    use katex::{KatexContext, render_to_dom_tree};

    let ctx = KatexContext::default();
    let settings = katex::Settings {
        display_mode,
        output: OutputFormat::Mathml,
        ..Default::default()
    };

    let dom = render_to_dom_tree(&ctx, latex, &settings)
        .map_err(|e| anyhow::anyhow!("katex error: {e:?}"))?;
    let body = html_dom_children_to_math_html(&dom.children);
    let class = if display_mode {
        "txp-math txp-math-display"
    } else {
        "txp-math txp-math-inline"
    };
    Ok(format!(r#"<span class="{class}">{body}</span>"#))
}

fn math_fallback_markup(latex: &str, display_mode: bool) -> String {
    let class = if display_mode {
        "txp-math txp-math-display txp-math-error"
    } else {
        "txp-math txp-math-inline txp-math-error"
    };
    format!(r#"<span class="{class}">{}</span>"#, escape_html(latex))
}

#[cfg(feature = "mermaid-render")]
fn detect_mermaid_system_font(prefer_cjk: bool) -> Option<(PathBuf, &'static str)> {
    let cjk_candidates: &[(&str, &str)] = &[
        (
            "WenQuanYi Zen Hei",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        ),
        (
            "WenQuanYi Micro Hei",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        ),
        ("Microsoft YaHei", "/mnt/c/Windows/Fonts/msyh.ttc"),
        ("SimSun", "/mnt/c/Windows/Fonts/simsun.ttc"),
    ];
    let latin_candidates: &[(&str, &str)] = &[
        (
            "DejaVu Sans",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ),
        (
            "Liberation Sans",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        ),
        (
            "WenQuanYi Zen Hei",
            "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        ),
    ];

    let chains: [&[(&str, &str)]; 2] = if prefer_cjk {
        [cjk_candidates, latin_candidates]
    } else {
        [latin_candidates, cjk_candidates]
    };

    for chain in chains {
        for (family, path) in chain {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some((p, *family));
            }
        }
    }

    None
}

fn process_math(html: &mut String) -> Result<usize> {
    *html = html.replace("\\$", ESCAPED_PLACEHOLDER);

    let display_re = Regex::new(r"(?s)\$\$(.+?)\$\$")?;
    let inline_re = Regex::new(r"\$([^$]+?)\$")?;

    let mut count = 0usize;

    // Display math $$...$$
    let display_matches: Vec<_> = display_re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).unwrap().range(),
                c.get(1).unwrap().as_str().to_string(),
            )
        })
        .collect();
    for (range, latex) in display_matches.into_iter().rev() {
        let rendered = match render_math_markup(&latex, true) {
            Ok(markup) => {
                count += 1;
                markup
            }
            Err(e) => {
                eprintln!("Warning: display math render failed for '{latex}': {e}");
                math_fallback_markup(&latex, true)
            }
        };
        html.replace_range(range, &rendered);
    }

    // Inline math $...$
    let inline_matches: Vec<_> = inline_re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).unwrap().range(),
                c.get(1).unwrap().as_str().to_string(),
            )
        })
        .collect();
    for (range, latex) in inline_matches.into_iter().rev() {
        let rendered = match render_math_markup(&latex, false) {
            Ok(markup) => {
                count += 1;
                markup
            }
            Err(e) => {
                eprintln!("Warning: inline math render failed for '{latex}': {e}");
                math_fallback_markup(&latex, false)
            }
        };
        html.replace_range(range, &rendered);
    }

    *html = html.replace(ESCAPED_PLACEHOLDER, "\\$");
    Ok(count)
}

// ── Mermaid Processing ─────────────────────────────────────────────────

#[cfg(feature = "mermaid-render")]
fn process_mermaid(html: &mut String) -> Result<usize> {
    use mermaid_render::{EstimatedMeasure, render_diagram};

    let re = Regex::new(r"(?s)```mermaid\r?\n(.*?)```")?;
    let mut count = 0usize;

    let matches: Vec<_> = re
        .captures_iter(html)
        .map(|c| {
            (
                c.get(0).unwrap().range(),
                c.get(1).unwrap().as_str().to_string(),
            )
        })
        .collect();

    for (range, source) in matches.into_iter().rev() {
        let mermaid_font = detect_mermaid_system_font(source.chars().any(|c| !c.is_ascii()));
        let mut style = mermaid_render::DiagramStyle::default();
        if let Some((_, family)) = mermaid_font.as_ref() {
            style.font_family = (*family).to_string();
        }

        match render_diagram(&source, &style, &mut EstimatedMeasure) {
            Ok((svg, w, h)) => {
                let svg_w = w.max(100.0);
                let svg_h = h.max(100.0);
                let svg_doc = format!(
                    r#"<div class="txp-mermaid" style="text-align:center;margin:1em 0"><svg xmlns="http://www.w3.org/2000/svg" width="{svg_w}" height="{svg_h}" viewBox="0 0 {svg_w} {svg_h}" style="display:block;margin:0 auto">{svg}</svg></div>"#
                );
                html.replace_range(range, &svg_doc);
                count += 1;
            }
            Err(e) => {
                eprintln!("Warning: mermaid render failed: {e}");
                let fallback = format!(
                    r#"<div class="mermaid-placeholder" style="border:2px dashed #ccc;padding:2em;text-align:center;margin:1em 0;color:#888;font-style:italic">Mermaid render failed: {}</div>"#,
                    escape_html(source.trim())
                );
                html.replace_range(range, &fallback);
            }
        }
    }

    Ok(count)
}

// ── Font Scanning ──────────────────────────────────────────────────────

/// Auto-detect a math-capable system font (DejaVu Serif, Liberation Serif, etc.)
/// that contains glyphs for mathematical symbols (∫, ∇, ±, ∂, ∞, etc.).
/// Returns (font_path, font_family_name).
fn detect_math_system_font() -> Option<(PathBuf, String)> {
    // Priority-ordered list of math-capable fonts available on most Linux systems
    let candidates: &[(&str, &str)] = &[
        (
            "DejaVu Serif",
            "/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf",
        ),
        (
            "DejaVu Sans",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        ),
        (
            "Liberation Serif",
            "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf",
        ),
        (
            "FreeSerif",
            "/usr/share/fonts/truetype/freefont/FreeSerif.ttf",
        ),
    ];
    for (family, path) in candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some((p, family.to_string()));
        }
    }
    None
}

/// Generate @font-face CSS to register a math font with the name 'TypePressMath'.
/// fulgur's extract_font_faces_from_html will pick this up and load the font file.
fn math_font_face_css(font_path: &Path) -> String {
    format!(
        r#"@font-face {{ font-family: 'TypePressMath'; src: url('{}'); }}"#,
        font_path.display()
    )
}

/// Detect system emoji font and return its path.
/// Noto Serif CJK doesn't cover all emoji glyphs (missing 👦👧👩🛠 etc).
fn detect_emoji_font() -> Option<PathBuf> {
    for path in &[
        "/usr/share/fonts/truetype/noto/NotoColorEmoji.ttf",
        "/usr/share/fonts/noto/NotoColorEmoji.ttf",
        "/System/Library/Fonts/Apple Color Emoji.ttc",
        "C:\\Windows\\Fonts\\seguiemj.ttf",
    ] {
        let p = std::path::Path::new(path);
        if p.exists() {
            eprintln!("Emoji font: {}", p.display());
            return Some(p.to_path_buf());
        }
    }
    None
}

fn auto_detect_katex_fonts() -> Option<PathBuf> {
    // 1. Common npm global locations (no subprocess, pure path check)
    for npm_root in katex_npm_roots() {
        let direct = npm_root.join("katex/dist/fonts");
        if direct.is_dir() {
            return Some(direct);
        }
        if let Some(found) = find_katex_fonts_in(&npm_root, 0, 3) {
            return Some(found);
        }
    }

    // 2. System paths (Linux, macOS Homebrew)
    for p in &[
        "/usr/share/katex/fonts",
        "/usr/local/share/katex/fonts",
        "/opt/homebrew/share/katex/fonts",
    ] {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    None
}

/// Common npm global node_modules root directories across platforms.
fn katex_npm_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    // npm prefix (env var or standard locations)
    if let Ok(prefix) = std::env::var("npm_config_prefix") {
        roots.push(PathBuf::from(&prefix).join("lib/node_modules"));
    }
    // Unix: $HOME/.npm-global or /usr/local
    if let Ok(home) = std::env::var("HOME") {
        roots.push(PathBuf::from(&home).join(".npm-global/lib/node_modules"));
        roots.push(PathBuf::from(&home).join("node_modules"));
    }
    #[cfg(target_os = "linux")]
    roots.push(PathBuf::from("/usr/local/lib/node_modules"));
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/opt/homebrew/lib/node_modules"));
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            roots.push(PathBuf::from(&appdata).join("npm/node_modules"));
        }
    }
    roots
}

fn find_katex_fonts_in(dir: &Path, depth: usize, max: usize) -> Option<PathBuf> {
    if depth > max || !dir.is_dir() {
        return None;
    }
    let candidate = dir.join("katex/dist/fonts");
    if candidate.is_dir() {
        return Some(candidate);
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let nested = entry.path().join("node_modules/katex/dist/fonts");
            if nested.is_dir() {
                return Some(nested);
            }
            if entry.path().is_dir()
                && let Some(found) = find_katex_fonts_in(&entry.path(), depth + 1, max)
            {
                return Some(found);
            }
        }
    }
    None
}

// ── Header / Footer ────────────────────────────────────────────────────

// ── Utility ────────────────────────────────────────────────────────────

fn inject_css(html: &mut String, css: &str) {
    let tag = format!("<style>{css}</style>");
    if let Some(pos) = html.find("</head>") {
        html.insert_str(pos, &tag);
    } else if let Some(pos) = html.find("<body") {
        html.insert_str(pos, &format!("{tag}\n"));
    }
}

fn read_input(input: Option<&PathBuf>, stdin: bool) -> Result<String> {
    if stdin {
        let mut buf = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf)?;
        Ok(buf)
    } else if let Some(path) = input {
        Ok(std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?)
    } else {
        anyhow::bail!("provide an input HTML file or use --stdin")
    }
}

// ── Multi-Format Output ────────────────────────────────────────────────

fn render_png_from_pdf(pdf_bytes: &[u8], scale: f32) -> Result<Vec<u8>> {
    use tiny_skia::Pixmap;

    let tmp = tempfile::NamedTempFile::new()?;
    let path = tmp.path().to_path_buf();
    std::fs::write(&path, pdf_bytes)?;
    let result = fulgur::inspect::inspect(&path)?;

    // Read actual page dimensions from PDF MediaBox (not hardcoded A4)
    let (w_pt, h_pt) = lopdf::Document::load(&path)
        .ok()
        .and_then(|doc| {
            let pages = doc.get_pages();
            let first_page_id = pages.values().next().copied()?;
            let dict = doc.get_dictionary(first_page_id).ok()?;
            let media_box = dict.get(b"MediaBox").ok()?;
            let arr = media_box.as_array().ok()?;
            let w = arr.get(2).and_then(|o| o.as_f32().ok()).unwrap_or(595.0);
            let h = arr.get(3).and_then(|o| o.as_f32().ok()).unwrap_or(842.0);
            Some((w, h))
        })
        .unwrap_or((595.0, 842.0));
    let w = w_pt * scale;
    let h = h_pt * scale;

    let first_page = result.text_items.iter().filter(|t| t.page == 1);

    let mut pixmap = Pixmap::new(w as u32, h as u32)
        .ok_or_else(|| anyhow::anyhow!("failed to create pixmap {w}x{h}"))?;
    pixmap.fill(tiny_skia::Color::WHITE);

    // Simple approach: draw text items as colored rectangles
    // (Full text rendering would need font loading)
    let paint = tiny_skia::Paint {
        shader: tiny_skia::Shader::SolidColor(tiny_skia::Color::from_rgba8(0, 0, 0, 255)),
        ..Default::default()
    };
    for item in first_page {
        let rx = item.x * scale;
        let ry = item.y * scale;
        let rw = item.width.max(4.0) * scale;
        let rh = item.height * scale;
        let rect = tiny_skia::Rect::from_xywh(rx, ry, rw, rh)
            .unwrap_or(tiny_skia::Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap());
        pixmap.fill_rect(rect, &paint, tiny_skia::Transform::default(), None);
    }

    let png_data = pixmap.encode_png()?;
    Ok(png_data)
}

fn render_svg_from_pdf(pdf_bytes: &[u8]) -> Result<String> {
    svg::svg_unicode(pdf_bytes, 1)
}

/// Generate multi-page output filenames from a base path.
/// e.g., "out.svg" → ["out_page1.svg", "out_page2.svg"]
/// If only 1 page, returns just ["out.svg"].
fn page_output_paths(base: &Path, page_count: u32) -> Vec<PathBuf> {
    if page_count <= 1 {
        return vec![base.to_path_buf()];
    }
    let stem = base.file_stem().unwrap_or_default().to_string_lossy();
    let ext = base.extension().unwrap_or_default().to_string_lossy();
    let parent = base.parent().unwrap_or(Path::new("."));
    (1..=page_count)
        .map(|p| {
            if ext.is_empty() {
                parent.join(format!("{stem}_page{p}"))
            } else {
                parent.join(format!("{stem}_page{p}.{ext}"))
            }
        })
        .collect()
}

fn write_svg_multi(pdf_bytes: &[u8], output: &Path) -> Result<()> {
    let pages = svg::page_count(pdf_bytes)?;
    let paths = page_output_paths(output, pages);
    for (i, path) in paths.iter().enumerate() {
        let svg_content = svg::svg_unicode(pdf_bytes, (i + 1) as u32)?;
        std::fs::write(path, svg_content)?;
        eprintln!("SVG page {} written to {}", i + 1, path.display());
    }
    Ok(())
}

// ── Main ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cli = Cli::parse();

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
            match cli.format.as_str() {
                "svg" => print!("{}", render_svg_from_pdf(&pdf_bytes)?),
                "png" => {
                    use std::io::Write;
                    std::io::stdout().write_all(&render_png_from_pdf(&pdf_bytes, cli.scale)?)?;
                }
                _ => {
                    use std::io::Write;
                    std::io::stdout().write_all(&pdf_bytes)?;
                }
            }
        } else if let Some(ref output) = cli.output {
            match cli.format.as_str() {
                "svg" => write_svg_multi(&pdf_bytes, output)?,
                "png" => {
                    std::fs::write(output, render_png_from_pdf(&pdf_bytes, cli.scale)?)?;
                    eprintln!(
                        "PNG written to {} (note: rasterized as text bounding boxes, not rendered text)",
                        output.display()
                    );
                }
                _ => {
                    std::fs::write(output, &pdf_bytes)?;
                    eprintln!("PDF written to {}", output.display());
                }
            }
        }
        return Ok(());
    }

    let mut html = read_input(input_file.as_ref(), cli.stdin)?;

    // ── Math font detection (before any processing) ──
    let math_enabled = cli.math || cli.math_dir.is_some();
    let math_fonts: Vec<PathBuf> = if math_enabled {
        let target = cli.math_dir.or_else(|| {
            if cli.math {
                auto_detect_katex_fonts()
            } else {
                None
            }
        });
        if let Some(ref dir) = target {
            let fonts = fonts::scan_font_dir(dir);
            if !fonts.is_empty() {
                eprintln!("Math: {} font(s) from {}", fonts.len(), dir.display());
            }
            fonts
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let header_css;

    if is_md {
        // MD pipeline: Mermaid → Math → Markdown→HTML → Header/Footer → Highlight

        // 0a. Mermaid (raw markdown)
        #[cfg(feature = "mermaid-render")]
        match process_mermaid(&mut html) {
            Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
            Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
            _ => {}
        }

        // 0b. Math (raw markdown — pre-empts pulldown-cmark's ENABLE_MATH)
        let math_count = if math_enabled {
            match process_math(&mut html) {
                Ok(n) => {
                    if n > 0 {
                        eprintln!("Rendered {n} math expression(s)");
                    }
                    n
                }
                Err(e) => {
                    eprintln!("Warning: math processing failed: {e}");
                    0
                }
            }
        } else {
            0
        };

        // 0c. Convert markdown to HTML
        html = if math_count > 0 {
            typepress::markdown_to_html_with_css(&html, KATEX_CSS)
        } else {
            markdown_to_html(&html)
        };

        // 1. Inject header/footer
        header_css = inject_header_footer(&mut html, header.as_deref(), footer.as_deref());

        // Inject @font-face for math system font (maps 'TypePressMath' to a real font file)
        // This must happen BEFORE extract_font_faces_from_html() so the @font-face rule
        // is picked up and the font file is added to the AssetBundle.
        if math_count > 0 {
            if let Some((math_font_path, family)) = detect_math_system_font() {
                let ff_css = math_font_face_css(&math_font_path);
                inject_css(&mut html, &ff_css);
                eprintln!("Math font: using {family} ({})", math_font_path.display());
            } else {
                eprintln!(
                    "Warning: no math-capable system font found. \
                     Math symbols (∫, ∇, ±, ∂, ∞) may render as empty boxes. \
                     Install DejaVu or Liberation fonts."
                );
            }
        }
    } else {
        // HTML pipeline: CSS Layout → Header/Footer → Math → Mermaid → Highlight

        // 0a. CSS Layout: using native blitz-html 0.3 (flex/grid → taffy natively)
        // (old Grid→Table preprocessing removed — no longer needed)

        // Emoji → PNG replacement (before network resources so file:// img tags work)
        {
            let (new_html, n) = typepress::emoji::replace_emoji_with_images(&html);
            html = new_html;
            if n > 0 {
                eprintln!("Replaced {n} emoji(s) with images");
            }
        }

        // 0b. Network resources: download remote CSS <link> + <img>
        match typepress::network::inject_remote_css(&mut html) {
            Ok(n) if n > 0 => eprintln!("Downloaded {n} remote CSS file(s)"),
            Ok(_) => {}
            Err(e) => eprintln!("Warning: remote CSS: {e}"),
        }
        if let Some(ref bp) = base_path {
            match typepress::network::inject_local_css(&mut html, bp) {
                Ok(n) if n > 0 => eprintln!("Embedded {n} local CSS file(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("Warning: local CSS: {e}"),
            }
        }
        match typepress::network::download_remote_images(&mut html) {
            Ok((n, _)) if n > 0 => eprintln!("Downloaded {n} remote image(s)"),
            Ok(_) => {}
            Err(e) => eprintln!("Warning: remote images: {e}"),
        }

        // 1. Inject header/footer
        header_css = inject_header_footer(&mut html, header.as_deref(), footer.as_deref());

        // 2. Math
        if math_enabled {
            match process_math(&mut html) {
                Ok(n) if n > 0 => eprintln!("Rendered {n} math expression(s)"),
                Err(e) => eprintln!("Warning: math processing failed: {e}"),
                _ => {}
            }
        }

        // 3. Mermaid
        #[cfg(feature = "mermaid-render")]
        match process_mermaid(&mut html) {
            Ok(n) if n > 0 => eprintln!("Rendered {n} mermaid diagram(s)"),
            Err(e) => eprintln!("Warning: mermaid processing failed: {e}"),
            _ => {}
        }
    }

    // 4. Apply code syntax highlighting (syntect)
    match typepress::highlight::highlight_code_blocks(&mut html) {
        Ok(n) if n > 0 => eprintln!("Highlighted {n} code block(s)"),
        Err(e) => eprintln!("Warning: code highlighting failed: {e}"),
        _ => {}
    }

    // 3. Build asset bundle — start with @font-face font resolution
    // Parse @font-face from inline styles in the HTML

    let mut font_face_paths: Vec<PathBuf> = Vec::new();
    // Emoji font fallback: register system emoji font for glyphs missing
    // from Noto Serif CJK (👦👧👩🛠 etc). Note: Krilla does not support
    // color bitmap fonts, so color emoji glyphs render as monochrome outlines.
    if let Some(emoji_path) = detect_emoji_font() {
        font_face_paths.push(emoji_path);
    }
    for ff in fonts::extract_font_faces_from_html(&html) {
        match fonts::resolve_font_path(&ff.src_url, base_path.as_deref()) {
            Ok(path) => font_face_paths.push(path),
            Err(e) => eprintln!("Warning: @font-face '{}': {e}", ff.family),
        }
    }

    // Parse @font-face from external CSS files
    for css_path in &cli.css_files {
        if let Ok(css_content) = std::fs::read_to_string(css_path) {
            for ff in fonts::parse_font_faces(&css_content) {
                let css_dir = css_path.parent();
                match fonts::resolve_font_path(&ff.src_url, css_dir.or(base_path.as_deref())) {
                    Ok(path) => font_face_paths.push(path),
                    Err(e) => eprintln!(
                        "Warning: @font-face '{}' in {}: {e}",
                        ff.family,
                        css_path.display()
                    ),
                }
            }
        }
    }

    let needs_assets = !cli.fonts.is_empty()
        || !cli.css_files.is_empty()
        || header_css.is_some()
        || !math_fonts.is_empty()
        || !font_face_paths.is_empty();

    let assets = if needs_assets {
        let mut bundle = AssetBundle::new();
        if let Some(ref css) = header_css {
            bundle.add_css(css);
        }
        for f in &cli.fonts {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: font {}: {e}", f.display()));
        }
        for f in &math_fonts {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: math font {}: {e}", f.display()));
        }
        for f in &cli.css_files {
            bundle
                .add_css_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: CSS {}: {e}", f.display()));
        }
        for f in &font_face_paths {
            bundle
                .add_font_file(f)
                .unwrap_or_else(|e| eprintln!("Warning: @font-face font {}: {e}", f.display()));
        }
        Some(bundle)
    } else {
        None
    };

    // 4. Build engine
    let mut builder = Engine::builder();

    // ── Merge YAML config (CLI args override YAML) ──
    if cli.no_system_fonts {
        builder = builder.system_fonts(false);
    }
    if let Some(ref c) = cfg {
        if let Some(ref pc) = c.page {
            if resolved_size.is_none()
                && let Some(ref size) = pc.size
            {
                builder = builder.page_size(parse_page_size(size));
            }
            if !resolved_landscape && let Some(ls) = pc.landscape {
                builder = builder.landscape(ls);
            }
            if resolved_margin.is_none()
                && let Some(ref margin) = pc.margin
            {
                builder = builder.margin(parse_margin(margin));
            }
        }
        if let Some(ref mc) = c.metadata {
            if cli.title.is_none()
                && let Some(ref title) = mc.title
            {
                builder = builder.title(title.clone());
            }
            if cli.authors.is_empty() && !mc.author.is_empty() {
                builder = builder.authors(mc.author.clone());
            }
            if cli.language.is_none()
                && let Some(ref lang) = mc.language
            {
                builder = builder.lang(lang.clone());
            }
        }
        if let Some(ref pdf_cfg) = c.pdf {
            if !cli.bookmarks
                && let Some(bm) = pdf_cfg.bookmarks
            {
                builder = builder.bookmarks(bm);
            }
            if !cli.tagged
                && let Some(tg) = pdf_cfg.tagged
            {
                builder = builder.tagged(tg);
            }
            if !cli.pdf_ua
                && let Some(ua) = pdf_cfg.pdf_ua
            {
                builder = builder.pdf_ua(ua);
            }
        }
    }

    // ── CLI args (override YAML or set directly) ──
    if let Some(ref s) = resolved_size {
        // Custom page-size: "W H" or standard "A4"
        builder = builder.page_size(parse_page_size(s));
    }
    let landscape = resolved_landscape;
    if landscape {
        builder = builder.landscape(true);
    }
    if let Some(m) = resolved_margin {
        builder = builder.margin(m);
    }
    // --zoom: scale CSS by wrapping content with transform
    if (cli.zoom - 1.0).abs() > f32::EPSILON {
        html = format!(
            "<div style=\"transform:scale({});transform-origin:top left;width:{}%;\">{}</div>",
            cli.zoom,
            (100.0 / cli.zoom) as u32,
            html
        );
    }
    // --no-outline: invert bookmarks default
    let bookmarks = if cli.no_outline { false } else { cli.bookmarks };
    if let Some(t) = cli.title {
        builder = builder.title(t);
    }
    if !cli.authors.is_empty() {
        builder = builder.authors(cli.authors);
    }
    if let Some(l) = cli.language {
        builder = builder.lang(l);
    }
    builder = builder
        .bookmarks(bookmarks)
        .tagged(cli.tagged)
        .pdf_ua(cli.pdf_ua);
    if let Some(ref bp) = base_path {
        builder = builder.base_path(bp);
    }
    if let Some(a) = assets {
        builder = builder.assets(a);
    }

    let engine = builder.build();
    let mut pdf = engine.render_html(&html)?;

    // --fit: if multi-page, scale CSS uniformly and re-render
    if cli.fit {
        let pages = typepress::css_layout::count_pdf_pages(&pdf);
        if pages > 1 {
            let scale = 0.95 / pages as f64; // 95% safety margin
            eprintln!(
                "Fitting {pages} pages → 1 page (scale {:.1}%)",
                scale * 100.0
            );
            let scaled_html = typepress::css_layout::scale_css_for_fit(&html, scale);
            pdf = engine.render_html(&scaled_html)?;
            let new_pages = typepress::css_layout::count_pdf_pages(&pdf);
            eprintln!(" → {new_pages} page(s) after fitting");
        }
    }

    // 5. Route output by format. YAML config triggers multi-format.
    let to_stdout = cli.output.as_ref().is_some_and(|o| o.as_os_str() == "-");

    // Config-driven multi-format output (from YAML output section)
    if let Some(oc) = cfg.as_ref().and_then(|c| c.output.as_ref()) {
        if let Some(ref path) = oc.pdf {
            std::fs::write(path, &pdf)?;
            eprintln!("PDF written to {}", path.display());
        }
        if let Some(ref path) = oc.svg {
            write_svg_multi(&pdf, path)?;
        }
        if let Some(ref path) = oc.png {
            std::fs::write(path, render_png_from_pdf(&pdf, cli.scale)?)?;
            eprintln!(
                "PNG written to {} (note: rasterized as text bounding boxes, not rendered text)",
                path.display()
            );
        }
    }

    // CLI-driven output (--format + -o). Skip if YAML config already handles this format.
    if to_stdout {
        match cli.format.as_str() {
            "svg" => print!("{}", render_svg_from_pdf(&pdf)?),
            "png" => {
                use std::io::Write;
                std::io::stdout().write_all(&render_png_from_pdf(&pdf, cli.scale)?)?;
            }
            _ => {
                use std::io::Write;
                std::io::stdout().write_all(&pdf)?;
            }
        }
    } else if let Some(ref output) = cli.output {
        // Check if YAML already handles this specific format
        let yaml_has_format = cfg
            .as_ref()
            .and_then(|c| c.output.as_ref())
            .is_some_and(|oc| match cli.format.as_str() {
                "svg" => oc.svg.is_some(),
                "png" => oc.png.is_some(),
                _ => oc.pdf.is_some(),
            });
        if !yaml_has_format {
            match cli.format.as_str() {
                "svg" => write_svg_multi(&pdf, output)?,
                "png" => {
                    std::fs::write(output, render_png_from_pdf(&pdf, cli.scale)?)?;
                    eprintln!(
                        "PNG written to {} (note: rasterized as text bounding boxes, not rendered text)",
                        output.display()
                    );
                }
                _ => {
                    std::fs::write(output, &pdf)?;
                    eprintln!("PDF written to {}", output.display());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod preprocess_tests {
    use super::*;

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
    fn process_mermaid_generates_inline_svg() {
        let mut markdown = String::from("```mermaid\ngraph TD\n  A --> B\n```");
        let rendered = process_mermaid(&mut markdown).unwrap();

        assert_eq!(rendered, 1);
        assert!(markdown.contains("<svg"));
        assert!(markdown.contains("viewBox="));
        assert!(markdown.contains("A"));
        assert!(!markdown.contains("mermaid-placeholder"));
    }
}
