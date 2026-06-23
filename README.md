# TypePress

**Pure Rust HTML/CSS → PDF engine. No browser required.**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Crates.io](https://img.shields.io/crates/v/typepress.svg)](https://crates.io/crates/typepress)

TypePress renders HTML and Markdown to PDF using fulgur (Blitz → Taffy → Krilla) — zero external dependencies, no Chromium, no Node.js.

## Features

- **HTML/CSS → PDF** — Full HTML rendering with CSS styling
- **Markdown → PDF** — GFM extensions, code highlighting via syntect
- **LaTeX Math** — `$...$` and `$$...$$` rendered via katex-rs
- **Mermaid Diagrams** — Flowchart, sequence, class, state, ER diagrams
- **CJK Support** — Chinese/Japanese/Korean with automatic font subsetting
- **Multi-Format** — PDF, SVG, PNG output
- **CSS Grid/Flexbox → Table** — Automatic layout degradation for taffy compatibility
- **Header/Footer** — CSS GCPM running elements
- **@font-face** — Web font loading and embedding
- **Small Output** — 93KB PDF vs browser screenshots (MB scale)

## Quick Start

### Install

```bash
cargo install typepress
```

### Basic Usage

```bash
# Markdown → PDF
typepress doc.md -o out.pdf

# HTML → PDF with CJK font
typepress page.html -o out.pdf -f /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc

# With math support
typepress doc.md -o out.pdf --math

# PDF → SVG (multi-page)
typepress existing.pdf --format svg -o out.svg

# YAML-driven workflow
typepress render  # auto-detects typepress.yaml
```

### Configuration

Create `typepress.yaml` in your project root:

```yaml
input: doc.md
from: md
output:
  pdf: out.pdf
  svg: out.svg
page:
  size: A4
math: true
```

## Comparison

| | TypePress | wkhtmltopdf | Puppeteer | Paper Muncher |
|---|---|---|---|---|
| **No browser** | ✅ | ✅ | ❌ | ❌ |
| **Binary size** | ~15MB | ~40MB | ~300MB | ~200MB |
| **CSS Grid** | 🟡 table fallback | ✅ | ✅ | ✅ |
| **Math (KaTeX)** | ✅ | ❌ | ❌ | ❌ |
| **Mermaid** | ✅ | ❌ | ❌ | ❌ |
| **Markdown input** | ✅ | ❌ | ❌ | ❌ |
| **Output size** | 93KB | 200KB | 2MB | varies |

## Architecture

```
Markdown/HTML → CSS Layout Preprocess → Header/Footer → Math → Mermaid → Code Highlight → fulgur → PDF
                                                                               ↑
                                                                    Blitz → Taffy → Krilla
```

- **Blitz** — HTML/CSS parsing
- **Taffy** — CSS box layout engine
- **Krilla** — PDF generation
- **TypePress** — Preprocessing pipeline + CLI

## Known Limitations

Taffy (layout engine) does not yet support:
- CSS Grid (`display: grid`) — automatically converted to `<table>`
- CSS Flexbox (`display: flex`) — automatically converted to `<table>`
- CSS gradients — degraded to solid colors

These are transparent preprocess steps; your HTML renders correctly, just with simplified layout.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and workflow.

TypePress follows [OpenSpec](openspec/) spec-driven development. Changes are planned in `proposal.md` → `design.md` → `specs/` → `tasks.md` before implementation.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.

Based on [fulgur](https://github.com/fulgur-org/fulgur) (MIT/Apache-2.0).
