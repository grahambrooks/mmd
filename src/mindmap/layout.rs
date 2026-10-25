//! A two-sided tidy tree: the root in the middle, its branches shared
//! between the right and the left, each subtree stacked so siblings never
//! overlap and a parent sits centred on its children. Each branch takes a
//! series colour for its outlines and curves.

use diagram_scene::{
    Edge, Font, Marker, Node, Paint, Pen, Role, Scene, Seg, Shape, Weight, geom::Point, geom::Rect,
    route, shape::natural_size,
};

use super::parse::{Mindmap, NodeShape};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Upstream's `maxNodeWidth`.
const LABEL_WIDTH: f32 = 200.0;
/// Between a parent and its children, and between sibling subtrees.
const H_GAP: f32 = 48.0;
const V_GAP: f32 = 12.0;

pub fn layout(map: &Mindmap, measure: &dyn Measure) -> Scene {
    let mut nodes: Vec<Node> = map
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let font = if i == 0 { Font::Bold } else { Font::Body };
            let mut node = Node::new(format!("n{i}"), shape(n.shape), Rect::default());
            node.label =
                crate::flowchart::text_lines(measure, &n.label, n.markdown, font, LABEL_WIDTH);
            if n.shape == NodeShape::Bang {
                node.style.emphasis = true;
            }
            let (w, h) = natural_size(&node, FONT_SIZE, measure);
            node.rect = Rect::new(0.0, 0.0, w.max(40.0), h.max(30.0));
            node
        })
        .collect();

    // Branch colours, by top-level child.
    let mut branch = vec![0u8; nodes.len()];
    for (b, &child) in map.nodes[0].children.iter().enumerate() {
        paint_branch(map, child, b as u8, &mut branch);
    }
    for (i, node) in nodes.iter_mut().enumerate() {
        if i == 0 {
            node.style.fill = Paint::Role(Role::Accent);
            node.style.stroke = Paint::Role(Role::Accent);
            node.style.text = Paint::Role(Role::Background);
        } else {
            node.style.stroke = Paint::Role(Role::Series(branch[i]));
        }
    }

    // Subtree heights.
    let mut height = vec![0.0_f32; nodes.len()];
    measure_subtree(map, &nodes, 0, &mut height);

    // Share the branches: each goes to the lighter side, in order.
    let (mut right, mut left) = (Vec::new(), Vec::new());
    let (mut rh, mut lh) = (0.0_f32, 0.0_f32);
    for &child in &map.nodes[0].children {
        if rh <= lh {
            rh += height[child] + V_GAP;
            right.push(child);
        } else {
            lh += height[child] + V_GAP;
            left.push(child);
        }
    }

    let root = nodes[0].rect;
    nodes[0].rect = Rect::new(
        -root.width / 2.0,
        -root.height / 2.0,
        root.width,
        root.height,
    );
    let mut edges = Vec::new();
    for (side, total, dir) in [(&right, rh, 1.0_f32), (&left, lh, -1.0)] {
        let mut top = -(total - V_GAP) / 2.0;
        for &child in side.iter() {
            place(
                map, &mut nodes, &height, child, 0, top, dir, &mut edges, &branch,
            );
            top += height[child] + V_GAP;
        }
    }

    // Move everything to the page.
    let bounds = nodes
        .iter()
        .map(|n| n.rect)
        .reduce(|a, b| a.union(&b))
        .unwrap_or_default();
    let (dx, dy) = (PADDING - bounds.x, PADDING - bounds.y);
    for node in &mut nodes {
        node.rect.x += dx;
        node.rect.y += dy;
    }
    for edge in &mut edges {
        for seg in &mut edge.path {
            let p = |p: Point| p.offset(dx, dy);
            *seg = match *seg {
                Seg::Move(a) => Seg::Move(p(a)),
                Seg::Line(a) => Seg::Line(p(a)),
                Seg::Cubic(a, b, c) => Seg::Cubic(p(a), p(b), p(c)),
                Seg::Close => Seg::Close,
            };
        }
    }
    let mut scene = Scene::new(bounds.width + 2.0 * PADDING, bounds.height + 2.0 * PADDING);
    scene.font_size = FONT_SIZE;
    scene.nodes = nodes;
    scene.edges = edges;
    scene
}

fn shape(shape: NodeShape) -> Shape {
    match shape {
        NodeShape::Default | NodeShape::Rounded => Shape::Rounded,
        NodeShape::Rect => Shape::Rect,
        NodeShape::Circle => Shape::Circle,
        NodeShape::Cloud => Shape::Cloud,
        NodeShape::Bang => Shape::Ellipse,
        NodeShape::Hexagon => Shape::Hexagon,
    }
}

fn paint_branch(map: &Mindmap, node: usize, b: u8, out: &mut [u8]) {
    out[node] = b;
    for &c in &map.nodes[node].children {
        paint_branch(map, c, b, out);
    }
}

fn measure_subtree(map: &Mindmap, nodes: &[Node], node: usize, out: &mut [f32]) -> f32 {
    let children = &map.nodes[node].children;
    let mut sum = 0.0;
    for &c in children {
        sum += measure_subtree(map, nodes, c, out);
    }
    if !children.is_empty() {
        sum += V_GAP * (children.len() - 1) as f32;
    }
    let h = nodes[node].rect.height.max(sum);
    out[node] = h;
    h
}

/// Place `node`'s subtree in a band starting at `top`, beside its parent
/// on the side `dir` points to (+1 right, -1 left).
#[allow(clippy::too_many_arguments)]
fn place(
    map: &Mindmap,
    nodes: &mut [Node],
    height: &[f32],
    node: usize,
    parent: usize,
    top: f32,
    dir: f32,
    edges: &mut Vec<Edge>,
    branch: &[u8],
) {
    let p = nodes[parent].rect;
    let (w, h) = (nodes[node].rect.width, nodes[node].rect.height);
    let x = if dir > 0.0 {
        p.right() + H_GAP
    } else {
        p.x - H_GAP - w
    };
    let cy = top + height[node] / 2.0;
    nodes[node].rect = Rect::new(x, cy - h / 2.0, w, h);

    let r = nodes[node].rect;
    let (from_side, to_side) = if dir > 0.0 {
        (route::Side::Right, route::Side::Left)
    } else {
        (route::Side::Left, route::Side::Right)
    };
    let start = if parent == 0 {
        route::clip(nodes[0].shape, p, r.center())
    } else {
        route::port(p, from_side)
    };
    let mut edge = Edge::new(route::curve_between(
        start,
        from_side,
        route::port(r, to_side),
        to_side,
    ))
    .between(&nodes[parent].id, &nodes[node].id);
    edge.end = Marker::None;
    edge.pen = Pen {
        weight: if parent == 0 {
            Weight::Thick
        } else {
            Weight::Normal
        },
        paint: Paint::Role(Role::Series(branch[node])),
        ..Pen::default()
    };
    edges.push(edge);

    let children = &map.nodes[node].children;
    let block: f32 = children.iter().map(|&c| height[c]).sum::<f32>()
        + V_GAP * children.len().saturating_sub(1) as f32;
    let mut child_top = cy - block / 2.0;
    for &c in children {
        place(map, nodes, height, c, node, child_top, dir, edges, branch);
        child_top += height[c] + V_GAP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    const SOURCE: &str = "mindmap\n  root((Release 2.0))\n    Scope\n      Features\n        Search\n        Export\n      Fixes\n    Schedule\n      Beta\n      GA\n    Risks\n      ))Late API((\n      )Staffing(\n    Comms\n      {{Blog}}\n      [Changelog]\n    Ops\n";

    fn scene() -> Scene {
        layout(&super::super::parse(SOURCE).unwrap(), &Estimate)
    }

    #[test]
    fn every_node_is_drawn_and_none_overlap() {
        let s = scene();
        let texts = s.texts();
        for t in [
            "Release 2.0",
            "Scope",
            "Features",
            "Search",
            "Export",
            "Fixes",
            "Schedule",
            "Beta",
            "GA",
            "Risks",
            "Late API",
            "Staffing",
            "Comms",
            "Blog",
            "Changelog",
            "Ops",
        ] {
            assert!(texts.iter().any(|x| x == t), "missing {t}");
        }
        for (i, a) in s.nodes.iter().enumerate() {
            assert!(s.bounds().encloses(&a.rect, 0.01));
            for b in &s.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
        assert_eq!(s.edges.len(), s.nodes.len() - 1);
    }

    #[test]
    fn branches_spread_to_both_sides() {
        let s = scene();
        let root = s.node("n0").unwrap().rect.center().x;
        let sides: Vec<bool> = [1, 6, 9, 12, 15]
            .iter()
            .map(|i| s.node(&format!("n{i}")).unwrap().rect.center().x > root)
            .collect();
        assert!(sides.contains(&true) && sides.contains(&false), "{sides:?}");
    }
}
