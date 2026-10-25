//! Flowchart layout: nodes sized to their labels, placed by the layered
//! layout, links as smooth curves through the points it leaves between
//! ranks, and subgraphs as boxes around their members.
//!
//! A subgraph whose own `direction` differs from its parent's, and that no
//! link crosses, is laid out on its own in that direction and then placed
//! as one block, as mermaid does. (When a link does cross, mermaid ignores
//! the subgraph's direction, and so does this.)

use diagram_scene::shape::{line_height, natural_size};
use diagram_scene::{
    Container, ContainerKind, Dash, Edge, EdgeLabel, Font, Marker, Node, Paint, Pen, Point, Rect,
    Run, Scene, Seg, Shape, Style, TextLine, Weight,
};

use super::parse::{Direction, End, Flowchart, LineKind, NodeShape, Stroke};
use crate::layered::{self, Graph, GraphEdge, Route, Spacing};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Space inside a subgraph's box around its members, and for its title.
const GROUP_PAD: f32 = 14.0;
const TITLE: f32 = 22.0;
/// Labels wrap at this width.
const LABEL_WIDTH: f32 = 200.0;
const EDGE_LABEL_WIDTH: f32 = 160.0;

pub fn layout(chart: &Flowchart, measure: &dyn Measure) -> Scene {
    let ctx = Ctx::new(chart, measure);
    let mut block = ctx.place(None, chart.direction);
    block.shift(PADDING, PADDING);
    let mut scene = Scene::new(block.width + 2.0 * PADDING, block.height + 2.0 * PADDING);
    scene.font_size = FONT_SIZE;

    // Outermost first, so inner boxes draw over their parents.
    block
        .boxes
        .sort_by_key(|&(s, _)| ctx.chain_of(Some(s)).len());
    for &(s, rect) in &block.boxes {
        let sub = &chart.subgraphs[s];
        let kind = ContainerKind::Subgraph;
        let mut style = Style::for_container(&kind);
        let colors = chart.subgraph_colors(sub);
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
        scene.containers.push(Container {
            rect,
            label: ctx.titles[s].clone(),
            style,
            kind,
        });
    }

    let mut nodes = ctx.nodes.clone();
    for &(n, rect) in &block.nodes {
        nodes[n].rect = rect;
    }
    // A link to a subgraph stops at the subgraph's box; its anchor inside
    // is not drawn.
    let sub_box = |sub: Option<usize>, n: usize| {
        let node = &chart.nodes[n];
        sub.or_else(|| {
            (node.shape == NodeShape::Anchor)
                .then_some(node.parent)
                .flatten()
        })
        .and_then(|s| block.boxes.iter().find(|(b, _)| *b == s))
        .map(|&(_, rect)| rect)
    };
    for (i, edge) in &mut block.edges {
        let link = &chart.links[*i];
        let mut clipped = false;
        if let Some(rect) = sub_box(link.to_sub, link.to)
            && let Some(path) = clip_at_box(&edge.path, rect, true)
        {
            edge.path = path;
            clipped = true;
        }
        if let Some(rect) = sub_box(link.from_sub, link.from)
            && let Some(path) = clip_at_box(&edge.path, rect, false)
        {
            edge.path = path;
            clipped = true;
        }
        if let Some(s) = link.to_sub {
            edge.to = Some(chart.subgraphs[s].id.clone());
        }
        if let Some(s) = link.from_sub {
            edge.from = Some(chart.subgraphs[s].id.clone());
        }
        // The label goes back to the middle of what is left of the line.
        if clipped && let Some(pos) = diagram_scene::route::label_position(&edge.path, 0.0) {
            for label in &mut edge.labels {
                label.pos = pos;
            }
        }
    }
    scene.nodes = nodes
        .into_iter()
        .zip(&chart.nodes)
        .filter(|(_, source)| source.shape != NodeShape::Anchor)
        .map(|(node, _)| node)
        .collect();
    block.edges.sort_by_key(|(i, _)| *i);
    scene.edges = block.edges.into_iter().map(|(_, e)| e).collect();
    scene
}

/// What every block shares: nodes drawn and sized, labels wrapped, and
/// which subgraphs are laid out on their own.
struct Ctx<'a> {
    chart: &'a Flowchart,
    measure: &'a dyn Measure,
    nodes: Vec<Node>,
    sizes: Vec<(f32, f32)>,
    labels: Vec<Option<Vec<TextLine>>>,
    label_sizes: Vec<Option<(f32, f32)>>,
    titles: Vec<Vec<TextLine>>,
    /// The direction of each subgraph laid out on its own.
    own: Vec<Option<Direction>>,
}

/// A laid-out part of the chart, in its own coordinates from (0, 0).
#[derive(Default)]
struct Block {
    nodes: Vec<(usize, Rect)>,
    boxes: Vec<(usize, Rect)>,
    /// (link index, edge)
    edges: Vec<(usize, Edge)>,
    width: f32,
    height: f32,
}

enum Item {
    Node(usize),
    Block(usize, Block),
}

impl<'a> Ctx<'a> {
    fn new(chart: &'a Flowchart, measure: &'a dyn Measure) -> Self {
        let nodes: Vec<Node> = chart
            .nodes
            .iter()
            .map(|n| {
                let mut node = Node::new(n.id.clone(), shape(n.shape), Rect::default());
                if !n.shape.unlabelled() {
                    node.label = text_lines(measure, &n.label, n.markdown, Font::Body, LABEL_WIDTH);
                }
                if n.shape == NodeShape::Text {
                    node.style.fill = Paint::None;
                    node.style.stroke = Paint::None;
                }
                let colors = chart.colors(n);
                if let Some((r, g, b)) = colors.fill {
                    node.style.fill = Paint::Rgb(r, g, b);
                }
                if let Some((r, g, b)) = colors.stroke {
                    node.style.stroke = Paint::Rgb(r, g, b);
                }
                if let Some((r, g, b)) = colors.text {
                    node.style.text = Paint::Rgb(r, g, b);
                }
                if colors.dashed {
                    node.style.dash = Dash::Dashed;
                }
                node
            })
            .collect();
        let sizes = nodes
            .iter()
            .zip(&chart.nodes)
            .map(|(n, source)| {
                let (w, h) = natural_size(n, FONT_SIZE, measure);
                match n.shape {
                    _ if source.shape == NodeShape::Anchor => (2.0, 2.0),
                    Shape::Start | Shape::End | Shape::ForkBar => (w, h),
                    _ => (w.max(48.0), h.max(36.0)),
                }
            })
            .collect();
        let labels: Vec<Option<Vec<TextLine>>> = chart
            .links
            .iter()
            .map(|l| {
                l.label.as_ref().map(|text| {
                    // Markdown runs are body-sized: there is no small bold.
                    let font = if l.markdown { Font::Body } else { Font::Small };
                    text_lines(measure, text, l.markdown, font, EDGE_LABEL_WIDTH)
                })
            })
            .collect();
        let label_sizes = labels
            .iter()
            .map(|label| {
                label.as_ref().map(|lines| {
                    let w = lines
                        .iter()
                        .map(|line| diagram_scene::line_width(measure, line, FONT_SIZE))
                        .fold(0.0_f32, f32::max);
                    (w + 16.0, lines_height(lines) + 6.0)
                })
            })
            .collect();
        let titles = chart
            .subgraphs
            .iter()
            .map(|s| {
                if s.title.is_empty() {
                    Vec::new()
                } else {
                    text_lines(measure, &s.title, s.markdown, Font::Bold, f32::INFINITY)
                }
            })
            .collect();

        // Which subgraphs go their own way. Parents come before children.
        let mut ctx = Ctx {
            chart,
            measure,
            nodes,
            sizes,
            labels,
            label_sizes,
            titles,
            own: vec![None; chart.subgraphs.len()],
        };
        let mut effective = vec![chart.direction; chart.subgraphs.len()];
        for (s, sub) in chart.subgraphs.iter().enumerate() {
            let parent = sub.parent.map_or(chart.direction, |p| effective[p]);
            let inside = |n: usize| ctx.chain_of(chart.nodes[n].parent).contains(&s);
            let crossed = chart.links.iter().any(|l| inside(l.from) != inside(l.to));
            ctx.own[s] = sub.direction.filter(|&d| d != parent && !crossed);
            effective[s] = ctx.own[s].unwrap_or(parent);
        }
        ctx
    }

    /// A subgraph and its ancestors, outermost first.
    fn chain_of(&self, mut sub: Option<usize>) -> Vec<usize> {
        let mut chain = Vec::new();
        while let Some(s) = sub {
            chain.push(s);
            sub = self.chart.subgraphs[s].parent;
        }
        chain.reverse();
        chain
    }

    /// The nearest enclosing subgraph laid out on its own, from `sub` out.
    fn owner(&self, mut sub: Option<usize>) -> Option<usize> {
        while let Some(s) = sub {
            if self.own[s].is_some() {
                return Some(s);
            }
            sub = self.chart.subgraphs[s].parent;
        }
        None
    }

    fn title_width(&self, s: usize) -> f32 {
        self.titles[s]
            .iter()
            .map(|l| diagram_scene::line_width(self.measure, l, FONT_SIZE))
            .fold(0.0_f32, f32::max)
    }

    /// The narrowest a box can be and still hold its title (drawn 10 in).
    fn min_box_width(&self, s: usize) -> f32 {
        match self.title_width(s) {
            0.0 => 0.0,
            w => w + 20.0,
        }
    }

    fn title_height(&self, s: usize) -> f32 {
        match self.titles[s].len() {
            0 => 0.0,
            n => TITLE + (n - 1) as f32 * line_height(FONT_SIZE),
        }
    }

    /// A subgraph's box, widened to the left where a link would run
    /// through its title, so the title sits clear of it — unless something
    /// outside the subgraph is in the way.
    fn clear_title(&self, s: usize, rect: Rect, block: &Block) -> Rect {
        let width = self.title_width(s);
        if width == 0.0 {
            return rect;
        }
        // The title starts 10 in; keep a little air after it.
        let band = Rect::new(rect.x, rect.y, 10.0 + width + 6.0, self.title_height(s));
        let Some(x) = block
            .edges
            .iter()
            .filter_map(|(_, e)| leftmost_crossing(&e.path, band))
            .reduce(f32::min)
        else {
            return rect;
        };
        let left = x - band.width;
        if left >= rect.x {
            return rect;
        }
        let strip = Rect::new(left, rect.y, rect.x - left, rect.height);
        let outside = |sub: Option<usize>| !self.chain_of(sub).contains(&s);
        let blocked = block
            .nodes
            .iter()
            .any(|&(n, r)| outside(self.chart.nodes[n].parent) && r.intersects(&strip))
            || block
                .boxes
                .iter()
                .any(|&(b, r)| outside(Some(b)) && r.intersects(&strip));
        if blocked {
            return rect;
        }
        Rect::new(left, rect.y, rect.right() - left, rect.height)
    }

    /// Lay out what `scope` (a subgraph, or the whole chart) holds directly,
    /// with the subgraphs inside it that go their own way as blocks.
    fn place(&self, scope: Option<usize>, direction: Direction) -> Block {
        let chart = self.chart;
        let horizontal = matches!(direction, Direction::LeftRight | Direction::RightLeft);
        let within =
            |chain: Vec<usize>| match scope.and_then(|s| chain.iter().position(|&c| c == s)) {
                Some(at) => chain[at + 1..].to_vec(),
                None => chain,
            };

        let mut items = Vec::new();
        let mut sizes = Vec::new();
        let mut groups = Vec::new();
        let mut item_of = vec![usize::MAX; chart.nodes.len()];
        for (n, node) in chart.nodes.iter().enumerate() {
            if self.owner(node.parent) == scope {
                item_of[n] = items.len();
                let (w, h) = self.sizes[n];
                // A bar stands across the flow.
                let size = if horizontal && node.shape == NodeShape::Bar {
                    (h, w)
                } else {
                    (w, h)
                };
                sizes.push(size);
                groups.push(within(self.chain_of(node.parent)));
                items.push(Item::Node(n));
            }
        }
        for (s, sub) in chart.subgraphs.iter().enumerate() {
            if let Some(dir) = self.own[s]
                && self.owner(sub.parent) == scope
            {
                let block = self.place(Some(s), dir);
                sizes.push((
                    (block.width + 2.0 * GROUP_PAD).max(self.min_box_width(s)),
                    block.height + 2.0 * GROUP_PAD + self.title_height(s),
                ));
                groups.push(within(self.chain_of(sub.parent)));
                items.push(Item::Block(s, block));
            }
        }
        // Links stay within one block: none crosses a block's boundary.
        let local: Vec<usize> = (0..chart.links.len())
            .filter(|&i| item_of[chart.links[i].from] != usize::MAX)
            .collect();
        let depth = groups.iter().map(Vec::len).max().unwrap_or(0) as f32;
        let graph = Graph {
            sizes: sizes.clone(),
            edges: local
                .iter()
                .map(|&i| GraphEdge {
                    from: item_of[chart.links[i].from],
                    to: item_of[chart.links[i].to],
                    label: self.label_sizes[i],
                })
                .collect(),
            groups,
            horizontal,
            reversed: matches!(direction, Direction::BottomUp | Direction::RightLeft),
        };
        let spacing = Spacing {
            node_gap: 32.0,
            // Room between ranks for nested subgraphs' borders and titles.
            rank_gap: 22.0 + depth * (GROUP_PAD + TITLE) / 2.0,
            group_gap: GROUP_PAD * 2.0,
            margin: 0.0,
        };
        let placed = layered::layout(&graph, &spacing);

        let mut block = Block::default();
        let mut rects = Vec::with_capacity(items.len());
        let mut shapes = Vec::with_capacity(items.len());
        let mut boxes: Vec<Option<Rect>> = vec![None; chart.subgraphs.len()];
        for ((item, &(w, h)), &c) in items.into_iter().zip(&sizes).zip(&placed.centers) {
            let rect = Rect::centered(c, w, h);
            rects.push(rect);
            match item {
                Item::Node(n) => {
                    shapes.push(self.nodes[n].shape);
                    block.nodes.push((n, rect));
                }
                Item::Block(s, mut inner) => {
                    shapes.push(Shape::Rect);
                    inner.shift(
                        rect.x + (rect.width - inner.width) / 2.0,
                        rect.y + GROUP_PAD + self.title_height(s),
                    );
                    block.nodes.append(&mut inner.nodes);
                    block.boxes.append(&mut inner.boxes);
                    block.edges.append(&mut inner.edges);
                    boxes[s] = Some(rect);
                    block.boxes.push((s, rect));
                }
            }
        }

        // Links: curves through the layout's points, leaving and arriving
        // along the flow. Invisible ones only shaped the layout.
        let drawn: Vec<usize> = (0..local.len())
            .filter(|&k| chart.links[local[k]].line != LineKind::Invisible)
            .collect();
        let ends: Vec<(usize, usize, &Route)> = drawn
            .iter()
            .map(|&k| {
                let link = &chart.links[local[k]];
                (item_of[link.from], item_of[link.to], &placed.routes[k])
            })
            .collect();
        let ports = ports(&ends, &rects, &shapes, horizontal);
        for (&k, (port, &(from, _, route))) in drawn.iter().zip(ports.iter().zip(&ends)) {
            let i = local[k];
            let link = &chart.links[i];
            let path = match port {
                Some((start, end)) => curve(*start, *end, &route.points, horizontal),
                None => self_loop(rects[from], horizontal),
            };
            let label_pos = route
                .label
                .or_else(|| diagram_scene::route::label_position(&path, 0.0));
            let mut edge =
                Edge::new(path).between(&chart.nodes[link.from].id, &chart.nodes[link.to].id);
            edge.pen = pen(chart, i);
            edge.start = marker(link.start);
            edge.end = marker(link.end);
            if let (Some(lines), Some(pos)) = (self.labels[i].clone(), label_pos) {
                let mut label = EdgeLabel::new(pos, lines, true, self.measure, FONT_SIZE);
                if let Some((r, g, b)) = chart.link_style(link).color {
                    label.paint = Paint::Rgb(r, g, b);
                }
                edge.labels.push(label);
            }
            block.edges.push((i, edge));
        }
        // Boxes of the subgraphs drawn in this block, innermost first so
        // their parents can wrap them.
        let mut inline: Vec<usize> = (0..chart.subgraphs.len())
            .filter(|&s| self.own[s].is_none() && self.owner(chart.subgraphs[s].parent) == scope)
            .collect();
        inline.sort_by_key(|&s| std::cmp::Reverse(self.chain_of(Some(s)).len()));
        for s in inline {
            let mut extent: Option<Rect> = None;
            let mut grow = |r: Rect| extent = Some(extent.map_or(r, |e| e.union(&r)));
            for &(n, rect) in &block.nodes {
                if chart.nodes[n].parent == Some(s) {
                    grow(rect);
                }
            }
            for (child, sub) in chart.subgraphs.iter().enumerate() {
                if sub.parent == Some(s)
                    && let Some(r) = boxes[child]
                {
                    grow(r);
                }
            }
            if let Some(r) = extent {
                let r = r.inflate(GROUP_PAD);
                // Wide enough for the title, growing evenly both ways.
                let grow = (self.min_box_width(s) - r.width).max(0.0);
                let r = Rect::new(r.x - grow / 2.0, r.y, r.width + grow, r.height);
                let title = self.title_height(s);
                let rect = self.clear_title(
                    s,
                    Rect::new(r.x, r.y - title, r.width, r.height + title),
                    &block,
                );
                boxes[s] = Some(rect);
                block.boxes.push((s, rect));
            }
        }

        block.fit(Rect::new(0.0, 0.0, placed.width, placed.height));
        block
    }
}

impl Block {
    fn shift(&mut self, dx: f32, dy: f32) {
        let move_point = |p: &mut Point| {
            p.x += dx;
            p.y += dy;
        };
        for (_, r) in self.nodes.iter_mut().chain(self.boxes.iter_mut()) {
            r.x += dx;
            r.y += dy;
        }
        for (_, e) in &mut self.edges {
            for seg in &mut e.path {
                match seg {
                    Seg::Move(p) | Seg::Line(p) => move_point(p),
                    Seg::Cubic(a, b, p) => {
                        move_point(a);
                        move_point(b);
                        move_point(p);
                    }
                    Seg::Close => {}
                }
            }
            for l in &mut e.labels {
                move_point(&mut l.pos);
            }
        }
    }

    /// Move everything drawn, and the layout's own frame, to start at
    /// (0, 0), and take its size.
    fn fit(&mut self, frame: Rect) {
        let mut extent = Some(frame);
        let mut grow = |r: Rect| extent = Some(extent.map_or(r, |e| e.union(&r)));
        for &(_, r) in self.nodes.iter().chain(&self.boxes) {
            grow(r);
        }
        for (_, e) in &self.edges {
            for poly in diagram_scene::flatten::flatten(&e.path, 1.0) {
                for p in poly.points {
                    grow(Rect::new(p.x, p.y, 0.0, 0.0));
                }
            }
            for l in &e.labels {
                let h = lines_height(&l.lines) + 4.0;
                grow(Rect::centered(l.pos, l.text_width(FONT_SIZE) + 12.0, h));
            }
        }
        let extent = extent.unwrap_or_default();
        self.shift(-extent.x, -extent.y);
        self.width = extent.width;
        self.height = extent.height;
    }
}

/// `path` cut where it first enters `rect`, coming from its start (`at_end`)
/// or from its end; `None` when it never enters it from outside.
fn clip_at_box(path: &[Seg], rect: Rect, at_end: bool) -> Option<Vec<Seg>> {
    let mut points: Vec<Point> = diagram_scene::flatten::flatten(path, 0.5)
        .into_iter()
        .flat_map(|poly| poly.points)
        .collect();
    if !at_end {
        points.reverse();
    }
    let k = points.iter().position(|p| rect.contains(*p))?;
    if k == 0 {
        return None;
    }
    // Bisect the step that crosses the box's edge.
    let (mut out, mut inside) = (points[k - 1], points[k]);
    for _ in 0..24 {
        let mid = out.lerp(inside, 0.5);
        if rect.contains(mid) {
            inside = mid;
        } else {
            out = mid;
        }
    }
    let mut kept = points[..k].to_vec();
    kept.push(out);
    if !at_end {
        kept.reverse();
    }
    let mut segs = vec![Seg::Move(kept[0])];
    segs.extend(kept[1..].iter().map(|&p| Seg::Line(p)));
    Some(segs)
}

/// The leftmost x at which `path` passes through `band`.
fn leftmost_crossing(path: &[Seg], band: Rect) -> Option<f32> {
    let mut best: Option<f32> = None;
    for poly in diagram_scene::flatten::flatten(path, 0.5) {
        for w in poly.points.windows(2) {
            let (p, q) = (w[0], w[1]);
            let (lo, hi) = (band.y, band.bottom());
            if p.y.max(q.y) < lo || p.y.min(q.y) > hi {
                continue;
            }
            let at = |y: f32| {
                if (q.y - p.y).abs() < 1e-6 {
                    p.x
                } else {
                    p.x + (q.x - p.x) * ((y - p.y) / (q.y - p.y)).clamp(0.0, 1.0)
                }
            };
            let (y0, y1) = (p.y.min(q.y).max(lo), p.y.max(q.y).min(hi));
            let (x0, x1) = if (q.y - p.y).abs() < 1e-6 {
                (p.x.min(q.x), p.x.max(q.x))
            } else {
                (at(y0).min(at(y1)), at(y0).max(at(y1)))
            };
            if x1 < band.x || x0 > band.right() {
                continue;
            }
            let x = x0.max(band.x);
            best = Some(best.map_or(x, |b: f32| b.min(x)));
        }
    }
    best
}

fn lines_height(lines: &[TextLine]) -> f32 {
    lines
        .iter()
        .map(|l| line_height(l.font().size(FONT_SIZE)))
        .sum()
}

fn shape(shape: NodeShape) -> Shape {
    match shape {
        NodeShape::Rect | NodeShape::Asymmetric | NodeShape::Text => Shape::Rect,
        NodeShape::Round => Shape::Rounded,
        NodeShape::Stadium => Shape::Stadium,
        NodeShape::Subroutine => Shape::Subroutine,
        NodeShape::Cylinder => Shape::Cylinder,
        NodeShape::Circle | NodeShape::DoubleCircle => Shape::Circle,
        NodeShape::Ellipse => Shape::Ellipse,
        NodeShape::Rhombus => Shape::Diamond,
        NodeShape::Hexagon => Shape::Hexagon,
        NodeShape::Parallelogram | NodeShape::ParallelogramAlt => Shape::Parallelogram,
        NodeShape::Trapezoid | NodeShape::TrapezoidAlt => Shape::Trapezoid,
        NodeShape::Document => Shape::Document,
        NodeShape::HorizontalCylinder => Shape::Queue,
        NodeShape::Cloud => Shape::Cloud,
        NodeShape::Note => Shape::Note,
        NodeShape::Dot => Shape::Start,
        NodeShape::RingedDot => Shape::End,
        NodeShape::Bar => Shape::ForkBar,
        NodeShape::Anchor => Shape::Start,
    }
}

fn marker(end: End) -> Marker {
    match end {
        End::None => Marker::None,
        End::Arrow => Marker::Arrow,
        End::Circle => Marker::Circle,
        End::Cross => Marker::Cross,
    }
}

/// A link's line: its kind, then any `linkStyle`. (A `linkStyle`'s `color`
/// colours the link's label instead.)
fn pen(chart: &Flowchart, i: usize) -> Pen {
    let link = &chart.links[i];
    let mut pen = Pen {
        dash: match link.line {
            LineKind::Dotted => Dash::Dashed,
            _ => Dash::Solid,
        },
        weight: match link.line {
            LineKind::Thick => Weight::Thick,
            _ => Weight::Normal,
        },
        ..Pen::default()
    };
    let style = chart.link_style(link);
    if let Some((r, g, b)) = style.stroke {
        pen.paint = Paint::Rgb(r, g, b);
    }
    if let Some(w) = style.width {
        pen.weight = if w <= 1.0 {
            Weight::Thin
        } else if w < 3.0 {
            Weight::Normal
        } else if w < 5.0 {
            Weight::Thick
        } else {
            Weight::Heavy
        };
    }
    if let Some(dash) = style.dash {
        pen.dash = match dash {
            Stroke::Solid => Dash::Solid,
            Stroke::Dashed => Dash::Dashed,
            Stroke::Dotted => Dash::Dotted,
        };
    }
    pen
}

/// A label as lines: wrapped at `max_width`, in `font`, with markdown's
/// `**bold**` and `*italic*` (or `__`/`_`) as bold and italic runs.
pub(crate) fn text_lines(
    measure: &dyn Measure,
    text: &str,
    markdown: bool,
    font: Font,
    max_width: f32,
) -> Vec<TextLine> {
    if !markdown {
        return text
            .split('\n')
            .flat_map(|line| {
                crate::text::wrap(measure, line, font.size(FONT_SIZE), font.bold(), max_width)
            })
            .map(|line| TextLine::new(line, font))
            .collect();
    }
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        // Words, each a run or more (`**a**b` is one word in two fonts).
        let mut words: Vec<Vec<Run>> = Vec::new();
        let mut open = false;
        for (span, emphasis) in markdown_spans(paragraph) {
            let run_font = match emphasis {
                Emphasis::Plain => font,
                Emphasis::Bold => Font::Bold,
                Emphasis::Italic => Font::Italic,
            };
            let mut first = true;
            for piece in span.split(' ') {
                if !first {
                    open = false;
                }
                first = false;
                if piece.is_empty() {
                    continue;
                }
                let run = Run {
                    text: piece.to_string(),
                    font: run_font,
                };
                match words.last_mut() {
                    Some(word) if open => word.push(run),
                    _ => words.push(vec![run]),
                }
                open = true;
            }
        }
        let mut line: Vec<Run> = Vec::new();
        for word in words {
            let mut candidate = line.clone();
            // The space between takes the font on both sides of it, or
            // else the label's own.
            if let Some(last) = candidate.last()
                && let Some(next) = word.first()
            {
                let font = if last.font == next.font {
                    last.font
                } else {
                    font
                };
                candidate.push(Run {
                    text: " ".into(),
                    font,
                });
            }
            candidate.extend(word.iter().cloned());
            let width = diagram_scene::line_width(
                measure,
                &TextLine {
                    runs: candidate.clone(),
                },
                FONT_SIZE,
            );
            if !line.is_empty() && width > max_width {
                out.push(merged(std::mem::take(&mut line)));
                line = word;
            } else {
                line = candidate;
            }
        }
        out.push(merged(line));
    }
    out
}

/// Adjacent runs in one font made one.
fn merged(runs: Vec<Run>) -> TextLine {
    let mut out: Vec<Run> = Vec::new();
    for run in runs {
        match out.last_mut() {
            Some(last) if last.font == run.font => last.text.push_str(&run.text),
            _ => out.push(run),
        }
    }
    TextLine { runs: out }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Emphasis {
    Plain,
    Bold,
    Italic,
}

/// Markdown's emphasis in one line: `**`/`__` bold, `*`/`_` italic. A
/// marker without its partner is literal text, and `_` inside a word
/// (`snake_case`) is not a marker. Bold italic is drawn bold.
fn markdown_spans(line: &str) -> Vec<(String, Emphasis)> {
    let mut out = Vec::new();
    spans_into(line, Emphasis::Plain, &mut out);
    out
}

fn spans_into(s: &str, outer: Emphasis, out: &mut Vec<(String, Emphasis)>) {
    let mut plain = String::new();
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        let delim = ["**", "__", "*", "_"]
            .into_iter()
            .find(|d| rest.starts_with(d));
        if let Some(d) = delim
            && opens(s, i, d)
            && let Some(close) = closer(s, i + d.len(), d)
        {
            if !plain.is_empty() {
                out.push((std::mem::take(&mut plain), outer));
            }
            let emphasis = match (outer, d.len()) {
                (Emphasis::Bold, _) | (_, 2) => Emphasis::Bold,
                _ => Emphasis::Italic,
            };
            spans_into(&s[i + d.len()..close], emphasis, out);
            i = close + d.len();
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        plain.push(c);
        i += c.len_utf8();
    }
    if !plain.is_empty() {
        out.push((plain, outer));
    }
}

/// A marker at `i` can open: text follows it, and `_` starts a word.
fn opens(s: &str, i: usize, d: &str) -> bool {
    let next = s[i + d.len()..].chars().next();
    let before = s[..i].chars().next_back();
    next.is_some_and(|c| !c.is_whitespace())
        && !(d.starts_with('_') && before.is_some_and(char::is_alphanumeric))
}

/// Where the marker opened before `from` closes.
fn closer(s: &str, from: usize, d: &str) -> Option<usize> {
    let mut j = from;
    while j < s.len() {
        let rest = &s[j..];
        if rest.starts_with(d) {
            let before = s[..j].chars().next_back();
            let after = s[j + d.len()..].chars().next();
            let doubled = d.len() == 1 && after == d.chars().next();
            let ok = j > from
                && before.is_some_and(|c| !c.is_whitespace())
                && !(d.starts_with('_') && after.is_some_and(char::is_alphanumeric));
            if ok && !doubled {
                return Some(j);
            }
            if doubled {
                // `**` inside an `*…*`: skip the pair.
                j += 2;
                continue;
            }
        }
        j += rest.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// Where each link leaves and arrives (`None` for self-loops): on the
/// side of each item facing along the flow, with the links sharing a side
/// spread across it in the order their other ends lie, so they don't
/// bunch into one point.
fn ports(
    ends: &[(usize, usize, &Route)],
    rects: &[Rect],
    shapes: &[Shape],
    horizontal: bool,
) -> Vec<Option<(Point, Point)>> {
    use diagram_scene::route::Side;
    use std::collections::HashMap;
    // The links ending on an item's side: (link, whether it starts there,
    // where its other end lies across the flow).
    type Ends = Vec<(usize, bool, f32)>;
    let mut sides: HashMap<(usize, Side), Ends> = HashMap::new();
    let across = |p: Point| if horizontal { p.y } else { p.x };
    let side_toward = |rect: Rect, toward: Point| {
        let c = rect.center();
        if horizontal {
            if toward.x >= c.x {
                Side::Right
            } else {
                Side::Left
            }
        } else if toward.y >= c.y {
            Side::Bottom
        } else {
            Side::Top
        }
    };
    for (i, &(from, to, route)) in ends.iter().enumerate() {
        if from == to {
            continue;
        }
        let first = route.points.first().copied().unwrap_or(rects[to].center());
        let last = route.points.last().copied().unwrap_or(rects[from].center());
        sides
            .entry((from, side_toward(rects[from], first)))
            .or_default()
            .push((i, true, across(first)));
        sides
            .entry((to, side_toward(rects[to], last)))
            .or_default()
            .push((i, false, across(last)));
    }
    let mut out: Vec<(Option<Point>, Option<Point>)> = vec![(None, None); ends.len()];
    for ((item, side), mut list) in sides {
        list.sort_by(|a, b| a.2.total_cmp(&b.2));
        let rect = rects[item];
        let length = match side {
            Side::Top | Side::Bottom => rect.width,
            Side::Left | Side::Right => rect.height,
        };
        let count = list.len();
        let span = (length * 0.6).min((count.saturating_sub(1)) as f32 * 16.0);
        for (k, (link, is_start, _)) in list.into_iter().enumerate() {
            let offset = if count > 1 {
                -span / 2.0 + span * k as f32 / (count - 1) as f32
            } else {
                0.0
            };
            let base = diagram_scene::route::port(rect, side);
            let p = match side {
                Side::Top | Side::Bottom => base.offset(offset, 0.0),
                Side::Left | Side::Right => base.offset(0.0, offset),
            };
            // Slanted, pointed and round shapes: meet the outline itself.
            let p = match shapes[item] {
                s @ (Shape::Parallelogram
                | Shape::Trapezoid
                | Shape::Hexagon
                | Shape::Diamond
                | Shape::Circle
                | Shape::Ellipse) => diagram_scene::route::clip(s, rect, p),
                _ => p,
            };
            if is_start {
                out[link].0 = Some(p);
            } else {
                out[link].1 = Some(p);
            }
        }
    }
    out.into_iter().map(|(a, b)| Some((a?, b?))).collect()
}

/// A smooth curve from `start` through `points` to `end`, each step
/// leaving and arriving along the flow.
fn curve(start: Point, end: Point, points: &[Point], horizontal: bool) -> Vec<Seg> {
    let mut all = vec![start];
    all.extend_from_slice(points);
    all.push(end);
    let mut segs = vec![Seg::Move(start)];
    for w in all.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (c1, c2) = if horizontal {
            let mid = (b.x - a.x) * 0.5;
            (a.offset(mid, 0.0), b.offset(-mid, 0.0))
        } else {
            let mid = (b.y - a.y) * 0.5;
            (a.offset(0.0, mid), b.offset(0.0, -mid))
        };
        segs.push(Seg::Cubic(c1, c2, b));
    }
    segs
}

/// A loop out of a node's side and back in.
fn self_loop(r: Rect, horizontal: bool) -> Vec<Seg> {
    let c = r.center();
    if horizontal {
        let (a, b) = (
            Point::new(c.x - r.width / 4.0, r.bottom()),
            Point::new(c.x + r.width / 4.0, r.bottom()),
        );
        vec![
            Seg::Move(a),
            Seg::Cubic(a.offset(-10.0, 34.0), b.offset(10.0, 34.0), b),
        ]
    } else {
        let (a, b) = (
            Point::new(r.right(), c.y - r.height / 4.0),
            Point::new(r.right(), c.y + r.height / 4.0),
        );
        vec![
            Seg::Move(a),
            Seg::Cubic(a.offset(34.0, -10.0), b.offset(34.0, 10.0), b),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Estimate)
    }

    fn fonts(line: &TextLine) -> Vec<(String, Font)> {
        line.runs.iter().map(|r| (r.text.clone(), r.font)).collect()
    }

    #[test]
    fn markdown_emphasis() {
        let spans = markdown_spans("a **bold** and *it* or _it_ snake_case **x");
        assert_eq!(
            spans,
            vec![
                ("a ".into(), Emphasis::Plain),
                ("bold".into(), Emphasis::Bold),
                (" and ".into(), Emphasis::Plain),
                ("it".into(), Emphasis::Italic),
                (" or ".into(), Emphasis::Plain),
                ("it".into(), Emphasis::Italic),
                (" snake_case **x".into(), Emphasis::Plain),
            ]
        );
        assert_eq!(
            markdown_spans("*a **b** c*"),
            vec![
                ("a ".into(), Emphasis::Italic),
                ("b".into(), Emphasis::Bold),
                (" c".into(), Emphasis::Italic),
            ]
        );
    }

    #[test]
    fn markdown_labels_become_runs() {
        let s = scene("flowchart TD\n  A[\"`The **cat** in *the* hat`\"] --> B[\"**literal**\"]\n");
        let a = &s.node("A").unwrap().label;
        assert_eq!(a.len(), 1);
        assert_eq!(
            fonts(&a[0]),
            vec![
                ("The ".into(), Font::Body),
                ("cat".into(), Font::Bold),
                (" in ".into(), Font::Body),
                ("the".into(), Font::Italic),
                (" hat".into(), Font::Body),
            ]
        );
        // Not a markdown string: shown as written.
        assert_eq!(s.node("B").unwrap().label[0].plain(), "**literal**");
    }

    #[test]
    fn markdown_wraps_across_runs() {
        let long = "word ".repeat(20).trim().to_string();
        let s = scene(&format!("flowchart TD\n  A[\"`**{long}**`\"]\n"));
        let lines = &s.node("A").unwrap().label;
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|l| l.font() == Font::Bold));
    }

    #[test]
    fn invisible_links_place_but_are_not_drawn() {
        let s = scene("flowchart TD\n  A ~~~ B\n  B --> C\n");
        assert_eq!(s.edges.len(), 1);
        let (a, b) = (s.node("A").unwrap().rect, s.node("B").unwrap().rect);
        assert!(a.bottom() < b.y, "the invisible link still ranks B below A");
    }

    #[test]
    fn link_styles_reach_the_pen() {
        let s = scene(
            "flowchart TD\n  A --> B --> C --> D\n  linkStyle 0 stroke:#ff0000,stroke-width:4px\n  linkStyle 2 stroke-dasharray: 2 4\n  linkStyle default stroke:#00ff00\n",
        );
        assert_eq!(s.edges[0].pen.paint, Paint::Rgb(255, 0, 0));
        assert_eq!(s.edges[0].pen.weight, Weight::Thick);
        assert_eq!(s.edges[1].pen.paint, Paint::Rgb(0, 255, 0));
        assert_eq!(s.edges[2].pen.dash, Dash::Dotted);
        // Its own linkStyle replaces the default, as in mermaid.
        assert_eq!(s.edges[2].pen.paint, Paint::Role(diagram_scene::Role::Line));
    }

    #[test]
    fn a_link_style_color_paints_the_label() {
        let s =
            scene("flowchart TD\n  A -->|yes| B -->|no| C\n  linkStyle 0 stroke:#00f,color:#c00\n");
        assert_eq!(s.edges[0].labels[0].paint, Paint::Rgb(0xcc, 0, 0));
        assert_eq!(
            s.edges[1].labels[0].paint,
            Paint::Role(diagram_scene::Role::Muted)
        );
    }

    #[test]
    fn subgraph_styles_reach_the_container() {
        let s = scene(
            "flowchart TD\n  subgraph one [One]\n    A\n  end\n  style one fill:#eef,stroke:#336,color:#112,stroke-dasharray:4 4\n",
        );
        let style = s.containers[0].style;
        assert_eq!(style.fill, Paint::Rgb(0xee, 0xee, 0xff));
        assert_eq!(style.stroke, Paint::Rgb(0x33, 0x33, 0x66));
        assert_eq!(style.text, Paint::Rgb(0x11, 0x11, 0x22));
        assert_eq!(style.dash, Dash::Dashed);
    }

    #[test]
    fn a_subgraph_with_its_own_direction_flows_its_way() {
        let s = scene(
            "flowchart TD\n  X --> Y\n  subgraph side [Side]\n    direction LR\n    A --> B --> C\n  end\n",
        );
        let r = |id: &str| s.node(id).unwrap().rect;
        // Inside: left to right, on one row.
        assert!(r("A").right() < r("B").x && r("B").right() < r("C").x);
        assert!((r("A").center().y - r("C").center().y).abs() < 0.5);
        // Outside: still top-down.
        assert!(r("X").bottom() < r("Y").y);
        let group = s.containers[0].rect;
        for id in ["A", "B", "C"] {
            assert!(group.encloses(&r(id), 0.01), "{id} outside");
        }
        for id in ["X", "Y"] {
            assert!(!group.intersects(&r(id)), "{id} inside");
        }
        for e in &s.edges {
            let (start, _) = diagram_scene::flatten::start(&e.path).unwrap();
            let from = e.from.as_deref().unwrap();
            assert!(r(from).inflate(1.0).contains(start), "{from}: {start:?}");
        }
    }

    #[test]
    fn a_crossed_subgraph_keeps_the_charts_direction() {
        // Mermaid ignores the subgraph's direction when a link crosses it.
        let s = scene(
            "flowchart TD\n  X --> A\n  subgraph side\n    direction LR\n    A --> B\n  end\n",
        );
        let r = |id: &str| s.node(id).unwrap().rect;
        assert!(r("A").bottom() < r("B").y);
    }

    #[test]
    fn nested_own_directions() {
        let s = scene(
            "flowchart LR\n  subgraph outer\n    direction TB\n    A --> B\n    subgraph inner\n      direction LR\n      C --> D\n    end\n  end\n  E --> F\n",
        );
        let r = |id: &str| s.node(id).unwrap().rect;
        assert!(r("A").bottom() < r("B").y, "outer runs top-down");
        assert!(r("C").right() < r("D").x, "inner runs left-right");
        assert!(r("E").right() < r("F").x, "the chart runs left-right");
        let (outer, inner) = (s.containers[0].rect, s.containers[1].rect);
        assert!(outer.encloses(&inner, 0.01));
        for id in ["C", "D"] {
            assert!(inner.encloses(&r(id), 0.01));
        }
        let boxes: Vec<Rect> = s.nodes.iter().map(|n| n.rect).collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn v11_shapes_are_drawn() {
        let s = scene(
            "flowchart LR\n  A@{ shape: doc } --> B@{ shape: h-cyl } --> C@{ shape: fork } --> D@{ shape: sm-circ } --> E@{ shape: text, label: \"just text\" }\n",
        );
        assert_eq!(s.node("A").unwrap().shape, Shape::Document);
        assert_eq!(s.node("B").unwrap().shape, Shape::Queue);
        let bar = s.node("C").unwrap();
        assert_eq!(bar.shape, Shape::ForkBar);
        assert!(
            bar.rect.height > bar.rect.width,
            "a bar stands across the flow"
        );
        let dot = s.node("D").unwrap();
        assert!(dot.label.is_empty());
        let text = s.node("E").unwrap();
        assert_eq!(text.style.stroke, Paint::None);
        assert_eq!(text.label[0].plain(), "just text");
    }

    #[test]
    fn links_to_a_subgraph_stop_at_its_box() {
        let s = scene(
            "flowchart TB\n  A --> g\n  subgraph g [Group]\n    B --> C\n  end\n  g --> D\n  subgraph e [Empty]\n  end\n  D --> e\n",
        );
        let boxes: Vec<Rect> = s.containers.iter().map(|c| c.rect).collect();
        // No anchor or subgraph-id node is drawn.
        assert!(s.nodes.iter().all(|n| n.id != "g" && n.id != "e"));
        // Links: A→g, B→C inside g, g→D, D→e.
        for (edge, rect, at_end) in [
            (&s.edges[0], boxes[0], true),
            (&s.edges[2], boxes[0], false),
            (&s.edges[3], boxes[1], true),
        ] {
            let tip = if at_end {
                diagram_scene::flatten::end(&edge.path)
            } else {
                diagram_scene::flatten::start(&edge.path)
            }
            .unwrap()
            .0;
            let on_edge = (tip.y - rect.y).abs() < 0.5 || (tip.y - rect.bottom()).abs() < 0.5;
            assert!(on_edge, "{tip:?} is not on {rect:?}");
        }
        assert_eq!(s.edges[0].to.as_deref(), Some("g"));
        assert_eq!(s.edges[2].from.as_deref(), Some("g"));
    }
}
