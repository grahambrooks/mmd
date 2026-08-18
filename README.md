# mmd

Render [Mermaid](https://mermaid.js.org) diagrams to SVG, for web and PDF
publication. Pure Rust: no Node, no browser, no JavaScript runtime, no C
toolchain.

> **Status:** early development. gitGraph and C4 context diagrams render; see
> [DESIGN.md](DESIGN.md) for the exact supported syntax and for what is
> deliberately out of scope.

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

Text is measured by shaping real fonts, so a label never overflows the box
drawn around it. Hosts with their own font stack can implement the `Measure`
trait and drop the bundled fonts entirely:

```toml
mmd = { version = "*", default-features = false }
```

## Supported diagrams

| Diagram | Supported |
|:---|:---|
| `gitGraph` | `commit` (`id:`, `tag:`, `type:`), `branch`, `checkout`/`switch`, `merge` |
| `C4Context` | `title`, `Person`, `Person_Ext`, `System`, `System_Ext`, `SystemDb`, `Rel` |

Anything outside that subset is a **parse error naming the construct and its
line** — never a silently missing element:

```console
$ mmd notes.mmd
mmd: notes.mmd: line 7: `cherry-pick` is not supported (see DESIGN.md)
```

Adding a diagram type is a self-contained module: parse, layout, render, and
one line in `detect()`. See [DESIGN.md](DESIGN.md#adding-a-diagram-type).

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
