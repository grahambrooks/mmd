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
mermaid-cli output; mermaid's theme engine and `%%{init}%%` directives (they
are read as comments); the hand-drawn ("rough") look; HTML labels; font
fallback across scripts; interactive features (click handlers, tooltips);
animation; and a CSS-based stylesheet of any kind.

Amended 2026-10: flowchart `classDef`/`class`/`:::`/`style` *are* supported,
because they carry meaning in real diagrams (a highlighted node). They map to
per-node fill, stroke and text colours emitted as presentation attributes — not
to a stylesheet — so decision 4 still holds.

## Supported syntax

This is the contract. **Anything outside it is a parse error with a line
number, never a silent omission.** Grammar checked against the upstream
sources in `mermaid/` — `packages/parser/src/language/gitGraph/gitGraph.langium`
and `packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison` for those two;
`packages/mermaid/src/diagrams/flowchart/parser/flow.jison` for flowcharts.

Across every type: the first line that is not blank or a `%%` comment must be
the diagram keyword. `%%{init: …}%%` directives are comments and are ignored.
YAML front matter (`---` … `---`) is rejected as an unrecognised diagram.

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

**Accepted but ignored** — a documented exception to the error rule, for
statements that change interaction or fine styling but never drop an element:
`click`, `linkStyle`, `accTitle`/`accDescr`, a subgraph's own `direction`, and
`style` on a subgraph. Markdown in labels (`**bold**`, `` `code` ``) is shown
as literal text.

**Not supported (error):** the v11 shape syntax `A@{ shape: … }`, edge ids
(`A e1@--> B`), invisible links `~~~`, `flowchart-elk`, and front matter. A
`style` or `class` naming an unknown id creates that node, as mermaid does.

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
3. A `mod.rs` exposing `scene(source, measure)`.
4. Add a `Kind` variant and register its keyword in `detect()` and `scene()`
   in `lib.rs`.
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
layout of this crate's own instead (`src/layered.rs`, about 600 lines with its
tests) — which means **dagre is no longer the gate for state, class, ER or
requirement diagrams either**: each is a node-and-edge graph that `layered`
can place, so their cost is now parse + node drawing, not a layout engine.
`diagram-scene` already carries what they draw, because puml uses it: `Start`,
`End`, `ForkBar`, `Note` and `Table` shapes, inheritance/composition/
aggregation markers and crow's-foot cardinalities.

No sebastian code is copied; the layouts here are written from the mermaid
sources in `mermaid/` and from its rendered output. Sebastian is MIT, so
porting with attribution remains available if `dagre` is ever wanted.

## Roadmap

Reviewed 2026-10-25 against the upstream checkout in `mermaid/` (92dc556,
2026-05-01), which has 28 diagram types. Three are supported, C4 only in
part. Order is by **value to prose documents** (what md2pdf and emx readers
actually write) over **cost** (whether a layout already exists in this
crate), not by upstream's list. Each step is one module per *Adding a diagram type*; none
should touch another type.

### 1. Gaps in the supported types

Cheapest wins — the module, the tests and the showcase case already exist.

| Type | Missing | Notes |
|:---|:---|:---|
| C4 | `C4Container`, `C4Component`, `Container*`, `Component*`, `*_Ext`, `*Db`, `*Queue` | Same flat grid as `C4Context`; new element kinds only. `detect()` must route every `C4*` keyword to the C4 module so its "only `C4Context`" error is reachable — today `C4Container` gets the generic "unrecognised diagram". |
| C4 | `*_Boundary` | Needs nested layout: boundaries are containers around a sub-grid. |
| C4 | `BiRel`, `Rel_U/D/L/R`, `Rel_Back` | `BiRel` is a marker change; the directional forms are layout hints and could be accepted and ignored. |
| gitGraph | `TB`/`BT` direction | Swap axes in layout; today `TB` silently draws left to right — contrary to the error rule. |
| gitGraph | `msg:`, `branch … order:`, `cherry-pick` | `cherry-pick` is a commit with a dashed second parent. |
| flowchart | `@{ shape: … }`, `~~~`, edge ids | v11 syntax, increasingly common in generated diagrams. |

### 2. Graph diagrams on `layered`

`src/layered.rs` already does ranks, crossing reduction, subgraphs and edge
labels, and `diagram-scene` already has the shapes and markers. These are
parse + node drawing on top of both.

| Type | Keyword | Notes |
|:---|:---|:---|
| **state** | `stateDiagram-v2`, `stateDiagram` | Highest value of the graph types. `[*]` start/end, composite states as `layered` subgraphs, `<<choice>>`/`<<fork>>`/`<<join>>` (`Diamond`, `ForkBar`), notes. Concurrency `--` regions are the hard part — defer behind an error. |
| **ER** | `erDiagram` | Entity boxes with attribute tables (`Table`); `CrowFoot` markers exist. Relationship labels are mid-edge, so no end-label work is needed — likely the cheapest graph type. |
| **class** | `classDiagram` | Compartmented boxes (name / attributes / methods); relation markers exist in the scene; cardinality labels at both ends need `layered` to place labels near an edge's ends, not only its middle. |
| requirement | `requirementDiagram` | Boxes of key/value lines and labelled relations; small once class boxes exist. |

### 3. Own-layout diagrams

No graph layout; each is a direct geometric layout like gitGraph.

| Type | Keyword | Notes |
|:---|:---|:---|
| **sequence** | `sequenceDiagram` | The most-used type after flowchart and the largest gap. Participants in columns, messages down the page, activations, notes, `loop`/`alt`/`opt`/`par` frames. Upstream is 3,400 lines, much of it frame nesting; a first cut without `box`, `create`/`destroy` and `autonumber` is far smaller. |
| **gantt** | `gantt` | Needs date arithmetic (no dependency — days only), sections, `after` dependencies. Reject `excludes weekends` until done. |
| pie | `pie` | Arcs and a legend; tiny. |
| timeline | `timeline` | Columns of periods with events. |
| mindmap | `mindmap` | Indentation-based tree; a tidy-tree layout. |
| journey | `journey` | Sections, tasks, a 1–5 score per task. |
| quadrant | `quadrantChart` | Four labelled quadrants and points. |
| xychart | `xychart` (`xychart-beta` also accepted upstream) | Bar and line on axes; needs tick selection. |
| packet | `packet` | Bit-field rows. |
| kanban | `kanban` | Columns of cards. |
| sankey | `sankey` (`sankey-beta` also accepted upstream) | Node ordering plus ribbon paths; the most layout of this group. |
| treemap | `treemap` | Squarified rectangles. |
| radar | `radar-beta` | Wait for it to leave beta. |

### 4. Deferred

| Type | Why |
|:---|:---|
| architecture | Upstream lays it out with fcose (force-directed, via cytoscape); a faithful look needs a different engine. Revisit if a real document needs it. |
| block | Its own column/span grid with nested blocks; moderate cost, rare in prose. |
| `venn-beta`, `wardley-beta`, `treeView-beta`, `ishikawa`, `eventmodeling` | Beta or new upstream; the grammar is still moving. |
| `info` | Prints mermaid's version; meaningless here. |
| `flowchart-elk` | An alternative layout engine for the same syntax. Could alias to `flowchart`, but it would not look like ELK's output. |

## Testing

Layout is asserted on the scene, not on SVG strings. Two properties matter most
and are cheap to check: **nothing is dropped** (every parsed element appears in
the scene) and **nothing overlaps** where it must not. `examples/showcase.rs`
renders every supported diagram plus prose into one PDF — the visual check that
tests cannot make.
