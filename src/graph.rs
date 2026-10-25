//! The node-and-edge layout the graph diagrams share (state, class,
//! requirement), over `layered`.
//!
//! The caller draws and sizes each node; this places them and draws the
//! rest:
//!
//! - **Groups** are boxes around their members. An *inline* group keeps
//!   its members together in its parent's layout (a class namespace); a
//!   *block* group is laid out on its own, in its own direction, and then
//!   placed as one box, which links may end on (a composite state).
//! - **Links** are curves through the points the layout leaves between
//!   ranks, leaving and arriving along the flow, spread across a side in
//!   the order their other ends lie. Each may carry a label mid-way and a
//!   short label at either end (a class relation's cardinalities).
//! - **Pinned notes** sit beside their node, joined to it by a dashed line;
//!   the layout reserves the room for them.

use std::collections::HashMap;

use diagram_scene::route::Side;
use diagram_scene::shape::line_height;
use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge, EdgeLabel, Font, Marker, Node, Paint, Pen, Point,
    Rect, Role, Scene, Seg, Shape, Style, Text, TextLine, Weight,
};

use crate::layered::{self, Graph, GraphEdge, Spacing};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Space inside a group's box around its members, and for its title.
const GROUP_PAD: f32 = 14.0;
const TITLE: f32 = 22.0;
/// Between a node and the notes pinned beside it, and between notes.
const NOTE_GAP: f32 = 26.0;
const NOTE_STACK: f32 = 8.0;
/// How far a self-link's loop stands off its node.
const LOOP: f32 = 44.0;
/// An end label sits this far back along its link from the end.
const END_ALONG: f32 = 20.0;

/// Which way the flow runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

impl Direction {
    /// `TB`/`TD`, `BT`, `LR` or `RL`.
    pub(crate) fn from_word(word: &str) -> Option<Self> {
        match word {
            "TB" | "TD" => Some(Direction::TopDown),
            "BT" => Some(Direction::BottomUp),
            "LR" => Some(Direction::LeftRight),
            "RL" => Some(Direction::RightLeft),
            _ => None,
        }
    }

    pub(crate) fn horizontal(self) -> bool {
        matches!(self, Direction::LeftRight | Direction::RightLeft)
    }

    fn reversed(self) -> bool {
        matches!(self, Direction::BottomUp | Direction::RightLeft)
    }
}

/// A node to place: drawn (shape, text, style) but not yet positioned.
pub(crate) struct Item {
    pub node: Node,
    pub size: (f32, f32),
    /// The innermost group it belongs to.
    pub group: Option<usize>,
    pub notes: Vec<Pinned>,
}

/// A note pinned to one side of an item.
pub(crate) struct Pinned {
    pub node: Node,
    pub size: (f32, f32),
    pub side: Side,
}

pub(crate) struct Group {
    pub title: Vec<TextLine>,
    pub parent: Option<usize>,
    pub style: Style,
    /// Laid out on its own, in this direction, and placed as one box that
    /// links may end on; `None` keeps its members together inline.
    pub block: Option<Direction>,
}

/// One end of a link: an item, or a block group's box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum End {
    Item(usize),
    Group(usize),
}

pub(crate) struct Link {
    pub from: End,
    pub to: End,
    /// Mid-way; empty for none.
    pub label: Vec<TextLine>,
    /// Beside the link, near each end.
    pub from_label: Option<TextLine>,
    pub to_label: Option<TextLine>,
    pub pen: Pen,
    pub start: Marker,
    pub end: Marker,
}

impl Link {
    pub(crate) fn new(from: End, to: End) -> Self {
        Link {
            from,
            to,
            label: Vec::new(),
            from_label: None,
            to_label: None,
            pen: Pen::default(),
            start: Marker::None,
            end: Marker::Arrow,
        }
    }
}

pub(crate) struct Diagram {
    pub items: Vec<Item>,
    pub groups: Vec<Group>,
    pub links: Vec<Link>,
    pub direction: Direction,
}

/// `text` as lines: split at `\n`, each wrapped at `max_width`.
pub(crate) fn lines(
    measure: &dyn Measure,
    text: &str,
    font: Font,
    max_width: f32,
) -> Vec<TextLine> {
    text.split('\n')
        .flat_map(|line| {
            crate::text::wrap(measure, line, font.size(FONT_SIZE), font.bold(), max_width)
        })
        .map(|line| TextLine::new(line, font))
        .collect()
}

/// The height of `lines` stacked.
pub(crate) fn lines_height(lines: &[TextLine]) -> f32 {
    lines
        .iter()
        .map(|l| line_height(l.font().size(FONT_SIZE)))
        .sum()
}

pub(crate) fn lines_width(measure: &dyn Measure, lines: &[TextLine]) -> f32 {
    lines
        .iter()
        .map(|l| diagram_scene::line_width(measure, l, FONT_SIZE))
        .fold(0.0_f32, f32::max)
}

/// Lay `diagram` out as a scene.
pub(crate) fn layout(diagram: &Diagram, measure: &dyn Measure) -> Scene {
    let ctx = Ctx::new(diagram, measure);
    let mut block = ctx.place(None, diagram.direction);
    block.shift(PADDING, PADDING);
    ctx.draw(block)
}

/// Where an item's node and notes sit in its footprint, from its top-left.
struct Arrangement {
    size: (f32, f32),
    node: Point,
    notes: Vec<Point>,
}

fn arrange(item: &Item) -> Arrangement {
    let (nw, nh) = item.size;
    let on = |side: Side| item.notes.iter().filter(move |n| n.side == side);
    // Notes on a side stack along it.
    let stack = |side: Side, along_x: bool| {
        let (mut along, mut across, mut count) = (0.0_f32, 0.0_f32, 0);
        for n in on(side) {
            let (w, h) = n.size;
            if along_x {
                along += w;
                across = across.max(h);
            } else {
                along += h;
                across = across.max(w);
            }
            count += 1;
        }
        if count > 0 {
            along += (count - 1) as f32 * NOTE_STACK;
        }
        (along, across)
    };
    let (lh, lw) = stack(Side::Left, false);
    let (rh, rw) = stack(Side::Right, false);
    let (tw, th) = stack(Side::Top, true);
    let (bw, bh) = stack(Side::Bottom, true);
    let reach = |thick: f32| if thick > 0.0 { thick + NOTE_GAP } else { 0.0 };
    let mid_w = nw.max(tw).max(bw);
    let mid_h = nh.max(lh).max(rh);
    // As much room on both sides, so the node stays at the footprint's
    // centre, where the layout lines its links up.
    let across = reach(lw).max(reach(rw));
    let along = reach(th).max(reach(bh));
    let size = (2.0 * across + mid_w, 2.0 * along + mid_h);
    let node = Point::new(across + mid_w / 2.0, along + mid_h / 2.0);
    let mut cursor: HashMap<Side, f32> = HashMap::new();
    cursor.insert(Side::Left, node.y - lh / 2.0);
    cursor.insert(Side::Right, node.y - rh / 2.0);
    cursor.insert(Side::Top, node.x - tw / 2.0);
    cursor.insert(Side::Bottom, node.x - bw / 2.0);
    let notes = item
        .notes
        .iter()
        .map(|n| {
            let (w, h) = n.size;
            let at = cursor.get_mut(&n.side).expect("every side has a cursor");
            let p = match n.side {
                Side::Left => Point::new(node.x - mid_w / 2.0 - NOTE_GAP - w / 2.0, *at + h / 2.0),
                Side::Right => Point::new(node.x + mid_w / 2.0 + NOTE_GAP + w / 2.0, *at + h / 2.0),
                Side::Top => Point::new(*at + w / 2.0, node.y - mid_h / 2.0 - NOTE_GAP - h / 2.0),
                Side::Bottom => {
                    Point::new(*at + w / 2.0, node.y + mid_h / 2.0 + NOTE_GAP + h / 2.0)
                }
            };
            *at += match n.side {
                Side::Left | Side::Right => h,
                Side::Top | Side::Bottom => w,
            } + NOTE_STACK;
            p
        })
        .collect();
    Arrangement { size, node, notes }
}

struct Ctx<'a> {
    d: &'a Diagram,
    measure: &'a dyn Measure,
    arrangements: Vec<Arrangement>,
    label_sizes: Vec<Option<(f32, f32)>>,
    /// For each link: the scope it is laid out in and the items standing
    /// for its ends there, or `None` when it is drawn without the layout
    /// (a self-loop, or a link from a block to something inside it).
    resolved: Vec<Option<(Option<usize>, End, End)>>,
    /// The self-links at each end.
    loops: HashMap<End, Vec<usize>>,
}

/// A laid-out scope, in its own coordinates from (0, 0).
#[derive(Default)]
struct Block {
    /// (item, footprint, node rect, note rects)
    items: Vec<(usize, Rect, Rect, Vec<Rect>)>,
    boxes: Vec<(usize, Rect)>,
    /// (link, points, label position, horizontal)
    routes: Vec<(usize, Vec<Point>, Option<Point>, bool)>,
    width: f32,
    height: f32,
}

impl Block {
    fn shift(&mut self, dx: f32, dy: f32) {
        let mv = |r: &mut Rect| {
            r.x += dx;
            r.y += dy;
        };
        for (_, foot, rect, notes) in &mut self.items {
            mv(foot);
            mv(rect);
            notes.iter_mut().for_each(mv);
        }
        for (_, r) in &mut self.boxes {
            mv(r);
        }
        for (_, points, label, _) in &mut self.routes {
            for p in points.iter_mut().chain(label.iter_mut()) {
                *p = p.offset(dx, dy);
            }
        }
    }
}

impl<'a> Ctx<'a> {
    fn new(d: &'a Diagram, measure: &'a dyn Measure) -> Self {
        let arrangements = d.items.iter().map(arrange).collect();
        let label_sizes = d
            .links
            .iter()
            .map(|l| {
                (!l.label.is_empty()).then(|| {
                    (
                        lines_width(measure, &l.label) + 16.0,
                        lines_height(&l.label) + 6.0,
                    )
                })
            })
            .collect();
        let mut ctx = Ctx {
            d,
            measure,
            arrangements,
            label_sizes,
            resolved: Vec::new(),
            loops: HashMap::new(),
        };
        for (i, link) in d.links.iter().enumerate() {
            if link.from == link.to {
                ctx.loops.entry(link.from).or_default().push(i);
                ctx.resolved.push(None);
                continue;
            }
            let pa = ctx.path(ctx.scope_of(link.from));
            let pb = ctx.path(ctx.scope_of(link.to));
            let common = pa.iter().zip(&pb).take_while(|(a, b)| a == b).count();
            let scope = pa[common - 1];
            let rep = |end: End, path: &[Option<usize>]| match path.get(common) {
                Some(&Some(g)) => End::Group(g),
                _ => end,
            };
            let (a, b) = (rep(link.from, &pa), rep(link.to, &pb));
            ctx.resolved.push((a != b).then_some((scope, a, b)));
        }
        ctx
    }

    /// The nearest enclosing block group, from `group` out.
    fn owner(&self, mut group: Option<usize>) -> Option<usize> {
        while let Some(g) = group {
            if self.d.groups[g].block.is_some() {
                return Some(g);
            }
            group = self.d.groups[g].parent;
        }
        None
    }

    /// The scope an end is laid out in.
    fn scope_of(&self, end: End) -> Option<usize> {
        match end {
            End::Item(i) => self.owner(self.d.items[i].group),
            End::Group(g) => self.owner(self.d.groups[g].parent),
        }
    }

    /// The scopes from the root down to `scope`.
    fn path(&self, scope: Option<usize>) -> Vec<Option<usize>> {
        let mut out = vec![scope];
        let mut at = scope;
        while let Some(g) = at {
            at = self.owner(self.d.groups[g].parent);
            out.push(at);
        }
        out.reverse();
        out
    }

    /// The inline groups between `group` and its scope, outermost first.
    fn chain(&self, mut group: Option<usize>, scope: Option<usize>) -> Vec<usize> {
        let mut out = Vec::new();
        while let Some(g) = group {
            if Some(g) == scope {
                break;
            }
            out.push(g);
            group = self.d.groups[g].parent;
        }
        out.reverse();
        out
    }

    fn depth(&self, mut group: Option<usize>) -> usize {
        let mut depth = 0;
        while let Some(g) = group {
            depth += 1;
            group = self.d.groups[g].parent;
        }
        depth
    }

    fn title_height(&self, g: usize) -> f32 {
        match self.d.groups[g].title.len() {
            0 => 0.0,
            n => TITLE + (n - 1) as f32 * line_height(FONT_SIZE),
        }
    }

    /// The narrowest a box can be and still hold its title (drawn 10 in).
    fn min_box_width(&self, g: usize) -> f32 {
        match lines_width(self.measure, &self.d.groups[g].title) {
            0.0 => 0.0,
            w => w + 20.0,
        }
    }

    fn direction_of(&self, scope: Option<usize>) -> Direction {
        scope
            .and_then(|g| self.d.groups[g].block)
            .unwrap_or(self.d.direction)
    }

    /// How far the self-loops at `end` reach out of it, across the flow
    /// (`horizontal`: below it).
    fn loop_reach(&self, end: End, horizontal: bool) -> f32 {
        let Some(loops) = self.loops.get(&end) else {
            return 0.0;
        };
        loops
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let label =
                    self.label_sizes[i].map_or(0.0, |(w, h)| if horizontal { h } else { w });
                LOOP * (k + 1) as f32 * 0.75 + label + 6.0
            })
            .fold(0.0, f32::max)
    }

    /// Lay out what `scope` (a block group, or the whole diagram) holds
    /// directly, with the block groups inside it as boxes.
    fn place(&self, scope: Option<usize>, direction: Direction) -> Block {
        let d = self.d;
        let horizontal = direction.horizontal();
        enum Slot {
            Item(usize, (f32, f32)),
            Block(usize, (f32, f32), Block),
        }
        let mut slots = Vec::new();
        let mut sizes = Vec::new();
        let mut groups = Vec::new();
        let mut index: HashMap<End, usize> = HashMap::new();
        let padded = |(w, h): (f32, f32), end: End| {
            let extra = 2.0 * self.loop_reach(end, horizontal);
            if horizontal {
                (w, h + extra)
            } else {
                (w + extra, h)
            }
        };
        for (i, item) in d.items.iter().enumerate() {
            if self.owner(item.group) == scope {
                index.insert(End::Item(i), slots.len());
                let size = self.arrangements[i].size;
                sizes.push(padded(size, End::Item(i)));
                groups.push(self.chain(item.group, scope));
                slots.push(Slot::Item(i, size));
            }
        }
        for (g, group) in d.groups.iter().enumerate() {
            if let Some(dir) = group.block
                && self.owner(group.parent) == scope
            {
                let inner = self.place(Some(g), dir);
                let size = (
                    (inner.width + 2.0 * GROUP_PAD).max(self.min_box_width(g)),
                    inner.height + 2.0 * GROUP_PAD + self.title_height(g),
                );
                index.insert(End::Group(g), slots.len());
                sizes.push(padded(size, End::Group(g)));
                groups.push(self.chain(group.parent, scope));
                slots.push(Slot::Block(g, size, inner));
            }
        }
        let local: Vec<(usize, End, End)> = self
            .resolved
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match *r {
                Some((s, a, b)) if s == scope => Some((i, a, b)),
                _ => None,
            })
            .collect();
        let depth = groups.iter().map(Vec::len).max().unwrap_or(0) as f32;
        let graph = Graph {
            sizes: sizes.clone(),
            edges: local
                .iter()
                .map(|&(i, a, b)| GraphEdge {
                    from: index[&a],
                    to: index[&b],
                    label: self.label_sizes[i],
                })
                .collect(),
            groups,
            horizontal,
            reversed: direction.reversed(),
        };
        let spacing = Spacing {
            node_gap: 40.0,
            rank_gap: 28.0 + depth * (GROUP_PAD + TITLE) / 2.0,
            group_gap: GROUP_PAD * 2.0,
            margin: 0.0,
        };
        let placed = layered::layout(&graph, &spacing);

        let mut block = Block::default();
        let mut boxes: HashMap<usize, Rect> = HashMap::new();
        for (slot, &c) in slots.into_iter().zip(&placed.centers) {
            match slot {
                Slot::Item(i, (w, h)) => {
                    let foot = Rect::centered(c, w, h);
                    let a = &self.arrangements[i];
                    let at = |p: Point| Point::new(foot.x + p.x, foot.y + p.y);
                    let (nw, nh) = d.items[i].size;
                    let rect = Rect::centered(at(a.node), nw, nh);
                    let notes = d.items[i]
                        .notes
                        .iter()
                        .zip(&a.notes)
                        .map(|(n, &p)| Rect::centered(at(p), n.size.0, n.size.1))
                        .collect();
                    block.items.push((i, foot, rect, notes));
                }
                Slot::Block(g, (w, h), mut inner) => {
                    let rect = Rect::centered(c, w, h);
                    inner.shift(
                        rect.x + (rect.width - inner.width) / 2.0,
                        rect.y + GROUP_PAD + self.title_height(g),
                    );
                    block.items.append(&mut inner.items);
                    block.boxes.append(&mut inner.boxes);
                    block.routes.append(&mut inner.routes);
                    boxes.insert(g, rect);
                    block.boxes.push((g, rect));
                }
            }
        }
        for (k, &(i, _, _)) in local.iter().enumerate() {
            let route = &placed.routes[k];
            block
                .routes
                .push((i, route.points.clone(), route.label, horizontal));
        }

        // Inline groups in this scope, innermost first so their parents
        // can wrap them.
        let mut inline: Vec<usize> = (0..d.groups.len())
            .filter(|&g| d.groups[g].block.is_none() && self.owner(Some(g)) == scope)
            .collect();
        inline.sort_by_key(|&g| std::cmp::Reverse(self.depth(Some(g))));
        for g in inline {
            let mut extent: Option<Rect> = None;
            let mut grow = |r: Rect| extent = Some(extent.map_or(r, |e| e.union(&r)));
            for (i, foot, _, _) in &block.items {
                if d.items[*i].group == Some(g) {
                    grow(*foot);
                }
            }
            for (child, sub) in d.groups.iter().enumerate() {
                if sub.parent == Some(g)
                    && let Some(&r) = boxes.get(&child)
                {
                    grow(r);
                }
            }
            if let Some(r) = extent {
                let r = r.inflate(GROUP_PAD);
                let widen = (self.min_box_width(g) - r.width).max(0.0);
                let title = self.title_height(g);
                let rect = Rect::new(
                    r.x - widen / 2.0,
                    r.y - title,
                    r.width + widen,
                    r.height + title,
                );
                boxes.insert(g, rect);
                block.boxes.push((g, rect));
            }
        }

        // Fit: everything from (0, 0).
        let mut extent = Rect::new(0.0, 0.0, placed.width, placed.height);
        for (_, foot, _, _) in &block.items {
            extent = extent.union(foot);
        }
        for (_, r) in &block.boxes {
            extent = extent.union(r);
        }
        for (i, points, label, _) in &block.routes {
            for p in points {
                extent = extent.union(&Rect::new(p.x, p.y, 0.0, 0.0));
            }
            if let (Some(p), Some((w, h))) = (label, self.label_sizes[*i]) {
                extent = extent.union(&Rect::centered(*p, w, h));
            }
        }
        block.shift(-extent.x, -extent.y);
        block.width = extent.width;
        block.height = extent.height;
        block
    }

    /// Draw a placed diagram.
    fn draw(&self, block: Block) -> Scene {
        let d = self.d;
        let mut scene = Scene::new(0.0, 0.0);
        scene.font_size = FONT_SIZE;

        let mut node_rects = vec![Rect::default(); d.items.len()];
        let mut note_rects = vec![Vec::new(); d.items.len()];
        for (i, _, rect, notes) in &block.items {
            node_rects[*i] = *rect;
            note_rects[*i] = notes.clone();
        }
        let mut group_rects: HashMap<usize, Rect> = HashMap::new();
        for &(g, r) in &block.boxes {
            group_rects.insert(g, r);
        }
        let rect_of = |end: End| match end {
            End::Item(i) => node_rects[i],
            End::Group(g) => group_rects.get(&g).copied().unwrap_or_default(),
        };
        let shape_of = |end: End| match end {
            End::Item(i) => d.items[i].node.shape,
            End::Group(_) => Shape::Rect,
        };
        let id_of = |end: End| match end {
            End::Item(i) => d.items[i].node.id.clone(),
            End::Group(g) => format!("group-{g}"),
        };

        // Containers, outermost first.
        let mut drawn_boxes = block.boxes.clone();
        drawn_boxes.sort_by_key(|&(g, _)| self.depth(Some(g)));
        for (g, rect) in drawn_boxes {
            let group = &d.groups[g];
            scene.containers.push(Container {
                rect,
                kind: ContainerKind::Subgraph,
                label: group.title.clone(),
                style: group.style,
            });
        }

        // Links routed through the layout.
        let routes: HashMap<usize, (&Vec<Point>, Option<Point>, bool)> = block
            .routes
            .iter()
            .map(|(i, p, l, h)| (*i, (p, *l, *h)))
            .collect();
        let ports = self.ports(&routes, &rect_of, &shape_of);
        let mut loop_count: HashMap<End, usize> = HashMap::new();
        for (i, link) in d.links.iter().enumerate() {
            let (from, to) = (rect_of(link.from), rect_of(link.to));
            let (path, label_pos) = if link.from == link.to {
                let k = loop_count.entry(link.from).or_default();
                let horizontal = self.direction_of(self.scope_of(link.from)).horizontal();
                let out = self_loop(
                    from,
                    horizontal,
                    *k,
                    self.label_sizes[i].unwrap_or((0.0, 0.0)),
                );
                *k += 1;
                out
            } else if let (Some(&(points, label, horizontal)), Some((start, end))) =
                (routes.get(&i), ports.get(&i))
            {
                let path = curve(*start, *end, points, horizontal);
                let pos = label.or_else(|| diagram_scene::route::label_position(&path, 0.0));
                (path, pos)
            } else {
                // A link from a block to something inside it.
                let horizontal = self.direction_of(self.scope_of(link.to)).horizontal();
                let path = diagram_scene::route::spline(from, to, horizontal);
                let pos = diagram_scene::route::label_position(&path, 0.0);
                (path, pos)
            };
            let mut edge = Edge::new(path).between(&id_of(link.from), &id_of(link.to));
            edge.pen = link.pen;
            edge.start = link.start;
            edge.end = link.end;
            if !link.label.is_empty()
                && let Some(pos) = label_pos
            {
                edge.labels.push(EdgeLabel::new(
                    pos,
                    link.label.clone(),
                    true,
                    self.measure,
                    FONT_SIZE,
                ));
            }
            let ends = [
                (&link.from_label, diagram_scene::flatten::start(&edge.path)),
                (&link.to_label, diagram_scene::flatten::end(&edge.path)),
            ];
            for (label, at) in ends {
                if let (Some(line), Some((tip, dir))) = (label, at) {
                    scene.texts.push(self.end_label(line, tip, dir));
                }
            }
            scene.edges.push(edge);
        }

        // Nodes, then their notes joined to them.
        let mut nodes = Vec::new();
        let mut notes = Vec::new();
        for (i, item) in d.items.iter().enumerate() {
            let mut node = item.node.clone();
            node.rect = node_rects[i];
            for (pinned, &rect) in item.notes.iter().zip(&note_rects[i]) {
                let mut note = pinned.node.clone();
                note.rect = rect;
                let (a, b) = match pinned.side {
                    Side::Left => (Side::Left, Side::Right),
                    Side::Right => (Side::Right, Side::Left),
                    Side::Top => (Side::Top, Side::Bottom),
                    Side::Bottom => (Side::Bottom, Side::Top),
                };
                let from = diagram_scene::route::port(node.rect, a);
                let to = diagram_scene::route::port(rect, b);
                let mut edge =
                    Edge::new(vec![Seg::Move(from), Seg::Line(to)]).between(&node.id, &note.id);
                edge.end = Marker::None;
                edge.pen = Pen {
                    dash: Dash::Dashed,
                    weight: Weight::Thin,
                    paint: Paint::Role(Role::Muted),
                };
                scene.edges.push(edge);
                notes.push(note);
            }
            nodes.push(node);
        }
        nodes.extend(notes);
        scene.nodes = nodes;

        // Everything inside the bounds.
        let mut extent = Rect::new(
            0.0,
            0.0,
            block.width + 2.0 * PADDING,
            block.height + 2.0 * PADDING,
        );
        let mut grow = |r: Rect| extent = extent.union(&r.inflate(PADDING));
        for n in &scene.nodes {
            grow(n.rect);
        }
        for c in &scene.containers {
            grow(c.rect);
        }
        for e in &scene.edges {
            for poly in diagram_scene::flatten::flatten(&e.path, 1.0) {
                for p in poly.points {
                    grow(Rect::new(p.x, p.y, 0.0, 0.0));
                }
            }
            for l in &e.labels {
                grow(Rect::centered(
                    l.pos,
                    l.width + 16.0,
                    lines_height(&l.lines) + 6.0,
                ));
            }
        }
        for t in &scene.texts {
            grow(text_rect(self.measure, t));
        }
        let (dx, dy) = (-extent.x.min(0.0), -extent.y.min(0.0));
        if dx > 0.0 || dy > 0.0 {
            shift_scene(&mut scene, dx, dy);
        }
        scene.width = extent.right() + dx;
        scene.height = extent.bottom() + dy;
        scene
    }

    /// A short label beside a link near one of its ends: `tip` is the end,
    /// `dir` the unit direction pointing into the node there.
    fn end_label(&self, line: &TextLine, tip: Point, dir: Point) -> Text {
        let size = line.font().size(FONT_SIZE);
        let width = diagram_scene::line_width(self.measure, line, FONT_SIZE);
        let back = tip.offset(-dir.x * END_ALONG, -dir.y * END_ALONG);
        // To the right of the direction of travel into the node.
        let (nx, ny) = (-dir.y, dir.x);
        let reach = nx.abs() * (width / 2.0 + 7.0) + ny.abs() * (size * 0.6 + 5.0);
        let centre = back.offset(nx * reach, ny * reach);
        Text {
            pos: Point::new(centre.x, centre.y + size * 0.35),
            anchor: Anchor::Middle,
            line: line.clone(),
            paint: Paint::Role(Role::Text),
            backed: true,
        }
    }

    /// Where each routed link leaves and arrives: on the side of each end
    /// facing along the flow, the ones sharing a side spread across it in
    /// the order their other ends lie.
    fn ports(
        &self,
        routes: &HashMap<usize, (&Vec<Point>, Option<Point>, bool)>,
        rect_of: &dyn Fn(End) -> Rect,
        shape_of: &dyn Fn(End) -> Shape,
    ) -> HashMap<usize, (Point, Point)> {
        type Ends = Vec<(usize, bool, f32)>;
        // Kept in first-seen order, so the result never depends on hashing.
        let mut sides: Vec<(End, Side, Ends)> = Vec::new();
        let mut order: Vec<usize> = routes.keys().copied().collect();
        order.sort_unstable();
        for i in order {
            let (points, _, horizontal) = routes[&i];
            let link = &self.d.links[i];
            let (from, to) = (rect_of(link.from), rect_of(link.to));
            let first = points.first().copied().unwrap_or(to.center());
            let last = points.last().copied().unwrap_or(from.center());
            for (end, rect, toward, start) in
                [(link.from, from, first, true), (link.to, to, last, false)]
            {
                let side = side_toward(rect, toward, horizontal);
                let across = if horizontal { toward.y } else { toward.x };
                let at = match sides.iter().position(|(e, s, _)| *e == end && *s == side) {
                    Some(at) => at,
                    None => {
                        sides.push((end, side, Vec::new()));
                        sides.len() - 1
                    }
                };
                sides[at].2.push((i, start, across));
            }
        }
        let mut out: HashMap<usize, (Option<Point>, Option<Point>)> = HashMap::new();
        for (end, side, mut ends) in sides {
            ends.sort_by(|a, b| a.2.total_cmp(&b.2));
            let rect = rect_of(end);
            let length = match side {
                Side::Top | Side::Bottom => rect.width,
                Side::Left | Side::Right => rect.height,
            };
            let count = ends.len();
            let span = (length * 0.7).min(count.saturating_sub(1) as f32 * 28.0);
            for (k, (i, start, _)) in ends.into_iter().enumerate() {
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
                let p = match shape_of(end) {
                    s @ (Shape::Diamond | Shape::Circle | Shape::Start | Shape::End) => {
                        diagram_scene::route::clip(s, rect, p)
                    }
                    _ => p,
                };
                let entry = out.entry(i).or_default();
                if start {
                    entry.0 = Some(p);
                } else {
                    entry.1 = Some(p);
                }
            }
        }
        out.into_iter()
            .filter_map(|(i, (a, b))| Some((i, (a?, b?))))
            .collect()
    }
}

fn side_toward(rect: Rect, toward: Point, horizontal: bool) -> Side {
    let c = rect.center();
    match (
        horizontal,
        horizontal && toward.x >= c.x || !horizontal && toward.y >= c.y,
    ) {
        (true, true) => Side::Right,
        (true, false) => Side::Left,
        (false, true) => Side::Bottom,
        (false, false) => Side::Top,
    }
}

/// Roughly where a free text is drawn.
fn text_rect(measure: &dyn Measure, t: &Text) -> Rect {
    let size = t.line.font().size(FONT_SIZE);
    let width = diagram_scene::line_width(measure, &t.line, FONT_SIZE) + 6.0;
    let x = match t.anchor {
        Anchor::Start => t.pos.x,
        Anchor::Middle => t.pos.x - width / 2.0,
        Anchor::End => t.pos.x - width,
    };
    Rect::new(x, t.pos.y - size, width, size * 1.3)
}

/// A smooth curve from `start` through `points` to `end`, each step
/// leaving and arriving along the flow, so markers sit square to the node.
pub(crate) fn curve(start: Point, end: Point, points: &[Point], horizontal: bool) -> Vec<Seg> {
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

/// The `k`th loop out of a node's side (right when the flow is vertical,
/// bottom when horizontal) and back in, and where its label of `label`
/// size goes: just beyond the loop.
pub(crate) fn self_loop(
    r: Rect,
    horizontal: bool,
    k: usize,
    label: (f32, f32),
) -> (Vec<Seg>, Option<Point>) {
    let out = LOOP * (k + 1) as f32;
    let c = r.center();
    if horizontal {
        let half = (r.width * 0.3).clamp(8.0, 36.0);
        let (a, b) = (
            Point::new(c.x - half, r.bottom()),
            Point::new(c.x + half, r.bottom()),
        );
        let path = vec![
            Seg::Move(a),
            Seg::Cubic(a.offset(0.0, out), b.offset(0.0, out), b),
        ];
        // A cubic's apex is three quarters of its handles' reach.
        let apex = r.bottom() + out * 0.75;
        (path, Some(Point::new(c.x, apex + label.1 / 2.0 + 2.0)))
    } else {
        let half = (r.height * 0.3).clamp(8.0, 36.0);
        let (a, b) = (
            Point::new(r.right(), c.y - half),
            Point::new(r.right(), c.y + half),
        );
        let path = vec![
            Seg::Move(a),
            Seg::Cubic(a.offset(out, 0.0), b.offset(out, 0.0), b),
        ];
        let apex = r.right() + out * 0.75;
        (path, Some(Point::new(apex + label.0 / 2.0 + 2.0, c.y)))
    }
}

/// Move everything in a scene by (dx, dy).
pub(crate) fn shift_scene(scene: &mut Scene, dx: f32, dy: f32) {
    let mv = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for n in &mut scene.nodes {
        n.rect.x += dx;
        n.rect.y += dy;
    }
    for c in &mut scene.containers {
        c.rect.x += dx;
        c.rect.y += dy;
    }
    for t in &mut scene.texts {
        mv(&mut t.pos);
    }
    for e in &mut scene.edges {
        for seg in &mut e.path {
            match seg {
                Seg::Move(p) | Seg::Line(p) => mv(p),
                Seg::Cubic(a, b, p) => {
                    mv(a);
                    mv(b);
                    mv(p);
                }
                Seg::Close => {}
            }
        }
        for l in &mut e.labels {
            mv(&mut l.pos);
        }
    }
}

/// A style's colours from the source, over the shape's or container's
/// defaults.
pub(crate) fn paint(style: &mut Style, colors: &crate::style::Colors) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    fn item(id: &str, group: Option<usize>) -> Item {
        Item {
            node: Node::new(id, Shape::Rounded, Rect::default()).with_label(id),
            size: (60.0, 36.0),
            group,
            notes: Vec::new(),
        }
    }

    fn group(parent: Option<usize>, block: Option<Direction>) -> Group {
        Group {
            title: vec![TextLine::new("G", Font::Bold)],
            parent,
            style: Style::for_container(&ContainerKind::Subgraph),
            block,
        }
    }

    fn check(scene: &Scene) {
        let bounds = scene.bounds();
        for (i, a) in scene.nodes.iter().enumerate() {
            assert!(bounds.encloses(&a.rect, 0.01), "{} outside", a.id);
            for b in &scene.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
        for e in &scene.edges {
            for poly in diagram_scene::flatten::flatten(&e.path, 0.5) {
                for p in poly.points {
                    assert!(bounds.contains(p), "{p:?} outside {bounds:?}");
                }
            }
        }
        for t in &scene.texts {
            assert!(bounds.contains(t.pos));
        }
    }

    #[test]
    fn blocks_hold_their_members_and_links_cross_them() {
        for direction in [Direction::TopDown, Direction::LeftRight] {
            let mut d = Diagram {
                items: vec![item("a", None), item("b", Some(0)), item("c", Some(0))],
                groups: vec![group(None, Some(Direction::LeftRight))],
                links: vec![
                    Link::new(End::Item(0), End::Group(0)),
                    Link::new(End::Item(1), End::Item(2)),
                    Link::new(End::Item(2), End::Item(0)),
                    Link::new(End::Group(0), End::Group(0)),
                    Link::new(End::Group(0), End::Item(1)),
                ],
                direction,
            };
            d.links[0].from_label = Some(TextLine::new("1", Font::Small));
            d.links[0].to_label = Some(TextLine::new("many", Font::Small));
            d.items[0].notes.push(Pinned {
                node: Node::new("n", Shape::Note, Rect::default()).with_label("note"),
                size: (50.0, 30.0),
                side: Side::Right,
            });
            let scene = layout(&d, &Estimate);
            check(&scene);
            let boxed = scene.containers[0].rect;
            for id in ["b", "c"] {
                assert!(boxed.encloses(&scene.node(id).unwrap().rect, 0.01));
            }
            assert!(!boxed.intersects(&scene.node("a").unwrap().rect));
            assert_eq!(scene.edges.len(), 6);
            assert_eq!(scene.texts.len(), 2);
        }
    }

    #[test]
    fn inline_groups_wrap_their_members() {
        let d = Diagram {
            items: vec![item("a", Some(0)), item("b", Some(1)), item("c", None)],
            groups: vec![group(None, None), group(Some(0), None)],
            links: vec![
                Link::new(End::Item(0), End::Item(1)),
                Link::new(End::Item(1), End::Item(2)),
            ],
            direction: Direction::TopDown,
        };
        let scene = layout(&d, &Estimate);
        check(&scene);
        let outer = scene.containers[0].rect;
        let inner = scene.containers[1].rect;
        assert!(outer.encloses(&inner, 0.01));
        assert!(inner.encloses(&scene.node("b").unwrap().rect, 0.01));
        assert!(!outer.intersects(&scene.node("c").unwrap().rect));
    }
}
