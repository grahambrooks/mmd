# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

`mmd` renders Mermaid diagrams to SVG for web and PDF publication.
[DESIGN.md](DESIGN.md) holds the locked scope decisions and the **supported
syntax subset** — read it before adding anything, and record new scope
decisions there.

## The two constraints that shape everything

1. **Pure Rust, no external toolchain.** No Node, no browser, no JS runtime, no
   C. This is why `sebastian` — the most complete Rust port of mermaid.js — is
   a reference rather than a dependency: it compiles C through `core-math-sys`
   for V8-identical trigonometry.
2. **SVG `<text>`, never `<foreignObject>`, and never a CSS cascade.** Mermaid
   puts labels in embedded HTML, which every SVG-only consumer drops silently —
   boxes and arrows survive, all text vanishes, no error. This crate exists
   because of that. It must stay true by construction, not by a config flag
   that can regress. `diagram_scene::svg::to_svg` is the only place that
   writes markup; there is a test asserting no `foreignObject` and no
   `<style>` in output.

## Architecture

```
source ──parse──> model ──layout──> Scene ──diagram_scene::svg──> SVG
```

Each diagram type owns its `parse` and `layout` and shares only a thin base:
`text` (the `Measure` trait plus bundled fonts), `layered` (the layered
graph layout flowcharts and ER use) and `style` (`classDef`/`style`
declarations and CSS colours). The `Scene`, its shapes and markers, the theme
and the SVG writer live in the `diagram-scene` crate, shared with puml and
forge.

**Layout stops at a `Scene`** — positioned shapes and text in diagram
coordinates. That is what lets layout be tested without string-matching SVG,
and it keeps a non-SVG consumer possible later.

Two things worth knowing before editing a layout:

- **Paint order is load-bearing.** Lines are pushed before boxes so box fills
  cover the line ends; relationship labels are pushed *after* boxes so a box
  cannot cover a label. Getting this wrong looks like clipped text.
- **Bounds must include what hangs outside the lanes.** gitGraph tags sit above
  their commit and labels below it, so the first lane's origin reserves
  headroom and the height reserves room for the last label. A wrong bound
  silently clips at the SVG edge.

## Adding a diagram type

`src/<type>/{mod,parse,layout}.rs`, a `Kind` variant with arms in `detect()`
and `scene()`, a sample in `examples/`, and a case in `examples/showcase.rs`.
Nothing else should need to change — if it does, the base is wrong.

The order of new types is the **Roadmap** in DESIGN.md — update it when a type
lands. Sebastian's measurements shaped it: C4, gitGraph, sequence, gantt and
most chart types need no graph-layout engine. State, class and requirement
need one, but `layered` already exists. ER proved that, so they don't need
`dagre` either.

## Unsupported is an error, not an omission

Anything outside the subset in DESIGN.md must fail with a line number and name
the construct — `` shape `bolt` is not supported (see DESIGN.md) ``. That
includes values: a colour that cannot be read is an error, not a fallback to
the theme. The few statements that are accepted but not drawn are listed in
DESIGN.md, and none of them may lose an element. A diagram
that quietly loses an element is worse than one that refuses to render.

## Commands

```bash
make check          # fmt-check + clippy + test — the pre-commit gate
make showcase       # target/showcase.pdf — every diagram plus prose
cargo test --test render          # integration tests only
cargo build --no-default-features # must keep working: no bundled fonts
```

**Run `make check` before every commit** and only commit once it prints
`All pre-commit checks passed.`

**Look at the PDF when you change a layout.** Tests prove nothing was dropped
and that boxes do not overlap; they cannot tell you a label collided with a
curve or that a tag was clipped at the top edge. `make showcase`, then render
page 1 (`qlmanage -t -s 1400 -o . target/showcase.pdf`) and look. The PDF
has one page per sample and `qlmanage` renders only the first, so move the
sample you changed to the front while you look, then put it back.
Don't judge text fit from `rsvg-convert` or a browser: they substitute fonts
for the bundled ones, so tables and labels can look overfull when they are
not.

## Reference material

`mermaid/` is a checkout of the upstream mermaid.js repo, gitignored, kept for
grammar reference. The authoritative sources for the current subset are
`packages/parser/src/language/gitGraph/gitGraph.langium` and
`packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison`,
`packages/mermaid/src/diagrams/flowchart/parser/flow.jison` and
`packages/mermaid/src/diagrams/er/parser/erDiagram.jison`. Check the grammar
there rather than inferring syntax from examples.

## Git and release

- **Trunk-based: commit and push directly to `main`.** PRs are not required.
- **CalVer** (`2026.10.2`), stamped into `Cargo.toml` by `make release`;
  `Formula/mmd.rb` is generated by `scripts/release.py` — do not edit it.
- **`publish = false` is deliberate** — no crates.io until beta.
- **MSRV tracks latest stable**, pinned in `rust-toolchain.toml`.

## Conventions vault

`~/knowledge-base` holds the portfolio-wide conventions. Start from the task
page — `wiki/when-starting-a-new-project.md`, `wiki/when-cutting-a-release.md`,
`wiki/when-setting-up-git-and-ci.md`, `wiki/when-you-need-to-trust-a-result.md`.
Never edit vault files directly; mutations go through the `claude-obsidian`
skills.
