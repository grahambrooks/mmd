---
name: render-mermaid
description: Render a Mermaid diagram to a standalone SVG with the `mmd` CLI (pure Rust, no Node or browser). Use when the user asks to "render this mermaid diagram", "turn this .mmd into an SVG", "export the flowchart / gitGraph / C4 / ER diagram as an image", or needs diagram SVG whose labels survive conversion to PDF (real `<text>`, no `<foreignObject>`). Supports `flowchart`/`graph`, `gitGraph`, `C4Context`/`C4Container`/`C4Component` and `erDiagram`; sequence, class, state, gantt, pie and the rest are rejected.
argument-hint: <diagram.mmd> [-o out.svg]
---

# Render Mermaid to SVG with mmd

`mmd` reads one Mermaid diagram and writes one SVG. Labels are real SVG
`<text>` with presentation attributes (no CSS, no `<foreignObject>`), so the
SVG converts to PDF without losing text.

## Invoking it

Use `mmd` if it is on PATH (`mmd --version` prints `mmd 2026.x.y`). Otherwise
run the latest release without installing — the repo is private, so `bx`
needs a token:

```bash
GITHUB_TOKEN=$(gh auth token) bx grahambrooks/mmd -- diagram.mmd -o diagram.svg
```

Everything after `--` is passed to `mmd` unchanged.

## Which binary

Releases up to `2026.10.2` support only `flowchart`, `gitGraph` (left to
right, no `cherry-pick`) and `C4Context`. Everything below describes `main`
and later releases. If `mmd --version` is older and the diagram needs newer
syntax, say so rather than rewriting the diagram.

## Flags

| Invocation | Effect |
|:---|:---|
| `mmd diagram.mmd` | Writes `diagram.svg` next to the input; prints that path on stderr. |
| `mmd diagram.mmd -o out.svg` | Writes to `out.svg` (`--output` is the long form). |
| `mmd diagram.mmd -o -` | Writes the SVG to stdout. |
| `cat diagram.mmd \| mmd - -o out.svg` | Reads stdin. Stdin **requires** `-o`; without it: `reading standard input needs an explicit --output`. |

There are no theme, size, or format options. Output is always SVG.

## What it can render

The first line that is not blank or a `%%` comment must be the keyword.
`%%{init: …}%%` lines are ignored. YAML front matter is an error.

### `flowchart` (or `graph`)

Directions are `TB`/`TD`, `BT`, `LR` and `RL`; the default is `TB`.

```
flowchart LR
    A[Start] --> B{Decide}
    B -->|yes| C([Done])
    B -- no --> D[[Retry]]
    R@{ shape: doc, label: "Report for **ops**" }
    subgraph tools [Tools]
        direction TB
        C & D --> E[(Store)]
    end
    E --> R
    A ~~~ E
    classDef hot fill:#f96,stroke:#333,color:#222
    class B hot
    style tools fill:aliceblue
    linkStyle 0 stroke:darkgoldenrod,stroke-width:2px
```

- **Shapes:** `[rect]`, `(round)`, `([stadium])`, `[[subroutine]]`,
  `[(cylinder)]`, `((circle))`, `(((double circle)))`, `(-ellipse-)`,
  `>flag]`, `{rhombus}`, `{{hexagon}}`, `[/parallelogram/]`, `[\alt\]`,
  `[/trapezoid\]` and `[\alt/]`.
- **v11 shapes:** `A@{ shape: … }` accepts the names for those outlines
  (`rect`, `rounded`, `stadium`, `subproc`, `cyl`, `circle`, `diam`, `hex`,
  `lean-r`/`lean-l`, `trap-b`/`trap-t`, `odd`). It also accepts `doc`,
  `cloud`, `note`, `h-cyl`, `text`, `sm-circ`, `fr-circ` and `fork`. Any
  other name (`bolt`, `card`, `delay`, `triangle`, `icon`, `img`, …) is an
  error.
- **Links:**
  - Kinds: `-->`, `---`, `-.->`, `-.-`, `==>`, `===`, `--o`, `--x`, `<-->`,
    `o--o`, `x--x`, and the invisible `~~~`.
  - Text goes in `|label|` or `-- label -->`.
  - A link may carry an id (`A e1@--> B`).
  - Chains, `&` and `;` work.
- **Labels:** quoting and `<br>` work. Markdown needs the backtick string form
  `` "`**bold** _it_`" `` or an `@{ label }`; a plain label shows `**`
  literally.
- **Styling:**
  - `classDef`, `class`, `:::` and `style` work on nodes and subgraphs. They
    take `fill`, `stroke`, `color` and `stroke-dasharray`.
  - `linkStyle N|default` takes `stroke`, `stroke-width` and
    `stroke-dasharray`.
  - A subgraph may set its own `direction`.

### `gitGraph`

Write `gitGraph`, `gitGraph TB:` or `gitGraph BT:`. The default runs left to
right.

```
gitGraph TB:
    title Release flow
    commit id: "init"
    branch develop order: 1
    commit id: "parser"
    commit id: "fix crash" type: HIGHLIGHT
    checkout main
    branch release order: 2
    cherry-pick id: "fix crash"
    commit id: "notes" tag: "v1.0.1"
    checkout main
    merge develop id: "ship" tag: "v1.1"
```

- **Commits:**
  - `commit` takes any mix of `id:`, `msg:`, repeated `tag:`, and
    `type: NORMAL|REVERSE|HIGHLIGHT`.
  - `msg:` is stored but not drawn, which is also what mermaid does.
  - Ids must be unique.
- **Branches:**
  - `branch <name> [order: N]` creates a branch.
  - `checkout <name>` (or `switch <name>`) moves to one.
  - A branch must exist before it is checked out or merged.
- **Merges:** `merge <name>` takes `id:`, `tag:` and `type:`.
- **Cherry-picks:**
  - Write `cherry-pick id: "x"`, optionally with `tag:` and `parent: "p"`.
  - `parent:` is required when the picked commit is a merge.
  - The commit must not already be on the current branch.

### C4

Three levels: `C4Context`, `C4Container` and `C4Component`.

```
C4Container
    title Publishing service
    Person(author, "Author", "Writes Markdown")
    System_Ext(git, "Git host")
    System_Boundary(pub, "Publisher") {
        Container(api, "API", "Rust, axum", "Accepts requests")
        ContainerQueue(jobs, "Render jobs", "SQS")
        ContainerDb(store, "Document store", "Postgres")
    }
    Rel(author, api, "Publishes", "HTTPS")
    Rel(git, api, "Notifies")
    Rel(api, jobs, "Enqueues")
    BiRel(api, store, "Reads and writes", "SQL")
```

- **Elements:**
  - The kinds are `Person`, `System`, `Container` and `Component`.
  - Each has `Db` and `Queue` variants and `_Ext` forms, such as
    `SystemDb_Ext` and `ContainerQueue_Ext`.
  - `Container` and `Component` take `(alias, label, techn, descr)`. The
    others take `(alias, label, descr)`.
  - Trailing arguments are optional; `$techn=` and `$descr=` also work.
- **Boundaries:**
  - `Boundary(a, "l", "type") { … }`, `Enterprise_Boundary`,
    `System_Boundary` and `Container_Boundary`.
  - They nest.
- **Relations:**
  - `Rel`, `BiRel`, `Rel_Back` and `Rel_U/D/L/R` take
    `(from, to, label, techn, descr)`.
  - The directional forms only choose which sides of the boxes the curve
    uses.
  - Every alias must be declared.
  - A relation cannot target a boundary.
- **Layout:**
  - `UpdateLayoutConfig($c4ShapeInRow="3")` sets how many elements go in a
    row.
  - Elements are placed in declaration order, so declare related elements
    next to each other.

### `erDiagram`

```
erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ "LINE ITEM" : contains
    PRODUCT |o..o{ "LINE ITEM" : "ordered as"
    CUSTOMER {
        int id PK
        string email UK "login"
    }
```

- **Cardinality:**
  - Symbols: `|o`, `||`, `}o`, `}|` and their mirror images.
  - Words also work: `one or more`, `zero or many`, `1+`, ….
- **Lines:** `--` is identifying (solid); `..` is non-identifying (dashed).
- **Labels:** every relationship needs `: label`. Quote any label with spaces.
- **Attributes:** written `type name [PK|FK|UK[, …]] ["comment"]`.
- **Also accepted:** aliases (`p["Person"]`), `direction TB|BT|LR|RL`, and
  `classDef`/`class`/`:::`/`style`.

### Colours

Any colour in a style must be hex, `rgb()`/`rgba()` or a CSS colour name.
`none`, `transparent`, `hsl()` and typos are errors, not silently ignored.

## What it cannot render — do not generate these

- **Other diagram types:** `sequenceDiagram`, `classDiagram`,
  `stateDiagram`/`stateDiagram-v2`, `requirementDiagram`, `gantt`, `pie`,
  `mindmap`, `timeline`, `journey`, `C4Dynamic`, `C4Deployment` and the
  rest. The planned order is in DESIGN.md → Roadmap.
- **flowchart:**
  - `flowchart-elk`;
  - a link to a subgraph id;
  - any `@{ shape }` name outside the list above, plus `icon` and `img`;
  - `[|field:value|]` vertex properties;
  - `linkStyle` indexes beyond the links defined so far.
- **C4:**
  - `Deployment_Node`, `Node*`, `RelIndex`;
  - `UpdateElementStyle`, `UpdateRelStyle`;
  - sprite, tags or link arguments;
  - `accTitle`, `direction`.
- **ER:**
  - `title`;
  - a relationship without a label;
  - an unquoted multi-word label or alias.
- **Accepted but not drawn:**
  - `accTitle`/`accDescr` (except in C4, where they are errors);
  - flowchart `click` and edge animation;
  - gitGraph `msg:`;
  - a subgraph `direction` when a link crosses into the subgraph, which
    mermaid ignores too.

If the user needs an unsupported type, say so. Do not silently rewrite the
diagram as a different type.

## Errors

Unsupported input is a hard error: exit status 1, naming the line and the
construct.

```
mmd: x.mmd: line 1: unrecognised diagram: expected one of `flowchart`, `erDiagram`, `gitGraph`, `C4Context`, `C4Container`, `C4Component`
mmd: x.mmd: line 1: YAML front matter (`---`) is not supported; use `title` where the diagram takes one (see DESIGN.md)
mmd: x.mmd: line 1: `C4Dynamic` is not supported; only `C4Context`, `C4Container` and `C4Component` (see DESIGN.md)
mmd: x.mmd: line 2: shape `bolt` is not supported (see DESIGN.md)
mmd: x.mmd: line 3: unrecognised colour `blurple` (see DESIGN.md)
mmd: x.mmd: line 3: unknown branch `nope`
mmd: x.mmd: line 2: a relationship needs a label: `A ||--o{ B : label`
```

No file is written on error. A stale SVG from an earlier run stays in place,
so check the exit status rather than whether the file exists. Fix the named
line and re-run.

## Checking the output

```bash
mmd diagram.mmd -o diagram.svg && test -s diagram.svg && head -c 5 diagram.svg   # <svg
grep -c '<text' diagram.svg     # > 0: labels are present
```

## Worked example

```bash
cat > "$TMPDIR/schema.mmd" <<'END'
erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains many
END
mmd "$TMPDIR/schema.mmd"
# exit 1 and no SVG: the multi-word label `contains many` must be quoted
```

Quote the label:

```bash
cat > "$TMPDIR/schema.mmd" <<'END'
erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : "contains many"
END
mmd "$TMPDIR/schema.mmd" -o "$TMPDIR/schema.svg" && test -s "$TMPDIR/schema.svg"
```
