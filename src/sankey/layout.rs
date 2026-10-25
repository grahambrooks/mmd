//! A sankey: nodes in columns by their depth in the flow (sinks pushed to
//! the last column, as d3-sankey's `justify`), heights proportional to the
//! flow through them, and ribbons as wide as each flow between them.

use diagram_scene::{
    Anchor, Font, Node, Paint, Point, Rect, Role, Run, Scene, Seg, Shape, Style, Text, TextLine,
};

use super::parse::Sankey;
use crate::chart;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const WIDTH: f32 = 600.0;
const HEIGHT: f32 = 400.0;
const NODE_WIDTH: f32 = 10.0;
/// Mermaid's 12 px node padding plus 15 for the value it shows.
const NODE_GAP: f32 = 27.0;
/// Mermaid's link `stroke-opacity`.
const OPACITY: f32 = 0.5;
/// The thinnest a ribbon is drawn, as mermaid's `max(1, width)`.
const MIN_RIBBON: f32 = 1.0;

pub(crate) fn layout(s: &Sankey, measure: &dyn Measure) -> Scene {
    let n = s.nodes.len();
    let incoming = |i: usize| s.links.iter().filter(move |l| l.target == i);
    let outgoing = |i: usize| s.links.iter().filter(move |l| l.source == i);
    let value: Vec<f64> = (0..n)
        .map(|i| {
            let a: f64 = incoming(i).map(|l| l.value).sum();
            let b: f64 = outgoing(i).map(|l| l.value).sum();
            a.max(b)
        })
        .collect();

    // Depth: the longest path from a source. Sinks go to the last column.
    let mut depth = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for l in &s.links {
            if depth[l.target] < depth[l.source] + 1 {
                depth[l.target] = depth[l.source] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let last = depth.iter().copied().max().unwrap_or(0);
    for (i, d) in depth.iter_mut().enumerate() {
        if outgoing(i).next().is_none() {
            *d = last;
        }
    }
    let columns = last + 1;
    let mut order: Vec<Vec<usize>> = vec![Vec::new(); columns];
    for i in 0..n {
        order[depth[i]].push(i);
    }

    // One scale for every column, so the fullest one fills the height.
    let ky = order
        .iter()
        .filter(|col| !col.is_empty())
        .map(|col| {
            let total: f64 = col.iter().map(|i| value[*i]).sum();
            let room = HEIGHT - (col.len() as f32 - 1.0) * NODE_GAP;
            if total > 0.0 {
                room.max(HEIGHT * 0.25) / total as f32
            } else {
                f32::MAX
            }
        })
        .fold(f32::MAX, f32::min);
    let ky = if ky.is_finite() && ky < f32::MAX {
        ky
    } else {
        1.0
    };
    let height: Vec<f32> = value.iter().map(|v| (*v as f32 * ky).max(1.0)).collect();

    let title_h = 0.0;
    let top = PADDING + title_h;
    let mut y = vec![0.0f32; n];
    let stack = |order: &[Vec<usize>], y: &mut Vec<f32>| {
        for col in order {
            let total: f32 =
                col.iter().map(|i| height[*i]).sum::<f32>() + (col.len() as f32 - 1.0) * NODE_GAP;
            let mut at = top + ((HEIGHT - total) / 2.0).max(0.0);
            for &i in col {
                y[i] = at;
                at += height[i] + NODE_GAP;
            }
        }
    };
    stack(&order, &mut y);
    // Relax: order each column by where its flows come from, then by where
    // they go, so ribbons cross as little as a few sweeps can manage.
    let centre = |y: &[f32], i: usize| y[i] + height[i] / 2.0;
    for _ in 0..6 {
        for c in 1..columns {
            let key: Vec<(usize, f32)> = order[c]
                .iter()
                .map(|&i| {
                    let (sum, weight) = incoming(i).fold((0.0, 0.0), |(s, w), l| {
                        (
                            s + centre(&y, l.source) * l.value as f32,
                            w + l.value as f32,
                        )
                    });
                    (
                        i,
                        if weight > 0.0 {
                            sum / weight
                        } else {
                            centre(&y, i)
                        },
                    )
                })
                .collect();
            sort_by_key(&mut order[c], &key);
            stack(&order, &mut y);
        }
        for c in (0..columns.saturating_sub(1)).rev() {
            let key: Vec<(usize, f32)> = order[c]
                .iter()
                .map(|&i| {
                    let (sum, weight) = outgoing(i).fold((0.0, 0.0), |(s, w), l| {
                        (
                            s + centre(&y, l.target) * l.value as f32,
                            w + l.value as f32,
                        )
                    });
                    (
                        i,
                        if weight > 0.0 {
                            sum / weight
                        } else {
                            centre(&y, i)
                        },
                    )
                })
                .collect();
            sort_by_key(&mut order[c], &key);
            stack(&order, &mut y);
        }
    }

    let step = if columns > 1 {
        (WIDTH - NODE_WIDTH) / (columns - 1) as f32
    } else {
        0.0
    };
    let x: Vec<f32> = depth.iter().map(|d| PADDING + *d as f32 * step).collect();

    // Where each link leaves its source and enters its target: stacked in
    // the order of the other end.
    let mut out_at = vec![0.0f32; s.links.len()];
    let mut in_at = vec![0.0f32; s.links.len()];
    for i in 0..n {
        let mut outs: Vec<usize> = (0..s.links.len())
            .filter(|k| s.links[*k].source == i)
            .collect();
        outs.sort_by(|a, b| {
            centre(&y, s.links[*a].target).total_cmp(&centre(&y, s.links[*b].target))
        });
        let mut at = y[i];
        for k in outs {
            out_at[k] = at;
            at += s.links[k].value as f32 * ky;
        }
        let mut ins: Vec<usize> = (0..s.links.len())
            .filter(|k| s.links[*k].target == i)
            .collect();
        ins.sort_by(|a, b| {
            centre(&y, s.links[*a].source).total_cmp(&centre(&y, s.links[*b].source))
        });
        let mut at = y[i];
        for k in ins {
            in_at[k] = at;
            at += s.links[k].value as f32 * ky;
        }
    }

    let mut scene = Scene::new(0.0, 0.0);
    for (k, l) in s.links.iter().enumerate() {
        let w = l.value as f32 * ky;
        // A flow thinner than the least ribbon is widened about its middle.
        let grow = (MIN_RIBBON - w).max(0.0) / 2.0;
        let (w, top0, top1) = (w + 2.0 * grow, out_at[k] - grow, in_at[k] - grow);
        let (x0, x1) = (x[l.source] + NODE_WIDTH, x[l.target]);
        let mid = (x0 + x1) / 2.0;
        // Along the top, down the target's side, back along the bottom.
        let path = vec![
            Seg::Move(Point::new(x0, top0)),
            Seg::Cubic(
                Point::new(mid, top0),
                Point::new(mid, top1),
                Point::new(x1, top1),
            ),
            Seg::Line(Point::new(x1, top1 + w)),
            Seg::Cubic(
                Point::new(mid, top1 + w),
                Point::new(mid, top0 + w),
                Point::new(x0, top0 + w),
            ),
            Seg::Close,
        ];
        scene.areas.push(chart::area(
            path,
            Paint::Role(Role::Series(l.source as u8)),
            OPACITY,
            None,
        ));
    }

    let size = FONT_SIZE;
    let small = Font::Small.size(FONT_SIZE);
    let (mut left_edge, mut right_edge) = (PADDING, PADDING + WIDTH);
    for i in 0..n {
        let rect = Rect::new(x[i], y[i], NODE_WIDTH, height[i]);
        let mut node = Node::new(format!("node-{i}"), Shape::Rect, rect);
        let paint = Paint::Role(Role::Series(i as u8));
        node.style = Style {
            fill: paint,
            stroke: Paint::None,
            ..Style::for_shape(Shape::Rect)
        };
        scene.nodes.push(node);
        let line = TextLine {
            runs: vec![
                Run {
                    text: s.nodes[i].clone(),
                    font: Font::Body,
                },
                Run {
                    text: format!(" {}", chart::format(value[i])),
                    font: Font::Small,
                },
            ],
        };
        let w = measure.width(&s.nodes[i], size, false)
            + measure.width(&format!(" {}", chart::format(value[i])), small, false);
        // Labels face into the chart, as mermaid places them, except in the
        // last column, where they go outside so no ribbon end is covered.
        let label_right = x[i] < PADDING + WIDTH / 2.0 || depth[i] == last;
        let (pos, anchor) = if label_right {
            right_edge = right_edge.max(rect.right() + 6.0 + w + 3.0);
            (
                Point::new(rect.right() + 6.0, rect.center().y + size * 0.35),
                Anchor::Start,
            )
        } else {
            left_edge = left_edge.min(rect.x - 6.0 - w - 3.0);
            (
                Point::new(rect.x - 6.0, rect.center().y + size * 0.35),
                Anchor::End,
            )
        };
        scene.texts.push(Text {
            pos,
            anchor,
            line,
            paint: Paint::Role(Role::Text),
            backed: true,
            rotate: 0.0,
        });
    }

    let shift = (PADDING - left_edge).max(0.0);
    if shift > 0.0 {
        move_all(&mut scene, shift);
    }
    let bottom = y
        .iter()
        .zip(&height)
        .map(|(y, h)| y + h)
        .fold(top + HEIGHT, f32::max);
    scene.width = right_edge + shift + PADDING;
    scene.height = bottom + PADDING;
    scene
}

/// Sort `items` by their keys, keeping ties in place.
fn sort_by_key(items: &mut [usize], key: &[(usize, f32)]) {
    let k = |i: &usize| key.iter().find(|(j, _)| j == i).map_or(0.0, |(_, v)| *v);
    items.sort_by(|a, b| k(a).total_cmp(&k(b)));
}

fn move_all(scene: &mut Scene, dx: f32) {
    for node in &mut scene.nodes {
        node.rect.x += dx;
    }
    for t in &mut scene.texts {
        t.pos.x += dx;
    }
    for a in &mut scene.areas {
        chart::move_path(&mut a.path, dx, 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::tests::{Fixed, assert_inside};

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Fixed)
    }

    #[test]
    fn every_node_and_value_reaches_the_scene() {
        let s = scene("sankey\nA,B,10\nA,C,5\nB,D,10\nC,D,2.5\n");
        let texts = s.texts();
        for expected in ["A 15", "B 10", "C 5", "D 12.5"] {
            assert!(texts.iter().any(|t| t == expected), "{expected}: {texts:?}");
        }
        assert_eq!(s.nodes.len(), 4);
        assert_eq!(s.areas.len(), 4, "one ribbon per flow");
        assert!(s.edges.is_empty(), "no strands");
        for (a, source) in s.areas.iter().zip([0u8, 0, 1, 2]) {
            assert_eq!(a.fill, Paint::Role(Role::Series(source)));
            assert_eq!(a.opacity, OPACITY);
            assert_eq!(a.path.last(), Some(&Seg::Close));
        }
        assert_inside(&s);
    }

    #[test]
    fn sinks_line_up_in_the_last_column() {
        let s = scene("sankey\nA,B,1\nB,C,1\nA,D,1\n");
        let x = |id: &str| s.node(id).unwrap().rect.x;
        assert!(x("node-1") < x("node-2"));
        assert_eq!(x("node-2"), x("node-3"), "C and D are both sinks");
    }

    #[test]
    fn node_heights_follow_their_flow() {
        let s = scene("sankey\nA,B,30\nA,C,10\n");
        let h = |id: &str| s.node(id).unwrap().rect.height;
        assert!((h("node-1") / h("node-2") - 3.0).abs() < 0.01);
        assert!((h("node-0") - h("node-1") - h("node-2")).abs() < 0.01);
    }

    #[test]
    fn zero_flows_still_render() {
        let s = scene("sankey\nA,B,0\n");
        assert_inside(&s);
        assert_eq!(s.areas.len(), 1);
        assert!(s.texts().iter().any(|t| t == "B 0"));
    }
}
