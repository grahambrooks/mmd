//! Requirement layout: each requirement or element as a box of its kind,
//! name and `Key: value` rows, placed by the layered layout; relationships
//! as labelled curves, `contains` solid from the container, the rest
//! dashed with an open arrow.

use diagram_scene::shape::natural_size;
use diagram_scene::{Dash, Font, Marker, Node, Rect, Scene, Shape, TextLine};

use super::parse::{RelationKind, RequirementDiagram};
use crate::FONT_SIZE;
use crate::graph::{self, Diagram, End, Item, Link};
use crate::text::Measure;

const ROW_WIDTH: f32 = 240.0;

pub fn layout(d: &RequirementDiagram, measure: &dyn Measure) -> Scene {
    let items = d
        .blocks
        .iter()
        .map(|block| {
            let mut node = Node::new(block.name.clone(), Shape::Class, Rect::default());
            node.stereotype = vec![TextLine::new(format!("<<{}>>", block.title()), Font::Small)];
            node.label = graph::lines(measure, &block.name, Font::Bold, ROW_WIDTH);
            let rows: Vec<TextLine> = block
                .rows()
                .iter()
                .flat_map(|row| graph::lines(measure, row, Font::Body, ROW_WIDTH))
                .collect();
            if !rows.is_empty() {
                node.compartments = vec![rows];
            }
            graph::paint(&mut node.style, &d.colors(block));
            let (w, h) = natural_size(&node, FONT_SIZE, measure);
            Item {
                node,
                size: (w.max(140.0), h),
                group: None,
                notes: Vec::new(),
            }
        })
        .collect();
    let links = d
        .relations
        .iter()
        .map(|r| {
            let mut link = Link::new(End::Item(r.from), End::Item(r.to));
            link.label = vec![TextLine::new(format!("<<{}>>", r.kind.name()), Font::Small)];
            if r.kind == RelationKind::Contains {
                link.start = Marker::Circle;
                link.end = Marker::None;
            } else {
                link.pen.dash = Dash::Dashed;
                link.end = Marker::OpenArrow;
            }
            link
        })
        .collect();
    graph::layout(
        &Diagram {
            items,
            groups: Vec::new(),
            links,
            direction: d.direction,
        },
        measure,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requirement::parse::parse;
    use diagram_scene::Estimate;

    const SAMPLE: &str = "requirementDiagram
    requirement test_req {
        id: 1
        text: the test text.
        risk: high
        verifymethod: test
    }
    functionalRequirement test_req2 {
        id: 1.1
        text: the second test text.
        risk: low
        verifymethod: inspection
    }
    designConstraint test_req6 {
        id: 1.5
        text: the sixth test text.
        risk: medium
        verifymethod: analysis
    }
    element test_entity {
        type: simulation
        docRef: reqs/test_entity
    }
    test_entity - satisfies -> test_req2
    test_req - contains -> test_req2
    test_req6 - derives -> test_req
    test_entity - verifies -> test_req6
";

    #[test]
    fn nothing_is_dropped_and_boxes_never_overlap() {
        for direction in ["TB", "LR"] {
            let source = SAMPLE.replace(
                "requirementDiagram\n",
                &format!("requirementDiagram\n  direction {direction}\n"),
            );
            let scene = layout(&parse(&source).unwrap(), &Estimate);
            let texts = scene.texts();
            for expected in [
                "<<Requirement>>",
                "test_req",
                "ID: 1",
                "Text: the test text.",
                "Risk: High",
                "Verification: Test",
                "<<Functional Requirement>>",
                "<<Design Constraint>>",
                "Verification: Analysis",
                "<<Element>>",
                "Type: simulation",
                "Doc Ref: reqs/test_entity",
                "<<satisfies>>",
                "<<contains>>",
                "<<derives>>",
                "<<verifies>>",
            ] {
                assert!(
                    texts.iter().any(|t| t == expected),
                    "missing {expected}: {texts:?}"
                );
            }
            let bounds = scene.bounds();
            for (i, a) in scene.nodes.iter().enumerate() {
                assert!(bounds.encloses(&a.rect, 0.01));
                for b in &scene.nodes[i + 1..] {
                    assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
                }
            }
            assert_eq!(scene.edges[1].start, Marker::Circle);
            assert_eq!(scene.edges[0].pen.dash, Dash::Dashed);
        }
    }
}
