//! Event model layout, after mermaid's `eventmodeling/db.ts`: one swimlane
//! per kind (UI/automation, commands and read models, events), split by
//! namespace; frames left to right in source order, each overlapping the
//! one before when it changes lane, so the flow reads as a staircase.

use diagram_scene::shape::natural_size;
use diagram_scene::{
    Anchor, Container, ContainerKind, Edge, Font, Node, Paint, Point, Rect, Role, Scene, Seg,
    Shape, Style, Text, TextLine,
};

use super::parse::{EntityType, EventModel, Frame};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const LANE_PAD: f32 = 14.0;
const LANE_GAP: f32 = 8.0;
const BOX_GAP: f32 = 16.0;
const MIN_W: f32 = 110.0;
const MIN_H: f32 = 56.0;
const INK: Paint = Paint::Rgb(0x1f, 0x23, 0x28);

/// Mermaid's default fill and stroke for each kind.
fn colours(kind: EntityType) -> (Paint, Paint) {
    match kind {
        EntityType::Ui => (Paint::Rgb(0xff, 0xff, 0xff), Paint::Rgb(0xdb, 0xda, 0xda)),
        EntityType::Processor => (Paint::Rgb(0xed, 0xb3, 0xf6), Paint::Rgb(0xb8, 0x8c, 0xbf)),
        EntityType::ReadModel => (Paint::Rgb(0xd3, 0xf1, 0xa2), Paint::Rgb(0xa3, 0xb7, 0x32)),
        EntityType::Command => (Paint::Rgb(0xbc, 0xd6, 0xfe), Paint::Rgb(0x67, 0x9a, 0xc3)),
        EntityType::Event => (Paint::Rgb(0xff, 0xb7, 0x78), Paint::Rgb(0xc1, 0x9a, 0x0f)),
    }
}

/// Which band a frame's lane sits in, top to bottom.
fn band(kind: EntityType) -> usize {
    match kind {
        EntityType::Ui | EntityType::Processor => 0,
        EntityType::Command | EntityType::ReadModel => 1,
        EntityType::Event => 2,
    }
}

fn lane_label(band: usize, namespace: Option<&str>) -> String {
    match (band, namespace) {
        (0, None) => "UI/Automation".into(),
        (0, Some(ns)) => format!("UI/A: {ns}"),
        (1, None) => "Command/Read Model".into(),
        (1, Some(ns)) => format!("C/RM: {ns}"),
        (_, None) => "Events".into(),
        (_, Some(ns)) => format!("Stream: {ns}"),
    }
}

fn frame_node(frame: &Frame, measure: &dyn Measure) -> Node {
    let has_data = !frame.data.is_empty();
    let shape = if has_data {
        Shape::Table
    } else {
        Shape::Rounded
    };
    let mut node = Node::new(format!("frame:{}", frame.name), shape, Rect::default());
    node.label = vec![TextLine::new(frame.short_name(), Font::Bold)];
    if has_data {
        node.compartments = vec![
            frame
                .data
                .iter()
                .map(|l| TextLine::new(l.clone(), Font::Mono))
                .collect(),
        ];
    }
    let (fill, stroke) = colours(frame.kind);
    node.style.fill = fill;
    node.style.stroke = stroke;
    node.style.text = INK;
    let (w, h) = natural_size(&node, FONT_SIZE, measure);
    node.rect = Rect::new(0.0, 0.0, w.max(MIN_W), h.max(MIN_H));
    node
}

pub fn layout(model: &EventModel, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let title_size = Font::Title.size(FONT_SIZE);
    let top = PADDING
        + if model.title.is_some() {
            title_size + PADDING
        } else {
            0.0
        };

    // Lanes, in band order then first appearance.
    let mut lanes: Vec<(usize, Option<String>)> = Vec::new();
    for frame in &model.frames {
        let key = (band(frame.kind), frame.namespace().map(String::from));
        if !lanes.contains(&key) {
            lanes.push(key);
        }
    }
    lanes.sort_by_key(|(band, _)| *band);
    let lane_of = |frame: &Frame| {
        let key = (band(frame.kind), frame.namespace().map(String::from));
        lanes.iter().position(|l| *l == key).unwrap_or(0)
    };
    let labels: Vec<String> = lanes
        .iter()
        .map(|(band, ns)| lane_label(*band, ns.as_deref()))
        .collect();
    let gutter = labels
        .iter()
        .map(|l| measure.width(l, FONT_SIZE, true))
        .fold(0.0_f32, f32::max)
        + 30.0;

    // Frame boxes, x first; heights fix each lane's height after.
    let mut nodes: Vec<Node> = model
        .frames
        .iter()
        .map(|f| frame_node(f, measure))
        .collect();
    let mut lane_right = vec![PADDING + gutter; lanes.len()];
    let mut previous: Option<(usize, Rect)> = None;
    for (i, frame) in model.frames.iter().enumerate() {
        let lane = lane_of(frame);
        let w = nodes[i].rect.width;
        let after_lane = lane_right[lane] + BOX_GAP;
        let x = match previous {
            Some((prev_lane, _)) if prev_lane == lane => after_lane,
            Some((_, prev)) => after_lane.max(prev.right() - prev.width.min(w) * 0.35),
            None => after_lane,
        };
        nodes[i].rect.x = x;
        lane_right[lane] = x + w;
        previous = Some((lane, nodes[i].rect));
    }
    let mut lane_y = Vec::with_capacity(lanes.len());
    let mut lane_h = Vec::with_capacity(lanes.len());
    let mut y = top;
    for lane in 0..lanes.len() {
        let tallest = model
            .frames
            .iter()
            .zip(&nodes)
            .filter(|(f, _)| lane_of(f) == lane)
            .map(|(_, n)| n.rect.height)
            .fold(0.0_f32, f32::max);
        let h = tallest + LANE_PAD * 2.0 + FONT_SIZE;
        lane_y.push(y);
        lane_h.push(h);
        y += h + LANE_GAP;
    }
    for (frame, node) in model.frames.iter().zip(nodes.iter_mut()) {
        node.rect.y = lane_y[lane_of(frame)] + LANE_PAD + FONT_SIZE;
    }
    let content_right = lane_right.iter().copied().fold(PADDING + gutter, f32::max);

    // Notes and scenarios, in a band of their own under the lanes, each
    // under its frame's column.
    let mut note_nodes = Vec::new();
    let notes_top = y;
    let mut notes_right = content_right;
    let mut notes_bottom = y - LANE_GAP;
    if !model.notes.is_empty() {
        let mut cursor = PADDING + gutter;
        let mut tallest: f32 = 0.0;
        // Left to right by their frames, so each can sit under its own.
        let mut order: Vec<usize> = (0..model.notes.len()).collect();
        order.sort_by(|&a, &b| {
            let x = |i: usize| nodes[model.notes[i].frame].rect.x;
            x(a).total_cmp(&x(b))
        });
        for i in order {
            let note = &model.notes[i];
            let frame = &model.frames[note.frame];
            let mut node = Node::new(format!("note:{i}"), Shape::Note, Rect::default());
            let kind = if note.scenario { "gwt" } else { "note" };
            node.stereotype = vec![TextLine::new(
                format!("{kind} · {} {}", frame.name, frame.short_name()),
                Font::Small,
            )];
            node.label = note
                .lines
                .iter()
                .map(|l| TextLine::new(l.clone(), Font::Small))
                .collect();
            let (w, h) = natural_size(&node, FONT_SIZE, measure);
            let x = nodes[note.frame].rect.x.max(cursor);
            node.rect = Rect::new(x, notes_top + LANE_PAD + FONT_SIZE, w, h);
            cursor = x + w + BOX_GAP;
            tallest = tallest.max(h);
            note_nodes.push(node);
        }
        notes_right = notes_right.max(cursor - BOX_GAP);
        notes_bottom = notes_top + tallest + LANE_PAD * 2.0 + FONT_SIZE;
    }

    let width = content_right.max(notes_right) + LANE_PAD + PADDING;
    let lane_width = width - PADDING * 2.0;
    for (lane, label) in labels.iter().enumerate() {
        scene.containers.push(Container {
            rect: Rect::new(PADDING, lane_y[lane], lane_width, lane_h[lane]),
            kind: ContainerKind::Group,
            label: vec![TextLine::new(label.clone(), Font::Bold)],
            style: Style {
                fill: Paint::Role(Role::Surface),
                ..Style::for_container(&ContainerKind::Group)
            },
        });
    }
    if !note_nodes.is_empty() {
        scene.containers.push(Container {
            rect: Rect::new(PADDING, notes_top, lane_width, notes_bottom - notes_top),
            kind: ContainerKind::Group,
            label: vec![TextLine::new("Notes", Font::Bold)],
            style: Style {
                dash: diagram_scene::Dash::Dashed,
                ..Style::for_container(&ContainerKind::Group)
            },
        });
    }

    // Relations: each frame from its `->>` sources, or else from the
    // latest earlier frame in another lane. Reset frames and the opening
    // frame start afresh, as in mermaid.
    for (i, frame) in model.frames.iter().enumerate() {
        if frame.reset || (i == 0 && frame.sources.is_empty()) {
            continue;
        }
        let sources: Vec<usize> = if frame.sources.is_empty() {
            let lane = lane_of(frame);
            (0..i)
                .rev()
                .find(|&j| lane_of(&model.frames[j]) != lane)
                .into_iter()
                .collect()
        } else {
            frame.sources.clone()
        };
        for source in sources {
            let (a, b) = (nodes[source].rect, nodes[i].rect);
            let (start, end) = if (a.y - b.y).abs() < 0.5 {
                if a.x <= b.x {
                    (
                        Point::new(a.right(), a.center().y),
                        Point::new(b.x, b.center().y),
                    )
                } else {
                    (
                        Point::new(a.x, a.center().y),
                        Point::new(b.right(), b.center().y),
                    )
                }
            } else if a.y < b.y {
                (
                    Point::new(a.x + a.width * 2.0 / 3.0, a.bottom()),
                    Point::new(b.x + b.width / 3.0, b.y),
                )
            } else {
                (
                    Point::new(a.x + a.width * 2.0 / 3.0, a.y),
                    Point::new(b.x + b.width / 3.0, b.bottom()),
                )
            };
            scene.edges.push(
                Edge::new(vec![Seg::Move(start), Seg::Line(end)])
                    .between(&nodes[source].id, &nodes[i].id),
            );
        }
    }

    scene.nodes = nodes;
    scene.nodes.extend(note_nodes);
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
    scene.height = notes_bottom.max(top) + PADDING;
    scene
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;
    use diagram_scene::Estimate;

    #[test]
    fn every_frame_note_and_lane_reaches_the_scene_without_overlap() {
        let model = parse(
            "eventmodeling\ntitle Cart\ntf 01 ui CartUI\ntf 02 cmd AddItem { productId: 7 }\n\
             tf 03 evt Inventory.ItemAdded\ntf 04 rmo CartItems\ntf 05 ui CartUI\n\
             tf 06 cmd RemoveItem ->> 05\ntf 07 evt ItemRemoved\nrf 08 rmo CartItems\n\
             note 02 {\n  only signed in\n}\ngwt 07 given evt ItemAdded then evt ItemRemoved\n",
        )
        .unwrap();
        let scene = layout(&model, &Estimate);
        let texts = scene.texts();
        for expected in [
            "Cart",
            "CartUI",
            "AddItem",
            "productId: 7",
            "ItemAdded",
            "UI/Automation",
            "Command/Read Model",
            "Events",
            "Stream: Inventory",
            "only signed in",
            "Then event ItemRemoved",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
        let frames: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| n.id.starts_with("frame:"))
            .collect();
        assert_eq!(frames.len(), model.frames.len());
        for (i, a) in scene.nodes.iter().enumerate() {
            assert!(scene.bounds().encloses(&a.rect, 0.01), "{a:?}");
            for b in &scene.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
        // The opening and reset frames start afresh; the rest have a source.
        assert_eq!(scene.edges.len(), model.frames.len() - 2);
    }
}
