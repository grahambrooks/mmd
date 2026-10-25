//! Architecture layout on a nested grid.
//!
//! Mermaid runs a force-directed layout (fcose) constrained by the sides
//! each edge names. Here the sides are taken literally instead: `a:R -- L:b`
//! puts `b` one cell right of `a`, `a:B -- T:b` one cell below, and a mixed
//! pair (`a:R -- T:b`) one cell diagonally. Each group is laid out as its
//! own grid, then placed as one block in its parent's, so a group's members
//! always stay inside it.

use std::collections::{HashMap, VecDeque};

use diagram_scene::route::rounded_polyline;
use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge as SceneEdge, EdgeLabel, Font, Marker, Node,
    Paint, Point, Rect, Role, Scene, Shape, Style, Text, TextLine,
};

use super::parse::{Architecture, ItemKind, Side};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const ICON: f32 = 48.0;
const JUNCTION: f32 = 10.0;
const LABEL_GAP: f32 = 6.0;
const COL_GAP: f32 = 80.0;
const ROW_GAP: f32 = 64.0;
const GROUP_PAD: f32 = 18.0;
/// Mermaid's icon background.
const ICON_FILL: Paint = Paint::Rgb(0x08, 0x7e, 0xbf);
const WHITE: Paint = Paint::Rgb(0xff, 0xff, 0xff);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Child {
    Item(usize),
    Group(usize),
}

/// A laid-out container: its content size and each child's top-left.
struct Arrangement {
    size: (f32, f32),
    offsets: Vec<(Child, Point)>,
}

struct Ctx<'a> {
    model: &'a Architecture,
    item_sizes: Vec<(f32, f32)>,
    /// Each service's icon square side (wider for a text icon).
    icon_sizes: Vec<(f32, f32)>,
    group_label: f32,
    /// Each group's title width, which its box must fit.
    group_title: Vec<f32>,
    arrangements: HashMap<Option<usize>, Arrangement>,
}

pub fn layout(model: &Architecture, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let line_height = measure.line_height(FONT_SIZE);

    let icon_label = |kind: &ItemKind| -> Option<String> {
        match kind {
            ItemKind::Service {
                icon_text: Some(text),
                ..
            } => Some(text.clone()),
            ItemKind::Service {
                icon: Some(icon), ..
            } if glyph(icon).is_none() && icon != "blank" => {
                Some(icon.rsplit(':').next().unwrap_or(icon).to_string())
            }
            _ => None,
        }
    };
    let small = Font::Small.size(FONT_SIZE);
    let icon_sizes: Vec<(f32, f32)> = model
        .items
        .iter()
        .map(|item| match &item.kind {
            ItemKind::Junction => (JUNCTION, JUNCTION),
            kind => {
                let text_w = icon_label(kind)
                    .map(|t| measure.width(&t, small, false) + 12.0)
                    .unwrap_or(0.0);
                (ICON.max(text_w), ICON)
            }
        })
        .collect();
    let item_sizes: Vec<(f32, f32)> = model
        .items
        .iter()
        .zip(&icon_sizes)
        .map(|(item, &(w, h))| match &item.kind {
            ItemKind::Service { title: Some(t), .. } => (
                w.max(measure.width(t, FONT_SIZE, false)),
                h + LABEL_GAP + line_height,
            ),
            _ => (w, h),
        })
        .collect();
    let mut ctx = Ctx {
        model,
        item_sizes,
        icon_sizes,
        group_label: line_height + 6.0,
        group_title: model
            .groups
            .iter()
            .map(|g| {
                g.title
                    .as_ref()
                    .map(|t| measure.width(t, FONT_SIZE, true) + 20.0)
                    .unwrap_or(0.0)
            })
            .collect(),
        arrangements: HashMap::new(),
    };
    arrange(&mut ctx, None);

    let title_size = Font::Title.size(FONT_SIZE);
    let top = PADDING
        + if model.title.is_some() {
            title_size + PADDING
        } else {
            0.0
        };
    let mut item_rects = vec![Rect::default(); model.items.len()];
    let mut group_rects = vec![Rect::default(); model.groups.len()];
    place(
        &ctx,
        None,
        Point::new(PADDING, top),
        &mut item_rects,
        &mut group_rects,
    );
    let (content_w, content_h) = ctx.arrangements[&None].size;

    // Groups, outermost first: a parent is always declared before what it
    // holds.
    for (g, group) in model.groups.iter().enumerate() {
        let label = group
            .title
            .as_ref()
            .map(|t| vec![TextLine::new(t.clone(), Font::Bold)])
            .unwrap_or_default();
        scene.containers.push(Container {
            rect: group_rects[g],
            kind: ContainerKind::Group,
            label,
            style: Style {
                dash: Dash::Dashed,
                ..Style::for_container(&ContainerKind::Group)
            },
        });
    }

    // Services and junctions.
    let mut extent = Rect::new(
        0.0,
        0.0,
        content_w + PADDING * 2.0,
        top + content_h + PADDING,
    );
    for (i, item) in model.items.iter().enumerate() {
        let block = item_rects[i];
        let (iw, ih) = ctx.icon_sizes[i];
        let icon = Rect::new(block.center().x - iw / 2.0, block.y, iw, ih);
        match &item.kind {
            ItemKind::Junction => {
                let mut node = Node::new(item.id.clone(), Shape::Circle, icon);
                node.style.fill = Paint::Role(Role::Line);
                node.style.stroke = Paint::Role(Role::Line);
                scene.nodes.push(node);
            }
            ItemKind::Service {
                icon: name, title, ..
            } => {
                let mut node = Node::new(item.id.clone(), Shape::Rect, icon);
                node.style.fill = ICON_FILL;
                node.style.stroke = ICON_FILL;
                node.style.text = WHITE;
                if name.is_none() && icon_label(&item.kind).is_none() {
                    node.style.fill = Paint::Role(Role::Surface);
                    node.style.stroke = Paint::Role(Role::Line);
                }
                if let Some(text) = icon_label(&item.kind) {
                    node.label = vec![TextLine::new(text, Font::Small)];
                }
                scene.nodes.push(node);
                if let Some(shape) = name.as_deref().and_then(glyph) {
                    let (w, h) = if shape == Shape::Cloud {
                        (ICON * 0.7, ICON * 0.45)
                    } else if shape == Shape::Rect {
                        (ICON * 0.55, ICON * 0.6)
                    } else {
                        (ICON * 0.55, ICON * 0.62)
                    };
                    let mut mark = Node::new(
                        format!("{}:icon", item.id),
                        shape,
                        Rect::centered(icon.center(), w, h),
                    );
                    mark.style.fill = Paint::None;
                    mark.style.stroke = WHITE;
                    scene.nodes.push(mark);
                }
                if let Some(title) = title {
                    let pos = Point::new(
                        block.center().x,
                        icon.bottom() + LABEL_GAP + measure.ascent(FONT_SIZE),
                    );
                    scene.texts.push(Text {
                        pos,
                        anchor: Anchor::Middle,
                        line: TextLine::new(title.clone(), Font::Body),
                        paint: Paint::Role(Role::Text),
                        backed: true,
                        rotate: 0.0,
                    });
                }
            }
        }
    }

    // Edges, from the side each end names.
    for edge in &model.edges {
        let end = |item: usize, side: Side, to_group: bool, other: usize| -> Point {
            let block = item_rects[item];
            let (iw, ih) = ctx.icon_sizes[item];
            let icon = Rect::new(block.center().x - iw / 2.0, block.y, iw, ih);
            let rect = if to_group {
                boundary(model, item, other)
                    .map(|g| group_rects[g])
                    .unwrap_or(block)
            } else {
                block
            };
            let c = icon.center();
            if matches!(model.items[item].kind, ItemKind::Junction) && !to_group {
                return c;
            }
            match side {
                Side::Left => Point::new(rect.x, c.y),
                Side::Right => Point::new(rect.right(), c.y),
                Side::Top => Point::new(c.x, rect.y),
                Side::Bottom => Point::new(c.x, rect.bottom()),
            }
        };
        let a = end(edge.from, edge.from_side, edge.from_group, edge.to);
        let b = end(edge.to, edge.to_side, edge.to_group, edge.from);
        let points = route(a, edge.from_side, b, edge.to_side);
        let path = rounded_polyline(&points, 8.0);
        let mut scene_edge = SceneEdge::new(path.clone())
            .between(&model.items[edge.from].id, &model.items[edge.to].id);
        scene_edge.start = if edge.from_arrow {
            Marker::Arrow
        } else {
            Marker::None
        };
        scene_edge.end = if edge.to_arrow {
            Marker::Arrow
        } else {
            Marker::None
        };
        for p in &points {
            extent = extent.union(&Rect::new(p.x, p.y, 0.0, 0.0));
        }
        if let Some(title) = &edge.title {
            let pos = diagram_scene::route::label_position(&path, 0.0).unwrap_or(a.lerp(b, 0.5));
            let label = EdgeLabel::new(
                pos,
                vec![TextLine::new(title.clone(), Font::Small)],
                true,
                measure,
                FONT_SIZE,
            );
            extent = extent.union(&Rect::centered(pos, label.width + 12.0, small * 1.8));
            scene_edge.labels.push(label);
        }
        scene.edges.push(scene_edge);
    }

    let title_width = model
        .title
        .as_ref()
        .map(|t| measure.width(t, title_size, true) + PADDING * 2.0)
        .unwrap_or(0.0);
    let width = (extent.right() + PADDING).max(title_width);
    if let Some(title) = &model.title {
        scene.title = Some(Text {
            pos: Point::new(width / 2.0, PADDING + title_size),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
    }
    scene.width = width;
    scene.height = extent.bottom().max(top + content_h) + PADDING;
    scene
}

/// The white mark drawn on a built-in icon.
fn glyph(icon: &str) -> Option<Shape> {
    Some(match icon {
        "database" => Shape::Cylinder,
        "server" | "disk" => Shape::Rect,
        "internet" => Shape::Circle,
        "cloud" => Shape::Cloud,
        _ => return None,
    })
}

/// The outermost group holding `item` that does not hold `other`: where a
/// `{group}` end meets the boundary.
fn boundary(model: &Architecture, item: usize, other: usize) -> Option<usize> {
    let mine = model.group_chain(model.items[item].parent);
    let theirs = model.group_chain(model.items[other].parent);
    mine.into_iter().rev().find(|g| !theirs.contains(g))
}

/// Right-angled points from `a` leaving `sa` to `b` entering through `sb`.
fn route(a: Point, sa: Side, b: Point, sb: Side) -> Vec<Point> {
    const STUB: f32 = 20.0;
    let out = |p: Point, side: Side| match side {
        Side::Left => p.offset(-STUB, 0.0),
        Side::Right => p.offset(STUB, 0.0),
        Side::Top => p.offset(0.0, -STUB),
        Side::Bottom => p.offset(0.0, STUB),
    };
    match (sa.horizontal(), sb.horizontal()) {
        (true, true) => {
            let facing = matches!((sa, sb), (Side::Right, Side::Left)) && a.x < b.x
                || matches!((sa, sb), (Side::Left, Side::Right)) && a.x > b.x;
            if facing {
                let mx = (a.x + b.x) / 2.0;
                vec![a, Point::new(mx, a.y), Point::new(mx, b.y), b]
            } else {
                let (oa, ob) = (out(a, sa), out(b, sb));
                let x = if sa == Side::Right {
                    oa.x.max(ob.x)
                } else {
                    oa.x.min(ob.x)
                };
                vec![a, Point::new(x, a.y), Point::new(x, b.y), b]
            }
        }
        (false, false) => {
            let facing = matches!((sa, sb), (Side::Bottom, Side::Top)) && a.y < b.y
                || matches!((sa, sb), (Side::Top, Side::Bottom)) && a.y > b.y;
            if facing {
                let my = (a.y + b.y) / 2.0;
                vec![a, Point::new(a.x, my), Point::new(b.x, my), b]
            } else {
                let (oa, ob) = (out(a, sa), out(b, sb));
                let y = if sa == Side::Bottom {
                    oa.y.max(ob.y)
                } else {
                    oa.y.min(ob.y)
                };
                vec![a, Point::new(a.x, y), Point::new(b.x, y), b]
            }
        }
        (true, false) => vec![a, Point::new(b.x, a.y), b],
        (false, true) => vec![a, Point::new(a.x, b.y), b],
    }
}

/// Where `b` sits relative to `a`, in cells, for an edge leaving `a`'s
/// `sa` side and entering `b`'s `sb` side.
fn offset(sa: Side, sb: Side) -> (i32, i32) {
    let dx = match (sa, sb) {
        (Side::Right, _) => 1,
        (Side::Left, _) => -1,
        (_, Side::Left) => 1,
        (_, Side::Right) => -1,
        _ => 0,
    };
    let dy = match (sa, sb) {
        (Side::Bottom, _) => 1,
        (Side::Top, _) => -1,
        (_, Side::Top) => 1,
        (_, Side::Bottom) => -1,
        _ => 0,
    };
    (dx, dy)
}

/// What `item` counts as inside `container`: itself, the child group
/// holding it, or nothing if it lies outside.
fn lift(model: &Architecture, item: usize, container: Option<usize>) -> Option<Child> {
    let chain = model.group_chain(model.items[item].parent);
    match container {
        None => Some(match chain.last() {
            None => Child::Item(item),
            Some(&outer) => Child::Group(outer),
        }),
        Some(c) => match chain.iter().position(|&g| g == c)? {
            0 => Some(Child::Item(item)),
            at => Some(Child::Group(chain[at - 1])),
        },
    }
}

fn arrange(ctx: &mut Ctx, container: Option<usize>) {
    let model = ctx.model;
    let mut children: Vec<Child> = Vec::new();
    for (i, item) in model.items.iter().enumerate() {
        if item.parent == container {
            children.push(Child::Item(i));
        }
    }
    for (g, group) in model.groups.iter().enumerate() {
        if group.parent == container {
            arrange(ctx, Some(g));
            children.push(Child::Group(g));
        }
    }
    let size = |ctx: &Ctx, child: Child| match child {
        Child::Item(i) => ctx.item_sizes[i],
        Child::Group(g) => {
            let (w, h) = ctx.arrangements[&Some(g)].size;
            let title_w = ctx.group_title[g];
            (
                (w + GROUP_PAD * 2.0).max(title_w),
                h + GROUP_PAD * 2.0 + ctx.group_label,
            )
        }
    };
    let sizes: Vec<(f32, f32)> = children.iter().map(|&c| size(ctx, c)).collect();
    let index = |child: Child| children.iter().position(|&c| c == child);

    // Neighbours in cells, from the edges between two different children.
    let mut adjacent: Vec<Vec<(usize, i32, i32)>> = vec![Vec::new(); children.len()];
    for edge in &model.edges {
        let (Some(a), Some(b)) = (
            lift(model, edge.from, container),
            lift(model, edge.to, container),
        ) else {
            continue;
        };
        if a == b {
            continue;
        }
        let (Some(a), Some(b)) = (index(a), index(b)) else {
            continue;
        };
        let (dx, dy) = offset(edge.from_side, edge.to_side);
        adjacent[a].push((b, dx, dy));
        adjacent[b].push((a, -dx, -dy));
    }

    let mut cells: HashMap<(i32, i32), usize> = HashMap::new();
    let mut at: Vec<Option<(i32, i32)>> = vec![None; children.len()];
    let put = |cells: &mut HashMap<(i32, i32), usize>,
               at: &mut Vec<Option<(i32, i32)>>,
               child: usize,
               mut cell: (i32, i32),
               step: (i32, i32)| {
        let step = if step == (0, 0) { (1, 0) } else { step };
        while cells.contains_key(&cell) {
            cell = (cell.0 + step.0, cell.1 + step.1);
        }
        cells.insert(cell, child);
        at[child] = Some(cell);
    };
    for start in 0..children.len() {
        if at[start].is_some() {
            continue;
        }
        // A disconnected part starts to the right of what's placed.
        let column = cells.keys().map(|c| c.0 + 1).max().unwrap_or(0);
        put(&mut cells, &mut at, start, (column, 0), (0, 1));
        let mut queue = VecDeque::from([start]);
        while let Some(current) = queue.pop_front() {
            let Some(here) = at[current] else { continue };
            for &(next, dx, dy) in &adjacent[current] {
                if at[next].is_none() {
                    put(
                        &mut cells,
                        &mut at,
                        next,
                        (here.0 + dx, here.1 + dy),
                        (dx, dy),
                    );
                    queue.push_back(next);
                }
            }
        }
    }

    // Cells to coordinates: each column as wide as its widest child.
    let mut xs: Vec<i32> = at.iter().flatten().map(|c| c.0).collect();
    let mut ys: Vec<i32> = at.iter().flatten().map(|c| c.1).collect();
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    let mut col_w = vec![0.0_f32; xs.len()];
    let mut row_h = vec![0.0_f32; ys.len()];
    let col = |x: i32| xs.binary_search(&x).unwrap_or(0);
    let row = |y: i32| ys.binary_search(&y).unwrap_or(0);
    for (i, cell) in at.iter().enumerate() {
        let (cx, cy) = cell.unwrap_or((0, 0));
        col_w[col(cx)] = col_w[col(cx)].max(sizes[i].0);
        row_h[row(cy)] = row_h[row(cy)].max(sizes[i].1);
    }
    let mut col_x = Vec::with_capacity(col_w.len());
    let mut x = 0.0;
    for w in &col_w {
        col_x.push(x);
        x += w + COL_GAP;
    }
    let mut row_y = Vec::with_capacity(row_h.len());
    let mut y = 0.0;
    for h in &row_h {
        row_y.push(y);
        y += h + ROW_GAP;
    }
    let width = (x - COL_GAP).max(0.0);
    let height = (y - ROW_GAP).max(0.0);
    let offsets = children
        .iter()
        .enumerate()
        .map(|(i, &child)| {
            let (cx, cy) = at[i].unwrap_or((0, 0));
            let (c, r) = (col(cx), row(cy));
            let (w, h) = sizes[i];
            // Centred across the column; vertically, items line up on
            // their icons' centres, so they sit at the top of the row.
            let x = col_x[c] + (col_w[c] - w) / 2.0;
            let y = match child {
                Child::Group(_) => row_y[r] + (row_h[r] - h) / 2.0,
                Child::Item(_) => {
                    row_y[r]
                        + ((row_h[r] - h) / 2.0)
                            .min(row_h[r] / 2.0 - ICON / 2.0)
                            .max(0.0)
                }
            };
            (child, Point::new(x, y))
        })
        .collect();
    ctx.arrangements.insert(
        container,
        Arrangement {
            size: (width, height),
            offsets,
        },
    );
}

fn place(
    ctx: &Ctx,
    container: Option<usize>,
    origin: Point,
    items: &mut [Rect],
    groups: &mut [Rect],
) {
    let arrangement = &ctx.arrangements[&container];
    for &(child, offset) in &arrangement.offsets {
        let at = Point::new(origin.x + offset.x, origin.y + offset.y);
        match child {
            Child::Item(i) => {
                let (w, h) = ctx.item_sizes[i];
                items[i] = Rect::new(at.x, at.y, w, h);
            }
            Child::Group(g) => {
                let (w, h) = ctx.arrangements[&Some(g)].size;
                let title_w = ctx.group_title[g];
                let outer_w = (w + GROUP_PAD * 2.0).max(title_w);
                groups[g] = Rect::new(at.x, at.y, outer_w, h + GROUP_PAD * 2.0 + ctx.group_label);
                let inner = Point::new(
                    at.x + (outer_w - w) / 2.0,
                    at.y + GROUP_PAD + ctx.group_label,
                );
                place(ctx, Some(g), inner, items, groups);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;
    use diagram_scene::Estimate;

    const SAMPLE: &str = "architecture-beta\n title Web\n group api(cloud)[API]\n \
        group data[Data] in api\n service db(database)[Database] in data\n \
        service disk1(disk)[Storage] in data\n service server(server)[Server] in api\n \
        service gw(logos:aws-api-gateway)[Gateway]\n junction hub in api\n \
        db:L -- R:server\n disk1:T -- B:db\n server:T -[calls]-> B:hub\n \
        gw:R --> L:server{group}\n";

    #[test]
    fn nothing_is_dropped_groups_hold_members_and_nothing_overlaps() {
        let model = parse(SAMPLE).unwrap();
        let scene = layout(&model, &Estimate);
        let texts = scene.texts();
        for expected in [
            "Web", "API", "Data", "Database", "Storage", "Server", "Gateway", "calls",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
        assert_eq!(scene.edges.len(), model.edges.len());
        let item = |id: &str| scene.node(id).unwrap().rect;
        let api = scene.containers[0].rect;
        let data = scene.containers[1].rect;
        for id in ["db", "disk1", "server", "hub"] {
            assert!(api.encloses(&item(id), 0.01), "{id} outside API");
        }
        for id in ["db", "disk1"] {
            assert!(data.encloses(&item(id), 0.01), "{id} outside Data");
        }
        assert!(!api.intersects(&item("gw")));
        assert!(!data.intersects(&item("server")));
        // Sides are honoured: db is right of server, disk below db.
        assert!(item("db").x > item("server").right());
        assert!(item("disk1").y > item("db").bottom());
        let bounds = scene.bounds();
        for node in &scene.nodes {
            assert!(bounds.encloses(&node.rect, 0.01), "{node:?}");
        }
        for c in &scene.containers {
            assert!(bounds.encloses(&c.rect, 0.01), "{c:?}");
        }
        let services: Vec<_> = scene.nodes.iter().filter(|n| !n.id.contains(':')).collect();
        for (i, a) in services.iter().enumerate() {
            for b in &services[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
    }

    #[test]
    fn a_crowded_cell_pushes_along_the_edge() {
        let model = parse(
            "architecture-beta\n service a\n service b\n service c\n a:R -- L:b\n a:R -- L:c\n",
        )
        .unwrap();
        let scene = layout(&model, &Estimate);
        let r = |id: &str| scene.node(id).unwrap().rect;
        assert!(r("b").x > r("a").right());
        assert!(r("c").x > r("b").right());
    }
}
