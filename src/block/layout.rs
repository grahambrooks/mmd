//! The column grid, as upstream's `block/layout.ts` builds it: in each
//! composite every column is as wide as its widest child (per column
//! spanned), children fill cells row by row, and a child spanning N columns
//! takes N cells and the gaps between them. A composite is then stretched to
//! the cell its parent gives it, its own cells growing to fill it. Unlike
//! upstream, each row is only as tall as its own tallest child.
//!
//! Links are straight lines between block outlines; their labels are drawn
//! after the blocks so no block can cover one.

use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge, EdgeLabel, Font, Marker, Node, Paint, Pen, Role,
    Scene, Seg, Shape, Style, Text, TextLine, Weight, geom::Point, geom::Rect, route,
    shape::natural_size,
};

use super::parse::{BlockDiagram, BlockKind, BlockShape, Dir, End, Stroke};
use crate::style::Colors;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Between cells, and around a composite's cells.
const GAP: f32 = 12.0;
const LABEL_WIDTH: f32 = 200.0;
/// Room above a labelled composite's cells.
const HEADER: f32 = 22.0;

pub fn layout(diagram: &BlockDiagram, measure: &dyn Measure) -> Scene {
    let mut ctx = Ctx {
        diagram,
        natural: vec![(0.0, 0.0); diagram.blocks.len()],
        nodes: Vec::new(),
        rects: vec![None; diagram.blocks.len()],
        measure,
    };
    for i in 0..diagram.blocks.len() {
        if !matches!(diagram.blocks[i].kind, BlockKind::Composite { .. }) {
            ctx.natural[i] = ctx.leaf_size(i);
        }
    }
    let (w, h) = ctx.size(0);
    let mut scene = Scene::new(w + 2.0 * PADDING - 2.0 * GAP, h + 2.0 * PADDING - 2.0 * GAP);
    scene.font_size = FONT_SIZE;
    // The diagram itself is a composite whose outer gap is the padding.
    let root = Rect::new(PADDING - GAP, PADDING - GAP, w, h);
    let mut containers = Vec::new();
    let mut texts = Vec::new();
    let mut arrows = Vec::new();
    ctx.place(0, root, &mut containers, &mut texts, &mut arrows);
    scene.containers = containers;
    scene.nodes = std::mem::take(&mut ctx.nodes);

    for link in &diagram.links {
        if link.stroke == Stroke::Invisible {
            continue;
        }
        let (Some(a), Some(b)) = (diagram.index(&link.from), diagram.index(&link.to)) else {
            continue;
        };
        let (ra, rb) = (
            ctx.rects[a].unwrap_or_default(),
            ctx.rects[b].unwrap_or_default(),
        );
        let (sa, sb) = (ctx.outline(a), ctx.outline(b));
        let (pa, pb) = (
            route::clip(sa, ra, rb.center()),
            route::clip(sb, rb, ra.center()),
        );
        let mut edge = Edge::new(vec![Seg::Move(pa), Seg::Line(pb)]).between(&link.from, &link.to);
        edge.start = marker(link.start);
        edge.end = marker(link.end);
        edge.pen = Pen {
            dash: if link.stroke == Stroke::Dotted {
                Dash::Dotted
            } else {
                Dash::Solid
            },
            weight: if link.stroke == Stroke::Thick {
                Weight::Thick
            } else {
                Weight::Normal
            },
            ..Pen::default()
        };
        if let Some(label) = &link.label {
            let lines = TextLine::lines(label, Font::Small);
            edge.labels.push(EdgeLabel::new(
                pa.lerp(pb, 0.5),
                lines,
                true,
                measure,
                FONT_SIZE,
            ));
        }
        scene.edges.push(edge);
    }
    scene.edges.extend(arrows);
    scene.texts = texts;
    scene
}

fn marker(end: End) -> Marker {
    match end {
        End::None => Marker::None,
        End::Arrow => Marker::Arrow,
        End::Cross => Marker::Cross,
        End::Circle => Marker::Circle,
    }
}

fn shape(shape: BlockShape) -> Shape {
    match shape {
        BlockShape::Square | BlockShape::Asymmetric => Shape::Rect,
        BlockShape::Round => Shape::Rounded,
        BlockShape::Circle | BlockShape::DoubleCircle => Shape::Circle,
        BlockShape::Diamond => Shape::Diamond,
        BlockShape::Hexagon => Shape::Hexagon,
        BlockShape::Stadium => Shape::Stadium,
        BlockShape::Subroutine => Shape::Subroutine,
        BlockShape::Cylinder => Shape::Cylinder,
        BlockShape::LeanRight | BlockShape::LeanLeft => Shape::Parallelogram,
        BlockShape::Trapezoid | BlockShape::InvTrapezoid => Shape::Trapezoid,
    }
}

fn apply(style: &mut Style, colors: &Colors) {
    if let Some((r, g, b)) = colors.fill {
        style.fill = Paint::Rgb(r, g, b);
    }
    if let Some((r, g, b)) = colors.stroke {
        style.stroke = Paint::Rgb(r, g, b);
    }
    if let Some((r, g, b)) = colors.text {
        style.text = Paint::Rgb(r, g, b);
    }
    if colors.dashed {
        style.dash = Dash::Dashed;
    }
}

/// A composite's grid: columns, rows, and each child's (row, column).
struct Grid {
    columns: usize,
    rows: usize,
    cells: Vec<(usize, usize, usize)>,
}

struct Ctx<'a> {
    diagram: &'a BlockDiagram,
    natural: Vec<(f32, f32)>,
    nodes: Vec<Node>,
    rects: Vec<Option<Rect>>,
    measure: &'a dyn Measure,
}

impl Ctx<'_> {
    fn children(&self, i: usize) -> &[usize] {
        match &self.diagram.blocks[i].kind {
            BlockKind::Composite { children, .. } => children,
            _ => &[],
        }
    }

    fn label(&self, i: usize) -> Vec<TextLine> {
        let b = &self.diagram.blocks[i];
        crate::flowchart::text_lines(self.measure, b.text(), b.markdown, Font::Body, LABEL_WIDTH)
    }

    fn leaf_size(&self, i: usize) -> (f32, f32) {
        let b = &self.diagram.blocks[i];
        match &b.kind {
            BlockKind::Space => (0.0, 0.0),
            BlockKind::Arrow(dirs) => {
                let w = self
                    .label(i)
                    .iter()
                    .map(|l| diagram_scene::line_width(self.measure, l, FONT_SIZE))
                    .fold(0.0, f32::max);
                let horizontal = dirs
                    .iter()
                    .any(|d| matches!(d, Dir::Right | Dir::Left | Dir::X));
                if horizontal {
                    (w + 56.0, 48.0)
                } else {
                    // The label sits beside the shaft, right of centre.
                    (2.0 * w + 40.0, 56.0)
                }
            }
            BlockKind::Leaf(s) => {
                let mut node = Node::new("", shape(*s), Rect::default());
                node.label = self.label(i);
                let (w, h) = natural_size(&node, FONT_SIZE, self.measure);
                (w.max(60.0), h.max(40.0))
            }
            BlockKind::Composite { .. } => unreachable!(),
        }
    }

    fn grid(&self, i: usize) -> Grid {
        let children = self.children(i);
        let spans: Vec<usize> = children
            .iter()
            .map(|&c| self.diagram.blocks[c].span)
            .collect();
        let columns = match &self.diagram.blocks[i].kind {
            BlockKind::Composite {
                columns: Some(n), ..
            } => *n,
            _ => spans.iter().sum::<usize>().max(1),
        };
        let mut cells = Vec::new();
        let mut at = 0;
        for span in spans {
            let span = span.min(columns);
            if at % columns + span > columns {
                at += columns - at % columns;
            }
            cells.push((at / columns, at % columns, span));
            at += span;
        }
        Grid {
            columns,
            rows: at.div_ceil(columns).max(1),
            cells,
        }
    }

    fn header(&self, i: usize) -> f32 {
        if i != 0 && self.diagram.blocks[i].label.is_some() {
            HEADER
        } else {
            0.0
        }
    }

    /// The natural size of block `i`, a composite's from its children.
    fn size(&mut self, i: usize) -> (f32, f32) {
        if !matches!(self.diagram.blocks[i].kind, BlockKind::Composite { .. }) {
            return self.natural[i];
        }
        let children = self.children(i).to_vec();
        let grid = self.grid(i);
        let mut cw = 0.0_f32;
        for (&c, &(_, _, span)) in children.iter().zip(&grid.cells) {
            let (w, _) = self.size(c);
            if matches!(self.diagram.blocks[c].kind, BlockKind::Space) {
                continue;
            }
            cw = cw.max((w - GAP * (span - 1) as f32) / span as f32);
        }
        if cw == 0.0 {
            cw = 40.0;
        }
        let rows_height: f32 = self.row_heights(i, &grid).iter().sum();
        let header = self.header(i);
        let title = if header > 0.0 {
            let line = TextLine::new(self.diagram.blocks[i].text(), Font::Bold);
            diagram_scene::line_width(self.measure, &line, FONT_SIZE) + 20.0
        } else {
            0.0
        };
        let w = (grid.columns as f32 * cw + (grid.columns + 1) as f32 * GAP).max(title);
        let h = header + rows_height + (grid.rows + 1) as f32 * GAP;
        self.natural[i] = (w, h);
        (w, h)
    }

    /// Each grid row's height: its tallest child's. Children's sizes must
    /// already be known.
    fn row_heights(&self, i: usize, grid: &Grid) -> Vec<f32> {
        let mut rows = vec![0.0_f32; grid.rows];
        for (&c, &(row, _, _)) in self.children(i).iter().zip(&grid.cells) {
            if !matches!(self.diagram.blocks[c].kind, BlockKind::Space) {
                rows[row] = rows[row].max(self.natural[c].1);
            }
        }
        for r in &mut rows {
            if *r == 0.0 {
                *r = 20.0;
            }
        }
        rows
    }

    fn outline(&self, i: usize) -> Shape {
        self.nodes
            .iter()
            .find(|n| n.id == self.diagram.blocks[i].id)
            .map_or(Shape::Rect, |n| n.shape)
    }

    fn place(
        &mut self,
        i: usize,
        rect: Rect,
        containers: &mut Vec<Container>,
        texts: &mut Vec<Text>,
        arrows: &mut Vec<Edge>,
    ) {
        let block = &self.diagram.blocks[i];
        self.rects[i] = Some(rect);
        match &block.kind {
            BlockKind::Space => {}
            BlockKind::Leaf(s) => {
                let shape = shape(*s);
                let rect = if shape == Shape::Circle {
                    let d = rect.width.min(rect.height);
                    Rect::centered(rect.center(), d, d)
                } else {
                    rect
                };
                self.rects[i] = Some(rect);
                let mut node = Node::new(block.id.clone(), shape, rect);
                node.label = self.label(i);
                if *s == BlockShape::DoubleCircle {
                    node.style.emphasis = true;
                }
                apply(&mut node.style, &block.colors);
                self.nodes.push(node);
            }
            BlockKind::Arrow(dirs) => {
                let c = rect.center();
                let horizontal = dirs
                    .iter()
                    .any(|d| matches!(d, Dir::Right | Dir::Left | Dir::X));
                let vertical = dirs
                    .iter()
                    .any(|d| matches!(d, Dir::Up | Dir::Down | Dir::Y));
                let has = |d: Dir| dirs.contains(&d);
                let mut push = |a: Point, b: Point, start: bool, end: bool| {
                    let mut edge = Edge::new(vec![Seg::Move(a), Seg::Line(b)]);
                    edge.pen.weight = Weight::Thick;
                    edge.pen.paint = Paint::Role(Role::Muted);
                    if let Some((r, g, b)) = block.colors.stroke.or(block.colors.fill) {
                        edge.pen.paint = Paint::Rgb(r, g, b);
                    }
                    edge.start = if start { Marker::Arrow } else { Marker::None };
                    edge.end = if end { Marker::Arrow } else { Marker::None };
                    arrows.push(edge);
                };
                if horizontal {
                    push(
                        Point::new(rect.x + 4.0, c.y),
                        Point::new(rect.right() - 4.0, c.y),
                        has(Dir::Left) || has(Dir::X),
                        has(Dir::Right) || has(Dir::X),
                    );
                }
                if vertical {
                    push(
                        Point::new(c.x, rect.y + 4.0),
                        Point::new(c.x, rect.bottom() - 4.0),
                        has(Dir::Up) || has(Dir::Y),
                        has(Dir::Down) || has(Dir::Y),
                    );
                }
                // The label above a horizontal arrow, else beside a vertical one.
                let lines: Vec<TextLine> = self
                    .label(i)
                    .into_iter()
                    .filter(|l| !l.plain().trim().is_empty())
                    .collect();
                let n = lines.len() as f32;
                let pitch = FONT_SIZE * 1.35;
                for (k, line) in lines.into_iter().enumerate() {
                    let (pos, anchor) = if horizontal {
                        (
                            Point::new(c.x, c.y - 7.0 - (n - 1.0 - k as f32) * pitch),
                            Anchor::Middle,
                        )
                    } else {
                        (
                            Point::new(
                                c.x + 8.0,
                                c.y + FONT_SIZE * 0.35 + (k as f32 - (n - 1.0) / 2.0) * pitch,
                            ),
                            Anchor::Start,
                        )
                    };
                    texts.push(Text {
                        pos,
                        anchor,
                        line,
                        paint: Paint::Role(Role::Text),
                        backed: false,
                    });
                }
            }
            BlockKind::Composite { .. } => {
                if i != 0 {
                    let kind = ContainerKind::Group;
                    let mut style = Style::for_container(&kind);
                    apply(&mut style, &block.colors);
                    containers.push(Container {
                        rect,
                        kind,
                        label: if block.label.is_some() {
                            vec![TextLine::new(block.text(), Font::Bold)]
                        } else {
                            Vec::new()
                        },
                        style,
                    });
                }
                let grid = self.grid(i);
                let header = self.header(i);
                let cols = grid.columns as f32;
                let rows = grid.rows as f32;
                let cw = (rect.width - (cols + 1.0) * GAP) / cols;
                // Rows grow in proportion when the parent stretches this.
                let heights = self.row_heights(i, &grid);
                let natural: f32 = heights.iter().sum();
                let scale = (rect.height - header - (rows + 1.0) * GAP) / natural;
                let mut tops = Vec::with_capacity(heights.len());
                let mut y = rect.y + header + GAP;
                for h in &heights {
                    tops.push(y);
                    y += h * scale + GAP;
                }
                let children = self.children(i).to_vec();
                for (c, (row, col, span)) in children.into_iter().zip(grid.cells) {
                    let cell = Rect::new(
                        rect.x + GAP + col as f32 * (cw + GAP),
                        tops[row],
                        span as f32 * cw + (span - 1) as f32 * GAP,
                        heights[row] * scale,
                    );
                    self.place(c, cell, containers, texts, arrows);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Estimate)
    }

    fn no_overlaps(s: &Scene) {
        for (i, a) in s.nodes.iter().enumerate() {
            assert!(s.bounds().encloses(&a.rect, 0.01), "{} escapes", a.id);
            for b in &s.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
    }

    #[test]
    fn columns_wrap_and_spans_widen() {
        let s = scene("block\n  columns 3\n  a b:2 c:2 d\n");
        let r = |id: &str| s.node(id).unwrap().rect;
        // a, b on the first row; c wraps to the second; d after it.
        assert!((r("a").y - r("b").y).abs() < 0.01);
        assert!(r("c").y > r("a").bottom());
        assert!((r("d").y - r("c").y).abs() < 0.01);
        assert!((r("b").width - (2.0 * r("a").width + GAP)).abs() < 0.01);
        no_overlaps(&s);
    }

    #[test]
    fn composites_hold_their_children() {
        let s = scene(
            "block\n  columns 3\n  a:3\n  block:group1:2\n    columns 2\n    h i j k\n  end\n  g\n  block:group2:3\n    l m n o p q r\n  end\n",
        );
        no_overlaps(&s);
        assert_eq!(s.containers.len(), 2);
        let g1 = s.containers[0].rect;
        for id in ["h", "i", "j", "k"] {
            assert!(g1.encloses(&s.node(id).unwrap().rect, 0.01), "{id}");
        }
        let texts = s.texts();
        for t in ["a", "h", "k", "g", "l", "r"] {
            assert!(texts.iter().any(|x| x == t), "missing {t}");
        }
    }

    #[test]
    fn links_labels_spaces_and_arrows_are_drawn() {
        let s = scene(
            "block\n  columns 3\n  A space B\n  A-- \"X\" -->B\n  C<[\"go\"]>(right) D\n  B --> D\n  A ~~~ D\n  block:ID\n    E\n  end\n  ID --> A\n",
        );
        no_overlaps(&s);
        let texts = s.texts();
        for t in ["A", "B", "X", "go", "D", "E"] {
            assert!(texts.iter().any(|x| x == t), "missing {t}: {texts:?}");
        }
        // Three drawn links (the invisible one is not), one block arrow.
        assert_eq!(s.edges.len(), 4);
        assert_eq!(s.edges[0].end, Marker::Arrow);
    }

    #[test]
    fn styles_reach_the_nodes() {
        let s = scene(
            "block\n  A B\n  style A fill:#ff0000\n  classDef c stroke:#00ff00\n  class B c\n",
        );
        assert_eq!(s.node("A").unwrap().style.fill, Paint::Rgb(255, 0, 0));
        assert_eq!(s.node("B").unwrap().style.stroke, Paint::Rgb(0, 255, 0));
    }
}
