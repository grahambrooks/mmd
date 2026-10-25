---
name: render-mermaid
description: Render a Mermaid diagram to a standalone SVG with the `mmd` CLI (pure Rust, no Node or browser). Use when the user asks to "render this mermaid diagram", "turn this .mmd into an SVG", "export the flowchart / gitGraph / C4 diagram as an image", or needs diagram SVG whose labels survive conversion to PDF (real `<text>`, no `<foreignObject>`). Only `flowchart`/`graph`, `gitGraph` and `C4Context` are supported; sequence, class, state, ER, gantt, pie and the rest are rejected.
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

## Flags

| Invocation | Effect |
|:---|:---|
| `mmd diagram.mmd` | Writes `diagram.svg` next to the input; prints that path on stderr. |
| `mmd diagram.mmd -o out.svg` | Writes to `out.svg` (`--output` is the long form). |
| `mmd diagram.mmd -o -` | Writes the SVG to stdout. |
| `cat diagram.mmd \| mmd - -o out.svg` | Reads stdin. Stdin **requires** `-o`; without it: `reading standard input needs an explicit --output`. |

There are no theme, size, or format options. Output is always SVG.

## What it can render

Three diagram types. The first non-blank, non-`%%` line must be the keyword.

**`flowchart`** (or `graph`), direction `TB`/`TD`/`BT`/`LR`/`RL` (default `TB`)

```
flowchart LR
    A[Start] --> B{Decide}
    B -->|yes| C([Done])
    B -- no --> D[[Retry]]
    subgraph tools [Tools]
        C & D --> E[(Store)]
    end
    classDef hot fill:#f96,stroke:#333,color:#222
    class B hot
```

Shapes: `[rect]`, `(round)`, `([stadium])`, `[[subroutine]]`, `[(cylinder)]`,
`((circle))`, `(((double circle)))`, `>flag]`, `{rhombus}`, `{{hexagon}}`,
`[/parallelogram/]`, `[\alt\]`, `[/trapezoid\]`, `[\alt/]`. Labels may be
quoted and use `<br>`. Links: `-->`, `---`, `-.->`, `-.-`, `==>`, `===`,
`--o`, `--x`, `<-->`, `o--o`, `x--x`, with `|label|` or `-- label -->`. Chains
(`A --> B --> C`), `&` lists, `;` between statements, nested `subgraph id [title]`
… `end`. Styling: `classDef`, `class`, `A:::cls`, `style A fill:…,stroke:…,color:…`.

**`gitGraph`**

```
gitGraph
    commit id: "scaffold"
    commit id: "base" tag: "v0.1"
    branch feature
    commit type: HIGHLIGHT
    checkout main
    merge feature
    commit type: REVERSE
```

Statements: `commit` (any mix of `id:`, `tag:`, `type: NORMAL|REVERSE|HIGHLIGHT`),
`branch <name>`, `checkout <name>`, `switch <name>`, `merge <name>` (a bare
name — no `id:`/`tag:` on a merge). Quotes on names are optional. Branches must exist before `checkout`/`merge`; a branch
cannot merge into itself.

**`C4Context`**

```
C4Context
    title System context
    Person(user, "User", "Uses the app")
    System(app, "App", "Does the work")
    SystemDb(db, "Database")
    System_Ext(mail, "Mail service")
    Rel(user, app, "uses")
    Rel(app, mail, "sends via", "SMTP")
```

Elements: `Person`, `Person_Ext`, `System`, `System_Ext`, `SystemDb` — the
description argument is optional. `Rel(from, to, "label"[, "technology"])`;
every alias used in a `Rel` must be declared somewhere in the diagram.

## What it cannot render — do not generate these

- Any other diagram type: `sequenceDiagram`, `classDiagram`,
  `stateDiagram`/`stateDiagram-v2`, `erDiagram`, `gantt`, `pie`, `mindmap`,
  `timeline`, `journey`, `C4Container`, `C4Component`, `C4Dynamic`,
  `C4Deployment`, and the rest. (Planned order: DESIGN.md → Roadmap.)
- flowchart: the v11 `A@{ shape: … }` syntax, edge ids (`A e1@--> B`),
  invisible links `~~~`, `flowchart-elk`. Use the bracket shapes instead.
- gitGraph: `cherry-pick`, `commit msg:`, `branch ... order:`, `title`.
- C4: any `*_Boundary`, `Container*`/`Component*`/`Node*`, `SystemDb_Ext`,
  `SystemQueue`, `BiRel`, `Rel_U/D/L/R`, `Rel_Back`, `UpdateElementStyle`,
  `UpdateLayoutConfig`.
- YAML front matter (`---\ntitle: ...\n---`) is rejected.
- Accepted but **ignored**: `%%{init: ...}%%` directives (treated as comments);
  a gitGraph direction (`gitGraph TB:` still draws left to right); in
  flowcharts `click`, `linkStyle`, `accTitle`/`accDescr`, a subgraph's own
  `direction`, and `style` on a subgraph. Markdown in labels is shown
  literally — don't use `**bold**` in labels.

If the user needs an unsupported type, say so — do not silently rewrite the
diagram into a different type.

## Errors

Unsupported input is a hard error, exit status 1, naming the line and construct:

```
mmd: x.mmd: line 1: unrecognised diagram: expected one of `flowchart`, `gitGraph`, `C4Context`
mmd: x.mmd: line 6: `cherry-pick` is not supported (see DESIGN.md)
mmd: x.mmd: line 3: unknown branch `nope`
mmd: x.mmd: line 2: expected a link near `~~~ C`
mmd: x.mmd: line 1: unknown direction `XY`
```

No file is written on error — but a stale SVG from an earlier run stays in
place, so check the exit status, not just that the file exists. Fix the
named line and re-run.

## Checking the output

```bash
mmd diagram.mmd -o diagram.svg && test -s diagram.svg && head -c 5 diagram.svg   # <svg
grep -c '<text' diagram.svg     # > 0: labels are present
```

## Worked example

```bash
cat > "$TMPDIR/release.mmd" <<'EOF'
gitGraph
    commit id: "init"
    branch dev
    commit id: "feature" type: HIGHLIGHT
    checkout main
    merge dev tag: "v1.0"
EOF
mmd "$TMPDIR/release.mmd"
# mmd: .../release.mmd: line 6: `merge` takes one branch name, got `dev tag: "v1.0"`
```

`merge` takes only a branch name (exit 1, no SVG). Tag a commit instead:

```bash
cat > "$TMPDIR/release.mmd" <<'EOF'
gitGraph
    commit id: "init"
    branch dev
    commit id: "feature" type: HIGHLIGHT
    checkout main
    merge dev
    commit id: "release" tag: "v1.0"
EOF
mmd "$TMPDIR/release.mmd" -o "$TMPDIR/release.svg" && test -s "$TMPDIR/release.svg"
```
