# TypePress — Header & Footer Example

A short document demonstrating running headers and footers with page
numbers via the `--header` / `--footer` CLI flags.

## Section One

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod
tempor incididunt ut labore et dolore magna aliqua.

## Section Two

Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi
ut aliquip ex ea commodo consequat.

## Section Three

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum
dolore eu fugiat nulla pariatur.

```bash
typepress examples/header-footer.md \
  --header "TypePress Example" \
  --footer "Page {page} of {pages}" \
  -o out.pdf
```

The `{page}` and `{pages}` tokens are replaced with the current page and
total page count. The header/footer text accepts inline HTML for styling,
e.g. `--footer '<span style="color:#888">Page {page} of {pages}</span>'`.
