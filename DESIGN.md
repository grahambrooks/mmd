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

Amended 2026-10-25: **coverage over accuracy.** Every diagram type upstream
mermaid knows now has a module, and the previously deferred ones
(architecture, block, the beta types, `info`, `flowchart-elk`) render too. A
drawing that is recognisably right but differs from mermaid's is acceptable
when the difference is *recorded*: each sample lists its knowing differences
in `%% approx:` lines, and the showcase prints them under the diagram.
Silently dropping input is still never acceptable, so the error rule below is
unchanged. YAML front matter is accepted: its `title` is drawn and its
`config` ignored, like `%%{init}%%`.

## Supported syntax

This is the contract. **Anything outside it is a parse error with a line
number, never a silent omission.** Grammar checked against the upstream
sources in `mermaid/`:
`packages/parser/src/language/gitGraph/gitGraph.langium`,
`packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison`,
`packages/mermaid/src/diagrams/flowchart/parser/flow.jison`,
`packages/mermaid/src/diagrams/er/parser/erDiagram.jison`, and for every other
type its grammar under `packages/mermaid/src/diagrams/<type>/` or
`packages/parser/src/language/<type>/`.

These rules apply to every type:

- The first line that is not blank, a `%%` comment or YAML front matter must
  be the diagram keyword. Keywords and aliases are listed per type below and
  registered in `Kind::ALL` (`src/lib.rs`).
- `%%{init: …}%%` directives are comments and are ignored.
- YAML front matter (`---` … `---`) may come first. Its `title` is drawn
  centred above the diagram; `config` and `displayMode` are ignored; any other
  top-level key is an error naming it (`src/frontmatter.rs`). The block is
  blanked rather than removed, so later errors keep file line numbers.
- Real mermaid input that mmd does not handle yet lives in `examples/gaps/`.
  The tests require each to fail with a line number, and the showcase draws
  each as the error it reports.

### Accepted but not drawn

A short list of statements is parsed, validated and kept on the model, but
not drawn. Each one changes interaction or metadata, never what is on the
page, so no element is lost:

- `accTitle` and `accDescr` (single-line and `{ … }` blocks) in every type
  that has them, except C4, where `accTitle` is the drawn title as in
  mermaid's grammar. `diagram-scene` has no `<title>`/`<desc>` field yet.
- Interactions: flowchart, class and gantt `click`/`link`/`callback`/`call`
  (the ids they name must exist); sequence `link`/`links`/`properties`/
  `details` menus; C4 `$link`.
- Animation and curve choice: flowchart edge `animate`, `interpolate`/`curve:`.
- Icons, which need an icon font: C4 `$sprite`, mindmap `::icon()`, and
  architecture group icons. Architecture service icons are drawn as a simple
  glyph instead.
- gitGraph `msg:`, because mermaid's renderer never draws it either; gantt
  `todayMarker`, because a static render has no "today"; mindmap `:::class`,
  which is page CSS.
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
- A merge is drawn as a ring unless it is given a `type:`, and has no label
  unless it is given an `id:`, as in mermaid.
- A cherry-pick carries mermaid's cherry mark and a default
  `cherry-pick:<id>` tag. It is validated: the commit must exist, must not
  already be on the current branch, and must name a `parent:` when it is a
  merge.

**Ignored:** `%%{init}%%` gitGraph options (`parallelCommits`,
`rotateCommitLabel`, `showBranches`, `mainBranchName`), since directives are
comments. Tags stay flat in `TB`/`BT`, where mermaid rotates them, because
`diagram-scene` has no rotated text.

### C4

```
C4Context | C4Container | C4Component | C4Dynamic | C4Deployment
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
    RelIndex(index, from, to, "label", …)                 # C4Dynamic
    Deployment_Node | Node | Node_L | Node_R(alias, "label", "type", "descr") { … }
    UpdateElementStyle(alias, $bgColor=, $fontColor=, $borderColor=, $shape=)
    UpdateRelStyle(from, to, $lineColor=, $textColor=, $offsetX=, $offsetY=)
    UpdateBoundaryStyle(…), AddElementTag(…), AddRelTag(…)   # applied via $tags
    SHOW_LEGEND() | LAYOUT_WITH_LEGEND() | HIDE_STEREOTYPE() | LAYOUT_*()
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

**Dynamic and deployment views**

- `C4Dynamic` numbers every relation in declaration order (`n: label`);
  `RelIndex`'s own index is ignored, as mermaid ignores it.
- Deployment nodes nest like boundaries and draw as solid boxes with their
  `[type]` and description; `Node_L`/`Node_R` draw like `Node`.

**Styles and tags**

- Style statements use `src/style.rs` colours. `$tags="a+b"` applies tags
  defined by `AddElementTag`/`AddRelTag`, including line styles.
- `SHOW_LEGEND()` draws a legend; `HIDE_STEREOTYPE()` drops the «…» line.
- Some of these (`AddElementTag`, `UpdateBoundaryStyle`, `SHOW_LEGEND`,
  `HIDE_STEREOTYPE`) come from C4-PlantUML; mermaid's grammar rejects them,
  mmd accepts them.
- A relation's `textColor` is parsed but not drawn: edge labels use the
  theme's colour.

**Not supported (error):** relations to a boundary, `direction`, unknown
`$keys`, too many positional arguments, an unknown style or tag value, and a
style statement naming an undefined element.

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
- v11 `A@{ shape: …, label: … }` accepts every upstream shape name and alias.
  Names with no outline of their own are drawn with the nearest one;
  `examples/flowchart-shapes.mmd` shows them all and lists each substitution.
- `icon`/`img` nodes draw as a box of their `form:`, labelled with `label:` or
  the icon name. `pos`, `w`, `h` and `constraint` are validated and ignored.

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
- A link may name a subgraph. As in mermaid, it is laid out to a member and
  cut where it meets the subgraph's box.
- A subgraph `direction` that differs from its parent lays the subgraph out on
  its own when no link crosses its edge. When a link does cross, the direction
  is ignored, as mermaid documents.

**Styling**

- Styles apply to nodes and subgraphs.
- `linkStyle N[,M…]|default [interpolate <curve>] <styles>` takes `stroke`,
  `stroke-width` and `stroke-dasharray`.

`flowchart-elk` is accepted and laid out with mmd's layered layout, not ELK's.

**Not supported (error):**

- A node whose id is a subgraph's and that has its own label or shape.
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

### Other diagram types

Each type below has at least one sample in `examples/`. A sample's `%% approx:`
lines are the authoritative list of how its drawing differs from mermaid's,
and the showcase prints them under each diagram. The notes here cover syntax
and the errors that matter. Unless a type's notes say otherwise, `title`,
`accTitle`/`accDescr` and `%%` comments are accepted, and anything outside the
syntax listed is an error naming the construct.

**Graph types on `layered`** (`src/graph.rs` holds the shared ports, curves,
self-loops, block groups and pinned notes):

- **`stateDiagram-v2`** (alias `stateDiagram`):
  - states, `state "desc" as id`, `id : desc`, and transitions with labels;
  - `[*]` per scope;
  - composites `state X { … }`, nesting, with `--` concurrency regions;
  - `<<choice>>`/`<<fork>>`/`<<join>>`;
  - notes, including multi-line and floating notes;
  - `direction` per composite;
  - `classDef`/`class`/`:::`/`style`.
  - Error: a composite's body on the same line as its `{` (gap sample).
- **`classDiagram`** (alias `classDiagram-v2`):
  - classes, with generics, labels, annotations, and bodies or `X : member`
    lines;
  - every relation and two-headed relation, with labels and cardinalities;
  - lollipops;
  - nested and dotted namespaces;
  - notes, `direction` and styles.
- **`requirementDiagram`** (alias `requirement`, which mermaid itself rejects):
  - all six requirement kinds with `id`, `text`, `risk` and `verifyMethod`;
  - `element` with `type` and `docRef`;
  - all seven relationships in both arrow forms;
  - styles and `direction`.

**Interaction diagrams:**

- **`sequenceDiagram`** (`src/sequence/`):
  - participants:
    - `participant`/`actor`, with aliases and `@{ "type": … }`;
    - implicit participants;
    - `box … end`;
    - `create`/`destroy`.
  - messages and activations:
    - every arrow, including half-arrows (drawn as whole arrows);
    - activations, with `+`/`-` or `activate`/`deactivate`;
    - `autonumber [start [step]] | off`.
  - notes and frames:
    - notes left of, right of or over;
    - frames: `loop`, `alt`/`else`, `opt`, `par`/`and`, `par_over`,
      `critical`/`option`, `break`, `rect <colour>`.
  - text:
    - entities `#59;`;
    - `wrap:`/`nowrap:`.
  - Gap: decimal autonumber.

**Charts** (`src/chart.rs` holds shared ticks, legends, arcs and fills):

- `diagram-scene` edges are stroke-only, so filled areas are drawn as rows of
  strokes: dense ones read as solid, sparse ones as a translucent fill.
- **`pie`**: `pie [showData] [title …]` and `"label" : value`.
- **`xychart`** (alias `xychart-beta`):
  - `horizontal`;
  - categorical or numeric `x-axis`;
  - `y-axis` range;
  - `bar` and `line` series.
- **`quadrantChart`**:
  - axes and quadrant labels;
  - `Name: [x, y]` points, styled inline or by `classDef`.
- **`radar-beta`**:
  - `axis`;
  - `curve` with positional or named values;
  - `max`, `min`, `ticks`, `graticule`, `showLegend`.
- **`sankey`** (alias `sankey-beta`): CSV `source,target,value` rows with
  quoting. Cycles and self-loops are errors.

**Time and columns:**

- **`gantt`**:
  - `dateFormat`: the numeric moment.js tokens and `X`/`x`.
  - `axisFormat`: d3 strftime directives, except week numbers.
  - `tickInterval`, `weekday`, `topAxis`.
  - `excludes`/`includes`, `weekend`, `inclusiveEndDates`.
  - Sections.
  - Tasks with tags, ids, `after`, `until` and durations.
  - Date arithmetic is plain Rust.
  - Gaps: month names in `dateFormat`, and week numbers in `axisFormat`.
- **`timeline`**:
  - `LR`/`TD`;
  - sections;
  - `period : event : event`, with continuation lines.
- **`journey`**: sections and `task: score: actors`, scored 0–5.
- **`kanban`**:
  - columns and cards by indentation;
  - `@{ assigned, ticket, priority }` metadata.
  - Gap: card icons.

**Trees and blocks** (`src/meta.rs` holds the shared title, accessibility and
indentation helpers):

- **`mindmap`**:
  - an indentation tree with every node shape;
  - markdown labels.
- **`treemap`** (alias `treemap-beta`):
  - sections and `"leaf": value`, squarified;
  - `classDef`/`:::`.
- **`packet`** (alias `packet-beta`):
  - `start-end`, `n` and `+bits` fields in rows of 32;
  - contiguity is checked as upstream does.
- **`block`** (alias `block-beta`):
  - `columns`, spans, `space`;
  - nested `block … end`;
  - all 14 shapes;
  - block arrows;
  - links and styles.
- **`treeView-beta`**: an indentation file tree.
- **`ishikawa`** (alias `ishikawa-beta`): an effect with categories and
  causes, nested by indentation.

**Maps and models:**

- **`architecture-beta`** (alias `architecture`):
  - groups, services and junctions;
  - side-anchored edges with `{group}`;
  - placement on a grid derived from the edges' sides, where mermaid uses
    fcose.
- **`venn-beta`**:
  - up to three sets with unions, text items and styles.
  - Gap: four or more sets.
- **`wardley-beta`**: the full grammar — components, anchors, links, flows,
  evolve, pipelines, notes, annotations, accelerators.
- **`eventmodeling`**:
  - time frames and reset frames over UI, command, event, read-model and
    processor lanes;
  - data blocks, notes and `gwt` scenarios;
  - the validator's source-kind rules are errors.
- **`info`**: draws "mmd <version>".

**Not implemented:** `zenuml`, an external plugin with its own grammar (gap
sample). `sequenceDiagram` draws the same kind of picture.

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
| `layered` | The layered graph layout: flowchart, ER, and through `graph`, state, class and requirement. |
| `graph` | Ports, curves, self-loops, block groups and pinned notes over `layered`, for state, class and requirement. |
| `chart` | Statements, numbers, round ticks, legends, arcs and stroke fills for the data charts. |
| `meta` | `title`/`accTitle`/`accDescr`, headers and indentation for the tree and block types. |
| `style` | `classDef`/`style` declarations and CSS colours. |
| `frontmatter` | YAML front matter: title drawn, config ignored. |

Flowchart and ER predate `graph` and still carry their own port and curve
helpers. Moving them onto it is cleanup, not a feature.

Layout is testable without string-matching SVG, and a host (the emx editor)
draws the scene natively instead of going through SVG at all.

### Adding a diagram type

1. `src/<type>/parse.rs` — source to a model, with errors carrying line numbers.
2. `src/<type>/layout.rs` — model to a `diagram_scene::Scene`, using `Measure`
   for text.
3. A `mod.rs` exposing `scene(source, measure)`.
4. In `lib.rs`: a `Kind` variant, an entry in `Kind::ALL`, `keyword()`,
   `aliases()` if any, and an arm in `scene()`. `detect()` and the "expected
   one of" error are driven by `Kind::ALL`. Use `style` for colours and
   `meta`, `chart` or `graph` where they fit, rather than writing another
   copy.
5. Add `examples/<type>.mmd` with `%% showcase:`, `%% prose:` and
   `%% approx:` captions. Add `examples/gaps/<type>-<x>.mmd` for each construct
   left out. The showcase and the integration tests discover both; no Rust
   edit is needed.

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
2026-05-01). **32 of the 33 diagram types render**; `zenuml` is an external
plugin and is out of scope. The showcase's first page is the live coverage
matrix: per type, its samples, gaps and recorded approximations.

What remains is of three kinds, in priority order.

### 1. Known gaps

Each is a file in `examples/gaps/` that must keep failing with a line number
until it is closed. When one closes, the test that requires it to fail starts
failing; move the file into `examples/`.

| Gap | Notes |
|:---|:---|
| `gantt` month names in `dateFormat` (`MMM`, `Do`, `A`, `Z`) | Needs English month and day-name parsing. |
| `gantt` week numbers in `axisFormat` (`%U`, `%W`, `%V`) | ISO week arithmetic. |
| `sequenceDiagram` decimal `autonumber` | Small. |
| `stateDiagram` composite body on its `{` line | Needs a statement splitter that understands braces. |
| `kanban` card icons | Needs icons; see 3. |
| `venn-beta` with four or more sets | Needs venn.js-style area-proportional layout. |
| `zenuml` | External plugin. Not planned. |

### 2. Approximations worth narrowing

Each sample's `%% approx:` lines list its own approximations. The ones a
reader is most likely to notice:

- **Placement:**
  - C4 is placed in declaration order, so related elements can land far
    apart, and crossing relations can crowd their label pills.
  - Architecture uses a grid, not fcose.
  - The mindmap is a tidy tree, not cose.
- **Fills:**
  - Charts draw filled areas as strokes: pie wedges, bars, radar areas and
    sankey ribbons.
  - Moving this to real filled paths needs fill on `diagram-scene` edges.
- **Layout:** mmd's layered layout differs from dagre in node positions on
  every graph type. That is accepted under decision 1.
- **xychart:** bars from several series sit side by side; mermaid stacks
  them.

### 3. Needs `diagram-scene`

These are blocked on the shared scene crate, which mmd pins by tag and does
not modify:

- `accTitle`/`accDescr` as SVG `<title>`/`<desc>`.
- Rotated text: gitGraph tags in `TB`/`BT`, axis titles, wardley link labels.
- Filled paths, for the charts.
- Per-label colours, for flowchart `linkStyle color:` and C4 `textColor`.
- Icons, which would need an icon set bundled under decision 2's size
  constraints. Probably never.

## Testing

Layout is asserted on the scene, not on SVG strings. Two properties matter most
and are cheap to check: **nothing is dropped** (every parsed element appears in
the scene) and **nothing overlaps** where it must not.

`tests/render.rs` discovers its samples. Every `examples/*.mmd` must render
to SVG with real `<text>`, no `<foreignObject>` and no `<style>`. It must stay
inside its reported bounds and render deterministically. Every
`examples/gaps/*.mmd` must fail with a line number.

`examples/showcase.rs` renders the coverage matrix, then every sample and gap,
into `target/showcase.pdf`. That PDF is the visual check tests cannot make.
