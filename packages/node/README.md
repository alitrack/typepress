# TypePress PDF

Pure Rust HTML/CSS → PDF engine. No browser required.

```typescript
import { TypePress } from 'typepress-pdf';

const tp = new TypePress();

// HTML → PDF
await tp.htmlToPdf('report.html', 'report.pdf', {
  size: 'A3',
  landscape: true,
});

// Markdown → PDF
await tp.mdToPdf('README.md', 'readme.pdf');
```

## Install

```bash
npm install typepress-pdf
```

The package auto-downloads the `typepress` binary for your platform on first use.

## API

### `new TypePress(binaryPath?: string)`

Create a TypePress instance. Auto-discovers or downloads the binary.

### `tp.convert(input, output, options?)`

Convert HTML/Markdown → PDF.

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `size` | `string` | — | Page size: A4, A3, Letter |
| `landscape` | `boolean` | `false` | Landscape orientation |
| `margin` | `string` | — | Margins e.g. `'20mm'` |
| `inputFormat` | `'html' \| 'md'` | `'html'` | Input format |

### `tp.htmlToPdf(input, output, options?)`
### `tp.mdToPdf(input, output, options?)`

Convenience methods. Return the output path.

## Links

- [GitHub](https://github.com/alitrack/typepress)
- [crates.io](https://crates.io/crates/typepress)
- [PyPI](https://pypi.org/project/typepress)
