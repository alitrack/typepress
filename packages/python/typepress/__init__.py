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
import tarfile
import tempfile
import zipfile
from pathlib import Path
from typing import Optional
from urllib.request import urlretrieve

__version__ = "0.4.0"

_BINARY_NAME = "typepress"
if sys.platform == "win32":
    _BINARY_NAME = "typepress.exe"

_GITHUB_RELEASES = "https://github.com/alitrack/typepress/releases/download"


def _get_cache_dir() -> Path:
    if sys.platform == "darwin":
        base = Path.home() / "Library" / "Caches" / "typepress"
    elif sys.platform == "win32":
        base = Path(os.environ.get("LOCALAPPDATA", Path.home() / "AppData" / "Local")) / "typepress"
    else:
        base = Path(os.environ.get("XDG_CACHE_HOME", Path.home() / ".cache")) / "typepress"
    base.mkdir(parents=True, exist_ok=True)
    return base


def _get_platform_tag() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    if system == "linux":
        return "linux-x86_64"
    elif system == "darwin":
        # All modern Macs are Apple Silicon. Intel Macs use Rosetta 2.
        return "macos-arm64"
    elif system == "windows":
        return "windows-x86_64"
    raise RuntimeError(f"Unsupported platform: {system} {machine}")


def _find_system_binary() -> Optional[Path]:
    which = shutil.which(_BINARY_NAME)
    if which:
        return Path(which)
    for c in [
        Path("/usr/local/bin") / _BINARY_NAME,
        Path.home() / ".local" / "bin" / _BINARY_NAME,
        Path.home() / ".cargo" / "bin" / _BINARY_NAME,
    ]:
        if c.exists():
            return c
    return None


def _download_binary(version: str = __version__) -> Path:
    cache_dir = _get_cache_dir()
    cached = cache_dir / _BINARY_NAME
    if cached.exists():
        return cached

    plat = _get_platform_tag()
    ext = "zip" if sys.platform == "win32" else "tar.gz"
    url = f"{_GITHUB_RELEASES}/v{version}/typepress-{plat}.{ext}"

    sys.stderr.write(f"Downloading TypePress v{version} for {plat}...\n")
    with tempfile.NamedTemporaryFile(suffix=f".{ext}", delete=False) as tmp:
        urlretrieve(url, tmp.name)

    if ext == "tar.gz":
        with tarfile.open(tmp.name, "r:gz") as tf:
            tf.extract("typepress", cache_dir)
    else:
        with zipfile.ZipFile(tmp.name, "r") as zf:
            zf.extract("typepress.exe", cache_dir)

    os.unlink(tmp.name)
    os.chmod(cached, 0o755)
    sys.stderr.write(f"TypePress installed to {cached}\n")
    return cached


def _resolve_binary(binary_path: Optional[str | Path] = None) -> Path:
    if binary_path:
        p = Path(binary_path)
        if not p.exists():
            raise RuntimeError(f"TypePress binary not found at {p}")
        return p

    p = _find_system_binary()
    if p is not None:
        return p

    cached = _get_cache_dir() / _BINARY_NAME
    if cached.exists():
        return cached

    return _download_binary()


# ── API ─────────────────────────────────────────────────────────────────

_FORMATS = frozenset({"pdf"})
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
        scale: float = 1.0,
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
        if size:
            cmd.extend(["--size", size])
        if landscape:
            cmd.append("--landscape")
        if margin:
            cmd.extend(["--margin", margin])
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


def main():
    """CLI entry point."""
    binary = _resolve_binary()
    os.execv(str(binary), [str(binary)] + sys.argv[1:])
