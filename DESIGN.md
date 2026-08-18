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

## Architecture

```
source ──parse──> diagram model ──layout──> scene ──render──> SVG
```

Each diagram type owns its own `parse`, `layout` and `render`, and shares only
the base:

| Module | Role |
|:---|:---|
| `svg` | An SVG element builder. Escaping and attribute writing live here, nowhere else. |
| `text` | The `Measure` trait, and a bundled-font implementation over `rustybuzz`. |
| `theme` | Palette and font settings, as data. |
| `geom` | Points, sizes, rectangles. |

`layout` produces a **scene** — positioned shapes and text in diagram
coordinates — which `render` writes out. Keeping that split means layout is
testable without string-matching SVG, and a future consumer could draw a scene
directly instead of going through SVG at all.

### Adding a diagram type

1. `src/<type>/parse.rs` — source to a model, with errors carrying line numbers.
2. `src/<type>/layout.rs` — model to a `Scene`, using `Measure` for text.
3. `src/<type>/render.rs` — usually a thin call into the shared scene writer.
4. Register the type's keyword in `detect()` in `lib.rs`.
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
need `dagre` (~4,000 lines with `graphlib`), which is why those are not in v1
and why their absence costs nothing on the path to the rest.

No sebastian code is copied; the layouts here are written from the mermaid
sources in `mermaid/` and from its rendered output. Sebastian is MIT, so
porting with attribution remains available if `dagre` is ever wanted.

## Testing

Layout is asserted on the scene, not on SVG strings. Two properties matter most
and are cheap to check: **nothing is dropped** (every parsed element appears in
the scene) and **nothing overlaps** where it must not. `examples/showcase.rs`
renders every supported diagram plus prose into one PDF — the visual check that
tests cannot make.
