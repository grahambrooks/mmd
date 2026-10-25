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

Amended 2026-10: `classDef`/`class`/`:::`/`style` (flowchart, ER) and
`linkStyle` (flowchart) *are* supported, because they carry meaning in real
diagrams, such as a highlighted node. They map to per-element colours emitted
as presentation attributes, not to a stylesheet, so decision 4 still holds.

## Supported syntax

This is the contract. **Anything outside it is a parse error with a line
number, never a silent omission.** Grammar checked against the upstream
sources in `mermaid/`:
`packages/parser/src/language/gitGraph/gitGraph.langium`,
`packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison`,
`packages/mermaid/src/diagrams/flowchart/parser/flow.jison` and
`packages/mermaid/src/diagrams/er/parser/erDiagram.jison`.

These rules apply to every type:

- The first line that is not blank or a `%%` comment must be the diagram
  keyword.
- `%%{init: …}%%` directives are comments and are ignored.
- YAML front matter (`---` … `---`) is an error that names it.

### Accepted but not drawn

A short list of statements is parsed, validated and kept on the model, but
not drawn. Each one changes interaction or metadata, never what is on the
page, so no element is lost:

- `accTitle` and `accDescr` (single-line and `{ … }` blocks) in flowchart,
  gitGraph and ER. `diagram-scene` has no `<title>`/`<desc>` field yet.
- flowchart `click` (every form in the grammar; the ids it names must exist),
  edge animation (`e1@{ animate: true }`) and `interpolate`/`curve:`.
- gitGraph `msg:`, because mermaid's renderer never draws it either.
- Properties a style may set that the scene has no slot for (`stroke-width` on
  a node, `font-*`, `color` on a link label). A colour or length that cannot be
  read is an error, whatever the property.

### Styles

`classDef`, `class`, `:::` and `style` take `fill`, `stroke`, `color` and
`stroke-dasharray`. They become presentation attributes on that element, so
decision 4 holds. Colours may be `#rgb`, `#rrggbb` (with optional alpha),
`rgb()`/`rgba()` or any of the 148 CSS colour names. Anything else is an
error, including `none`, `transparent` and `hsl()` (`src/style.rs`).

### gitGraph

```
gitGraph [LR|TB|BT][:]
    title <text>
    commit [id: "x"] [msg: "m" | "m"] [tag: "t"]… [type: NORMAL|REVERSE|HIGHLIGHT]
    branch <name> [order: N]
    checkout <name> | switch <name>
    merge <name> [id: "x"] [tag: "t"]… [type: NORMAL|REVERSE|HIGHLIGHT]
    cherry-pick id: "x" [tag: "t"]… [parent: "p"]
```

**Syntax**

- Attributes may come in any order.
- Strings take `"…"` or `'…'`. Quotes are optional on single words.
- Several `tag:`s stack on one commit.
- Ids must be unique. Mermaid only warns about a duplicate id and drops the
  earlier commit; here it is an error.

**Directions**

- `LR` (the default) runs time left to right, one lane per branch.
- `TB` gives each branch a vertical lane, with its label at the top and time
  running down. Commit labels sit left of each commit and tags to its right.
- `BT` mirrors `TB`.

**Order and marks**

- `order:` sorts lanes as mermaid does: `main` is 0, unordered branches come
  next in declaration order, then explicit orders.
- A merge is drawn as a ring unless it is given a `type:`.
- A cherry-pick carries mermaid's cherry mark and a default
  `cherry-pick:<id>` tag. It is validated: the commit must exist, must not
  already be on the current branch, and must name a `parent:` when it is a
  merge.

**Not supported (error):** front matter and `%%{init}%%` gitGraph options
(`parallelCommits`, `rotateCommitLabel`, `showBranches`, `mainBranchName`),
since directives are comments.

### C4

```
C4Context | C4Container | C4Component
    title <text>
    UpdateLayoutConfig($c4ShapeInRow="n", $c4BoundaryInRow="m")
    Person | Person_Ext(alias, "label", "descr")
    System | System_Ext | SystemDb | SystemDb_Ext | SystemQueue | SystemQueue_Ext(alias, "label", "descr")
    Container[Db|Queue][_Ext](alias, "label", "techn", "descr")
    Component[Db|Queue][_Ext](alias, "label", "techn", "descr")
    Boundary(alias, "label", "type") { … }
    Enterprise_Boundary | System_Boundary | Container_Boundary(alias, "label") { … }
    Rel | BiRel | Rel_Back(from, to, "label", "techn", "descr")
    Rel_U | Rel_Up | Rel_D | Rel_Down | Rel_L | Rel_Left | Rel_R | Rel_Right(…)
```

**Arguments**

- `techn` and `descr` are optional.
- They may also be passed as `$techn=` and `$descr=`.
- Quotes are optional on arguments that contain no comma.

**Boundaries**

- Boundaries nest to any depth.
- The `{` may sit on the next line.
- Each boundary is a dashed box around a grid of its own, labelled with its
  `[type]`.

**Placement**

- Each scope places its elements `$c4ShapeInRow` to a row (default 3), with
  its child boundaries below them.
- Placement follows declaration order.
- The directional `Rel_*` forms choose which sides a curve leaves and arrives
  on; they never move an element.

**Relations**

- `BiRel` has an arrowhead at both ends.
- `Rel_Back(a, b)` puts the only arrowhead at `a`.
- A relation's `descr` is drawn as a third label line. Mermaid parses it but
  does not draw it.

**Not supported (error):** `C4Dynamic`, `C4Deployment`, `Deployment_Node`,
`Node*`, `RelIndex`, `UpdateElementStyle` and `UpdateRelStyle`. Also errors:
sprite, tags and link arguments (positional or `$tags=`/`$link=`/`$sprite=`),
relations to a boundary, and `accTitle`/`accDescr`/`direction`.

### flowchart

```
flowchart LR
    A[Start] --> B{Decide}
    B -->|yes| C([Done])
    B -- no --> D[[Retry]]
    D@{ shape: doc, label: "Report for **ops**" }
    subgraph tools [Tools]
        direction TB
        C & D --> E[(Store)]
    end
    A ~~~ E
    classDef hot fill:#f96,stroke:#333
    class B hot
    style tools fill:aliceblue
    linkStyle 0 stroke:#b8860b,stroke-width:2px
```

**Header and directions**

- `graph` is accepted for `flowchart`.
- Directions are `TB`/`TD`, `BT`, `LR` and `RL`, and statements may follow
  the header after `;`.

**Shapes**

- Bracket shapes: `[rect]`, `(round)`, `([stadium])`, `[[subroutine]]`,
  `[(cylinder)]`, `((circle))`, `(((double circle)))`, `(-ellipse-)`,
  `>flag]`, `{rhombus}`, `{{hexagon}}`, `[/parallelogram/]`, `[\alt\]`,
  `[/trapezoid\]` and `[\alt/]`.
- v11 `A@{ shape: …, label: … }` accepts mermaid's names and aliases for those
  outlines, plus `doc`, `cloud`, `note`, `h-cyl`, `text`, `sm-circ`/`start`,
  `fr-circ`/`stop` and `fork`/`join`.
- The other upstream shape names (`bolt`, `card`, `delay`, `triangle`, …) and
  `icon`/`img` nodes are errors that name the shape.

**Labels**

- Labels may be quoted and may use `<br>`.
- Markdown strings (`` "`…`" ``, and every `@{ label }`) draw `**bold**` and
  `_italic_` as styled runs. Plain labels show `**` literally, as mermaid does.

**Links**

- Link types: `-->`, `---`, `-.->`, `-.-`, `==>`, `===`, `--o`, `--x`, `<-->`,
  `o--o` and `x--x`.
- Link text goes in `|label|` or `-- label -->`.
- Links may carry ids (`A e1@--> B`).
- Invisible links (`~~~`) are placed but not drawn.
- Chains (`A --> B --> C`), `&` lists and `;` between statements are
  accepted.

**Subgraphs**

- Nested `subgraph id [title]` … `end`.
- A subgraph `direction` that differs from its parent lays the subgraph out on
  its own when no link crosses its edge. When a link does cross, the direction
  is ignored, as mermaid documents.

**Styling**

- Styles apply to nodes and subgraphs.
- `linkStyle N[,M…]|default [interpolate <curve>] <styles>` takes `stroke`,
  `stroke-width` and `stroke-dasharray`.

**Not supported (error):**

- `flowchart-elk`.
- A link to a subgraph id.
- A node whose id is a subgraph's.
- `[|field:value|]` vertex properties.
- A `linkStyle` index out of range.
- A top-level `direction` that differs from the header.

A `style` or `class` that names an unknown id creates that node, as mermaid
does.

**Layout**

Flowcharts use a layered layout (`src/layered.rs`), written for this crate
rather than ported from dagre:

- Cycles are broken by reversing back edges.
- Longest-path ranks sit two apart, so every edge has an in-between rank for
  its label.
- Long edges get dummy points.
- Barycentre sweeps keep subgraph members together and keep the ordering with
  the fewest crossings.
- Isotonic regression runs across each rank.

Links leave and arrive along the flow, spread across a node's side in the
order their other ends lie. A subgraph with its own direction is laid out
separately and placed as one block.

### erDiagram

```
erDiagram
    direction LR
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ "LINE ITEM" : "contains many"
    PRODUCT |o..o{ "LINE ITEM" : "ordered as"
    CATEGORY |o--o{ CATEGORY : "parent of"
    CAR 1 to zero or more NAMED-DRIVER : allows
    p["Person"]
    CUSTOMER:::hot {
        int id PK
        string email UK, FK "login"
        list~int~ tags
    }
    classDef hot fill:#f96,stroke:#333
```

**Entities**

- An entity may be bare, quoted (`"LINE ITEM"`) or aliased (`p[Person]`).
- Attribute blocks hold `type name [PK|FK|UK[, …]] ["comment"]`. A block may
  span lines or sit on one line.
- Types may use `[]`, `()` and `~generic~`, which is shown as `<generic>`.
- An entity with attributes is a table with type, name, keys and comment
  columns.

**Cardinality**

| Meaning | Written as |
|:---|:---|
| zero or one | `\|o`/`o\|`, `one or zero`, `zero or one` |
| exactly one | `\|\|`, `one`, `only one`, `1` |
| zero or more | `}o`/`o{`, `zero or more`, `zero or many`, `many`, `many(0)`, `0+` |
| one or more | `}\|`/`\|{`, `one or more`, `one or many`, `many(1)`, `1+` |
| master-detail parent | `u` |

**Lines and labels**

- Identifying lines (`--`, `to`) are solid.
- Non-identifying lines (`..`, `.-`, `-.`, `optionally to`) are dashed.
- Every relationship needs `: label`. The label must be one word or quoted.
- A self-relationship is drawn as a loop.
- Other statements: `direction`, plus styles on entities.

**Not supported (error):**

- `title`, which upstream's grammar has no token for either.
- A relationship without a label.
- An alias and a relationship on the same line.
- An unquoted multi-word alias.
- Unknown cardinality symbols.

Placement uses the same `layered` layout as flowcharts.

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
| `layered` | The layered graph layout flowcharts and ER diagrams use. |
| `style` | `classDef`/`style` declarations and CSS colours, shared by flowchart and ER. |

Layout is testable without string-matching SVG, and a host (the emx editor)
draws the scene natively instead of going through SVG at all.

### Adding a diagram type

1. `src/<type>/parse.rs` — source to a model, with errors carrying line numbers.
2. `src/<type>/layout.rs` — model to a `diagram_scene::Scene`, using `Measure`
   for text.
3. A `mod.rs` exposing `scene(source, measure)`.
4. Add a `Kind` variant and register its keyword in `detect()` and `scene()`
   in `lib.rs`. Use `style` for any `classDef`/`style` support rather than
   parsing colours again.
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
need `dagre` (~4,000 lines with `graphlib`). Flowcharts and ER diagrams use
a compact layered layout of this crate's own instead (`src/layered.rs`, about
600 lines with its tests). That means **dagre is not the gate for state,
class or requirement diagrams either**: each is a node-and-edge graph that
`layered` can place, so their cost is parse plus node drawing, not a layout
engine. ER proved it. `diagram-scene` already carries the shapes and markers
these types draw: `Start`, `End`, `ForkBar`, `Note` and `Table`, the
inheritance, composition and aggregation markers, and crow's-foot
cardinalities.

No sebastian code is copied; the layouts here are written from the mermaid
sources in `mermaid/` and from its rendered output. Sebastian is MIT, so
porting with attribution remains available if `dagre` is ever wanted.

## Roadmap

Reviewed 2026-10-25 against the upstream checkout in `mermaid/` (92dc556,
2026-05-01), which has 28 diagram types. Four are supported: flowchart,
gitGraph, C4 (Context, Container and Component) and ER.

Order is by **value to prose documents** (what md2pdf and emx readers actually
write) over **cost** (whether a layout already exists in this crate), not by
upstream's list. Each step is one module per *Adding a diagram type*, and no
step should touch another type.

### Done (2026-10-25)

- **C4:** `C4Container` and `C4Component`; every `Container*`, `Component*`,
  `*Db`, `*Queue` and `*_Ext` element; nested boundaries; `BiRel`, `Rel_Back`
  and the directional `Rel_*` forms; `UpdateLayoutConfig`. Every `C4*` header
  now reaches the C4 parser.
- **gitGraph:** `TB`/`BT` (previously ignored without an error), `cherry-pick`,
  `msg:`, `order:`, `title`, and ids, tags and types on `merge`.
- **flowchart:**
  - `linkStyle`, subgraph styles and subgraph `direction`, all previously
    dropped without an error;
  - the v11 `@{ shape: … }`;
  - `~~~`;
  - edge ids;
  - markdown strings.
- **erDiagram:** a new type on `layered` — the first new diagram type on the
  roadmap.
- **Styles:** every style that cannot be read is now an error.

### 1. Gaps in the supported types

The module, the tests and the showcase case already exist for each of these.

| Type | Missing | Notes |
|:---|:---|:---|
| C4 | `C4Dynamic` (`RelIndex`), `C4Deployment` (`Deployment_Node`, `Node*`) | Dynamic is numbered relations on the existing grid. Deployment is nested nodes, like boundaries. |
| C4 | `UpdateElementStyle`, `UpdateRelStyle`; sprites, tags, links | Style updates can reuse `src/style.rs`. Sprites need icons, which is out of scope. |
| C4 | Placement by relation | Placement is declaration order, so a related pair can land far apart. The directional `Rel_*` forms could feed a placement pass. |
| all | `accTitle`/`accDescr` as SVG `<title>`/`<desc>` | Needs a field on `diagram_scene::Scene`; the models already hold the text. |
| flowchart | The 72 upstream shapes with no outline here (`bolt`, `card`, `delay`, …) | Each one is a `diagram-scene` outline. Add them as real documents ask for them. |
| flowchart | Subgraph `direction` when a link crosses the subgraph | Mermaid ignores the direction in this case too. Revisit only if mermaid changes. |
| gitGraph | Rotated tags in `TB`/`BT`; merge labels | Tags sit flat beside the commit. A merge without an `id:` is labelled `merge <branch>`, where mermaid draws no label. |

### 2. Graph diagrams on `layered`

`src/layered.rs` already does ranks, crossing reduction, subgraphs and edge
labels, and `diagram-scene` already has the shapes and markers. ER showed what
each remaining type costs: parse plus node drawing, about 1,600 lines with
tests.

ER copied two things from flowchart. The colour handling has since moved
into `src/style.rs`. The port, curve and shift helpers are still copied;
move them into a shared module before the next type copies them a third
time.

| Type | Keyword | Notes |
|:---|:---|:---|
| **state** | `stateDiagram-v2`, `stateDiagram` | The next graph type, and the most valuable left. Covers `[*]` start/end, composite states as `layered` subgraphs, `<<choice>>`/`<<fork>>`/`<<join>>` (`Diamond`, `ForkBar`) and notes. Concurrency `--` regions are the hard part; defer them behind an error. |
| **class** | `classDiagram` | Compartmented boxes (name / attributes / methods). The relation markers exist in the scene. Cardinality labels at both ends need `layered` to place labels near an edge's ends, not only its middle. |
| requirement | `requirementDiagram` | Boxes of key/value lines and labelled relations. Small once class boxes exist. |

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
