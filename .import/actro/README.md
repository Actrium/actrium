# mermaid-little

[中文](README.zh.md) | English

A lightweight Rust reimplementation of [Mermaid](https://mermaid.js.org/),
targeting byte-exact SVG output parity with upstream `mermaid@11.14.0`.

## What Is This

mermaid-little takes `.mmd` source text and produces `.svg` output —
the same as Mermaid, but as a native Rust library + CLI with **zero
JS / DOM dependency at runtime**. Sibling project to
[plantuml-little](https://github.com/kookyleo/plantuml-little) and built
on top of the complete dagre.js port at
[dagre-rs](https://github.com/kookyleo/dagre-rs).

## Status

**Scaffolding phase — no diagram types rendered yet.** This repo
currently contains the workspace skeleton, the dependency survey, and
the phased execution plan. See [FEATURES.md](FEATURES.md) for the full
support matrix and roadmap.

| | |
|---|---|
| Upstream version | `mermaid@11.14.0` (`2b9d054d`, tagged 2026-04-01) |
| Target diagrams | 24 of 25 (architecture deferred; see plan) |
| Layout backend | [`dagre-rs`](https://github.com/kookyleo/dagre-rs) |
| Reference tests | 0 (pipeline coming in Phase 1) |

## Non-Goals

- ELK layout (opt-in upstream; add later if demand warrants)
- Architecture diagram (requires cytoscape; no Rust equivalent)
- KaTeX formulas, rough.js hand-drawn look (placeholders for MVP)
- Runtime DOM, JS interop, headless chromium

## Acknowledgments

This project is an independent Rust reimplementation of
[Mermaid](https://mermaid.js.org/), created by Knut Sveidqvist. We
deeply appreciate the Mermaid team's work in making diagram-as-code
accessible to everyone. All specification-level behavior follows the
upstream standard.

The layout backend is [`dagre-rs`](https://github.com/kookyleo/dagre-rs),
a complete Rust port of dagre.js. The font metric pipeline
(`src/font_data.rs`, `src/font_metrics.rs`) is vendored from the sister
project [plantuml-little](https://github.com/kookyleo/plantuml-little) —
the same DejaVu Sans glyph advance tables anchor both projects, which
keeps byte-exact output consistent across the two codebases.

Thanks also to the prior-art community Rust mermaid ports —
[mermaid-rs-renderer (mmdr)](https://github.com/1jehuang/mermaid-rs-renderer),
[selkie](https://github.com/btucker/selkie),
[mmdflux](https://github.com/kevinswiber/mmdflux) — for charting this
design space. mermaid-little aims at a different point of the tradeoff
(byte-exact parity with upstream first, performance second) but we
expect to consult their source when stuck on specific diagram types
and will cite such references in commit messages.

## License

MIT, same as upstream Mermaid. See [LICENSE](LICENSE).
