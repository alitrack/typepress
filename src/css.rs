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
///
/// v0.4: Replaced display:inline-flex/inline-block with inline-table
/// fallbacks because taffy (layout engine) does not support Flexbox.
pub const KATEX_CSS: &str = r#"
.txp-math{font-family:'TypePressMath','DejaVu Serif',serif;line-height:1.2}
.txp-math-inline{display:inline}
.txp-math-display{display:block;text-align:center;margin:1em 0}
.txp-math sup{font-size:.7em;vertical-align:.6em}
.txp-math sub{font-size:.7em;vertical-align:-.25em}
.txp-script-sup{font-size:.6em;vertical-align:.55em}
.txp-op-limits{display:inline-table;vertical-align:middle;line-height:1;margin-right:.08em}
.txp-op-over,.txp-op-under{font-size:.45em;line-height:1;display:table-row}
.txp-op-base{line-height:.85}
.txp-script-pair{display:inline-table;vertical-align:top}
.txp-script-pair td{padding:0}
.txp-script-base{line-height:1}
.txp-script-stack{display:inline-table;line-height:.8;font-size:.6em}
.txp-script-stack td{padding:0}
.txp-script-over{display:table-row}
.txp-script-under{display:table-row}
.txp-frac{display:inline-table;vertical-align:middle;text-align:center;line-height:1;margin:0 .15em}
.txp-frac td{padding:0}
.txp-frac-num{padding:0 .2em .08em;border-bottom:.04em solid currentColor}
.txp-frac-den{padding:.08em .2em 0}
.txp-sqrt{display:inline-table;vertical-align:middle}
.txp-sqrt td{padding:0}
.txp-sqrt-glyph{font-size:1.1em;line-height:1;padding-right:.08em;vertical-align:top}
.txp-sqrt-body{border-top:.04em solid currentColor;padding:.06em .1em 0 .08em;vertical-align:top}
.txp-root{display:inline-table;vertical-align:top}
.txp-root td{padding:0}
.txp-root>sup{margin-right:-.1em}
.txp-overunder{display:inline-table;line-height:.85;vertical-align:middle}
.txp-overunder td{padding:0}
.txp-overunder-base{line-height:1}
.txp-overunder .txp-script-over,.txp-overunder .txp-script-under{font-size:.6em;display:table-row}
.txp-matrix{display:inline-table;border-spacing:.35em .1em;vertical-align:middle;margin:0 .2em}
.txp-matrix td{padding:0}
.txp-menclose-box{display:inline-block;border:.04em solid currentColor;padding:.08em .2em}
.txp-math-error{color:#b42318;font-style:italic}
"#;
