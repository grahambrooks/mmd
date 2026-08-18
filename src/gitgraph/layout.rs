//! gitGraph layout: commits along a shared timeline, one lane per branch.

use super::parse::{CommitType, GitGraph};
use crate::geom::{Point, Rect};
use crate::scene::{Anchor, Item, PathData, Scene, Stroke};
use crate::text::Measure;
use crate::theme::Theme;

const LANE_HEIGHT: f32 = 52.0;
const DOT_RADIUS: f32 = 9.0;
const LANE_LINE_WIDTH: f32 = 6.0;
const MIN_SLOT: f32 = 68.0;
const LABEL_GAP: f32 = 16.0;
const TAG_GAP: f32 = 14.0;

/// Lay a parsed graph out, returning the scene and its bounds.
pub fn layout(graph: &GitGraph, theme: &Theme, measure: &dyn Measure) -> (Scene, Rect) {
    let mut scene = Scene::default();
    let label_size = theme.small_size;

    // Branch labels sit in a gutter to the left of the timeline.
    let gutter = graph
        .branches
        .iter()
        .map(|branch| measure.width(&branch.name, theme.font_size, true) + 20.0)
        .fold(0.0_f32, f32::max);
    let origin_x = gutter + theme.padding * 2.0;
    // Tags hang above their commit and labels below it, so the first lane must
    // sit low enough for a tag to fit inside the bounds.
    let tag_height = label_size + 8.0;
    let has_tags = graph.commits.iter().any(|commit| commit.tag.is_some());
    let headroom = if has_tags {
        tag_height + TAG_GAP + DOT_RADIUS
    } else {
        DOT_RADIUS
    };
    let origin_y = theme.padding + headroom;

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
    let timeline_end = x + theme.padding;

    let lane_y = |branch: usize| origin_y + branch as f32 * LANE_HEIGHT;
    let position = |index: usize| Point::new(xs[index], lane_y(graph.commits[index].branch));

    // Lane rules first, so commits and edges paint over them.
    for (index, _) in graph.branches.iter().enumerate() {
        let y = lane_y(index);
        scene.push(Item::Path {
            data: PathData::new()
                .move_to(Point::new(origin_x - theme.padding, y))
                .line_to(Point::new(timeline_end, y))
                .finish(),
            fill: None,
            stroke: Some(Stroke {
                color: theme.line,
                width: 1.0,
                dashed: true,
            }),
            arrow: false,
        });
    }

    // Edges from each commit back to its parents.
    for (index, commit) in graph.commits.iter().enumerate() {
        let to = position(index);
        for &parent in &commit.parents {
            let from = position(parent);
            let color = theme.series_color(graph.commits[parent].branch.max(commit.branch));
            let data = if (from.y - to.y).abs() < f32::EPSILON {
                PathData::new().move_to(from).line_to(to).finish()
            } else {
                // An S-curve between lanes, turning near the later commit.
                let midpoint = (from.x + to.x) / 2.0;
                PathData::new()
                    .move_to(from)
                    .curve_to(Point::new(midpoint, from.y), Point::new(midpoint, to.y), to)
                    .finish()
            };
            scene.push(Item::Path {
                data,
                fill: None,
                stroke: Some(Stroke {
                    color,
                    width: LANE_LINE_WIDTH,
                    dashed: false,
                }),
                arrow: false,
            });
        }
    }

    // Commits.
    for (index, commit) in graph.commits.iter().enumerate() {
        let at = position(index);
        let color = theme.series_color(commit.branch);
        let is_merge = commit.parents.len() > 1;

        match commit.kind {
            CommitType::Highlight => {
                scene.push(Item::Rect {
                    rect: Rect::new(
                        at.x - DOT_RADIUS,
                        at.y - DOT_RADIUS,
                        DOT_RADIUS * 2.0,
                        DOT_RADIUS * 2.0,
                    ),
                    radius: 2.0,
                    fill: Some(color),
                    stroke: Some(Stroke {
                        color: theme.text,
                        width: 2.0,
                        dashed: false,
                    }),
                });
            }
            CommitType::Reverse => {
                scene.push(Item::Circle {
                    center: at,
                    radius: DOT_RADIUS,
                    fill: Some(theme.background),
                    stroke: Some(Stroke {
                        color,
                        width: 2.5,
                        dashed: false,
                    }),
                });
                let arm = DOT_RADIUS * 0.55;
                scene.push(Item::Path {
                    data: PathData::new()
                        .move_to(Point::new(at.x - arm, at.y - arm))
                        .line_to(Point::new(at.x + arm, at.y + arm))
                        .move_to(Point::new(at.x + arm, at.y - arm))
                        .line_to(Point::new(at.x - arm, at.y + arm))
                        .finish(),
                    fill: None,
                    stroke: Some(Stroke {
                        color,
                        width: 2.0,
                        dashed: false,
                    }),
                    arrow: false,
                });
            }
            CommitType::Normal => {
                scene.push(Item::Circle {
                    center: at,
                    radius: DOT_RADIUS,
                    fill: Some(color),
                    stroke: None,
                });
                if is_merge {
                    // A merge reads as a ring, as it does in mermaid.
                    scene.push(Item::Circle {
                        center: at,
                        radius: DOT_RADIUS * 0.45,
                        fill: Some(theme.background),
                        stroke: None,
                    });
                }
            }
        }

        let label_width = measure.width(&commit.id, label_size, false);
        let label_y = at.y + DOT_RADIUS + LABEL_GAP;
        scene.push(Item::Rect {
            rect: Rect::new(
                at.x - label_width / 2.0 - 3.0,
                label_y - label_size,
                label_width + 6.0,
                label_size + 5.0,
            ),
            radius: 0.0,
            fill: Some(theme.background),
            stroke: None,
        });
        scene.push(Item::Text {
            at: Point::new(at.x, label_y),
            text: commit.id.clone(),
            size: label_size,
            bold: false,
            anchor: Anchor::Middle,
            fill: theme.muted,
            rotate: 0.0,
        });

        if let Some(tag) = &commit.tag {
            let width = measure.width(tag, label_size, true) + 14.0;
            let height = tag_height;
            let top = at.y - DOT_RADIUS - TAG_GAP - height;
            scene.push(Item::Rect {
                rect: Rect::new(at.x - width / 2.0, top, width, height),
                radius: 3.0,
                fill: Some(theme.background),
                stroke: Some(Stroke {
                    color,
                    width: 1.2,
                    dashed: false,
                }),
            });
            scene.push(Item::Text {
                at: Point::new(at.x, top + height - 6.0),
                text: tag.clone(),
                size: label_size,
                bold: true,
                anchor: Anchor::Middle,
                fill: color,
                rotate: 0.0,
            });
            // A stem joining the tag to its commit.
            scene.push(Item::Path {
                data: PathData::new()
                    .move_to(Point::new(at.x, top + height))
                    .line_to(Point::new(at.x, at.y - DOT_RADIUS))
                    .finish(),
                fill: None,
                stroke: Some(Stroke {
                    color,
                    width: 1.0,
                    dashed: false,
                }),
                arrow: false,
            });
        }
    }

    // Branch labels in the gutter.
    for (index, branch) in graph.branches.iter().enumerate() {
        let y = lane_y(index);
        let width = measure.width(&branch.name, theme.font_size, true) + 18.0;
        let height = theme.font_size + 10.0;
        scene.push(Item::Rect {
            rect: Rect::new(theme.padding, y - height / 2.0, width, height),
            radius: theme.corner_radius,
            fill: Some(theme.series_color(index)),
            stroke: None,
        });
        scene.push(Item::Text {
            at: Point::new(theme.padding + width / 2.0, y + theme.font_size * 0.35),
            text: branch.name.clone(),
            size: theme.font_size,
            bold: true,
            anchor: Anchor::Middle,
            fill: theme.series_text,
            rotate: 0.0,
        });
    }

    let height = lane_y(graph.branches.len().saturating_sub(1))
        + DOT_RADIUS
        + LABEL_GAP
        + label_size
        + theme.padding;
    let bounds = Rect::new(0.0, 0.0, timeline_end.max(200.0), height.max(80.0));
    (scene, bounds)
}
