# Changelog

All notable changes to TypePress are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versioning
follows [Semantic Versioning](https://semver.org/).

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
