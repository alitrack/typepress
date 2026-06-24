#!/usr/bin/env python3
"""Smart zoom calculator for TypePress — measures HTML content and picks optimal zoom.

Usage:
    python3 smart-zoom.py input.html -o output.pdf [--size A3|A4] [--min-fill 80]

Measures content by rendering at zoom=1.0 to A2, calculates optimal zoom,
then outputs a single-page PDF. Works around TypePress's hard page breaks
by merging pages with pypdf (BSD-3-Clause, MIT-compatible).
"""

import argparse
import subprocess
import sys
import tempfile
from pathlib import Path
from pypdf import PdfReader, PdfWriter, Transformation

# BSD-3-Clause — MIT compatible

PAGE_SIZES = {
    "A3": (841.89, 1190.55),
    "A4": (595.28, 841.89),
    "A3-L": (1190.55, 841.89),
    "A4-L": (841.89, 595.28),
}


def run_typepress(input_html: str, output: str, size: str, zoom: float,
                  cwd: str = "/mnt/d/wsl2/typepress") -> bool:
    """Run TypePress and return True on success."""
    result = subprocess.run(
        ["cargo", "run", "--", input_html, "-o", output, "-s", size,
         "--zoom", f"{zoom:.4f}"],
        cwd=cwd, capture_output=True, text=True, timeout=120,
    )
    return result.returncode == 0


def page_count(pdf_path: str) -> int:
    """Count pages in a PDF using pypdf."""
    reader = PdfReader(pdf_path)
    n = len(reader.pages)
    reader.stream.close()
    return n


def measure_content(input_html: str) -> tuple[float, float]:
    """Render to large canvas and measure content dimensions.

    Renders at zoom=1.0 to A2 and aggregates bounding boxes across all pages.
    Returns (width, height) in pt.
    """
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
        measure_pdf = f.name

    if not run_typepress(input_html, measure_pdf, "A2", 1.0):
        Path(measure_pdf).unlink(missing_ok=True)
        print("Measure render failed")
        sys.exit(1)

    reader = PdfReader(measure_pdf)
    all_w, all_h = [], []
    for page in reader.pages:
        w = float(page.mediabox.width)
        h = float(page.mediabox.height)
        all_w.append(w)
        all_h.append(h)

    n_pages = len(reader.pages)
    reader.stream.close()
    Path(measure_pdf).unlink()

    # Total content height = sum of all page heights
    # Content width = max page width
    content_w = max(all_w) if all_w else 595.0
    content_h = sum(all_h) if all_h else 842.0

    return content_w, content_h


def calculate_zoom(content_w: float, content_h: float,
                   page_w: float, page_h: float,
                   margin: float = 30) -> float:
    """Calculate zoom that maximizes content on one page."""
    avail_w = page_w - 2 * margin
    avail_h = page_h - 2 * margin
    zw = avail_w / content_w
    zh = avail_h / content_h
    return min(zw, zh)


def render_and_merge(input_html: str, output: str, size: str,
                     zoom: float, cwd: str) -> bool:
    """Render with TypePress and merge all pages into one tall page."""
    with tempfile.NamedTemporaryFile(suffix=".pdf", delete=False) as f:
        tmp_pdf = f.name

    if not run_typepress(input_html, tmp_pdf, size, zoom, cwd):
        Path(tmp_pdf).unlink(missing_ok=True)
        return False

    reader = PdfReader(tmp_pdf)
    n_pages = len(reader.pages)

    if n_pages == 1:
        # Already one page — just copy
        import shutil
        shutil.copy(tmp_pdf, output)
    else:
        # Merge pages into one tall page
        page_w = float(reader.pages[0].mediabox.width)
        page_h = float(reader.pages[0].mediabox.height)
        total_h = page_h * n_pages

        writer = PdfWriter()
        combined = writer.add_blank_page(width=page_w, height=total_h)

        for i in range(n_pages):
            offset_y = -(page_h * i)
            combined.merge_transformed_page(
                reader.pages[i],
                Transformation().translate(ty=offset_y)
            )

        writer.write(output)

    reader.stream.close()
    Path(tmp_pdf).unlink()
    return True


def main():
    parser = argparse.ArgumentParser(
        description="Smart zoom for TypePress — single-page output")
    parser.add_argument("input", help="Input HTML file")
    parser.add_argument("-o", "--output", required=True, help="Output PDF path")
    parser.add_argument("--size", default="A3",
                        choices=["A3", "A4", "A3-L", "A4-L"])
    parser.add_argument("--min-fill", type=float, default=75,
                        help="Minimum fill % for width (default: 75)")
    parser.add_argument("--margin", type=float, default=30,
                        help="Page margin in pt (default: 30)")
    parser.add_argument("--typepress-dir", default="/mnt/d/wsl2/typepress",
                        help="TypePress project directory")
    args = parser.parse_args()

    pw, ph = PAGE_SIZES[args.size]

    # Step 1: measure content at zoom 1.0
    print(f"Measuring: {args.input}...")
    content_w, content_h = measure_content(args.input)
    print(f"  Content: {content_w:.0f} x {content_h:.0f} pt  "
          f"({content_w/72:.1f}\" x {content_h/72:.1f}\")")

    # Step 2: calculate zoom for best width fill
    zoom = calculate_zoom(content_w, content_h, pw, ph, args.margin)
    fill_w = (content_w * zoom + 2 * args.margin) / pw * 100
    print(f"  Target:  {args.size} ({pw:.0f}x{ph:.0f} pt)")
    print(f"  Zoom:    {zoom:.3f}")
    print(f"  Fill:    {fill_w:.0f}% width")

    # Step 3: render + merge
    print(f"\nRendering...")
    if render_and_merge(args.input, args.output, args.size, zoom,
                        args.typepress_dir):
        pc = page_count(args.output)
        print(f"  Result:  {pc} page(s)")
        print(f"Done: {args.output}")
    else:
        print("Render failed")
        sys.exit(1)


if __name__ == "__main__":
    main()
