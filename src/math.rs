use anyhow::Result;
use regex::Regex;

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

#[cfg(test)]
mod tests {
    use super::*;

    // ── escape_html ──────────────────────────────────────────────
    #[test]
    fn escape_html_ampersand() {
        assert_eq!(escape_html("a & b"), "a &amp; b");
    }

    #[test]
    fn escape_html_lt_gt() {
        assert_eq!(escape_html("<div>"), "&lt;div&gt;");
    }

    #[test]
    fn escape_html_quotes() {
        assert_eq!(escape_html("\"hello\""), "&quot;hello&quot;");
    }

    #[test]
    fn escape_html_single_quote() {
        assert_eq!(escape_html("it's"), "it&#39;s");
    }

    #[test]
    fn escape_html_preserves_plain_text() {
        assert_eq!(escape_html("hello world"), "hello world");
    }

    #[test]
    fn escape_html_all_special_chars() {
        assert_eq!(
            escape_html("if a < b && c > d then \"yes\""),
            "if a &lt; b &amp;&amp; c &gt; d then &quot;yes&quot;"
        );
    }

    // ── unicode_superscript_char ─────────────────────────────────
    #[test]
    fn superscript_digits() {
        assert_eq!(unicode_superscript_char('0'), Some('⁰'));
        assert_eq!(unicode_superscript_char('5'), Some('⁵'));
        assert_eq!(unicode_superscript_char('9'), Some('⁹'));
    }

    #[test]
    fn superscript_operators() {
        assert_eq!(unicode_superscript_char('+'), Some('⁺'));
        assert_eq!(unicode_superscript_char('-'), Some('⁻'));
        assert_eq!(unicode_superscript_char('='), Some('⁼'));
    }

    #[test]
    fn superscript_lowercase_letters() {
        assert_eq!(unicode_superscript_char('n'), Some('ⁿ'));
        assert_eq!(unicode_superscript_char('i'), Some('ⁱ'));
        assert_eq!(unicode_superscript_char('w'), Some('ʷ'));
    }

    #[test]
    fn superscript_greek() {
        assert_eq!(unicode_superscript_char('β'), Some('ᵝ'));
        assert_eq!(unicode_superscript_char('θ'), Some('ᶿ'));
        assert_eq!(unicode_superscript_char('χ'), Some('ᵡ'));
    }

    #[test]
    fn superscript_unknown_char_returns_none() {
        assert_eq!(unicode_superscript_char('X'), None);
        assert_eq!(unicode_superscript_char('日'), None);
        assert_eq!(unicode_superscript_char(' '), None);
    }

    // ── unicode_subscript_char ───────────────────────────────────
    #[test]
    fn subscript_digits() {
        assert_eq!(unicode_subscript_char('0'), Some('₀'));
        assert_eq!(unicode_subscript_char('5'), Some('₅'));
        assert_eq!(unicode_subscript_char('9'), Some('₉'));
    }

    #[test]
    fn subscript_operators() {
        assert_eq!(unicode_subscript_char('+'), Some('₊'));
        assert_eq!(unicode_subscript_char('-'), Some('₋'));
    }

    #[test]
    fn subscript_lowercase_letters() {
        assert_eq!(unicode_subscript_char('a'), Some('ₐ'));
        assert_eq!(unicode_subscript_char('i'), Some('ᵢ'));
        assert_eq!(unicode_subscript_char('n'), Some('ₙ'));
    }

    #[test]
    fn subscript_unknown_char_returns_none() {
        assert_eq!(unicode_subscript_char('Z'), None);
        assert_eq!(unicode_subscript_char('日'), None);
    }

    // ── unicode_superscript / unicode_subscript ──────────────────
    #[test]
    fn unicode_superscript_string() {
        assert_eq!(unicode_superscript("23"), Some("²³".to_string()));
        assert_eq!(unicode_superscript("ni"), Some("ⁿⁱ".to_string()));
        assert_eq!(unicode_superscript("n+1"), Some("ⁿ⁺¹".to_string()));
    }

    #[test]
    fn unicode_superscript_fails_on_unknown() {
        assert_eq!(unicode_superscript("X"), None);
        assert_eq!(unicode_superscript("2X"), None);
    }

    #[test]
    fn unicode_subscript_string() {
        assert_eq!(unicode_subscript("12"), Some("₁₂".to_string()));
        assert_eq!(unicode_subscript("ai"), Some("ₐᵢ".to_string()));
    }

    #[test]
    fn unicode_subscript_fails_on_unknown() {
        assert_eq!(unicode_subscript("Z"), None);
        assert_eq!(unicode_subscript("iZ"), None);
    }

    // ── is_large_operator_text ───────────────────────────────────
    #[test]
    fn large_operator_integral() {
        assert!(is_large_operator_text("∫"));
        assert!(is_large_operator_text("  ∫  "));
    }

    #[test]
    fn large_operator_sum_product() {
        assert!(is_large_operator_text("∑"));
        assert!(is_large_operator_text("∏"));
    }

    #[test]
    fn non_large_operator() {
        assert!(!is_large_operator_text("x"));
        assert!(!is_large_operator_text("+"));
        assert!(!is_large_operator_text("α"));
    }

    // ── render_superscript_text ──────────────────────────────────
    #[test]
    fn render_superscript_mappable() {
        let result = render_superscript_text("2");
        assert_eq!(result, "²");
    }

    #[test]
    fn render_superscript_unmappable_fallback() {
        // "HELLO" contains uppercase letters that can't be superscripted
        let result = render_superscript_text("HELLO");
        assert!(result.contains("txp-script-sup"));
        assert!(result.contains("HELLO"));
    }

    // ── render_script_stack_fallback ─────────────────────────────
    #[test]
    fn script_stack_base_only() {
        let result = render_script_stack_fallback("x", None, None);
        assert!(result.contains("x"));
        assert!(result.contains("txp-script-base"));
    }

    #[test]
    fn script_stack_with_over() {
        let result = render_script_stack_fallback("x", Some("2"), None);
        assert!(result.contains("x"));
        assert!(result.contains("2"));
        assert!(result.contains("txp-script-over"));
        assert!(!result.contains("txp-script-under"));
    }

    #[test]
    fn script_stack_with_both() {
        let result = render_script_stack_fallback("A", Some("B"), Some("C"));
        assert!(result.contains("A"));
        assert!(result.contains("B"));
        assert!(result.contains("C"));
    }

    // ── render_large_operator_limits ─────────────────────────────
    #[test]
    fn large_op_with_over() {
        let result = render_large_operator_limits("∑", Some("∞"), None);
        assert!(result.contains("∑"));
        assert!(result.contains("∞"));
        assert!(result.contains("txp-op-over"));
    }

    #[test]
    fn large_op_with_under() {
        let result = render_large_operator_limits("∏", None, Some("i=1"));
        assert!(result.contains("∏"));
        assert!(result.contains("i=1"));
        assert!(result.contains("txp-op-under"));
    }

    #[test]
    fn large_op_with_both() {
        let result = render_large_operator_limits("∫", Some("∞"), Some("0"));
        assert!(result.contains("∫"));
        assert!(result.contains("∞"));
        assert!(result.contains("0"));
    }

    // ── math_fallback_markup ─────────────────────────────────────
    #[test]
    fn fallback_inline() {
        let result = math_fallback_markup("x^2", false);
        assert!(result.contains("txp-math-inline"));
        assert!(result.contains("txp-math-error"));
    }

    #[test]
    fn fallback_display() {
        let result = math_fallback_markup("E=mc^2", true);
        assert!(result.contains("txp-math-display"));
        assert!(result.contains("txp-math-error"));
    }

    #[test]
    fn fallback_escapes_html() {
        let result = math_fallback_markup("a < b", false);
        assert!(result.contains("&lt;"));
        assert!(!result.contains("a < b"));
    }

    // ── render_math_markup ───────────────────────────────────────
    #[test]
    fn render_inline_math() {
        let result = render_math_markup("x^2", false).unwrap();
        assert!(result.contains("txp-math-inline"));
        assert!(!result.contains("txp-math-display"));
    }

    #[test]
    fn render_display_math() {
        let result = render_math_markup("E = mc^2", true).unwrap();
        assert!(result.contains("txp-math-display"));
        assert!(!result.contains("txp-math-inline"));
    }

    #[test]
    fn render_fraction() {
        let result = render_math_markup(r"\frac{1}{2}", false).unwrap();
        assert!(result.contains("txp-frac"));
        assert!(result.contains("txp-frac-num"));
        assert!(result.contains("txp-frac-den"));
    }

    #[test]
    fn render_sqrt() {
        let result = render_math_markup(r"\sqrt{4}", false).unwrap();
        assert!(result.contains("txp-sqrt"));
        assert!(result.contains("√"));
    }

    #[test]
    fn render_subscript() {
        let result = render_math_markup("x_1", false).unwrap();
        assert!(result.contains("₁"));
    }

    #[test]
    fn render_superscript() {
        let result = render_math_markup("x^2", false).unwrap();
        assert!(result.contains("²"));
    }

    #[test]
    fn render_integral_with_limits() {
        let result = render_math_markup(r"\int_0^\infty x dx", false).unwrap();
        assert!(result.contains("txp-op-limits"));
    }

    #[test]
    fn render_matrix() {
        let result =
            render_math_markup(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", true).unwrap();
        assert!(result.contains("txp-matrix"));
    }

    #[test]
    fn render_invalid_latex_returns_err() {
        let result = render_math_markup(r"\garbage{}", false);
        assert!(result.is_err());
    }

    #[test]
    fn render_nth_root() {
        let result = render_math_markup(r"\sqrt[3]{8}", false).unwrap();
        assert!(result.contains("txp-root"));
    }

    // ── process_math ─────────────────────────────────────────────
    #[test]
    fn process_math_inline() {
        let mut html = "the formula is $E = mc^2$ inside text".to_string();
        let count = process_math(&mut html).unwrap();
        assert!(count >= 1);
        assert!(!html.contains("$E = mc^2$"));
        assert!(html.contains("txp-math"));
    }

    #[test]
    fn process_math_display() {
        let mut html = "Here: $$\\sum_{i=1}^n i$$ is a sum".to_string();
        let count = process_math(&mut html).unwrap();
        assert!(count >= 1);
        assert!(html.contains("txp-math-display"));
        assert!(!html.contains("$$"));
    }

    #[test]
    fn process_math_multiple() {
        let mut html = "$a^2$ and $b^2$ and $$c^2$$".to_string();
        let count = process_math(&mut html).unwrap();
        assert!(count >= 2);
        assert!(!html.contains('$'));
    }

    #[test]
    fn process_math_preserves_escaped_dollar() {
        let mut html = r"price is \$100 and $x^2$".to_string();
        process_math(&mut html).unwrap();
        assert!(html.contains(r"\$100"));
        assert!(html.contains("txp-math"));
    }

    #[test]
    fn process_math_no_math_returns_zero() {
        let mut html = "just plain text".to_string();
        let count = process_math(&mut html).unwrap();
        assert_eq!(count, 0);
        assert_eq!(html, "just plain text");
    }

    #[test]
    fn process_math_invalid_expression_not_fatal() {
        let mut html = "$valid$ and $\\invalid{ and $$also-ok$$".to_string();
        let result = process_math(&mut html);
        assert!(result.is_ok());
    }

    #[test]
    fn process_math_keeps_text_outside_math() {
        let mut html = "Introduction paragraph. $x=1$ Conclusion.".to_string();
        process_math(&mut html).unwrap();
        assert!(html.contains("Introduction paragraph."));
        assert!(html.contains("Conclusion."));
    }

    // ── math_dom_to_html ─────────────────────────────────────────
    #[test]
    fn dom_text_node() {
        use katex::mathml_tree::MathDomNode;
        let text = katex::mathml_tree::TextNode {
            text: "hello".to_string(),
        };
        let result = math_dom_to_html(&MathDomNode::Text(text));
        assert_eq!(result, "hello");
    }

    #[test]
    fn dom_space_node() {
        use katex::mathml_tree::MathDomNode;
        let space = katex::mathml_tree::SpaceNode {
            width: 0.0,
            character: Some(" ".to_string()),
        };
        let result = math_dom_to_html(&MathDomNode::Space(space));
        assert_eq!(result, " ");
    }
}
