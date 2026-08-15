# TypePress

**Pure Rust HTML/CSS → PDF engine. No browser required.**

[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Crates.io](https://img.shields.io/crates/v/typepress.svg)](https://crates.io/crates/typepress)

TypePress renders HTML and Markdown to PDF using fulgur (Blitz → Taffy → Krilla) — zero external dependencies, no Chromium, no Node.js.

## Features

- **HTML/CSS → PDF** — Full HTML rendering with CSS styling
- **Markdown → PDF** — GFM extensions, code highlighting via syntect
- **LaTeX Math** — `$...$` and `$$...$$` rendered via katex-rs
- **Mermaid Diagrams** — 10+ diagram types via [mermaid-render](https://crates.io/crates/mermaid-render): flowchart, sequence (autonumber, loops), class (generics), state, ER, gantt, pie, timeline, mindmap, gitGraph — all pure Rust, zero JS
- **CJK Support** — Chinese/Japanese/Korean with automatic font subsetting
- **Single Binary** — ~15MB, zero dependencies, no Chromium/Node.js
- **CSS Grid/Flexbox → Table** — Automatic layout degradation for taffy compatibility
- **Header/Footer** — CSS GCPM running elements
- **@font-face** — Web font loading and embedding
- **Small Output** — 93KB PDF vs browser screenshots (MB scale)

## Quick Start

### Install

```bash
# Rust / Cargo
cargo install typepress

# npm (Node.js)
npm install typepress-pdf

# pip (Python)
pip install typepress
```

### Basic Usage

```bash
# Markdown → PDF
typepress doc.md -o out.pdf

# HTML → PDF with CJK font
typepress page.html -o out.pdf -f /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc

# With math support
typepress doc.md -o out.pdf --math

# YAML-driven workflow
typepress render  # auto-detects typepress.yaml
```

### Remote Assets & Exit Codes

Remote assets (images, CSS, web fonts, emoji) are fetched with a safety
policy that can be tightened per run:

| Flag | Default | Effect |
|---|---|---|
| `--max-asset-size N` | `10485760` (10 MiB) | Max bytes per downloaded asset; `0` = unlimited |
| `--allow-http` | off | Allow plain-http (non-TLS) fetches |
| `--asset-allowlist glob1,glob2` | none | Only fetch hosts matching these globs (`*` wildcard) |

Fetch failures never abort the render — they are collected as structured
warnings (`TP-1001` download failed, `TP-1002` zero-byte, `TP-1003` over
size cap, `TP-1004` CSS, `TP-1005` font/emoji, `TP-1007` unsized image
skipped, `TP-1009` multi-page output, `TP-1010` empty result).

Exit codes (programmable rendering):

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Fatal error |
| `2` | `--strict` mode: rendered but warnings were emitted |

`--json` emits machine-readable output; `warnings` is an object array
(`[{"code": "TP-1001", "message": "..."}]`). All recoverable failures are
reported as warnings — nothing fails silently.

### Configuration

Create `typepress.yaml` in your project root:

```yaml
input: doc.md
from: md
output:
  pdf: out.pdf
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
