// CSS constants used by TypePress for print/document styling.

/// Default CSS for print/document styling injected into Markdown output.
pub const DEFAULT_PRINT_CSS: &str = r#"
    table { border-collapse: collapse; width: 100%; }
    th { background: #eee; font-weight: bold; }
    td, th { border: 1px solid #999; padding: 4pt 8pt; text-align: left; }
    pre { background: #f5f5f5; border: 1px solid #ddd; padding: 8pt; font-family: monospace; font-size: 9pt; }
    pre code { background: none; padding: 0; }
    code { background: #f0f0f0; padding: 1pt 3pt; }
    blockquote { border-left: 3px solid #ccc; margin: 10pt 0; padding: 4pt 12pt; color: #555; }
    tr { break-inside: avoid; page-break-inside: avoid; }
    thead { display: table-header-group; }
    h2, h3 { break-after: avoid; page-break-after: avoid; }
"#;

/// Math layout CSS used by the KaTeX preprocessing pipeline.
///
/// We intentionally avoid shipping the full KaTeX box model here because
/// fulgur/krilla does not faithfully reproduce its nested positioning rules.
/// Instead, math is converted to a simpler HTML structure that preserves the
/// key layouts we can render reliably: scripts, fractions, radicals, matrices,
/// and over/under annotations.
pub const KATEX_CSS: &str = r#"
.txp-math{font-family:'TypePressMath','DejaVu Serif',serif;line-height:1.2}
.txp-math-inline{display:inline}
.txp-math-display{display:block;text-align:center;margin:1em 0}
.txp-math sup{font-size:.7em;vertical-align:.6em}
.txp-math sub{font-size:.7em;vertical-align:-.25em}
.txp-script-sup{font-size:.6em;vertical-align:.55em}
.txp-op-limits{display:inline-flex;flex-direction:column;align-items:center;vertical-align:middle;line-height:1;margin-right:.08em}
.txp-op-over,.txp-op-under{font-size:.45em;line-height:1}
.txp-op-base{line-height:.85}
.txp-script-pair{display:inline-flex;align-items:flex-start;gap:.04em;vertical-align:middle}
.txp-script-base{line-height:1}
.txp-script-stack{display:inline-flex;flex-direction:column;line-height:.8;font-size:.6em}
.txp-script-over{display:block}
.txp-script-under{display:block;margin-top:.55em}
.txp-frac{display:inline-block;vertical-align:middle;text-align:center;line-height:1;margin:0 .15em}
.txp-frac-num{display:block;padding:0 .2em .08em;border-bottom:.04em solid currentColor}
.txp-frac-den{display:block;padding:.08em .2em 0}
.txp-sqrt{display:inline-flex;align-items:flex-start;vertical-align:middle}
.txp-sqrt-glyph{font-size:1.1em;line-height:1;padding-right:.08em}
.txp-sqrt-body{display:inline-block;border-top:.04em solid currentColor;padding:.06em .1em 0 .08em}
.txp-root{display:inline-flex;align-items:flex-start;vertical-align:middle}
.txp-root>sup{margin-right:-.1em}
.txp-overunder{display:inline-flex;flex-direction:column;align-items:center;line-height:.85;vertical-align:middle}
.txp-overunder-base{line-height:1}
.txp-overunder .txp-script-over,.txp-overunder .txp-script-under{font-size:.6em}
.txp-matrix{display:inline-table;border-spacing:.35em .1em;vertical-align:middle;margin:0 .2em}
.txp-matrix td{padding:0}
.txp-menclose-box{display:inline-block;border:.04em solid currentColor;padding:.08em .2em}
.txp-math-error{color:#b42318;font-style:italic}
"#;
