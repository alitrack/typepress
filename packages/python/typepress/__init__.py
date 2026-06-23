"""
TypePress — Pure Rust HTML/CSS → PDF engine (Python binding).

Usage:
    from typepress import TypePress
    tp = TypePress()
    tp.html_to_pdf("input.html", "output.pdf")
"""
from __future__ import annotations

import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Optional

__version__ = "0.3.0"

# ── Platform detection ──────────────────────────────────────────────────

_BINARY_NAME = "typepress"
if sys.platform == "win32":
    _BINARY_NAME = "typepress.exe"


def _get_binary_dir() -> Path:
    if sys.platform == "darwin":
        base = Path.home() / "Library" / "Caches" / "typepress"
    elif sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local")) / "typepress"
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "typepress"
    base.mkdir(parents=True, exist_ok=True)
    return base


def _find_system_binary() -> Optional[Path]:
    which = shutil.which(_BINARY_NAME)
    if which:
        return Path(which)
    candidates = [
        Path("/usr/local/bin") / _BINARY_NAME,
        Path.home() / ".local" / "bin" / _BINARY_NAME,
        Path.home() / ".cargo" / "bin" / _BINARY_NAME,
    ]
    for c in candidates:
        if c.exists():
            return c
    return None


def _resolve_binary(binary_path: Optional[str | Path]) -> Path:
    if binary_path:
        p = Path(binary_path)
    else:
        p = _find_system_binary()
        if p is None:
            cached = _get_binary_dir() / _BINARY_NAME
            if cached.exists():
                p = cached
            else:
                raise RuntimeError(
                    "TypePress binary not found. Install with: "
                    "pip install typepress or from https://github.com/alitrack/typepress"
                )
    if not p.exists():
        raise RuntimeError(f"TypePress binary not found at {p}")
    return p


# ── API ─────────────────────────────────────────────────────────────────

_FORMATS = frozenset({"pdf", "svg", "png"})
_INPUT_FORMATS = frozenset({"html", "md"})


class TypePress:
    """Pure Rust HTML/CSS → PDF engine."""

    def __init__(self, binary_path: Optional[str | Path] = None):
        self._binary = _resolve_binary(binary_path)

    @property
    def binary_path(self) -> Path:
        return self._binary

    def convert(
        self,
        input: str | Path,
        output: str | Path,
        *,
        fmt: str = "pdf",
        size: Optional[str] = None,
        landscape: bool = False,
        margin: Optional[str] = None,
        scale: float = 2.0,
        css_files: Optional[list[str | Path]] = None,
        fonts: Optional[list[str | Path]] = None,
        header: Optional[str] = None,
        footer: Optional[str] = None,
        title: Optional[str] = None,
        from_fmt: str = "html",
    ) -> subprocess.CompletedProcess:
        if fmt not in _FORMATS:
            raise ValueError(f"Unsupported format: {fmt}")
        if from_fmt not in _INPUT_FORMATS:
            raise ValueError(f"Unsupported input format: {from_fmt}")

        cmd = [str(self._binary)]
        if from_fmt == "md":
            cmd.extend(["--from", "md"])
        if fmt != "pdf":
            cmd.extend(["--format", fmt])
        if size:
            cmd.extend(["--size", size])
        if landscape:
            cmd.append("--landscape")
        if margin:
            cmd.extend(["--margin", margin])
        if scale != 2.0:
            cmd.extend(["--scale", str(scale)])
        if css_files:
            for f in css_files:
                cmd.extend(["--css", str(f)])
        if fonts:
            for f in fonts:
                cmd.extend(["--font", str(f)])
        if header:
            cmd.extend(["--header", header])
        if footer:
            cmd.extend(["--footer", footer])
        if title:
            cmd.extend(["--title", title])
        cmd.extend([str(input), "-o", str(output)])

        result = subprocess.run(cmd, capture_output=True, text=True)
        if result.returncode != 0:
            raise RuntimeError(f"TypePress failed: {result.stderr.strip()}")
        return result

    def html_to_pdf(self, input: str | Path, output: str | Path, **kwargs) -> Path:
        self.convert(input, output, fmt="pdf", from_fmt="html", **kwargs)
        return Path(output)

    def md_to_pdf(self, input: str | Path, output: str | Path, **kwargs) -> Path:
        self.convert(input, output, fmt="pdf", from_fmt="md", **kwargs)
        return Path(output)

    def html_to_svg(self, input: str | Path, output: str | Path, **kwargs) -> Path:
        self.convert(input, output, fmt="svg", from_fmt="html", **kwargs)
        return Path(output)

    def html_to_png(self, input: str | Path, output: str | Path, **kwargs) -> Path:
        self.convert(input, output, fmt="png", from_fmt="html", **kwargs)
        return Path(output)


def main():
    """CLI entry point."""
    binary = _find_system_binary()
    if binary is None:
        binary = _get_binary_dir() / _BINARY_NAME
    if not binary.exists():
        print("TypePress binary not found.", file=sys.stderr)
        sys.exit(1)
    os.execv(str(binary), [str(binary)] + sys.argv[1:])
