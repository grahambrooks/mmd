---
name: render-mermaid
description: Render a Mermaid diagram to a standalone SVG with the `mmd` CLI (pure Rust, no Node or browser). Use when the user asks to "render this mermaid diagram", "turn this .mmd into an SVG", "export the flowchart / sequence / class / gantt diagram as an image", or needs diagram SVG whose labels survive conversion to PDF (real `<text>`, no `<foreignObject>`). Renders 32 of mermaid's 33 diagram types — everything except the external `zenuml` plugin — with recorded approximations.
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
| Interactions | `sequenceDiagram`, `gitGraph`, `eventmodeling` |
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
    `autonumber`.
- **classDiagram:**
  - Members, generics `~T~`, annotations and every relation.
  - Cardinalities `"1" --> "*"`, namespaces and notes.
- **stateDiagram-v2:**
  - `[*]`, composites and `--` concurrency.
  - `<<choice>>`, `<<fork>>`, `<<join>>`, notes and `direction`.
  - Start a composite's body on the line after its `{`.
- **erDiagram:**
  - Every cardinality, written as symbols or words.
  - Every relationship needs `: label`; quote a label with spaces.
- **gantt:**
  - `dateFormat` takes numeric tokens only (`YYYY-MM-DD`, `X`, …), not month
    names. `axisFormat` takes strftime directives, not week numbers.
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
- **venn-beta:** at most three sets.

## What it cannot render — do not generate these

- `zenuml`: an external plugin. Use `sequenceDiagram` instead.
- These known gaps fail with an error naming them:
  - gantt month-name date formats (`MMM`, `Do`);
  - gantt week-number axis labels (`%W`, `%U`, `%V`);
  - decimal sequence `autonumber`;
  - a state composite body on its `{` line;
  - kanban card icons;
  - venn diagrams with four or more sets.
- **Accepted but not drawn:**
  - `accTitle`/`accDescr`;
  - `click`/`link`/`callback` and other interactions;
  - icons such as `::icon()`, `$sprite` and Font Awesome glyphs;
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
mmd: x.mmd: line 4: dateFormat token `MMM` is not supported; use YYYY, YY, MM, M, DD, D, HH, H, mm, m, ss, s, SSS, X or x (see DESIGN.md)
mmd: x.mmd: line 7: a venn diagram of more than 3 sets is not supported (see DESIGN.md)
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
    dateFormat MMM D YYYY
    Kickoff :a1, Jan 5 2026, 3d
END
mmd "$TMPDIR/plan.mmd"
# exit 1, no SVG — dateFormat token `MMM` is not supported
```

Use a numeric date format:

```bash
cat > "$TMPDIR/plan.mmd" <<'END'
gantt
    dateFormat YYYY-MM-DD
    section Build
    Kickoff :a1, 2026-01-05, 3d
    Parser  :after a1, 1w
END
mmd "$TMPDIR/plan.mmd" -o "$TMPDIR/plan.svg" && test -s "$TMPDIR/plan.svg"
```
