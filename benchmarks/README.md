# TypePress Benchmarks

Reproducible performance numbers for the CLI and the HTTP server.

## Quick run

```bash
bash benchmarks/bench.sh            # TypePress only — no extra deps
bash benchmarks/bench.sh --puppeteer   # + Puppeteer/Chromium comparison
```

Results append to `benchmarks/results/<timestamp>.txt`.

## What is measured

| Metric | Method |
|---|---|
| Binary size | `du -h target/release/typepress` |
| Startup latency | mean of 10 `typepress --version` runs |
| Render time | mean of 5 full renders of `examples/markdown-all-features.md` (math on) |
| Output size | rendered PDF size, page count |
| Determinism | SHA-256 of two consecutive renders of the same input |

## Sample results

Latest run on WSL2 (i7, 32 GB), release build:

| Metric | TypePress |
|---|---|
| Binary size | ~15 MB |
| Startup | ~5 ms |
| Render (all-features doc) | ~50 ms |
| Output size | ~90 KB |
| Determinism | identical SHA-256 |

## HTTP server benchmark

```bash
typepress serve &
# then, e.g. 50 sequential renders:
for i in $(seq 50); do
  curl -s -X POST localhost:8787/render -H 'Content-Type: application/json' \
    -d '{"markdown":"# t","options":{}}' -o /dev/null
done
```

Thread-per-request with tiny_http: concurrency is bounded by CPU (each
render is a full pipeline pass), not by a global lock — see the 6-way
concurrency e2e test.

## Puppeteer comparison (optional)

Needs Node + Chromium + Puppeteer:

```bash
npm i puppeteer   # in a scratch dir
bash benchmarks/bench.sh --puppeteer
```

Expected qualitative differences (documented in the README comparison
table): TypePress has no browser startup cost and a ~15 MB binary vs
~300 MB for Puppeteer; browser engines win on exotic CSS; TypePress is
the only one with native Markdown/Math/Mermaid input.
