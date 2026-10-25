# mmd

Render [Mermaid](https://mermaid.js.org) diagrams to SVG, for web and PDF
publication. Pure Rust: no Node, no browser, no JavaScript runtime, no C
toolchain.

> **Status:** early development, released as CalVer builds. Flowcharts,
> gitGraph, C4 (context, container, component) and ER diagrams render; see [DESIGN.md](DESIGN.md) for the
> exact supported syntax, what is deliberately out of scope, and the
> [roadmap](DESIGN.md#roadmap) for the other diagram types.

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

| Diagram | Supported |
|:---|:---|
| `flowchart`/`graph` | `TB`/`TD`/`BT`/`LR`/`RL`; every bracket shape and the v11 `@{ shape: … }` names that map to them; `-->`, `---`, `-.->`, `==>`, `--o`, `--x`, `<-->`, `~~~` with `\|labels\|` or `-- text -->`; edge ids; chains and `&`; nested `subgraph` with its own `direction`; markdown strings; `classDef`, `class`, `:::`, `style`, `linkStyle` |
| `gitGraph` | `LR`/`TB`/`BT`; `commit` (`id:`, `msg:`, `tag:`, `type:`), `branch` (`order:`), `checkout`/`switch`, `merge` (`id:`, `tag:`, `type:`), `cherry-pick`, `title` |
| `C4Context`, `C4Container`, `C4Component` | `Person`, `System`, `Container`, `Component` with their `Db`/`Queue`/`_Ext` variants; nested `*Boundary`; `Rel`, `BiRel`, `Rel_Back`, `Rel_U/D/L/R`; `UpdateLayoutConfig`; `title` |
| `erDiagram` | Entities with attribute tables (`PK`/`FK`/`UK`, comments); every cardinality, symbolic or in words; identifying and non-identifying lines; aliases; `direction`; `classDef`, `class`, `:::`, `style` |

Colours in styles are hex, `rgb()` or CSS names. Next, in
[roadmap](DESIGN.md#roadmap) order: `stateDiagram`, `classDiagram` and
`requirementDiagram` on the existing layered layout; then `sequenceDiagram`,
`gantt`, `pie` and the other self-laid-out types. Every other diagram type is
rejected today.

Anything outside that subset is a **parse error naming the construct and its
line** — never a silently missing element:

```console
$ mmd notes.mmd
mmd: notes.mmd: line 7: shape `bolt` is not supported (see DESIGN.md)
```

Adding a diagram type is a self-contained module: parse and layout to a
`Scene`, plus registration in `detect()` and `scene()`. See [DESIGN.md](DESIGN.md#adding-a-diagram-type).

## See it

```console
$ make showcase     # target/showcase.pdf — every supported diagram, plus prose
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
