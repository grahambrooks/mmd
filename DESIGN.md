# mmd — design

`mmd` renders [Mermaid](https://mermaid.js.org) diagrams to SVG, for web and
PDF publication. Pure Rust: no Node, no browser, no JS runtime, and no C
toolchain.

## Locked scope decisions

Decided 2026-08-18 at project kickoff.

1. **Visual fidelity, not byte-equivalence.** Output should look like the
   diagram mermaid draws; it is explicitly *not* a goal to match mermaid-cli
   byte for byte. This decision is what keeps the crate small — see
   [Why this is small](#why-this-is-small).
2. **SVG only.** The crate emits an SVG string and nothing else. No PDF stack,
   no rasteriser, no HTML. PDF consumers convert the SVG themselves (md2pdf
   does it with `usvg` + `krilla-svg`); the showcase example does this, but
   only as a dev-dependency. The library must never grow a PDF dependency.
3. **SVG `<text>` only — never `<foreignObject>`.** Mermaid puts labels in
   embedded HTML, which is why its output loses every label when converted by
   `usvg` or any other SVG-only consumer. Emitting real `<text>` is the point
   of this crate; it must stay true by construction rather than by a config
   flag that can regress.
4. **No CSS cascade.** Styling is emitted as presentation attributes on each
   element. SVG consumers vary in their CSS support (`usvg`'s is limited), and
   a stylesheet is a second way for output to be wrong.
5. **Text is measured, not estimated.** Node sizes come from real shaping via
   `rustybuzz`, not from emulated browser font metrics. Sizes therefore differ
   from mermaid's, which is accepted under decision 1.
6. **Diagram types are independent modules** over a small shared base. Adding
   a type must not require touching another type. See
   [Adding a diagram type](#adding-a-diagram-type).
7. **No crates.io until beta.**

### Deliberately not being built

Recorded so a later session does not "complete" them by mistake: byte-exact
mermaid-cli output; mermaid's theme engine, `%%{init}%%` directives or
`classDef`/style overrides; the hand-drawn ("rough") look; HTML labels; font
fallback across scripts; interactive features (click handlers, tooltips);
animation; and a CSS-based stylesheet of any kind.

## Supported syntax

This is the contract. **Anything outside it is a parse error with a line
number, never a silent omission.** Grammar checked against the upstream
sources in `mermaid/` — `packages/parser/src/language/gitGraph/gitGraph.langium`
and `packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison`.

### gitGraph

```
gitGraph
    commit
    commit id: "id"
    commit tag: "v1.0"
    commit type: NORMAL | REVERSE | HIGHLIGHT
    branch <name>
    checkout <name>
    switch <name>
    merge <name>
```

`commit` accepts any combination of `id:`, `tag:` and `type:` on one line.
Comments (`%%`) and blank lines are skipped. Quotes around names are optional.

**Not supported:** `cherry-pick`, `branch ... order:`, `msg:`, the `LR`/`TB`/`BT`
direction argument (left-to-right is always used), `title`, and
accessibility statements.

### C4

```
C4Context
    title <text>
    Person(alias, "label", "description")
    Person_Ext(alias, "label", "description")
    System(alias, "label", "description")
    System_Ext(alias, "label", "description")
    SystemDb(alias, "label", "description")
    Rel(from, to, "label")
    Rel(from, to, "label", "technology")
```

The description argument is optional on every element. Quotes are optional on
arguments containing no comma.

**Not supported:** `C4Container`, `C4Component`, `C4Dynamic`, `C4Deployment`;
any `*_Boundary`; `Container*`/`Component*`/`Node*` elements; `BiRel` and the
directional `Rel_U`/`Rel_D`/`Rel_L`/`Rel_R`/`Rel_Back`; `UpdateElementStyle`
and `UpdateLayoutConfig`.

### flowchart

```
flowchart LR
    A[Start] --> B{Decide}
    B -->|yes| C([Done])
    B -- no --> D[[Retry]]
    subgraph tools [Tools]
        C & D --> E[(Store)]
    end
    classDef hot fill:#f96,stroke:#333
    class B hot
```

`graph` is accepted for `flowchart`. Directions `TB`/`TD`, `BT`, `LR`, `RL`.
Shapes: `[rect]`, `(round)`, `([stadium])`, `[[subroutine]]`, `[(cylinder)]`,
`((circle))`, `(((double circle)))`, `>flag]`, `{rhombus}`, `{{hexagon}}`,
`[/parallelogram/]`, `[\alt\]`, `[/trapezoid\]`, `[\alt/]`; labels may be
quoted and use `<br>`. Links: `-->`, `---`, `-.->`, `-.-`, `==>`, `===`,
`--o`, `--x`, `<-->`, `o--o`, `x--x`, with `|label|` or `-- label -->` text.
Chains (`A --> B --> C`), `&` lists, `;` between statements, nested
`subgraph id [title]` … `end`, `classDef`, `class`, `:::class` and `style`
(fill, stroke, color, stroke-dasharray).

**Not supported (ignored):** `click`, `linkStyle`, a subgraph's own
`direction`, and styles on subgraphs. Markdown in labels is shown as text.

Layout is layered (`src/layered.rs`, written for this crate rather than
porting dagre): cycles broken by reversing back edges, longest-path ranks two
apart so every edge has an in-between rank for its label, dummy points for
long edges, barycentre sweeps that keep subgraph members together and keep the
ordering with fewest crossings, and isotonic regression across each rank. Links
leave and arrive along the flow, spread across a node's side in the order their
other ends lie.

## Architecture

```
source ──parse──> diagram model ──layout──> Scene ──diagram_scene::svg──> SVG
                                              └──> a host's own painter
```

Each diagram type owns its own `parse` and `layout`. The scene is
[`diagram-scene`](https://github.com/grahambrooks/diagram-scene)'s, shared
with puml and forge: nodes with semantic shapes, containers, routed edges with
markers and labels, colours as theme roles. That crate also holds the shapes'
outlines, edge routing helpers and the SVG writer, so every consumer draws a
scene the same way.

| Module | Role |
|:---|:---|
| `text` | `wrap`, and a bundled-font `Measure` over `rustybuzz`. |
| `layered` | The layered graph layout flowcharts use. |

Layout is testable without string-matching SVG, and a host (the emx editor)
draws the scene natively instead of going through SVG at all.

### Adding a diagram type

1. `src/<type>/parse.rs` — source to a model, with errors carrying line numbers.
2. `src/<type>/layout.rs` — model to a `diagram_scene::Scene`, using `Measure`
   for text.
3. Register the type's keyword in `detect()` and `scene()` in `lib.rs`.
5. Add a sample to `examples/` and a case to the showcase.

Nothing else in the crate should need to change.

## Why this is small

Measured against `sebastian` 0.6.0 (39,554 lines), the most complete Rust port
of mermaid.js, which was evaluated as a dependency and rejected: it compiles C
through `core-math-sys` for V8-identical trigonometry, and it emits labels as
`<foreignObject>`, which SVG-only consumers drop.

The bulk of that is machinery this crate does not need — byte-exact decimal
formatting, JS map iteration-order emulation, a ported JS colour library, the
rough.js hand-drawn renderer, browser font-metric emulation, and mermaid's CSS
theme engine.

The structural finding that shaped this project: **the advanced diagram types
are the cheap ones.** In sebastian, C4 is 618 lines and gitGraph 1,364, and
neither needs the graph-layout engine — they do their own layout and need only
an SVG writer and a text measurer. Only flowchart, state, class, ER and block
need `dagre` (~4,000 lines with `graphlib`). Flowcharts use a compact layered
layout of this crate's own instead (`src/layered.rs`, about 500 lines with its
tests).

No sebastian code is copied; the layouts here are written from the mermaid
sources in `mermaid/` and from its rendered output. Sebastian is MIT, so
porting with attribution remains available if `dagre` is ever wanted.

## Testing

Layout is asserted on the scene, not on SVG strings. Two properties matter most
and are cheap to check: **nothing is dropped** (every parsed element appears in
the scene) and **nothing overlaps** where it must not. `examples/showcase.rs`
renders every supported diagram plus prose into one PDF — the visual check that
tests cannot make.
