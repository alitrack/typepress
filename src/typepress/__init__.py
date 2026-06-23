"""
TypePress — Pure Rust HTML/CSS → PDF engine.

Usage:
    from typepress import convert
    convert("input.html", "output.pdf", format="pdf")
    convert("input.md", "output.svg", format="svg")

Or via CLI:
    python -m typepress input.html -o output.pdf
"""

import subprocess
import sys
import os
import platform
from pathlib import Path

_BIN_DIR = Path(__file__).parent / "bin"


def _get_binary() -> Path:
    """Return path to native typepress binary, downloading if needed."""
    name = "typepress.exe" if sys.platform == "win32" else "typepress"
    return _BIN_DIR / name


def _get_triple() -> str:
    p = sys.platform
    a = platform.machine().lower()
    if p == "linux" and a == "x86_64":
        return "linux-x86_64"
    if p == "darwin" and a in ("arm64", "aarch64"):
        return "macos-arm64"
    if p == "darwin" and a == "x86_64":
        return "macos-x86_64"
    if p == "win32" and a in ("x86_64", "amd64"):
        return "windows-x86_64"
    raise RuntimeError(f"Unsupported platform: {p}/{a}")


def _download(version: str) -> None:
    import urllib.request
    import tarfile
    import io

    _BIN_DIR.mkdir(parents=True, exist_ok=True)
    triple = _get_triple()
    ext = "zip" if sys.platform == "win32" else "tar.gz"
    url = f"https://github.com/alitrack/typepress/releases/download/v{version}/typepress-{triple}.{ext}"

    print(f"Downloading typepress v{version} for {triple}...", file=sys.stderr)
    with urllib.request.urlopen(url) as resp:
        data = resp.read()

    if sys.platform == "win32":
        import zipfile
        with zipfile.ZipFile(io.BytesIO(data)) as zf:
            zf.extractall(_BIN_DIR)
    else:
        with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as tf:
            tf.extractall(_BIN_DIR)

    bin_path = _get_binary()
    if not sys.platform == "win32":
        bin_path.chmod(0o755)

    (_BIN_DIR / ".version").write_text(version)


def _ensure_binary() -> Path:
    from importlib.metadata import version
    ver = version("typepress")
    bin_path = _get_binary()
    version_file = _BIN_DIR / ".version"

    if bin_path.exists() and version_file.exists():
        if version_file.read_text().strip() == ver:
            return bin_path

    _download(ver)
    return bin_path


def convert(
    input_path: str,
    output_path: str,
    *,
    format: str = "pdf",
    scale: float = 2.0,
    margin: str | None = None,
    size: str | None = None,
    landscape: bool = False,
    header: str | None = None,
    footer: str | None = None,
    css: list[str] | None = None,
) -> subprocess.CompletedProcess:
    """Convert HTML/Markdown to PDF/SVG/PNG.

    Args:
        input_path: Path to input HTML or Markdown file
        output_path: Output file path
        format: Output format — "pdf" (default), "svg", or "png"
        scale: Scale factor for PNG output (default 2.0)
        margin: CSS margin shorthand e.g. "20mm" or "10 20 30 40"
        size: Page size e.g. "A4", "Letter"
        landscape: Landscape orientation
        header: Header text (center, every page)
        footer: Footer text (center, every page)
        css: Additional CSS files to include

    Returns:
        CompletedProcess with returncode, stdout, stderr
    """
    binary = _ensure_binary()
    cmd = [str(binary), input_path, "-o", output_path, "-F", format, "--scale", str(scale)]

    if margin:
        cmd += ["--margin", margin]
    if size:
        cmd += ["-s", size]
    if landscape:
        cmd += ["--landscape"]
    if header:
        cmd += ["--header", header]
    if footer:
        cmd += ["--footer", footer]
    for f in (css or []):
        cmd += ["--css", f]

    return subprocess.run(cmd, capture_output=True, text=True)


def main() -> None:
    """CLI entry point — delegates to native binary."""
    binary = _ensure_binary()
    args = sys.argv[1:]
    os.execv(str(binary), [str(binary)] + args)
