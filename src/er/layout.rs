//! ER layout: entities as tables (a header over type / name / keys /
//! comment columns), placed by the layered layout, relationships as
//! curves with a crow's foot at each end and the label mid-edge.

use diagram_scene::route::Side;
use diagram_scene::shape::{line_height, natural_size, text_layout};
use diagram_scene::{
    Anchor, Cardinality as Card, Dash, Edge, EdgeLabel, Font, Marker, Node, Paint, Pen, Point,
    Rect, Role, Scene, Seg, Shape, Text, TextLine,
};

use super::parse::{Cardinality, Direction, Er};
use crate::layered::{self, Graph, GraphEdge, Spacing};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Rows start this far in from a table's left edge (as `diagram_scene`
/// indents compartment rows), and columns are this far apart.
const ROW_INDENT: f32 = 8.0;
const COLUMN_GAP: f32 = 14.0;
/// Relationship labels wrap at this width.
const LABEL_WIDTH: f32 = 160.0;
/// How far a self-relationship's loop stands off its entity.
const LOOP: f32 = 52.0;

/// An entity's attribute columns after the first (which the table's own
/// rows carry): x offsets from the table's left edge, and each row's cells.
struct Columns {
    offsets: [f32; 3],
    cells: Vec<[Option<TextLine>; 3]>,
}

pub fn layout(er: &Er, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let horizontal = matches!(er.direction, Direction::LeftRight | Direction::RightLeft);
    let width = |line: &TextLine| diagram_scene::line_width(measure, line, FONT_SIZE);

    // Entities, sized for their tables.
    let mut nodes = Vec::new();
    let mut columns = Vec::new();
    let mut sizes = Vec::new();
    for entity in &er.entities {
        let shape = if entity.attributes.is_empty() {
            Shape::Rect
        } else {
            Shape::Table
        };
        let mut node = Node::new(entity.name.clone(), shape, Rect::default());
        node.label = vec![TextLine::new(entity.label(), Font::Bold)];
        let colors = er.colors(entity);
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
        let mut cols = Columns {
            offsets: [0.0; 3],
            cells: Vec::new(),
        };
        let mut size = natural_size(&node, FONT_SIZE, measure);
        if !entity.attributes.is_empty() {
            let rows: Vec<TextLine> = entity
                .attributes
                .iter()
                .map(|a| TextLine::new(a.kind.clone(), Font::Body))
                .collect();
            cols.cells = entity
                .attributes
                .iter()
                .map(|a| {
                    [
                        Some(TextLine::new(a.name.clone(), Font::Body)),
                        (!a.keys.is_empty()).then(|| TextLine::new(a.keys.join(", "), Font::Small)),
                        a.comment
                            .as_ref()
                            .map(|c| TextLine::new(c.clone(), Font::Italic)),
                    ]
                })
                .collect();
            // Each column as wide as its widest cell; empty columns vanish.
            let mut x = ROW_INDENT + rows.iter().map(width).fold(0.0, f32::max);
            for c in 0..3 {
                let widest = cols
                    .cells
                    .iter()
                    .filter_map(|row| row[c].as_ref().map(width))
                    .fold(0.0_f32, f32::max);
                if widest > 0.0 {
                    x += COLUMN_GAP;
                }
                cols.offsets[c] = x;
                x += widest;
            }
            node.compartments = vec![rows];
            size = natural_size(&node, FONT_SIZE, measure);
            size.0 = size.0.max(x + ROW_INDENT);
        } else {
            size = (size.0.max(90.0), size.1.max(44.0));
        }
        nodes.push(node);
        columns.push(cols);
        sizes.push(size);
    }

    // Relationship labels.
    let small = Font::Small.size(FONT_SIZE);
    let labels: Vec<Option<Vec<TextLine>>> = er
        .relationships
        .iter()
        .map(|r| {
            (!r.label.trim().is_empty()).then(|| {
                r.label
                    .split('\n')
                    .flat_map(|line| crate::text::wrap(measure, line, small, false, LABEL_WIDTH))
                    .map(|line| TextLine::new(line, Font::Small))
                    .collect()
            })
        })
        .collect();
    let label_size = |lines: &Vec<TextLine>| {
        let w = lines.iter().map(width).fold(0.0_f32, f32::max);
        (w + 16.0, lines.len() as f32 * line_height(small) + 6.0)
    };

    // Self-relationships loop out across the flow, so the layout is told
    // the entity is wider (or taller) by the loops and their labels.
    let mut loops: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (i, r) in er.relationships.iter().enumerate() {
        if r.from == r.to {
            loops[r.from].push(i);
        }
    }
    let reach = |node: usize| -> f32 {
        loops[node]
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let label = labels[i].as_ref().map_or(0.0, |l| {
                    let (w, h) = label_size(l);
                    if horizontal { h } else { w }
                });
                LOOP * (k + 1) as f32 * 0.75 + label + 6.0
            })
            .fold(0.0, f32::max)
    };
    let graph = Graph {
        sizes: sizes
            .iter()
            .enumerate()
            .map(|(n, &(w, h))| {
                let extra = 2.0 * reach(n);
                if horizontal {
                    (w, h + extra)
                } else {
                    (w + extra, h)
                }
            })
            .collect(),
        edges: er
            .relationships
            .iter()
            .zip(&labels)
            .map(|(r, label)| GraphEdge {
                from: r.from,
                to: r.to,
                label: label.as_ref().map(label_size),
            })
            .collect(),
        groups: vec![Vec::new(); nodes.len()],
        horizontal,
        reversed: matches!(er.direction, Direction::BottomUp | Direction::RightLeft),
    };
    let spacing = Spacing {
        node_gap: 40.0,
        rank_gap: 30.0,
        group_gap: 0.0,
        margin: PADDING,
    };
    let placed = layered::layout(&graph, &spacing);
    for ((node, &(w, h)), &c) in nodes.iter_mut().zip(&sizes).zip(&placed.centers) {
        node.rect = Rect::centered(c, w, h);
    }

    // Relationships: under the entities, labels over them.
    let ports = ports(er, &nodes, &placed.routes, horizontal);
    let mut loop_count = vec![0usize; nodes.len()];
    for (i, r) in er.relationships.iter().enumerate() {
        let (from, to) = (&nodes[r.from], &nodes[r.to]);
        let (path, label_pos) = match ports[i] {
            Some((start, end)) => {
                let path = curve(start, end, &placed.routes[i].points, horizontal);
                let pos = placed.routes[i]
                    .label
                    .or_else(|| diagram_scene::route::label_position(&path, 0.0));
                (path, pos)
            }
            None => {
                let k = loop_count[r.from];
                loop_count[r.from] += 1;
                let size = labels[i].as_ref().map_or((0.0, 0.0), label_size);
                self_loop(from.rect, horizontal, k, size)
            }
        };
        let mut edge = Edge::new(path).between(&from.id, &to.id);
        edge.pen = Pen {
            dash: if r.identifying {
                Dash::Solid
            } else {
                Dash::Dashed
            },
            ..Pen::default()
        };
        edge.start = marker(r.from_card);
        edge.end = marker(r.to_card);
        if let (Some(lines), Some(pos)) = (labels[i].clone(), label_pos) {
            edge.labels
                .push(EdgeLabel::new(pos, lines, true, measure, FONT_SIZE));
        }
        scene.edges.push(edge);
    }

    // The attribute columns after the first, on the rows' baselines.
    for (n, (node, cols)) in nodes.iter().zip(&columns).enumerate() {
        if cols.cells.is_empty() {
            continue;
        }
        let placed_text = text_layout(node, FONT_SIZE);
        let rows = &placed_text.lines[node.label.len()..];
        let text = match er.colors(&er.entities[n]).text {
            Some((r, g, b)) => Paint::Rgb(r, g, b),
            None => Paint::Role(Role::Text),
        };
        for (row, cells) in rows.iter().zip(&cols.cells) {
            for (c, cell) in cells.iter().enumerate() {
                let Some(line) = cell else { continue };
                scene.texts.push(Text {
                    pos: Point::new(node.rect.x + cols.offsets[c], row.pos.y),
                    anchor: Anchor::Start,
                    line: line.clone(),
                    paint: if c == 2 {
                        Paint::Role(Role::Muted)
                    } else {
                        text
                    },
                    backed: false,
                });
            }
        }
    }

    // Everything inside the bounds: loops, labels and all.
    let mut extent = Rect::new(0.0, 0.0, placed.width, placed.height);
    let mut grow = |r: Rect| extent = extent.union(&r.inflate(PADDING));
    for n in &nodes {
        grow(n.rect);
    }
    for e in &scene.edges {
        for poly in diagram_scene::flatten::flatten(&e.path, 1.0) {
            for p in poly.points {
                grow(Rect::new(p.x, p.y, 0.0, 0.0));
            }
        }
        for l in &e.labels {
            let h = l.lines.len() as f32 * line_height(small) + 6.0;
            grow(Rect::centered(l.pos, l.width + 16.0, h));
        }
    }
    let (dx, dy) = (-extent.x.min(0.0), -extent.y.min(0.0));
    if dx > 0.0 || dy > 0.0 {
        shift(&mut scene, &mut nodes, dx, dy);
    }
    scene.nodes = nodes;
    scene.width = extent.width.max(extent.right() + dx);
    scene.height = extent.height.max(extent.bottom() + dy);
    scene
}

fn marker(card: Cardinality) -> Marker {
    match card {
        Cardinality::ZeroOrOne => Marker::CrowFoot(Card::ZeroOrOne),
        Cardinality::ExactlyOne => Marker::CrowFoot(Card::One),
        Cardinality::ZeroOrMore => Marker::CrowFoot(Card::ZeroOrMany),
        Cardinality::OneOrMore => Marker::CrowFoot(Card::OneOrMany),
        Cardinality::MdParent => Marker::FilledDiamond,
    }
}

/// Where each relationship leaves and arrives (`None` for self-loops): on
/// the side of each entity facing along the flow, the ones sharing a side
/// spread across it in the order their other ends lie.
fn ports(
    er: &Er,
    nodes: &[Node],
    routes: &[layered::Route],
    horizontal: bool,
) -> Vec<Option<(Point, Point)>> {
    use std::collections::BTreeMap;
    let across = |p: Point| if horizontal { p.y } else { p.x };
    let side_toward = |node: &Node, toward: Point| {
        let c = node.rect.center();
        match (
            horizontal,
            if horizontal {
                toward.x >= c.x
            } else {
                toward.y >= c.y
            },
        ) {
            (true, true) => Side::Right,
            (true, false) => Side::Left,
            (false, true) => Side::Bottom,
            (false, false) => Side::Top,
        }
    };
    let side_key = |s: Side| match s {
        Side::Top => 0,
        Side::Right => 1,
        Side::Bottom => 2,
        Side::Left => 3,
    };
    // The ends on one side of a node: (relationship, whether it starts
    // there, where its other end lies across the flow).
    type Ends = Vec<(usize, bool, f32)>;
    let mut sides: BTreeMap<(usize, u8), (Side, Ends)> = BTreeMap::new();
    for (i, r) in er.relationships.iter().enumerate() {
        if r.from == r.to {
            continue;
        }
        let (from, to) = (&nodes[r.from], &nodes[r.to]);
        let points = &routes[i].points;
        let first = points.first().copied().unwrap_or(to.rect.center());
        let last = points.last().copied().unwrap_or(from.rect.center());
        for (node, toward, start) in [(r.from, first, true), (r.to, last, false)] {
            let side = side_toward(&nodes[node], toward);
            sides
                .entry((node, side_key(side)))
                .or_insert_with(|| (side, Vec::new()))
                .1
                .push((i, start, across(toward)));
        }
    }
    let mut out: Vec<(Option<Point>, Option<Point>)> = vec![(None, None); er.relationships.len()];
    for ((node, _), (side, mut ends)) in sides {
        ends.sort_by(|a, b| a.2.total_cmp(&b.2));
        let rect = nodes[node].rect;
        let length = match side {
            Side::Top | Side::Bottom => rect.width,
            Side::Left | Side::Right => rect.height,
        };
        let count = ends.len();
        // Crow's feet are wide: keep them apart.
        let span = (length * 0.7).min(count.saturating_sub(1) as f32 * 44.0);
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
            if start {
                out[i].0 = Some(p);
            } else {
                out[i].1 = Some(p);
            }
        }
    }
    out.into_iter().map(|(a, b)| Some((a?, b?))).collect()
}

/// A smooth curve from `start` through `points` to `end`, each step
/// leaving and arriving along the flow, so the markers sit square to the
/// entity.
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

/// The `k`th loop out of an entity's side (right when the flow is
/// vertical, bottom when horizontal) and back in, and where its label of
/// `label` size goes: just beyond the loop.
fn self_loop(r: Rect, horizontal: bool, k: usize, label: (f32, f32)) -> (Vec<Seg>, Option<Point>) {
    let out = LOOP * (k + 1) as f32;
    let c = r.center();
    if horizontal {
        let half = (r.width * 0.3).clamp(16.0, 36.0);
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
        let half = (r.height * 0.3).clamp(16.0, 36.0);
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

fn shift(scene: &mut Scene, nodes: &mut [Node], dx: f32, dy: f32) {
    let move_point = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for n in nodes {
        n.rect.x += dx;
        n.rect.y += dy;
    }
    for t in &mut scene.texts {
        move_point(&mut t.pos);
    }
    for e in &mut scene.edges {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::er::parse::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.55
        }
    }

    const SAMPLE: &str = "erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    PRODUCT ||..o{ LINE_ITEM : \"appears in\"
    EMPLOYEE |o--o{ EMPLOYEE : manages
    CUSTOMER {
        string id PK
        string email UK \"login\"
    }
    ORDER {
        int id PK
        string customer FK
    }
    LINE_ITEM {
        int order PK, FK
        int qty
    }
";

    fn scene(source: &str) -> Scene {
        layout(&parse(source).unwrap(), &Fixed)
    }

    #[test]
    fn nothing_is_dropped() {
        let scene = scene(SAMPLE);
        let texts = scene.texts();
        for expected in [
            "CUSTOMER",
            "ORDER",
            "LINE_ITEM",
            "PRODUCT",
            "EMPLOYEE",
            "string",
            "id",
            "PK",
            "email",
            "UK",
            "login",
            "customer",
            "FK",
            "order",
            "PK, FK",
            "qty",
            "places",
            "contains",
            "appears in",
            "manages",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
        assert_eq!(scene.edges.len(), 4);
    }

    #[test]
    fn entities_never_overlap_and_everything_is_in_bounds() {
        for direction in ["TB", "BT", "LR", "RL"] {
            let source = SAMPLE.replace(
                "erDiagram\n",
                &format!("erDiagram\n  direction {direction}\n"),
            );
            let scene = scene(&source);
            let rects: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(!a.intersects(b), "{direction}: {a:?} overlaps {b:?}");
                }
            }
            let bounds = scene.bounds();
            for r in &rects {
                assert!(
                    bounds.encloses(r, 0.01),
                    "{direction}: {r:?} outside {bounds:?}"
                );
            }
            for e in &scene.edges {
                for poly in diagram_scene::flatten::flatten(&e.path, 0.5) {
                    for p in poly.points {
                        assert!(bounds.contains(p), "{direction}: {p:?} outside {bounds:?}");
                    }
                }
                for l in &e.labels {
                    assert!(bounds.contains(l.pos), "{direction}: label outside");
                }
            }
            for t in &scene.texts {
                assert!(bounds.contains(t.pos), "{direction}: text outside");
            }
        }
    }

    #[test]
    fn attribute_columns_sit_inside_their_table() {
        let scene = scene(SAMPLE);
        for t in &scene.texts {
            let inside = scene.nodes.iter().any(|n| {
                n.rect.contains(t.pos)
                    && t.pos.x + diagram_scene::line_width(&Fixed, &t.line, FONT_SIZE)
                        <= n.rect.right() + 0.01
            });
            assert!(
                inside,
                "{:?} at {:?} is outside every table",
                t.line.plain(),
                t.pos
            );
        }
    }

    #[test]
    fn markers_and_dashes_follow_the_relationship() {
        let scene = scene(SAMPLE);
        let e = &scene.edges;
        assert_eq!(e[0].start, Marker::CrowFoot(Card::One));
        assert_eq!(e[0].end, Marker::CrowFoot(Card::ZeroOrMany));
        assert_eq!(e[1].end, Marker::CrowFoot(Card::OneOrMany));
        assert_eq!(e[2].pen.dash, Dash::Dashed);
        assert_eq!(e[0].pen.dash, Dash::Solid);
        assert_eq!(e[3].start, Marker::CrowFoot(Card::ZeroOrOne));
    }

    #[test]
    fn a_self_relationship_loops_without_touching_its_neighbours() {
        let scene = scene(SAMPLE);
        let employee = scene.node("EMPLOYEE").unwrap().rect;
        let edge = scene
            .edges
            .iter()
            .find(|e| e.from.as_deref() == Some("EMPLOYEE"))
            .unwrap();
        let (start, _) = diagram_scene::flatten::start(&edge.path).unwrap();
        let (end, _) = diagram_scene::flatten::end(&edge.path).unwrap();
        assert!(employee.inflate(0.5).contains(start) && employee.inflate(0.5).contains(end));
        let label = &edge.labels[0];
        let label_box = Rect::centered(label.pos, label.width, 12.0);
        for n in &scene.nodes {
            assert!(!n.rect.intersects(&label_box), "loop label over {}", n.id);
        }
    }
}
