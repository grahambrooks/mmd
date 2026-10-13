//! C4 layout: elements on a grid, relationships as curves between the
//! sides that face each other.
//!
//! Mermaid's own C4 renderer routes relationship labels through the boxes it
//! just drew. Here labels sit on a pill at the curve's middle, which stays
//! legible where it crosses a line.

use diagram_scene::shape::natural_size;
use diagram_scene::{
    Anchor, Edge, EdgeLabel, Font, Node, Paint, Point, Rect, Role, Scene, Shape, Text, TextLine,
    route,
};

use super::parse::{C4, ElementKind};
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const BOX_WIDTH: f32 = 190.0;
const COLUMN_GAP: f32 = 130.0;
const ROW_GAP: f32 = 96.0;
const COLUMNS: usize = 3;

pub fn layout(model: &C4, measure: &dyn Measure) -> Scene {
    let small = Font::Small.size(FONT_SIZE);
    let inner_width = BOX_WIDTH - 24.0;
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;

    // Each element as a node, its text wrapped to the box width; the
    // node's height follows its content.
    let mut nodes: Vec<Node> = model
        .elements
        .iter()
        .map(|element| {
            let (shape, role) = match element.kind {
                ElementKind::Person => (Shape::Person, Role::Person),
                ElementKind::PersonExt => (Shape::Person, Role::External),
                ElementKind::System => (Shape::Rounded, Role::System),
                ElementKind::SystemExt => (Shape::Rounded, Role::External),
                ElementKind::SystemDb => (Shape::Cylinder, Role::Database),
            };
            let mut node = Node::new(element.alias.clone(), shape, Rect::default());
            node.stereotype = vec![TextLine::new(
                format!("\u{ab}{}\u{bb}", element.kind.stereotype()),
                Font::Small,
            )];
            node.label = wrap(measure, &element.label, FONT_SIZE, true, inner_width)
                .into_iter()
                .map(|line| TextLine::new(line, Font::Bold))
                .collect();
            node.sublabel = element
                .description
                .as_ref()
                .map(|text| {
                    wrap(measure, text, small, false, inner_width)
                        .into_iter()
                        .map(|line| TextLine::new(line, Font::Small))
                        .collect()
                })
                .unwrap_or_default();
            node.style.fill = Paint::Role(role);
            node
        })
        .collect();

    let title_height = if model.title.is_some() {
        Font::Title.size(FONT_SIZE) + PADDING
    } else {
        0.0
    };
    let top = PADDING + title_height;

    // Rows are as tall as their tallest element.
    let heights: Vec<f32> = nodes
        .iter()
        .map(|node| natural_size(node, FONT_SIZE, measure).1)
        .collect();
    let mut y = top;
    for row_start in (0..nodes.len()).step_by(COLUMNS) {
        let row_end = (row_start + COLUMNS).min(nodes.len());
        let row_height = heights[row_start..row_end]
            .iter()
            .copied()
            .fold(0.0_f32, f32::max);
        for (column, node) in nodes[row_start..row_end].iter_mut().enumerate() {
            let x = PADDING + column as f32 * (BOX_WIDTH + COLUMN_GAP);
            node.rect = Rect::new(x, y, BOX_WIDTH, row_height);
        }
        y += row_height + ROW_GAP;
    }
    let content_bottom = y - ROW_GAP + PADDING;

    for relation in &model.relations {
        let (from, to) = (&nodes[relation.from], &nodes[relation.to]);
        let (a, b) = (from.rect.center(), to.rect.center());
        // Along the grid's rows, curves leave sideways; between rows,
        // from the top or bottom.
        let horizontal = (b.x - a.x).abs() > (b.y - a.y).abs();
        let path = route::spline(from.rect, to.rect, horizontal);
        let mut lines = vec![TextLine::new(relation.label.clone(), Font::Small)];
        if let Some(technology) = &relation.technology {
            lines.push(TextLine::new(format!("[{technology}]"), Font::Small));
        }
        let pos = route::label_position(&path, 0.0).unwrap_or(a.lerp(b, 0.5));
        let mut edge = Edge::new(path).between(&from.id, &to.id);
        edge.labels
            .push(EdgeLabel::new(pos, lines, true, measure, FONT_SIZE));
        scene.edges.push(edge);
    }

    let columns_used = nodes.len().clamp(1, COLUMNS);
    let width = PADDING * 2.0
        + columns_used as f32 * BOX_WIDTH
        + (columns_used.saturating_sub(1)) as f32 * COLUMN_GAP;

    if let Some(title) = &model.title {
        scene.title = Some(Text {
            pos: Point::new(width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
        });
    }

    scene.nodes = std::mem::take(&mut nodes);
    scene.width = width;
    scene.height = content_bottom.max(80.0);
    scene
}
