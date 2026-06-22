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

/// KaTeX CSS — math formula styles. Only included when --math is active.
pub const KATEX_CSS: &str = r#"
.katex-display{display:block;text-align:center;margin:1em 0}
.katex-display>.katex{display:inline-block;text-align:initial}
.katex-inline{display:inline}
.katex{font:normal 1.21em KaTeX_Main,Times New Roman,serif;line-height:1.2;text-indent:0}
"#;
