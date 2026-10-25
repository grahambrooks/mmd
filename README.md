# mmd

Render [Mermaid](https://mermaid.js.org) diagrams to SVG, for web and PDF
publication. Pure Rust: no Node, no browser, no JavaScript runtime, no C
toolchain.

> **Status:** early development, released as CalVer builds. All 33 of
> mermaid's diagram types render, including the external `zenuml` plugin.
> Coverage comes before accuracy: where a drawing knowingly differs from
> mermaid's, the sample says so. See [DESIGN.md](DESIGN.md) for the exact
> syntax and the [roadmap](DESIGN.md#roadmap) for what remains.

## Why

Mermaid puts diagram labels in `<foreignObject>` — embedded HTML inside the
SVG. Every SVG-only consumer drops them, so converting mermaid output to PDF
gives you boxes and arrows with **no text at all**, and no error to tell you.

`mmd` emits real SVG `<text>`. Not as an option that can regress — it has no
other code path.

It also emits presentation attributes rather than a CSS cascade, because SVG
consumers vary in how much CSS they support.

## Usage

```console
$ mmd diagram.mmd            # writes diagram.svg
$ mmd diagram.mmd -o out.svg
$ cat diagram.mmd | mmd - -o - > out.svg
```

As a library:

```rust
let svg = mmd::render("gitGraph\n  commit\n  branch dev\n  commit\n")?;
```

Or as a laid-out [`diagram-scene`](https://github.com/grahambrooks/diagram-scene)
`Scene` (shapes, routed edges and theme roles) for a host that draws it
natively in its own colours, as the emx editor does:

```rust
let scene = mmd::scene(source, &my_measure)?;
```

Text is measured by shaping real fonts, so a label never overflows the box
drawn around it. Hosts with their own font stack can implement the `Measure`
trait and drop the bundled fonts entirely:

```toml
mmd = { version = "*", default-features = false }
```

## Supported diagrams

| Group | Diagram types |
|:---|:---|
| Graphs | `flowchart`/`graph` (every v11 shape), `stateDiagram-v2`, `classDiagram`, `erDiagram`, `requirementDiagram`, `block` |
| Architecture | `C4Context`, `C4Container`, `C4Component`, `C4Dynamic`, `C4Deployment`, `architecture-beta` |
| Interactions | `sequenceDiagram`, `zenuml`, `gitGraph`, `eventmodeling` |
| Time | `gantt`, `timeline`, `journey`, `kanban` |
| Charts | `pie`, `xychart`, `quadrantChart`, `radar-beta`, `sankey` |
| Trees and maps | `mindmap`, `treemap`, `treeView-beta`, `ishikawa`, `packet`, `venn-beta`, `wardley-beta` |
| Other | `info` |

YAML front matter is accepted: its `title` is drawn and its `config` ignored,
as `%%{init}%%` directives are. Colours in styles are hex, `rgb()` or CSS
names.

`make showcase` builds `target/showcase.pdf`. It opens with a coverage
matrix, then shows every sample with its recorded approximations, and every
known gap as the error mmd reports.

Anything outside that subset is a **parse error naming the construct and its
line** — never a silently missing element:

```console
$ mmd notes.mmd
mmd: flow.mmd: line 5: dividers (`==…==`) are not supported in zenuml (see DESIGN.md)
```

Adding a diagram type is a self-contained module: parse and layout to a
`Scene`, plus registration in `detect()` and `scene()`. See [DESIGN.md](DESIGN.md#adding-a-diagram-type).

## See it

```console
$ make showcase     # target/showcase.pdf — coverage matrix, samples and gaps
```

That PDF is the visual check the test suite cannot make. Tests prove nothing
was dropped and that boxes do not overlap; only the page proves it reads well.

## Build

```console
$ make check        # fmt-check + clippy + test — run before every commit
$ make build        # release binary at target/release/mmd
```

## Licence

MIT. Bundled fonts are Libertinus Serif (SIL OFL) and DejaVu Sans Mono, via
[`typst-assets`](https://github.com/typst/typst-assets).
