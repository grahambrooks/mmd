//! Flowchart layout: nodes sized to their labels, placed by the layered
//! layout, links as smooth curves through the points it leaves between
//! ranks, and subgraphs as boxes around their members.

use diagram_scene::shape::natural_size;
use diagram_scene::{
    Container, ContainerKind, Dash, Edge, EdgeLabel, Font, Marker, Node, Paint, Pen, Point, Rect,
    Scene, Seg, Shape, Style, TextLine, Weight,
};

use super::parse::{Direction, End, Flowchart, LineKind, NodeShape};
use crate::layered::{self, Graph, GraphEdge, Spacing};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Space inside a subgraph's box around its members, and for its title.
const GROUP_PAD: f32 = 14.0;
const TITLE: f32 = 22.0;
/// Labels wrap at this width.
const LABEL_WIDTH: f32 = 200.0;

pub fn layout(chart: &Flowchart, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let horizontal = matches!(chart.direction, Direction::LeftRight | Direction::RightLeft);

    // Nodes, sized for their labels.
    let mut nodes: Vec<Node> = chart
        .nodes
        .iter()
        .map(|n| {
            let mut node = Node::new(n.id.clone(), shape(n.shape), Rect::default());
            node.label = n
                .label
                .split('\n')
                .flat_map(|line| crate::text::wrap(measure, line, FONT_SIZE, false, LABEL_WIDTH))
                .map(|line| TextLine::new(line, Font::Body))
                .collect();
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
    let sizes: Vec<(f32, f32)> = nodes
        .iter()
        .map(|n| {
            let (w, h) = natural_size(n, FONT_SIZE, measure);
            (w.max(48.0), h.max(36.0))
        })
        .collect();

    // Each node's subgraphs, outermost first.
    let chain_of = |mut sub: Option<usize>| {
        let mut chain = Vec::new();
        while let Some(s) = sub {
            chain.push(s);
            sub = chart.subgraphs[s].parent;
        }
        chain.reverse();
        chain
    };
    let groups: Vec<Vec<usize>> = chart.nodes.iter().map(|n| chain_of(n.parent)).collect();
    let depth = chart
        .subgraphs
        .iter()
        .enumerate()
        .map(|(i, _)| chain_of(Some(i)).len())
        .max()
        .unwrap_or(0) as f32;

    let labels: Vec<Option<Vec<TextLine>>> = chart
        .links
        .iter()
        .map(|l| {
            l.label.as_ref().map(|text| {
                text.split('\n')
                    .flat_map(|line| {
                        crate::text::wrap(measure, line, Font::Small.size(FONT_SIZE), false, 160.0)
                    })
                    .map(|line| TextLine::new(line, Font::Small))
                    .collect()
            })
        })
        .collect();
    let graph = Graph {
        sizes: sizes.clone(),
        edges: chart
            .links
            .iter()
            .zip(&labels)
            .map(|(l, label)| GraphEdge {
                from: l.from,
                to: l.to,
                label: label.as_ref().map(|lines| {
                    let w = lines
                        .iter()
                        .map(|line| diagram_scene::line_width(measure, line, FONT_SIZE))
                        .fold(0.0_f32, f32::max);
                    let h = lines.len() as f32
                        * diagram_scene::shape::line_height(Font::Small.size(FONT_SIZE));
                    (w + 16.0, h + 6.0)
                }),
            })
            .collect(),
        groups: groups.clone(),
        horizontal,
        reversed: matches!(chart.direction, Direction::BottomUp | Direction::RightLeft),
    };
    let spacing = Spacing {
        node_gap: 32.0,
        // Room between ranks for nested subgraphs' borders and titles.
        rank_gap: 22.0 + depth * (GROUP_PAD + TITLE) / 2.0,
        group_gap: GROUP_PAD * 2.0,
        margin: PADDING + depth * GROUP_PAD + if depth > 0.0 { TITLE } else { 0.0 },
    };
    let placed = layered::layout(&graph, &spacing);
    for ((node, &(w, h)), &c) in nodes.iter_mut().zip(&sizes).zip(&placed.centers) {
        node.rect = Rect::centered(c, w, h);
    }

    // Subgraph boxes, innermost first so their parents can wrap them.
    let mut boxes: Vec<Option<Rect>> = vec![None; chart.subgraphs.len()];
    let mut by_depth: Vec<usize> = (0..chart.subgraphs.len()).collect();
    by_depth.sort_by_key(|&s| std::cmp::Reverse(chain_of(Some(s)).len()));
    for &s in &by_depth {
        let mut extent: Option<Rect> = None;
        let mut grow = |r: Rect| extent = Some(extent.map_or(r, |e| e.union(&r)));
        for (node, n) in nodes.iter().zip(&chart.nodes) {
            if n.parent == Some(s) {
                grow(node.rect);
            }
        }
        for (child, sub) in chart.subgraphs.iter().enumerate() {
            if sub.parent == Some(s)
                && let Some(r) = boxes[child]
            {
                grow(r);
            }
        }
        boxes[s] = extent.map(|r| {
            let r = r.inflate(GROUP_PAD);
            let title = if chart.subgraphs[s].title.is_empty() {
                0.0
            } else {
                TITLE
            };
            Rect::new(r.x, r.y - title, r.width, r.height + title)
        });
    }
    // Outermost first, so inner boxes draw over their parents.
    for &s in by_depth.iter().rev() {
        let Some(rect) = boxes[s] else { continue };
        let kind = ContainerKind::Subgraph;
        let title = &chart.subgraphs[s].title;
        scene.containers.push(Container {
            rect,
            label: if title.is_empty() {
                Vec::new()
            } else {
                vec![TextLine::new(title.clone(), Font::Bold)]
            },
            style: Style::for_container(&kind),
            kind,
        });
    }

    // Links: curves through the layout's points, leaving and arriving
    // along the flow.
    let ports = ports(chart, &nodes, &placed.routes, horizontal);
    for (i, link) in chart.links.iter().enumerate() {
        let (from, to) = (&nodes[link.from], &nodes[link.to]);
        let route = &placed.routes[i];
        let path = match ports[i] {
            Some((start, end)) => curve(start, end, &route.points, horizontal),
            None => self_loop(from.rect, horizontal),
        };
        let label_pos = route
            .label
            .or_else(|| diagram_scene::route::label_position(&path, 0.0));
        let mut edge = Edge::new(path).between(&from.id, &to.id);
        edge.pen = Pen {
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
        edge.start = marker(link.start);
        edge.end = marker(link.end);
        if let (Some(lines), Some(pos)) = (labels[i].clone(), label_pos) {
            edge.labels
                .push(EdgeLabel::new(pos, lines, true, measure, FONT_SIZE));
        }
        scene.edges.push(edge);
    }

    // Everything inside the bounds, self-loops and boxes included.
    let mut extent = Rect::new(0.0, 0.0, placed.width, placed.height);
    for c in &scene.containers {
        extent = extent.union(&c.rect.inflate(PADDING));
    }
    for e in &scene.edges {
        for poly in diagram_scene::flatten::flatten(&e.path, 1.0) {
            for p in poly.points {
                extent = extent.union(&Rect::new(p.x, p.y, 0.0, 0.0).inflate(PADDING));
            }
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

fn shape(shape: NodeShape) -> Shape {
    match shape {
        NodeShape::Rect | NodeShape::Asymmetric => Shape::Rect,
        NodeShape::Round => Shape::Rounded,
        NodeShape::Stadium => Shape::Stadium,
        NodeShape::Subroutine => Shape::Subroutine,
        NodeShape::Cylinder => Shape::Cylinder,
        NodeShape::Circle | NodeShape::DoubleCircle => Shape::Circle,
        NodeShape::Rhombus => Shape::Diamond,
        NodeShape::Hexagon => Shape::Hexagon,
        NodeShape::Parallelogram | NodeShape::ParallelogramAlt => Shape::Parallelogram,
        NodeShape::Trapezoid | NodeShape::TrapezoidAlt => Shape::Trapezoid,
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

/// Where each link leaves and arrives (`None` for self-loops): on the
/// side of each node facing along the flow, with the links sharing a side
/// spread across it in the order their other ends lie, so they don't
/// bunch into one point.
fn ports(
    chart: &Flowchart,
    nodes: &[Node],
    routes: &[layered::Route],
    horizontal: bool,
) -> Vec<Option<(Point, Point)>> {
    use diagram_scene::route::Side;
    use std::collections::HashMap;
    // The links ending on a node's side: (link, whether it starts there,
    // where its other end lies across the flow).
    type Ends = Vec<(usize, bool, f32)>;
    let mut sides: HashMap<(usize, Side), Ends> = HashMap::new();
    let across = |p: Point| if horizontal { p.y } else { p.x };
    let side_toward = |node: &Node, toward: Point| {
        let c = node.rect.center();
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
    for (i, link) in chart.links.iter().enumerate() {
        if link.from == link.to {
            continue;
        }
        let (from, to) = (&nodes[link.from], &nodes[link.to]);
        let first = routes[i]
            .points
            .first()
            .copied()
            .unwrap_or(to.rect.center());
        let last = routes[i]
            .points
            .last()
            .copied()
            .unwrap_or(from.rect.center());
        sides
            .entry((link.from, side_toward(from, first)))
            .or_default()
            .push((i, true, across(first)));
        sides
            .entry((link.to, side_toward(to, last)))
            .or_default()
            .push((i, false, across(last)));
    }
    let mut out: Vec<(Option<Point>, Option<Point>)> = vec![(None, None); chart.links.len()];
    for ((node, side), mut ends) in sides {
        ends.sort_by(|a, b| a.2.total_cmp(&b.2));
        let n = &nodes[node];
        let length = match side {
            Side::Top | Side::Bottom => n.rect.width,
            Side::Left | Side::Right => n.rect.height,
        };
        let count = ends.len();
        let span = (length * 0.6).min((count.saturating_sub(1)) as f32 * 16.0);
        for (k, (link, is_start, _)) in ends.into_iter().enumerate() {
            let offset = if count > 1 {
                -span / 2.0 + span * k as f32 / (count - 1) as f32
            } else {
                0.0
            };
            let base = diagram_scene::route::port(n.rect, side);
            let p = match side {
                Side::Top | Side::Bottom => base.offset(offset, 0.0),
                Side::Left | Side::Right => base.offset(0.0, offset),
            };
            // Slanted, pointed and round shapes: meet the outline itself.
            let p = match n.shape {
                Shape::Parallelogram
                | Shape::Trapezoid
                | Shape::Hexagon
                | Shape::Diamond
                | Shape::Circle
                | Shape::Ellipse => diagram_scene::route::clip(n.shape, n.rect, p),
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

fn shift(scene: &mut Scene, nodes: &mut [Node], dx: f32, dy: f32) {
    let move_point = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for n in nodes {
        n.rect.x += dx;
        n.rect.y += dy;
    }
    for c in &mut scene.containers {
        c.rect.x += dx;
        c.rect.y += dy;
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
