# Contributing to TypePress

## Development Setup

```bash
git clone https://github.com/alitrack/typepress.git
cd typepress
cargo build
cargo test
```

Requirements: Rust 1.85+ (2024 edition), Linux/macOS/Windows (WSL recommended).

## Spec-Driven Development

TypePress uses [OpenSpec](openspec/) workflow:

1. **Proposal** — `openspec/changes/<name>/proposal.md` — Why + What
2. **Design** — `design.md` — Architecture decisions
3. **Specs** — `specs/<capability>/spec.md` — Contracts per capability
4. **Tasks** — `tasks.md` — Checklist grouped by milestone

Before writing code for any architectural change, open an OpenSpec proposal first.

### Pre-Planning Gate

```
cargo check --lib   # Must pass
cargo test           # Must pass
cargo clippy         # Zero warnings
cargo fmt --check    # Must pass
```

## Code Style

- `cargo fmt` — Standard Rust formatting
- `cargo clippy -- -D warnings` — Zero tolerance for warnings
- Module files in `src/`, tests in `tests/`

## Pull Requests

1. Branch: `feat/short-description` or `fix/short-description`
2. OpenSpec proposal reviewed (for non-trivial changes)
3. All tests pass + no clippy warnings
4. Update CHANGELOG.md if applicable
5. Squash merge preferred

## Project Structure

```
src/
├── main.rs       # CLI binary + rendering pipeline
├── lib.rs        # Library entry point + markdown_to_html
├── css.rs        # CSS constants (default print, KaTeX math)
├── css_layout.rs # CSS Grid/Flexbox → Table preprocessor
├── config.rs     # YAML config (TypePressConfig)
├── fonts.rs      # @font-face parsing, font resolution
├── highlight.rs  # Code syntax highlighting (syntect)
└── svg.rs        # PDF → SVG Unicode text extraction
tests/
├── reftest.rs    # Integration tests
└── cli.rs        # CLI tests
```

## Dependencies

- [fulgur](https://github.com/fulgur-org/fulgur) — HTML → PDF engine (Blitz → Taffy → Krilla)
- [katex-rs](https://github.com/xkevio/katex-rs) — LaTeX math rendering
- [mermaid-rs](https://github.com/alitrack/mermaid-rs) — Mermaid diagram rendering
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) — Markdown parsing
- [syntect](https://github.com/trishume/syntect) — Code syntax highlighting

## Testing

```bash
cargo test                    # All tests
cargo test --lib              # Unit tests only
cargo test -- --nocapture     # With output
UPDATE_GOLDEN=1 cargo test    # Update golden files
```

Test assets live in `templates/` and `tests/fixtures/`.
