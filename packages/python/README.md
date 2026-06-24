# TypePress — Python

Pure Rust HTML/CSS → PDF engine. No browser required.

```python
from typepress import TypePress

tp = TypePress()

# HTML → PDF
tp.html_to_pdf("report.html", "report.pdf")

# With options
tp.html_to_pdf(
    "report.html",
    "report.pdf",
    size="A3",
    landscape=True,
    margin="10mm",
)

# Markdown → PDF
tp.md_to_pdf("report.md", "report.pdf")

# Full API
tp.convert(
    input="report.html",
    output="report.pdf",
    size="A4",
    landscape=False,
    margin="20mm",
)
```

## Install

```bash
pip install typepress
```

The package auto-downloads the `typepress` binary for your platform on first use.

## Requirements

- Python 3.8+
- Linux x86_64, macOS arm64/x86_64, or Windows x86_64
