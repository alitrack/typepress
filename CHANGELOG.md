# Changelog

## [0.4.0] — 2026-06-24

### Removed (Breaking)
- **SVG/PNG output removed** — TypePress is now PDF-only. `--format svg`/`--format png` no longer supported.
- PDF→SVG text extraction moved to independent [pdf2svg](https://github.com/alitrack/pdf2svg) project
- Removed `tiny-skia`, `resvg`, `lopdf` dependencies
- Removed `smart-zoom.py` — TypePress is pure Rust, no Python scripts

### Added
- COLRv1 color emoji native rendering (NotoColorEmoji auto-download)
- SVG font embedding for emoji fallback
- HTTP download timeouts (configurable, default 30s)
- Python subprocess timeout (60s for math/Mermaid rendering)
- npm and PyPI publish to GitHub Release workflow
- CI: format, clippy, build, test jobs

### Fixed
- CJK SVG text extraction: multi-block CMap parsing, Y-axis flip, per-page font subsets
- Interleaved CID format auto-detection in fulgur TJ streams
- UTF-16 surrogate pair handling for Type3/COLR fonts
- `--zoom` now scales CSS px values instead of transform wrapper
- `download_remote_images` counter bug (never incremented)
- Dead config fields `output.svg`/`output.png` removed
- `noyalib` → `serde_yaml` (legacy dependency)

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
