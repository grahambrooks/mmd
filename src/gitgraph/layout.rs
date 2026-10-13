//! gitGraph layout: commits along a shared timeline, one lane per branch.

use diagram_scene::{
    Anchor, Dash, Edge, Font, Marker, Node, Paint, Pen, Point, Rect, Role, Scene, Seg, Shape, Text,
    TextLine, Weight,
};

use super::parse::{CommitType, GitGraph};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const LANE_HEIGHT: f32 = 52.0;
const DOT_RADIUS: f32 = 9.0;
const MIN_SLOT: f32 = 68.0;
const LABEL_GAP: f32 = 16.0;
const TAG_GAP: f32 = 14.0;

/// Lay a parsed graph out.
pub fn layout(graph: &GitGraph, measure: &dyn Measure) -> Scene {
    let label_size = Font::Small.size(FONT_SIZE);
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;

    // Branch labels sit in a gutter to the left of the timeline.
    let gutter = graph
        .branches
        .iter()
        .map(|branch| measure.width(&branch.name, FONT_SIZE, true) + 20.0)
        .fold(0.0_f32, f32::max);
    let origin_x = gutter + PADDING * 2.0;
    // Tags hang above their commit and labels below it, so the first lane must
    // sit low enough for a tag to fit inside the bounds.
    let tag_height = label_size + 8.0;
    let has_tags = graph.commits.iter().any(|commit| commit.tag.is_some());
    let headroom = if has_tags {
        tag_height + TAG_GAP + DOT_RADIUS
    } else {
        DOT_RADIUS
    };
    let origin_y = PADDING + headroom;

    // Each commit gets a slot wide enough for its own label and tag.
    let mut xs = Vec::with_capacity(graph.commits.len());
    let mut x = origin_x;
    for commit in &graph.commits {
        let label = measure.width(&commit.id, label_size, false);
        let tag = commit
            .tag
            .as_ref()
            .map(|tag| measure.width(tag, label_size, true) + 16.0)
            .unwrap_or(0.0);
        let slot = MIN_SLOT.max(label + 18.0).max(tag + 18.0);
        x += slot / 2.0;
        xs.push(x);
        x += slot / 2.0;
    }
    let timeline_end = x + PADDING;

    let lane_y = |branch: usize| origin_y + branch as f32 * LANE_HEIGHT;
    let position = |index: usize| Point::new(xs[index], lane_y(graph.commits[index].branch));
    let series = |branch: usize| Paint::Role(Role::Series(branch as u8));
    let line = |from: Point, to: Point| vec![Seg::Move(from), Seg::Line(to)];
    let plain = |path: Vec<Seg>, pen: Pen| Edge {
        pen,
        end: Marker::None,
        ..Edge::new(path)
    };

    // Lane rules first, so commits and edges paint over them.
    for (index, _) in graph.branches.iter().enumerate() {
        let y = lane_y(index);
        scene.edges.push(plain(
            line(
                Point::new(origin_x - PADDING, y),
                Point::new(timeline_end, y),
            ),
            Pen {
                dash: Dash::Dashed,
                weight: Weight::Thin,
                paint: Paint::Role(Role::Line),
            },
        ));
    }

    // Edges from each commit back to its parents.
    for (index, commit) in graph.commits.iter().enumerate() {
        let to = position(index);
        for &parent in &commit.parents {
            let from = position(parent);
            let path = if (from.y - to.y).abs() < f32::EPSILON {
                line(from, to)
            } else {
                // An S-curve between lanes, turning near the later commit.
                let midpoint = (from.x + to.x) / 2.0;
                vec![
                    Seg::Move(from),
                    Seg::Cubic(Point::new(midpoint, from.y), Point::new(midpoint, to.y), to),
                ]
            };
            let mut edge = plain(
                path,
                Pen {
                    dash: Dash::Solid,
                    weight: Weight::Heavy,
                    paint: series(graph.commits[parent].branch.max(commit.branch)),
                },
            );
            edge.from = Some(format!("commit:{parent}"));
            edge.to = Some(format!("commit:{index}"));
            scene.edges.push(edge);
        }
    }

    // Commits.
    let dot = |at: Point, r: f32| Rect::centered(at, r * 2.0, r * 2.0);
    for (index, commit) in graph.commits.iter().enumerate() {
        let at = position(index);
        let color = series(commit.branch);
        let is_merge = commit.parents.len() > 1;
        let id = format!("commit:{index}");

        match commit.kind {
            CommitType::Highlight => {
                let mut node = Node::new(id, Shape::Rect, dot(at, DOT_RADIUS));
                node.style.fill = color;
                node.style.stroke = Paint::Role(Role::Text);
                node.style.emphasis = true;
                scene.nodes.push(node);
            }
            CommitType::Reverse => {
                let mut node = Node::new(id.clone(), Shape::Circle, dot(at, DOT_RADIUS));
                node.style.fill = Paint::Role(Role::Background);
                node.style.stroke = color;
                node.style.emphasis = true;
                scene.nodes.push(node);
                // The cross is drawn as a small node over the ring, so it
                // paints above it.
                let arm = DOT_RADIUS * 0.55;
                let mut cross = Node::new(format!("{id}:reverse"), Shape::Rect, dot(at, arm));
                cross.style.fill = Paint::None;
                cross.style.stroke = Paint::None;
                cross.label = vec![TextLine::new("\u{00d7}", Font::Bold)];
                cross.style.text = color;
                scene.nodes.push(cross);
            }
            CommitType::Normal => {
                let mut node = Node::new(id.clone(), Shape::Circle, dot(at, DOT_RADIUS));
                node.style.fill = color;
                node.style.stroke = color;
                scene.nodes.push(node);
                if is_merge {
                    // A merge reads as a ring, as it does in mermaid.
                    let mut hole = Node::new(
                        format!("{id}:merge"),
                        Shape::Circle,
                        dot(at, DOT_RADIUS * 0.45),
                    );
                    hole.style.fill = Paint::Role(Role::Background);
                    hole.style.stroke = Paint::None;
                    scene.nodes.push(hole);
                }
            }
        }

        scene.texts.push(Text {
            pos: Point::new(at.x, at.y + DOT_RADIUS + LABEL_GAP),
            anchor: Anchor::Middle,
            line: TextLine::new(commit.id.clone(), Font::Small),
            paint: Paint::Role(Role::Muted),
            // Merge curves can cross a label.
            backed: true,
        });

        if let Some(tag) = &commit.tag {
            let width = measure.width(tag, label_size, true) + 14.0;
            let top = at.y - DOT_RADIUS - TAG_GAP - tag_height;
            // A stem joining the tag to its commit, under the tag.
            scene.edges.push(plain(
                line(
                    Point::new(at.x, top + tag_height),
                    Point::new(at.x, at.y - DOT_RADIUS),
                ),
                Pen {
                    dash: Dash::Solid,
                    weight: Weight::Thin,
                    paint: color,
                },
            ));
            let mut node = Node::new(
                format!("tag:{index}"),
                Shape::Rounded,
                Rect::new(at.x - width / 2.0, top, width, tag_height),
            );
            node.label = vec![TextLine::new(tag.clone(), Font::Small)];
            node.style.fill = Paint::Role(Role::Background);
            node.style.stroke = color;
            node.style.text = color;
            scene.nodes.push(node);
        }
    }

    // Branch labels in the gutter.
    for (index, branch) in graph.branches.iter().enumerate() {
        let y = lane_y(index);
        let width = measure.width(&branch.name, FONT_SIZE, true) + 18.0;
        let height = FONT_SIZE + 10.0;
        let mut node = Node::new(
            format!("branch:{index}"),
            Shape::Rounded,
            Rect::new(PADDING, y - height / 2.0, width, height),
        )
        .with_label(&branch.name);
        node.style.fill = series(index);
        node.style.stroke = series(index);
        node.style.text = Paint::Role(Role::Background);
        scene.nodes.push(node);
    }

    let height = lane_y(graph.branches.len().saturating_sub(1))
        + DOT_RADIUS
        + LABEL_GAP
        + label_size
        + PADDING;
    scene.width = timeline_end.max(200.0);
    scene.height = height.max(80.0);
    scene
}
