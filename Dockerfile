# ─────────────────────────────────────────────────────────────
# TypePress — HTTP rendering server image (P2, v0.5.0)
#
# Multi-stage:
#   1. builder — compile the Rust binary (cached deps layer)
#   2. runtime — debian slim + fonts + binary, run `typepress serve`
#
# Build:
#   docker build -t typepress .
# Run:
#   docker run --rm -p 8787:8787 typepress
#   curl -X POST localhost:8787/render -H 'Content-Type: application/json' \
#     -d '{"markdown":"# Hi"}'
# ─────────────────────────────────────────────────────────────

# ── Stage 1: builder ──
# Edition 2024 requires rustc ≥1.85; use latest stable for long support.
FROM rust:1.97-bookworm AS builder

WORKDIR /build
# Cache dependencies first (Cargo.toml/lock change → rebuild deps only)
COPY Cargo.toml Cargo.lock ./
COPY deps/ ./deps/
# dummy src so `cargo build` can fetch & compile the dep graph
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs && \
    cargo build --release 2>/dev/null || true

# Real sources → real build
COPY src/ ./src/
RUN cargo build --release

# ── Stage 2: runtime ──
FROM debian:bookworm-slim

# Fonts required by TypePress: CJK (Chinese/Japanese/Korean), math (KaTeX
# fonts ship inside the binary), emoji, and core typefaces. Noto CJK is the
# big one (~60MB) — essential for non-Latin documents.
# CRITICAL: `fontconfig` must be installed too — fontdb (via fontconfig-parser)
# reads /etc/fonts/fonts.conf to discover font directories; without it the
# system-font scan finds nothing and the PDF renders blank.
RUN apt-get update && apt-get install -y --no-install-recommends \
        fonts-noto-cjk \
        fonts-noto-core \
        fonts-noto-color-emoji \
        fontconfig \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && fc-cache -f >/dev/null 2>&1 || true

# The binary bundles its own fonts (built-in) — the system fonts above are a
# complement for fallback coverage.
COPY --from=builder /build/target/release/typepress /usr/local/bin/typepress

# Serve API port (bind 0.0.0.0 inside the container; expose via -p)
EXPOSE 8787

# Health check for orchestrators
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -sf http://127.0.0.1:8787/healthz || exit 1

# Default: HTTP server on 0.0.0.0:8787 (local-only by default is impossible
# in a container — the network namespace is the isolation boundary).
ENTRYPOINT ["typepress"]
CMD ["serve", "--host", "0.0.0.0", "--port", "8787"]
