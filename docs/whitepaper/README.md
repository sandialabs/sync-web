# Synchronic Web whitepaper

LaTeX sources for the Synchronic Web whitepaper.

## Build

```sh
cd docs/whitepaper
make
```

The generated document is `whitepaper.pdf`. Build artifacts are ignored by the local `.gitignore`.

## Structure

- `main.tex` — article setup and section order
- `sections/` — independently editable section sources
- `figures/` — TikZ figures and shared styles
- `tables/` — table sources
- `algorithms/` — algorithm sources
- `references.bib` — bibliography
