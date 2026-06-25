use anyhow::{Context, Result};
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::cli::ESCAPED_PLACEHOLDER;

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn html_dom_children_to_math_html(children: &[katex::dom_tree::HtmlDomNode]) -> String {
    let mut html = String::new();
    for child in children {
        html.push_str(&html_dom_to_math_html(child));
    }
    html
}

pub fn html_dom_to_math_html(node: &katex::dom_tree::HtmlDomNode) -> String {
    use katex::dom_tree::HtmlDomNode;

    match node {
        HtmlDomNode::MathML(math) => math_node_to_html(math),
        HtmlDomNode::DomSpan(span) => html_dom_children_to_math_html(&span.children),
        HtmlDomNode::Fragment(fragment) => html_dom_children_to_math_html(&fragment.children),
        HtmlDomNode::Symbol(symbol) => escape_html(&symbol.text),
        _ => String::new(),
    }
}

pub fn math_children_to_plain_text(children: &[katex::mathml_tree::MathDomNode]) -> Option<String> {
    let mut text = String::new();
    for child in children {
        text.push_str(&math_dom_to_plain_text(child)?);
    }
    Some(text)
}

pub fn math_children_to_html(children: &[katex::mathml_tree::MathDomNode]) -> String {
    let mut html = String::new();
    for child in children {
        html.push_str(&math_dom_to_html(child));
    }
    html
}

pub fn math_dom_to_html(node: &katex::mathml_tree::MathDomNode) -> String {
    use katex::mathml_tree::MathDomNode;

    match node {
        MathDomNode::Math(math) => math_node_to_html(math),
        MathDomNode::Text(text) => escape_html(&text.text),
        MathDomNode::Space(space) => escape_html(space.character.as_deref().unwrap_or(" ")),
        MathDomNode::Fragment(fragment) => math_children_to_html(&fragment.children),
    }
}

pub fn math_dom_to_plain_text(node: &katex::mathml_tree::MathDomNode) -> Option<String> {
    use katex::mathml_tree::MathDomNode;

    match node {
        MathDomNode::Math(math) => math_node_to_plain_text(math),
        MathDomNode::Text(text) => Some(text.text.clone()),
        MathDomNode::Space(space) => Some(space.character.as_deref().unwrap_or(" ").to_string()),
        MathDomNode::Fragment(fragment) => math_children_to_plain_text(&fragment.children),
    }
}

pub fn math_child_html(node: &katex::mathml_tree::MathNode, index: usize) -> String {
    node.children
        .get(index)
        .map(math_dom_to_html)
        .unwrap_or_default()
}

pub fn math_child_plain_text(node: &katex::mathml_tree::MathNode, index: usize) -> Option<String> {
    node.children.get(index).and_then(math_dom_to_plain_text)
}

pub fn unicode_superscript_char(ch: char) -> Option<char> {
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

pub fn unicode_subscript_char(ch: char) -> Option<char> {
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

pub fn unicode_superscript(text: &str) -> Option<String> {
    let mut rendered = String::new();
    for ch in text.chars() {
        rendered.push(unicode_superscript_char(ch)?);
    }
    Some(rendered)
}

pub fn unicode_subscript(text: &str) -> Option<String> {
    let mut rendered = String::new();
    for ch in text.chars() {
        rendered.push(unicode_subscript_char(ch)?);
    }
    Some(rendered)
}

pub fn render_superscript_text(text: &str) -> String {
    if let Some(mapped) = unicode_superscript(text) {
        mapped
    } else {
        format!(
            r#"<span class="txp-script-sup">{}</span>"#,
            escape_html(text)
        )
    }
}

pub fn render_script_stack_fallback(base: &str, over: Option<&str>, under: Option<&str>) -> String {
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

pub fn is_large_operator_text(text: &str) -> bool {
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

pub fn render_large_operator_limits(base: &str, over: Option<&str>, under: Option<&str>) -> String {
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

pub fn math_node_to_plain_text(node: &katex::mathml_tree::MathNode) -> Option<String> {
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

pub fn math_node_to_html(node: &katex::mathml_tree::MathNode) -> String {
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

pub fn render_math_markup(latex: &str, display_mode: bool) -> Result<String> {
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

pub fn math_fallback_markup(latex: &str, display_mode: bool) -> String {
    let class = if display_mode {
        "txp-math txp-math-display txp-math-error"
    } else {
        "txp-math txp-math-inline txp-math-error"
    };
    format!(r#"<span class="{class}">{}</span>"#, escape_html(latex))
}

pub fn process_math(html: &mut String) -> Result<usize> {
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
