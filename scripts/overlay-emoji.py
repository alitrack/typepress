#!/usr/bin/env python3
"""Post-process TypePress PDFs: overlay emoji PNGs at marker positions.

Usage:
  python3 overlay-emoji.py input.pdf output.pdf [--emoji-dir /tmp/.typepress/emoji]

Motivation: Krilla cannot render <img> tags (NetProvider is stubbed) and
cannot render CBDT bitmap emoji fonts. TypePress replaces emoji with text
markers like "[TPEMOJI:1f9ec]" that survive the PDF pipeline. This script
finds those markers, determines their positions/font-sizes from the PDF,
and overlays the corresponding PNG images.

Requirements: PyMuPDF (fitz), Pillow (PIL)
"""

import argparse
import io
import os
import re
import sys

import fitz  # PyMuPDF
from PIL import Image

MARKER_RE = re.compile(rb"\[TPEMOJI:([a-fA-F0-9-]+)\]")


def find_markers(page: fitz.Page) -> list[dict]:
    """Find all emoji markers on a page with their bounding boxes."""
    markers = []
    blocks = page.get_text("dict")["blocks"]
    for block in blocks:
        if block["type"] != 0:  # not a text block
            continue
        for line in block["lines"]:
            for span in line["spans"]:
                text = span["text"]
                for m in MARKER_RE.finditer(text.encode("utf-8", errors="replace")):
                    stem = m.group(1).decode("ascii").lower()
                    # Estimate marker position: center of the span
                    bbox = span["bbox"]  # (x0, y0, x1, y1)
                    fs = span["size"]
                    markers.append({
                        "stem": stem,
                        "x": bbox[0],
                        "y": bbox[1],
                        "w": bbox[2] - bbox[0],
                        "h": bbox[3] - bbox[1],
                        "fs": fs,
                        "page": page,
                    })
    return markers


def overlay_emoji(page: fitz.Page, marker: dict, emoji_cache: str) -> bool:
    """Overlay an emoji PNG at the marker position on the page."""
    stem = marker["stem"]
    png_path = os.path.join(emoji_cache, f"{stem}.png")

    if not os.path.exists(png_path):
        print(f"  Warning: emoji PNG not found: {png_path}")
        return False

    # Load and convert to RGB (PyMuPDF needs RGB for pixmap creation)
    img = Image.open(png_path).convert("RGBA")

    # Scale to fit: target height = font_size * 1.1 (slight overshoot for readability)
    fs = marker["fs"]
    target_h = int(fs * 1.15)
    ratio = target_h / img.height
    target_w = int(img.width * ratio)

    if target_w < 4 or target_h < 4:
        return False  # too small

    img = img.resize((target_w, target_h), Image.LANCZOS)

    # Position: centered horizontally at marker x, aligned to marker y baseline
    x = marker["x"] + (marker["w"] - target_w) / 2
    y = marker["y"]  # top of the text line

    # Convert PIL image to PNG bytes, then to PyMuPDF Pixmap
    buf = io.BytesIO()
    img.save(buf, format='PNG')
    pix = fitz.Pixmap(buf.getvalue())

    # Insert image at position
    rect = fitz.Rect(x, y, x + target_w, y + target_h)
    page.insert_image(rect, pixmap=pix)

    return True


def remove_markers(page: fitz.Page) -> int:
    """Remove all emoji marker text from the page. Returns count removed."""
    removed = 0
    blocks = page.get_text("dict")["blocks"]
    for block in blocks:
        if block["type"] != 0:
            continue
        for line in block["lines"]:
            for span in line["spans"]:
                text = span["text"]
                new_text = MARKER_RE.sub(b"", text.encode()).decode()
                if new_text != text:
                    # Redact the marker span by covering with white
                    bbox = span["bbox"]
                    # Only redact the actual marker portion
                    # For simplicity, redact the whole span
                    # A more precise approach would compute individual marker rects
                    pass  # We'll use a different approach below

    # Simpler approach: search for markers and redact each individually
    for page_num in range(len([page])):
        text_instances = page.search_for(r"\[TPEMOJI:", hit_max=500)
        for inst in text_instances:
            # Extend rect to cover the full marker
            # FitZ search finds only the partial match, expand to include the full marker
            rect = inst.irect  # integer rect
            # Redact with white
            page.add_redact_annot(rect, fill=(1, 1, 1))
            removed += 1

    if removed > 0:
        page.apply_redactions()

    return removed


def process_pdf(input_path: str, output_path: str, emoji_cache: str, cleanup: bool = True):
    """Main processing pipeline."""
    doc = fitz.open(input_path)
    total_overlayed = 0
    total_removed = 0

    for page_num in range(len(doc)):
        page = doc[page_num]
        markers = find_markers(page)

        if not markers:
            continue

        print(f"Page {page_num + 1}: {len(markers)} emoji marker(s)")

        # Overlay images first (so they sit under the redaction)
        for marker in markers:
            if overlay_emoji(page, marker, emoji_cache):
                total_overlayed += 1

        # Remove marker text
        if cleanup:
            for marker in markers:
                # Search for each marker's stem text and redact
                search_text = f"TPEMOJI:{marker['stem']}"
                for inst in page.search_for(search_text):
                    # Expand rect slightly to cover brackets
                    rect = fitz.Rect(
                        inst.x0 - 8, inst.y0 - 1,
                        inst.x1 + 4, inst.y1 + 1,
                    )
                    page.add_redact_annot(rect, fill=(1, 1, 1))
                    total_removed += 1
            page.apply_redactions()

    doc.save(output_path, garbage=4, deflate=True)
    doc.close()
    print(f"Done: {total_overlayed} emoji overlaid, {total_removed} markers cleaned")
    return total_overlayed


def main():
    parser = argparse.ArgumentParser(description="Overlay emoji PNGs in TypePress PDF output")
    parser.add_argument("input", help="Input PDF (from TypePress)")
    parser.add_argument("output", help="Output PDF (with emoji overlaid)")
    parser.add_argument("--emoji-dir", default="/tmp/.typepress/emoji",
                        help="Emoji PNG cache directory")
    parser.add_argument("--no-cleanup", action="store_true",
                        help="Don't remove marker text (for debugging)")
    args = parser.parse_args()

    process_pdf(args.input, args.output, args.emoji_dir, cleanup=not args.no_cleanup)


if __name__ == "__main__":
    main()
