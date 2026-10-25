//! State layout: states as rounded boxes (a name over a rule over its
//! descriptions when it has several), start and end dots, choice
//! diamonds and fork bars; composite states as boxes laid out on their own
//! that transitions may end on, their concurrency regions as dashed boxes
//! side by side inside them; notes pinned beside their state.

use diagram_scene::route::Side;
use diagram_scene::shape::natural_size;
use diagram_scene::{ContainerKind, Dash, Font, Node, Paint, Rect, Role, Scene, Shape, Style};

use super::parse::{Direction, Kind, Side as NoteSide, StateDiagram};
use crate::FONT_SIZE;
use crate::graph::{self, Diagram, End, Group, Item, Link, Pinned};
use crate::text::Measure;

/// Labels wrap at this width.
const LABEL_WIDTH: f32 = 220.0;
const EDGE_LABEL_WIDTH: f32 = 160.0;
const NOTE_WIDTH: f32 = 180.0;

pub fn layout(d: &StateDiagram, measure: &dyn Measure) -> Scene {
    // Each scope's direction: its composite's, else its parent's.
    let mut directions: Vec<Direction> = Vec::with_capacity(d.groups.len());
    for g in &d.groups {
        let parent = g.parent.map_or(d.direction, |p| directions[p]);
        directions.push(g.direction.unwrap_or(parent));
    }
    let scope_direction = |g: Option<usize>| g.map_or(d.direction, |g| directions[g]);

    let groups: Vec<Group> = d
        .groups
        .iter()
        .enumerate()
        .map(|(g, group)| match group.state {
            Some(s) => {
                let state = &d.states[s];
                let mut style = Style::for_container(&ContainerKind::Subgraph);
                style.text = Paint::Role(Role::Text);
                graph::paint(&mut style, &d.colors(state));
                let text = match state.descriptions.len() {
                    0 | 1 => state.label().to_string(),
                    _ => format!("{}\n{}", state.id, state.descriptions.join("\n")),
                };
                let title = graph::lines(measure, &text, Font::Bold, f32::INFINITY);
                Group {
                    title,
                    parent: group.parent,
                    style,
                    block: Some(directions[g]),
                }
            }
            None => Group {
                title: Vec::new(),
                parent: group.parent,
                style: Style {
                    fill: Paint::None,
                    dash: Dash::Dashed,
                    ..Style::for_container(&ContainerKind::Subgraph)
                },
                block: Some(directions[g]),
            },
        })
        .collect();

    let mut items = Vec::new();
    // Each state's end: its item, or its composite's box.
    let mut ends = Vec::with_capacity(d.states.len());
    // Notes on composite states become items of their own, linked to it.
    let mut loose_notes = Vec::new();
    for state in &d.states {
        let horizontal = scope_direction(state.group).horizontal();
        let notes: Vec<Pinned> = state
            .notes
            .iter()
            .enumerate()
            .map(|(k, n)| {
                let mut node = Node::new(
                    format!("{} note {k}", state.id),
                    Shape::Note,
                    Rect::default(),
                );
                node.label = graph::lines(measure, &n.text, Font::Body, NOTE_WIDTH);
                let size = natural_size(&node, FONT_SIZE, measure);
                // Across the flow, so transitions along it stay clear.
                let side = match (n.side, horizontal) {
                    (NoteSide::Left, false) => Side::Left,
                    (NoteSide::Right, false) => Side::Right,
                    (NoteSide::Left, true) => Side::Top,
                    (NoteSide::Right, true) => Side::Bottom,
                };
                Pinned { node, size, side }
            })
            .collect();
        if let Some(g) = state.composite {
            ends.push(End::Group(g));
            loose_notes.push((g, state.group, notes));
            continue;
        }
        let (node, size) = draw(d, state, horizontal, measure);
        ends.push(End::Item(items.len()));
        items.push(Item {
            node,
            size,
            group: state.group,
            notes,
        });
    }

    let mut links: Vec<Link> = d
        .transitions
        .iter()
        .map(|t| {
            let mut link = Link::new(ends[t.from], ends[t.to]);
            if let Some(label) = &t.label {
                link.label = graph::lines(measure, label, Font::Small, EDGE_LABEL_WIDTH);
            }
            link
        })
        .collect();
    for (g, parent, notes) in loose_notes {
        for note in notes {
            let item = items.len();
            items.push(Item {
                node: note.node,
                size: note.size,
                group: parent,
                notes: Vec::new(),
            });
            let mut link = Link::new(End::Group(g), End::Item(item));
            link.end = diagram_scene::Marker::None;
            link.pen.dash = Dash::Dashed;
            link.pen.paint = Paint::Role(Role::Muted);
            links.push(link);
        }
    }

    graph::layout(
        &Diagram {
            items,
            groups,
            links,
            direction: d.direction,
        },
        measure,
    )
}

/// A state's node and its size.
fn draw(
    d: &StateDiagram,
    state: &super::parse::State,
    horizontal: bool,
    measure: &dyn Measure,
) -> (Node, (f32, f32)) {
    let shape = match state.kind {
        Kind::Start => Shape::Start,
        Kind::End => Shape::End,
        Kind::Choice => Shape::Diamond,
        Kind::Fork | Kind::Join => Shape::ForkBar,
        Kind::Note => Shape::Note,
        Kind::Normal if state.descriptions.len() > 1 => Shape::Class,
        Kind::Normal => Shape::Rounded,
    };
    let mut node = Node::new(state.id.clone(), shape, Rect::default());
    match shape {
        Shape::Class => {
            node.label = graph::lines(measure, &state.id, Font::Body, LABEL_WIDTH);
            node.compartments = vec![
                state
                    .descriptions
                    .iter()
                    .flat_map(|t| graph::lines(measure, t, Font::Body, LABEL_WIDTH))
                    .collect(),
            ];
        }
        Shape::Rounded | Shape::Note => {
            node.label = graph::lines(measure, state.label(), Font::Body, LABEL_WIDTH);
        }
        _ => {}
    }
    graph::paint(&mut node.style, &d.colors(state));
    let size = match shape {
        Shape::Diamond => (28.0, 28.0),
        Shape::ForkBar if horizontal => (6.0, 70.0),
        Shape::ForkBar => (70.0, 6.0),
        Shape::Start | Shape::End => natural_size(&node, FONT_SIZE, measure),
        _ => {
            let (w, h) = natural_size(&node, FONT_SIZE, measure);
            (w.max(60.0), h.max(36.0))
        }
    };
    (node, size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::parse::parse;
    use diagram_scene::Estimate;

    const SAMPLE: &str = "stateDiagram-v2
    [*] --> Idle
    Idle --> Active : start
    state Active {
        [*] --> Working
        Working --> Paused : pause
        Paused --> Working : resume
        --
        [*] --> Watching
        Watching --> Watching : tick
    }
    Active --> Done : finish
    state check <<choice>>
    Done --> check
    check --> Idle : again
    check --> [*] : quit
    state fork_state <<fork>>
    Idle --> fork_state
    fork_state --> A
    fork_state --> B
    state join_state <<join>>
    A --> join_state
    B --> join_state
    join_state --> Done
    note right of Idle : waiting for work
    note left of Active
        two regions
    end note
    Done : finished
    Done : cleaned up
";

    fn scene(source: &str) -> Scene {
        layout(&parse(source).unwrap(), &Estimate)
    }

    #[test]
    fn nothing_is_dropped() {
        let scene = scene(SAMPLE);
        let texts = scene.texts();
        for expected in [
            "Idle",
            "Active",
            "Working",
            "Paused",
            "Watching",
            "start",
            "pause",
            "resume",
            "tick",
            "finish",
            "again",
            "quit",
            "waiting for work",
            "two regions",
            "Done",
            "finished",
            "cleaned up",
            "A",
            "B",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
        let starts = scene
            .nodes
            .iter()
            .filter(|n| n.shape == Shape::Start)
            .count();
        assert_eq!(starts, 3);
        assert_eq!(
            scene
                .nodes
                .iter()
                .filter(|n| n.shape == Shape::ForkBar)
                .count(),
            2
        );
        assert_eq!(
            scene
                .nodes
                .iter()
                .filter(|n| n.shape == Shape::Diamond)
                .count(),
            1
        );
        // Two regions inside the composite.
        assert_eq!(scene.containers.len(), 3);
    }

    #[test]
    fn states_never_overlap_and_composites_hold_their_members() {
        for direction in ["TB", "LR", "BT", "RL"] {
            let source = SAMPLE.replace(
                "stateDiagram-v2\n",
                &format!("stateDiagram-v2\n  direction {direction}\n"),
            );
            let scene = scene(&source);
            let bounds = scene.bounds();
            for (i, a) in scene.nodes.iter().enumerate() {
                assert!(
                    bounds.encloses(&a.rect, 0.01),
                    "{direction}: {} outside",
                    a.id
                );
                for b in &scene.nodes[i + 1..] {
                    assert!(
                        !a.rect.intersects(&b.rect),
                        "{direction}: {} overlaps {}",
                        a.id,
                        b.id
                    );
                }
            }
            let active = scene.containers[0].rect;
            for id in ["Working", "Paused", "Watching"] {
                assert!(
                    active.encloses(&scene.node(id).unwrap().rect, 0.01),
                    "{direction}: {id}"
                );
            }
            for id in ["Idle", "Done"] {
                assert!(
                    !active.intersects(&scene.node(id).unwrap().rect),
                    "{direction}: {id}"
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
        }
    }

    #[test]
    fn every_keyword_and_alias_is_detected() {
        use crate::{Kind, detect};
        for (source, kind) in [
            ("stateDiagram-v2\n", Kind::State),
            ("stateDiagram\n", Kind::State),
            ("classDiagram\n", Kind::Class),
            ("classDiagram-v2\n", Kind::Class),
            ("requirementDiagram\n", Kind::Requirement),
            ("requirement\n", Kind::Requirement),
        ] {
            assert_eq!(detect(source), Some(kind), "{source}");
            crate::render_with(source, &diagram_scene::Theme::default(), &Estimate).unwrap();
        }
    }

    #[test]
    fn a_note_sits_beside_its_state() {
        let scene = scene(SAMPLE);
        let idle = scene.node("Idle").unwrap().rect;
        let note = scene.node("Idle note 0").unwrap().rect;
        assert!(note.x >= idle.right());
        assert!(note.y < idle.bottom() && note.bottom() > idle.y);
    }
}
