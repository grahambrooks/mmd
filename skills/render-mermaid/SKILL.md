---
name: render-mermaid
description: Render a Mermaid diagram to a standalone SVG with the `mmd` CLI (pure Rust, no Node or browser). Use when the user asks to "render this mermaid diagram", "turn this .mmd into an SVG", "export the flowchart / sequence / class / gantt diagram as an image", or needs diagram SVG whose labels survive conversion to PDF (real `<text>`, no `<foreignObject>`). Renders all 33 of mermaid's diagram types, including the external `zenuml` plugin, with recorded approximations.
argument-hint: <diagram.mmd> [-o out.svg]
---

# Render Mermaid to SVG with mmd

`mmd` reads one Mermaid diagram and writes one SVG. Labels are real SVG
`<text>` with presentation attributes (no CSS, no `<foreignObject>`), so the
SVG converts to PDF without losing text.

## Invoking it

Use `mmd` if it is on PATH (`mmd --version` prints `mmd 2026.x.y`). Otherwise
run the latest release without installing. The repo is private, so `bx`
needs a token:

```bash
GITHUB_TOKEN=$(gh auth token) bx grahambrooks/mmd -- diagram.mmd -o diagram.svg
```

Everything after `--` is passed to `mmd` unchanged.

## Which binary

Releases up to `2026.10.2` render only `flowchart`, left-to-right `gitGraph`
and `C4Context`. Everything below describes `main` and later releases. If
`mmd --version` is older and the diagram needs a newer type, say so instead
of rewriting the diagram.

## Flags

| Invocation | Effect |
|:---|:---|
| `mmd diagram.mmd` | Writes `diagram.svg` next to the input; prints that path on stderr. |
| `mmd diagram.mmd -o out.svg` | Writes to `out.svg` (`--output` is the long form). |
| `mmd diagram.mmd -o -` | Writes the SVG to stdout. |
| `cat diagram.mmd \| mmd - -o out.svg` | Reads stdin. Stdin **requires** `-o`; without it: `reading standard input needs an explicit --output`. |

There are no theme, size, or format options. Output is always SVG.

## What it renders

Write mermaid as you normally would. The first line that is not blank or a
`%%` comment must be the diagram keyword. These keywords render:

| Group | Keywords (aliases in brackets) |
|:---|:---|
| Graphs | `flowchart` (`graph`, `flowchart-elk`), `stateDiagram-v2` (`stateDiagram`), `classDiagram` (`classDiagram-v2`), `erDiagram`, `requirementDiagram` (`requirement`), `block` (`block-beta`) |
| Architecture | `C4Context`, `C4Container`, `C4Component`, `C4Dynamic`, `C4Deployment`, `architecture-beta` (`architecture`) |
| Interactions | `sequenceDiagram`, `zenuml`, `gitGraph`, `eventmodeling` |
| Time | `gantt`, `timeline`, `journey`, `kanban` |
| Charts | `pie`, `xychart` (`xychart-beta`), `quadrantChart`, `radar-beta`, `sankey` (`sankey-beta`) |
| Trees and maps | `mindmap`, `treemap` (`treemap-beta`), `treeView-beta`, `ishikawa` (`ishikawa-beta`), `packet` (`packet-beta`), `venn-beta`, `wardley-beta` |
| Other | `info` |

**Coverage before accuracy.** Each type follows mermaid's grammar. Its
drawing may differ from mermaid's: placement, filled areas drawn as strokes,
labels kept level instead of rotated. The output always contains every element
and label. If the user needs pixel-matching mermaid output, say that mmd
approximates.

**Front matter and directives:**
- A YAML front matter `title:` is drawn above the diagram.
- `config:` in front matter is ignored.
- `%%{init: …}%%` directives are ignored, so themes, `look: handDrawn` and
  per-type config options have no effect.

**Colours** in `classDef`, `style`, `linkStyle` and similar statements must be
hex, `rgb()`/`rgba()` or a CSS colour name. `none`, `transparent`, `hsl()` and
typos are errors, not silently ignored.

### Notes per type

- **flowchart:**
  - Every bracket shape and every v11 `A@{ shape: … }` name is accepted.
    Shapes without their own outline are drawn with the nearest one.
  - Links can be `-->`, `---`, `-.->`, `==>`, `--o`, `--x`, `<-->` or `~~~`,
    labelled with `|…|` or `-- … -->`, and may carry ids (`e1@-->`).
  - Links may point at a subgraph id, and a subgraph may set its own
    `direction`.
  - Markdown needs the backtick string form: `` "`**bold**`" ``.
- **sequenceDiagram:**
  - Participants and actors, `box`, `create`/`destroy`, every arrow type and
    activations.
  - Notes, the frames `loop`/`alt`/`opt`/`par`/`critical`/`break`/`rect`, and
    `autonumber`, including decimals (`autonumber 1.1 0.1`).
- **classDiagram:**
  - Members, generics `~T~`, annotations and every relation.
  - Cardinalities `"1" --> "*"`, namespaces and notes.
- **stateDiagram-v2:**
  - `[*]`, composites and `--` concurrency.
  - `<<choice>>`, `<<fork>>`, `<<join>>`, notes and `direction`.
  - A composite's body may share the line with its braces.
- **erDiagram:**
  - Every cardinality, written as symbols or words.
  - Every relationship needs `: label`; quote a label with spaces.
- **gantt:**
  - `dateFormat` takes moment.js parse tokens: month and weekday names
    (`MMM D YYYY`, `Do`, `h:mm A`), week dates (`GGGG-[W]WW-E`), day of year
    (`YYYY-DDDD`). Not the format-only ordinals (`Mo`, `DDDo`).
  - `axisFormat` takes every strftime directive, including week numbers
    (`%V`, `%W`).
  - `after`, `until`, `excludes weekends`, milestones and `crit` all work.
- **gitGraph:**
  - `LR`/`TB`/`BT`.
  - `commit` with `id:`, `tag:`, `type:` and `msg:`.
  - `branch` with `order:`, `merge` with `id:` and `tag:`, and `cherry-pick`.
  - Commit ids must be unique.
- **C4:**
  - All five levels, nested boundaries, deployment nodes and `RelIndex`.
  - `UpdateElementStyle`, `UpdateRelStyle` and tags.
  - Elements are placed in declaration order, so declare related elements
    next to each other.
- **venn-beta:** any number of sets. Four sets use the classic four-ellipse
  diagram, five that overlap pairwise the five-ellipse one. With six or more,
  a region the circles cannot form is listed under the diagram, not dropped.
- **zenuml:** parsed into the sequence model.
  - Supported: participants and annotators, `group`, calls `A.m()` with
    `{ … }` bodies, `x = A.m()`, `return`, `new`, async `A->B: text`, `//`
    comments, `if`/`while`/`for`/`try`/`par`/`opt`.
  - Also `ref(…)` frames and `==dividers==`. Not supported: `section(…)`,
    participant colours and widths.
- **kanban:** a card's `icon` is drawn as a tag with its name.

## What it cannot render — do not generate these

- There are no known gaps, but constructs outside mermaid's grammar, or ones
  mermaid itself rejects, fail with an error naming them, e.g. zenuml
  `section(…)` or gantt's format-only `Mo`.
- **Accepted but not drawn:**
  - `accTitle`/`accDescr`;
  - `click`/`link`/`callback` and other interactions;
  - icons such as `::icon()`, `$sprite` and Font Awesome glyphs (kanban
    shows the icon's name);
  - gitGraph `msg:`;
  - gantt `todayMarker`.

If the user needs an unsupported construct, say so. Do not silently rewrite
the diagram into a different type.

## Errors

Unsupported input is a hard error: exit status 1, naming the line and the
construct:

```
mmd: x.mmd: line 1: unrecognised diagram: expected one of `flowchart`, `gitGraph`, … (every keyword above)
mmd: x.mmd: line 2: front matter key `layout` is not supported; expected `title`, `config` or `displayMode` (see DESIGN.md)
mmd: x.mmd: line 2: dateFormat token `Mo` is not supported: moment only formats it and cannot parse it (see DESIGN.md)
mmd: x.mmd: line 2: `section` is not supported in zenuml (see DESIGN.md)
mmd: x.mmd: line 10: `union A,C` cannot be drawn: in mmd's approximate layout of 5 sets those sets share no region of their own (see DESIGN.md)
mmd: x.mmd: line 3: unrecognised colour `blurple` (see DESIGN.md)
mmd: x.mmd: line 3: unknown branch `nope`
mmd: x.mmd: line 2: a relationship needs a label: `A ||--o{ B : label`
```

No file is written on error. A stale SVG from an earlier run stays in place,
so check the exit status, not whether the file exists. Fix the named line and
re-run.

## Checking the output

```bash
mmd diagram.mmd -o diagram.svg && test -s diagram.svg && head -c 5 diagram.svg   # <svg
grep -c '<text' diagram.svg     # > 0: labels are present
```

## Worked example

```bash
cat > "$TMPDIR/plan.mmd" <<'END'
gantt
    dateFormat Mo YYYY
    Kickoff :a1, 1st 2026, 3d
END
mmd "$TMPDIR/plan.mmd"
# exit 1, no SVG — dateFormat token `Mo` is not supported (moment cannot parse it)
```

Use a parseable format instead; month names are fine:

```bash
cat > "$TMPDIR/plan.mmd" <<'END'
gantt
    dateFormat MMM D YYYY
    section Build
    Kickoff :a1, Jan 5 2026, 3d
    Parser  :after a1, 1w
END
mmd "$TMPDIR/plan.mmd" -o "$TMPDIR/plan.svg" && test -s "$TMPDIR/plan.svg"
```
