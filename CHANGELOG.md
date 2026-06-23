# Changelog

## [0.4.0] — Unreleased

### Added
- CSS Layout preprocessor: Grid/Flexbox → Table automatic conversion
- CSS Gradient → Solid color degradation
- SVG element protection during CSS layout processing
- `templates/agent-knowledge-map.html` — complex layout test asset
- `CONTRIBUTING.md` — development guide
- KaTeX CSS taffy compatibility (inline-flex → inline-table fallbacks)

### Changed
- Rendering pipeline: CSS Layout step before header/footer injection
- PDF output for complex layouts: 3 pages → 2 pages (dual-column preserved)

### Fixed
- KaTeX CSS: removed `display: inline-flex` and `flex-direction` (taffy-incompatible)

## [0.3.2] — 2026-06-24

### Changed
- fulgur fork upgraded to blitz-html 0.3 (native CSS flex/grid layout)
- COLR color emoji support via krilla 0.7
- CSS Layout preprocessor removed — blitz-html 0.3 natively handles flex/grid

### Added
- `--font` CLI option for custom font files (Unifont COLR emoji, CJK)
- CI: `fonts-noto-cjk` for reftest CJK coverage
- `agent-knowledge-map.html` knowledge graph layout test case

### Fixed
- CI: `libfontconfig-dev` dependency for Linux builds
- Clippy warnings resolved across workspace (28 warnings)
- Version alignment: Cargo.toml, pyproject.toml, package.json, Python/Node in-code VERSION

## [0.3.0] — 2026-06-22

### Added
- Multi-format output: SVG, PNG from PDF
- Code syntax highlighting via syntect
- @font-face parsing and web font downloading
- SVG Unicode text extraction (ToUnicode CMap)
- KaTeX system font auto-detection
- `typepress.yaml` configuration support
- `render` subcommand for YAML-driven workflows

### Fixed
- Math rendering: `$$...$$` now processed before pulldown-cmark conversion
- KaTeX MathML annotation stripping (noise in PDF output)
- Mermaid diagram standalone SVG generation

## [0.2.0] — 2026-06-15

### Added
- LaTeX math rendering via katex-rs
- Mermaid diagram rendering via mermaid-rs
- CJK font handling with automatic subsetting
- Page header/footer (CSS GCPM running elements)
- CLI: `--from md`, `--math`, `--math-dir`, `--format`, `--scale`

## [0.1.0] — 2026-06-08

### Added
- Initial release: HTML → PDF via fulgur
- Markdown → PDF via pulldown-cmark
- CLI: `--size`, `--landscape`, `--margin`, `--fonts`, `--css`
- Page size: A4, A3, Letter
- Margin parsing: CSS shorthand support
