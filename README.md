# TypePress

**Pure Rust HTML/CSS → PDF/SVG/PNG engine. No browser required.**

Powered by [fulgur](https://github.com/fulgur-rs/fulgur) (Blitz → Taffy → Krilla), TypePress adds Markdown input, LaTeX math, Mermaid diagrams, code syntax highlighting, CJK font handling, and multi-format output on top.

## Features

- **Zero system dependencies** — `cargo build --release`, drop the binary, done
- **Markdown to PDF** — GFM tables, fenced code blocks, LaTeX math (`$...$` / `$$...$$`)
- **Mermaid diagrams** — flowchart, sequence, class, ER, state, Gantt, pie (rendered as standalone SVG)
- **Code highlighting** — 30+ languages via syntect (base16-ocean-dark theme)
- **CJK fonts** — automatic font subsetting for Chinese/Japanese/Korean
- **@font-face** — CSS web font loading with automatic download
- **Headers & footers** — CSS GCPM running elements, 16 @page margin box positions
- **Multi-format** — PDF (direct), SVG, PNG (with custom scale)
- **YAML workflow config** — `typepress.yaml` with page size, margins, metadata, fonts
- **Reftest framework** — 14 automated regression tests

## Quick Start

```bash
cargo install typepress

# Markdown → PDF
typepress README.md --from md -o readme.pdf

# With math rendering
typepress paper.md --from md --math -o paper.pdf

# With header/footer
typepress report.md --from md --header "My Report" --footer "Page {page}" -o report.pdf

# SVG output
typepress doc.md --from md --format svg -o doc.svg

# High-DPI PNG
typepress doc.md --from md --format png --scale 3 -o doc.png

# YAML config
typepress -c typepress.yaml
```

## CLI Reference

| Flag | Description |
|------|-------------|
| `--from md\|html` | Input format (default: html) |
| `-o, --output` | Output path (`-` for stdout) |
| `-F, --format pdf\|svg\|png` | Output format (default: pdf) |
| `-s, --size A4\|Letter\|A3` | Page size |
| `--landscape` | Landscape orientation |
| `--margin "20"` | Page margins in mm |
| `--font file.ttf` | Bundle font (repeatable) |
| `--css style.css` | Include CSS (repeatable) |
| `--header "text"` | Running header |
| `--footer "text"` | Running footer |
| `--math` | Enable LaTeX math rendering |
| `--math-dir path/` | KaTeX fonts directory |
| `-c, --config` | YAML config file |
| `--title`, `--author`, `--language` | PDF metadata |
| `--scale 2.0` | PNG scale factor |

## YAML Config (typepress.yaml)

```yaml
page:
  size: A4
  margin: [20, 25, 20, 25]  # top, right, bottom, left (mm)

metadata:
  title: "My Document"
  authors: ["Alice"]
  language: "zh-CN"

fonts:
  - "fonts/NotoSansSC-Regular.ttf"
  - "fonts/FiraCode-Regular.ttf"

header: "TypePress"
footer: "Page {page} of {pages}"

pdf:
  bookmarks: true
  tagged: false
```

## Architecture

```
Markdown → pulldown-cmark → Math (katex-rs) → Highlight (syntect)
    ↓
  HTML + CSS
    ↓
  fulgur (Blitz DOM → Taffy layout → Krilla rendering)
    ↓
  PDF / SVG / PNG
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
