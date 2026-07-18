# Synchronic Web whitepaper

LaTeX sources for the next Synchronic Web whitepaper.

## Build

```sh
cd docs/whitepaper
make
```

The generated document is `whitepaper.pdf`. Build artifacts are ignored by the local `.gitignore`.

## Structure

- `main.tex` — document setup and section order
- `sections/` — independently editable section sources
- `references.bib` — bibliography
- `sand_report_template/` — Sandia report class and PDF assets

## Sandia report metadata

The draft uses the relaxed Sandia report layout with a placeholder report number. Before formal release:

1. obtain an official SAND report number and release markings;
2. confirm the template is the current internally approved version;
3. update `\SANDnum` and `\SANDprintDate` in `main.tex`.
