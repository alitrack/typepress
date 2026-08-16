# Changelog

All notable changes to TypePress are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning
follows [Semantic Versioning](https://semver.org/).

## [0.5.0] - 2026-08-16

### Added

- **HTTP rendering server** (`typepress serve`) — turn TypePress into a
  long-running service:
  - `POST /render` — JSON body `{"html": "…"}` or `{"markdown": "…"}`
    plus render options → `application/pdf` (inline bytes, no filesystem
    writes).
  - `GET /healthz` — liveness probe for orchestrators.
  - Request body cap (`--max-body`, default 10 MiB, HTTP 413 over).
  - Per-request options mirror the CLI flags (size / landscape / margin /
    header / footer / math / fit / autofit / zoom / strict …).
  - `strict: true` returns HTTP 422 with the warning list instead of a
    PDF when any diagnostic fires.
  - Remote assets governed per-request by the same AssetLimits contract
    as the CLI (`max_asset_size` / `allow_http` / `asset_allowlist`).
  - Binds `127.0.0.1` by default; thread-per-request (tiny_http, no
    tokio/hyper dependency — keeps the single-binary footprint).
- **Docker image** (`Dockerfile`) — multi-stage build:
  - `docker build -t typepress .` → `docker run -p 8787:8787 typepress`.
  - Runtime image includes Noto CJK / Core / Color-Emoji fonts **plus
    `fontconfig`** (fontdb discovers font directories via
    `/etc/fonts/fonts.conf` — without it the PDF renders blank).
  - HEALTHCHECK against `/healthz`.
- **`src/render.rs`** — the full preprocessing pipeline extracted from
  `main.rs` into a reusable `render_document()` shared by CLI and server
  (Markdown/HTML dual path, math, mermaid, highlight, fonts, assets,
  constrain, engine, fit/autofit). No behavior change for the CLI.

### Fixed

- Container rendering produced blank PDFs when fontconfig was absent —
  documented in the Dockerfile and guarded by a runtime test.

### Verified

- 137 unit/integration tests pass (138 with `mermaid-render`), golden
  PDF baseline unchanged, `cargo clippy -D warnings` clean.
- Server e2e: healthz, markdown + HTML + math renders, 400/404/413
  error branches, 6-way concurrency — all pass.
- Docker e2e: CJK + math PDF rendered in-container, text-layer verified.

[0.5.0]: https://github.com/alitrack/typepress/compare/v0.4.0...v0.5.0

## [0.4.0] - 2026-08-15

### Breaking

- `--json` `warnings` is now an object array
  (`[{"code": "TP-1001", "message": "..."}]`) instead of a flat string
  array — machine consumers must parse the new shape.
- `--strict` now exits with code `2` when any warning was emitted
  (previously it exited `1` when an image was constrained). Exit-code
  contract: `0` success, `1` fatal error, `2` strict-with-warnings.

### Added

- Structured diagnostics: every recoverable failure is reported as a
  warning with a stable code (TP-1001…TP-1010) — nothing fails silently.
- Resource limits for remote assets:
  - `--max-asset-size N` (default 10 MiB, `0` = unlimited) with
    Content-Length pre-check, body post-check, and cache-bypass guard.
  - https-only by default; `--allow-http` opts into plain http.
  - `--asset-allowlist glob1,glob2` host allowlist (`*` wildcard,
    comma-separated or repeatable).
  - 5-hop redirect cap on all download paths (images, CSS, fonts, emoji).
- Vendor `blitz-html` to collect HTML parse errors instead of printing
  raw `ERROR:` lines to stdout (which corrupted `--json` output).

### Fixed

- `--json` output is now clean machine JSON — no stray `ERROR:` lines.
- Cached remote assets are re-validated against the size cap.
- `fonts` module lives in the library crate (was duplicated between
  lib and bin, causing inconsistent resolution).

### Verified

- CJK searchability: Type0 CID fonts with ToUnicode CMaps; text layer
  (BT/Tj) confirmed for Simplified/Traditional/GBK rare characters;
  copy-paste roundtrip lossless; deterministic output (identical
  SHA-256 across runs).

[0.4.0]: https://github.com/alitrack/typepress/compare/v0.3.0...v0.4.0
