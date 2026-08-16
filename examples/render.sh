#!/usr/bin/env bash
# Regenerate all example PDFs from source inputs.
#
#   bash examples/render.sh
#
# Outputs go to examples/out/ (gitignored — inputs are the source of truth;
# the gallery in README.md is regenerated from these).
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=./target/release/typepress
[[ -x "$BIN" ]] || { echo "build release first: cargo build --release"; exit 1; }

mkdir -p examples/out

echo "[render] markdown-all-features.md …"
"$BIN" examples/markdown-all-features.md --math -o examples/out/markdown-all-features.pdf 2>/dev/null

echo "[render] html-css-layout.html …"
"$BIN" examples/html-css-layout.html -o examples/out/html-css-layout.pdf 2>/dev/null

echo "[render] header-footer.md …"
"$BIN" examples/header-footer.md \
  --header "TypePress Example" \
  --footer "Page {page} of {pages}" \
  -o examples/out/header-footer.pdf 2>/dev/null

echo "done → examples/out/"
ls -la examples/out/
