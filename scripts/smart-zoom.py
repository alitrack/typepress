#!/usr/bin/env python3
"""Smart zoom calculator for TypePress — measures HTML content and picks optimal zoom.

Usage:
    python3 smart-zoom.py input.html -o output.pdf [--size A3|A4] [--min-fill 80]

The script renders the HTML once at zoom 1.0 to a large canvas, measures content
dimensions, then re-renders with calculated zoom to maximize page fill.
"""

import argparse
import subprocess
import sys
import fitz  # PyMuPDF


PAGE_SIZES = {
    "A3": (841.89, 1190.55),      # pt
    "A4": (595.28, 841.89),
    "A3-L": (1190.55, 841.89),
    "A4-L": (841.89, 595.28),
}


def measure_content(input_html: str) -> tuple[float, float]:
    """Render to large canvas and measure content bounding box. Returns (w, h) in pt."""
    result = subprocess.run(
        ["cargo", "run", "--", input_html,
         "-o", "/tmp/typepress-measure.pdf",
         "-s", "A2", "--zoom", "1.0"],
        cwd="/mnt/d/wsl2/typepress",
        capture_output=True, text=True, timeout=120,
    )
    if result.returncode != 0:
        print(f"Measure render failed: {result.stderr}")
        sys.exit(1)

    doc = fitz.open("/tmp/typepress-measure.pdf")
    all_xs, all_ys = [], []
    for page in doc:
        blocks = page.get_text("blocks")
        for b in blocks:
            all_xs.extend([b[0], b[2]])
            all_ys.extend([b[1], b[3]])
    doc.close()

    if not all_xs:
        print("Error: no content blocks found")
        sys.exit(1)

    content_w = max(all_xs) - min(all_xs)
    content_h = max(all_ys) - min(all_ys)
    return content_w, content_h


def calculate_zoom(content_w: float, content_h: float,
                   page_w: float, page_h: float,
                   margin: float = 30) -> tuple[float, float, float]:
    """Calculate zoom that maximizes fill while keeping 1 page."""
    avail_w = page_w - 2 * margin
    avail_h = page_h - 2 * margin
    zw = avail_w / content_w
    zh = avail_h / content_h
    zoom = min(zw, zh)  # Fit by the tighter dimension
    fill_w = (content_w * zoom + 2 * margin) / page_w * 100
    fill_h = (content_h * zoom + 2 * margin) / page_h * 100
    return zoom, fill_w, fill_h


def main():
    parser = argparse.ArgumentParser(description="Smart zoom for TypePress")
    parser.add_argument("input", help="Input HTML file")
    parser.add_argument("-o", "--output", required=True, help="Output PDF path")
    parser.add_argument("--size", default="A3", choices=["A3", "A4", "A3-L", "A4-L"])
    parser.add_argument("--min-fill", type=float, default=75,
                        help="Minimum fill percentage (default: 75)")
    parser.add_argument("--margin", type=float, default=30,
                        help="Page margin in pt (default: 30)")
    parser.add_argument("--typepress", default="cargo run --release --",
                        help="TypePress command prefix")
    args = parser.parse_args()

    pw, ph = PAGE_SIZES[args.size]

    # Step 1: measure
    print(f"Measuring content dimensions for {args.input}...")
    content_w, content_h = measure_content(args.input)
    print(f"  Content: {content_w:.0f} x {content_h:.0f} pt  ({content_w/72:.1f}\" x {content_h/72:.1f}\")")

    # Step 2: calculate
    zoom, fw, fh = calculate_zoom(content_w, content_h, pw, ph, args.margin)
    print(f"  Target:  {args.size} ({pw:.0f}x{ph:.0f} pt)")
    print(f"  Zoom:    {zoom:.3f}")
    print(f"  Fill:    {fw:.0f}% width, {fh:.0f}% height")

    if fw < args.min_fill and fh < args.min_fill:
        # Try page-size-up if undersized
        for alt, (apw, aph) in PAGE_SIZES.items():
            if alt == args.size:
                continue
            az, afw, afh = calculate_zoom(content_w, content_h, apw, aph, args.margin)
            if afw >= args.min_fill and afh >= args.min_fill:
                print(f"  → Switching to {alt}: zoom={az:.3f}, fill={afw:.0f}%W {afh:.0f}%H")
                pw, ph = apw, aph
                zoom, fw, fh = az, afw, afh
                break
        else:
            # None ideal — use best of available
            best = max(
                [(alt, *calculate_zoom(content_w, content_h, apw, aph, args.margin))
                 for alt, (apw, aph) in PAGE_SIZES.items() if 'L' not in alt],
                key=lambda x: min(x[2], x[3])
            )
            print(f"  → No ideal size, using {best[0]}: zoom={best[1]:.3f}, fill={best[2]:.0f}%W {best[3]:.0f}%H")

    # Step 3: render
    size_flag = args.size.replace("-L", "")
    landscape = "-l" if args.size.endswith("-L") else ""
    cmd = f"cargo run -- {args.input} -o {args.output} -s {size_flag} --zoom {zoom:.4f} {landscape}"
    if landscape:
        cmd = cmd.replace(f"-s {size_flag}", f"-s {size_flag} -l")
    print(f"\nRendering: {cmd}")
    result = subprocess.run(cmd, shell=True, cwd="/mnt/d/wsl2/typepress",
                            capture_output=True, text=True, timeout=120)
    if result.returncode != 0:
        print(f"Render failed: {result.stderr}")
        sys.exit(1)

    # Step 4: verify
    doc = fitz.open(args.output)
    pages = len(doc)
    # Verify actual fill
    blocks = doc[0].get_text("blocks")
    if blocks:
        xs = [b[0] for b in blocks] + [b[2] for b in blocks]
        ys = [b[1] for b in blocks] + [b[3] for b in blocks]
        actual_w = max(xs) - min(xs)
        actual_h = max(ys) - min(ys)
        actual_fw = 100 * actual_w / doc[0].rect.width
        actual_fh = 100 * actual_h / doc[0].rect.height
        print(f"  Result: {pages} page(s), actual fill {actual_fw:.0f}%W {actual_fh:.0f}%H")
    doc.close()

    print(f"Done: {args.output}")


if __name__ == "__main__":
    main()
