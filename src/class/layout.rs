//! Class layout: classes as compartmented boxes (annotations and name over
//! attributes over methods), placed by the layered layout; namespaces as
//! boxes around their classes; relations as curves with UML markers, a
//! label mid-way and cardinalities near each end; notes pinned beside
//! their class.

use diagram_scene::route::Side;
use diagram_scene::shape::natural_size;
use diagram_scene::{ContainerKind, Dash, Font, Marker, Node, Rect, Scene, Shape, Style, TextLine};

use super::parse::{ClassDiagram, Classifier, Head, Member};
use crate::FONT_SIZE;
use crate::graph::{self, Diagram, End, Group, Item, Link, Pinned};
use crate::text::Measure;

const LABEL_WIDTH: f32 = 240.0;
const EDGE_LABEL_WIDTH: f32 = 160.0;
const NOTE_WIDTH: f32 = 200.0;

pub fn layout(d: &ClassDiagram, measure: &dyn Measure) -> Scene {
    let horizontal = d.direction.horizontal();
    let groups = d
        .namespaces
        .iter()
        .map(|n| Group {
            title: vec![TextLine::new(n.label.clone(), Font::Bold)],
            parent: n.parent,
            style: Style::for_container(&ContainerKind::Subgraph),
            block: None,
        })
        .collect();

    let mut items: Vec<Item> = d
        .classes
        .iter()
        .map(|class| {
            let (node, size) = if class.interface {
                let mut node = Node::new(class.id.clone(), Shape::Circle, Rect::default());
                node.label = vec![TextLine::new(class.label.clone(), Font::Small)];
                let (w, h) = natural_size(&node, FONT_SIZE, measure);
                (node, (w, h))
            } else {
                let mut node = Node::new(class.id.clone(), Shape::Class, Rect::default());
                node.stereotype = class
                    .annotations
                    .iter()
                    .map(|a| TextLine::new(format!("<<{a}>>"), Font::Small))
                    .collect();
                node.label = graph::lines(measure, &class.label, Font::Bold, LABEL_WIDTH);
                node.compartments = vec![members(&class.attributes), members(&class.methods)];
                let (w, h) = natural_size(&node, FONT_SIZE, measure);
                (node, (w.max(100.0), h))
            };
            let mut node = node;
            graph::paint(&mut node.style, &d.colors(class));
            Item {
                node,
                size,
                group: class.namespace,
                notes: Vec::new(),
            }
        })
        .collect();

    // Notes: beside their class, across the flow; free ones on their own.
    for (k, note) in d.notes.iter().enumerate() {
        let mut node = Node::new(format!("note {k}"), Shape::Note, Rect::default());
        node.label = graph::lines(measure, &note.text, Font::Body, NOTE_WIDTH);
        let size = natural_size(&node, FONT_SIZE, measure);
        match note.class {
            Some(c) => items[c].notes.push(Pinned {
                node,
                size,
                side: if horizontal {
                    Side::Bottom
                } else {
                    Side::Right
                },
            }),
            None => items.push(Item {
                node,
                size,
                group: note.namespace,
                notes: Vec::new(),
            }),
        }
    }

    let links = d
        .relations
        .iter()
        .map(|r| {
            let mut link = Link::new(End::Item(r.from), End::Item(r.to));
            link.start = marker(r.from_head);
            link.end = marker(r.to_head);
            if r.dashed {
                link.pen.dash = Dash::Dashed;
            }
            if let Some(label) = &r.label {
                link.label = graph::lines(measure, label, Font::Small, EDGE_LABEL_WIDTH);
            }
            link.from_label = r
                .from_card
                .as_ref()
                .map(|c| TextLine::new(c.clone(), Font::Small));
            link.to_label = r
                .to_card
                .as_ref()
                .map(|c| TextLine::new(c.clone(), Font::Small));
            link
        })
        .collect();

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

fn members(members: &[Member]) -> Vec<TextLine> {
    members
        .iter()
        .map(|m| match m.classifier {
            Classifier::Abstract => TextLine::new(m.text.clone(), Font::Italic),
            // No underline in the scene: a static member keeps its `$`.
            Classifier::Static => TextLine::new(format!("{}$", m.text), Font::Body),
            Classifier::None => TextLine::new(m.text.clone(), Font::Body),
        })
        .collect()
}

fn marker(head: Head) -> Marker {
    match head {
        Head::None | Head::Lollipop => Marker::None,
        Head::Inheritance => Marker::HollowTriangle,
        Head::Composition => Marker::FilledDiamond,
        Head::Aggregation => Marker::HollowDiamond,
        Head::Dependency => Marker::Arrow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::parse::parse;
    use diagram_scene::{Estimate, Paint, Role};

    const SAMPLE: &str = "classDiagram
    namespace Zoo {
        class Animal {
            <<abstract>>
            +String name
            +int age$
            +makeSound()* void
        }
        class Duck {
            +swim()
        }
        class Fish
    }
    Animal <|-- Duck
    Animal <|-- Fish
    Zebra --|> Animal
    Keeper \"1\" --> \"*\" Animal : feeds
    Keeper *-- Badge
    Keeper o-- Shift
    Shift ..> Rota : uses
    Rota ..|> Plan
    Plan .. Calendar
    Badge -- Badge : renews
    IFeed ()-- Keeper
    note for Duck \"can fly\"
    note \"Everything is an Animal\"
";

    fn scene(source: &str) -> Scene {
        layout(&parse(source).unwrap(), &Estimate)
    }

    #[test]
    fn nothing_is_dropped() {
        let scene = scene(SAMPLE);
        let texts = scene.texts();
        for expected in [
            "Zoo",
            "<<abstract>>",
            "Animal",
            "+String name",
            "+int age$",
            "+makeSound() : void",
            "Duck",
            "+swim()",
            "Fish",
            "Zebra",
            "Keeper",
            "1",
            "*",
            "feeds",
            "Badge",
            "Shift",
            "uses",
            "Rota",
            "Plan",
            "Calendar",
            "renews",
            "IFeed",
            "can fly",
            "Everything is an Animal",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
        assert_eq!(
            scene
                .edges
                .iter()
                .filter(|e| e.pen.paint != Paint::Role(Role::Muted))
                .count(),
            11
        );
        assert_eq!(scene.edges[0].start, Marker::HollowTriangle);
        assert_eq!(scene.edges[2].end, Marker::HollowTriangle);
        assert_eq!(scene.edges[4].start, Marker::FilledDiamond);
        assert_eq!(scene.edges[5].start, Marker::HollowDiamond);
        assert_eq!(scene.edges[6].pen.dash, Dash::Dashed);
    }

    #[test]
    fn classes_never_overlap_and_namespaces_hold_their_members() {
        for direction in ["TB", "LR", "BT", "RL"] {
            let source = SAMPLE.replace(
                "classDiagram\n",
                &format!("classDiagram\n  direction {direction}\n"),
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
            let zoo = scene.containers[0].rect;
            for id in ["Animal", "Duck", "Fish"] {
                assert!(
                    zoo.encloses(&scene.node(id).unwrap().rect, 0.01),
                    "{direction}: {id}"
                );
            }
            for id in ["Keeper", "Zebra"] {
                assert!(
                    !zoo.intersects(&scene.node(id).unwrap().rect),
                    "{direction}: {id}"
                );
            }
            for t in &scene.texts {
                assert!(bounds.contains(t.pos), "{direction}: text outside");
            }
            for e in &scene.edges {
                for poly in diagram_scene::flatten::flatten(&e.path, 0.5) {
                    for p in poly.points {
                        assert!(bounds.contains(p), "{direction}: {p:?} outside");
                    }
                }
            }
        }
    }

    #[test]
    fn cardinalities_sit_near_their_ends() {
        let scene = scene(SAMPLE);
        let feeds = scene
            .edges
            .iter()
            .find(|e| e.labels.iter().any(|l| l.lines[0].plain() == "feeds"))
            .unwrap();
        let (start, _) = diagram_scene::flatten::start(&feeds.path).unwrap();
        let (end, _) = diagram_scene::flatten::end(&feeds.path).unwrap();
        let one = scene.texts.iter().find(|t| t.line.plain() == "1").unwrap();
        let many = scene.texts.iter().find(|t| t.line.plain() == "*").unwrap();
        assert!(
            one.pos.distance(start) < 45.0,
            "{:?} far from {start:?}",
            one.pos
        );
        assert!(
            many.pos.distance(end) < 45.0,
            "{:?} far from {end:?}",
            many.pos
        );
    }
}
