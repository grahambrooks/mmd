//! C4 layout: elements on a grid, boundaries as dashed boxes around a
//! sub-grid of their own, relationships as curves between the sides that
//! face each other.
//!
//! As in mermaid, each scope (the diagram, or a boundary) lays out its own
//! elements first, `shapes_per_row` to a row, then its child boundaries in
//! rows of `boundaries_per_row` below them.
//!
//! Mermaid's own C4 renderer routes relationship labels through the boxes it
//! just drew. Here labels sit on a pill at the curve's middle, which stays
//! legible where it crosses a line.

use std::collections::HashMap;

use diagram_scene::route::Side;
use diagram_scene::shape::{line_height, natural_size};
use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge, EdgeLabel, Font, Legend, LegendEntry, Marker,
    Node, Paint, Point, Rect, Role, Scene, Seg, Shape, Style, Text, TextLine, Weight, line_width,
    route,
};

use super::parse::{
    Arrows, BoundaryKind, C4, Colors, Direction, ElementKind, Form, Level, LineStyle, Relation,
    View,
};
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const BOX_WIDTH: f32 = 190.0;
const COLUMN_GAP: f32 = 130.0;
const ROW_GAP: f32 = 96.0;
/// Between a boundary's outline and what it holds.
const BOUNDARY_PAD: f32 = 20.0;
/// The narrowest an empty boundary is drawn.
const BOUNDARY_MIN_WIDTH: f32 = BOX_WIDTH;

pub fn layout(model: &C4, measure: &dyn Measure) -> Scene {
    let small = Font::Small.size(FONT_SIZE);
    let inner_width = BOX_WIDTH - 24.0;
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;

    // Each element as a node, its text wrapped to the box width; the
    // node's height follows its content.
    let nodes: Vec<Node> = model
        .elements
        .iter()
        .map(|element| {
            let kind = element.kind;
            let shape = match kind_shape(kind) {
                // `EightSidedShape()`: the scene has no octagon.
                Shape::Rounded if element.style.octagon => Shape::Hexagon,
                shape => shape,
            };
            let mut node = Node::new(element.alias.clone(), shape, Rect::default());
            if !model.hide_stereotype {
                node.stereotype = vec![TextLine::new(
                    format!("\u{ab}{}\u{bb}", kind.stereotype()),
                    Font::Small,
                )];
            }
            node.label = wrap(measure, &element.label, FONT_SIZE, true, inner_width)
                .into_iter()
                .map(|line| TextLine::new(line, Font::Bold))
                .collect();
            let small_lines = |text: &str| {
                wrap(measure, text, small, false, inner_width)
                    .into_iter()
                    .map(|line| TextLine::new(line, Font::Small))
                    .collect::<Vec<_>>()
            };
            if let Some(technology) = &element.technology {
                node.sublabel
                    .extend(small_lines(&format!("[{technology}]")));
            }
            if let Some(description) = &element.description {
                node.sublabel.extend(small_lines(description));
            }
            node.style.fill = Paint::Role(kind_role(kind));
            paint(&mut node.style, &element.style.colors);
            node
        })
        .collect();

    // A boundary's label, bold, over its `[type]`.
    let boundary_labels: Vec<Vec<TextLine>> = model
        .boundaries
        .iter()
        .map(|boundary| {
            let mut lines = vec![
                TextLine::new(boundary.label.clone(), Font::Bold),
                TextLine::new(format!("[{}]", boundary.type_label), Font::Small),
            ];
            if let Some(description) = &boundary.description {
                lines.extend(TextLine::lines(description, Font::Small));
            }
            lines
        })
        .collect();

    let title_height = if model.title.is_some() {
        Font::Title.size(FONT_SIZE) + PADDING
    } else {
        0.0
    };
    let top = PADDING + title_height;

    let heights = nodes
        .iter()
        .map(|node| natural_size(node, FONT_SIZE, measure).1)
        .collect();
    let mut grid = Grid {
        model,
        measure,
        nodes,
        heights,
        labels: &boundary_labels,
        boundary_rects: vec![Rect::default(); model.boundaries.len()],
    };
    let (content_width, content_height) = grid.place_scope(None, PADDING, top);
    let Grid {
        nodes,
        boundary_rects,
        ..
    } = grid;

    // Outermost first: a parent's index is always below its children's.
    scene.containers = boundary_rects
        .iter()
        .zip(boundary_labels)
        .zip(&model.boundaries)
        .map(|((&rect, label), boundary)| {
            let kind = if boundary.kind == BoundaryKind::Node {
                ContainerKind::DeploymentNode
            } else {
                ContainerKind::Boundary
            };
            let mut style = Style::for_container(&kind);
            // Mermaid draws a deployment node solid, a boundary dashed.
            style.dash = if boundary.kind == BoundaryKind::Node {
                Dash::Solid
            } else {
                Dash::Dashed
            };
            paint(&mut style, &boundary.style.colors);
            Container {
                rect,
                kind,
                label,
                style,
            }
        })
        .collect();

    let sides: Vec<(Side, Side)> = model
        .relations
        .iter()
        .map(|relation| relation_sides(relation, &nodes))
        .collect();
    let ports = spread_ports(model, &nodes, &sides);

    for (index, relation) in model.relations.iter().enumerate() {
        let (from, to) = (&nodes[relation.from], &nodes[relation.to]);
        let (a, b) = ports[index];
        let (side_a, side_b) = sides[index];
        let path = if side_a == side_b {
            // Out of one side and back in through the same side of the
            // other: a U deep enough to carry its label clear of the row.
            let out = |p: Point| match side_a {
                Side::Top => p.offset(0.0, -U_DEPTH),
                Side::Right => p.offset(U_DEPTH, 0.0),
                Side::Bottom => p.offset(0.0, U_DEPTH),
                Side::Left => p.offset(-U_DEPTH, 0.0),
            };
            vec![Seg::Move(a), Seg::Cubic(out(a), out(b), b)]
        } else {
            route::curve_between(a, side_a, b, side_b)
        };
        // A dynamic view numbers its relations in order, as mermaid does.
        let label = if model.view == View::Dynamic {
            format!("{}: {}", index + 1, relation.label)
        } else {
            relation.label.clone()
        };
        let mut lines = vec![TextLine::new(label, Font::Small)];
        if let Some(technology) = &relation.technology {
            lines.push(TextLine::new(format!("[{technology}]"), Font::Small));
        }
        if let Some(description) = &relation.description {
            lines.push(TextLine::new(description.clone(), Font::Small));
        }
        let (dx, dy) = relation.style.offset;
        let pos = route::label_position(&path, 0.0)
            .unwrap_or(a.lerp(b, 0.5))
            .offset(dx, dy);
        let mut edge = Edge::new(path).between(&from.id, &to.id);
        if let Some((r, g, b)) = relation.style.line_color {
            edge.pen.paint = Paint::Rgb(r, g, b);
        }
        match relation.style.line {
            Some(LineStyle::Dashed) => edge.pen.dash = Dash::Dashed,
            Some(LineStyle::Dotted) => edge.pen.dash = Dash::Dotted,
            Some(LineStyle::Bold) => edge.pen.weight = Weight::Thick,
            Some(LineStyle::Solid) | None => {}
        }
        (edge.start, edge.end) = match relation.arrows {
            Arrows::Forward => (Marker::None, Marker::Arrow),
            Arrows::Back => (Marker::Arrow, Marker::None),
            Arrows::Both => (Marker::Arrow, Marker::Arrow),
        };
        edge.labels
            .push(EdgeLabel::new(pos, lines, true, measure, FONT_SIZE));
        scene.edges.push(edge);
    }
    separate_labels(&mut scene.edges);

    let title_width = model
        .title
        .as_ref()
        .map(|title| {
            line_width(
                measure,
                &TextLine::new(title.clone(), Font::Title),
                FONT_SIZE,
            )
        })
        .unwrap_or(0.0);
    // Curves that bulge out of the grid (a U under the last row, a
    // self-relation) and their labels must stay inside the bounds too.
    let (mut right, mut bottom) = (0.0_f32, 0.0_f32);
    for edge in &scene.edges {
        for poly in diagram_scene::flatten::flatten(&edge.path, 0.5) {
            for p in &poly.points {
                (right, bottom) = (right.max(p.x), bottom.max(p.y));
            }
        }
        for label in &edge.labels {
            let height: f32 = label
                .lines
                .iter()
                .map(|line| line_height(line.font().size(FONT_SIZE)))
                .sum();
            right = right.max(label.pos.x + label.text_width(FONT_SIZE) / 2.0 + 6.0);
            bottom = bottom.max(label.pos.y + height / 2.0 + 2.0);
        }
    }
    let mut width =
        (PADDING * 2.0 + content_width.max(title_width).max(BOX_WIDTH)).max(right + PADDING);
    let mut content_bottom = (top + content_height).max(bottom);

    if model.legend {
        let legend = legend(model, measure, PADDING, content_bottom + PADDING);
        width = width.max(legend.rect.right() + PADDING);
        content_bottom = legend.rect.bottom();
        scene.legend = Some(legend);
    }

    if let Some(title) = &model.title {
        scene.title = Some(Text {
            pos: Point::new(width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
    }

    scene.nodes = nodes;
    scene.width = width;
    scene.height = (content_bottom + PADDING).max(80.0);
    scene
}

fn kind_shape(kind: ElementKind) -> Shape {
    match (kind.level(), kind.form()) {
        (Level::Person, _) => Shape::Person,
        (_, Form::Db) => Shape::Cylinder,
        (_, Form::Queue) => Shape::Queue,
        (_, Form::Plain) => Shape::Rounded,
    }
}

fn kind_role(kind: ElementKind) -> Role {
    if kind.is_external() {
        Role::External
    } else if kind.form() == Form::Db {
        Role::Database
    } else {
        match kind.level() {
            Level::Person => Role::Person,
            Level::System => Role::System,
            Level::Container => Role::Container,
            Level::Component => Role::Component,
        }
    }
}

/// The source's own colours over a style.
fn paint(style: &mut Style, colors: &Colors) {
    if let Some((r, g, b)) = colors.fill {
        style.fill = Paint::Rgb(r, g, b);
    }
    if let Some((r, g, b)) = colors.stroke {
        style.stroke = Paint::Rgb(r, g, b);
    }
    if let Some((r, g, b)) = colors.text {
        style.text = Paint::Rgb(r, g, b);
    }
    if colors.dashed {
        style.dash = Dash::Dashed;
    }
}

/// `SHOW_LEGEND()`: one entry per kind of element drawn, then one per
/// element tag in use, with its top-left at `(x, y)`.
fn legend(model: &C4, measure: &dyn Measure, x: f32, y: f32) -> Legend {
    let mut entries: Vec<LegendEntry> = Vec::new();
    for element in &model.elements {
        let label = element.kind.stereotype();
        if entries.iter().any(|e| e.label == label) {
            continue;
        }
        let shape = kind_shape(element.kind);
        let mut style = Style::for_shape(shape);
        style.fill = Paint::Role(kind_role(element.kind));
        entries.push(LegendEntry {
            shape,
            style,
            label,
        });
    }
    for tag in &model.tags {
        let used = model.elements.iter().any(|e| e.tags.contains(&tag.name))
            || model.boundaries.iter().any(|b| b.tags.contains(&tag.name));
        if !used || tag.element.colors == Colors::default() {
            continue;
        }
        let mut style = Style::for_shape(Shape::Rounded);
        paint(&mut style, &tag.element.colors);
        entries.push(LegendEntry {
            shape: Shape::Rounded,
            style,
            label: tag.legend.clone().unwrap_or_else(|| tag.name.clone()),
        });
    }
    // The painter's geometry: rows 1.6 em tall from 8 down, a swatch
    // 1.2 rows wide from 10 in, the label 8 after it.
    let row = FONT_SIZE * 1.6;
    let small = Font::Small.size(FONT_SIZE);
    let text = entries
        .iter()
        .map(|e| measure.width(&e.label, small, false))
        .fold(0.0_f32, f32::max);
    let width = 10.0 + row * 1.2 + 8.0 + text + 12.0;
    let height = 8.0 + row * entries.len() as f32 + 6.0;
    Legend {
        rect: Rect::new(x, y, width, height),
        entries,
    }
}

/// The pill a label is drawn on.
fn label_rect(label: &EdgeLabel) -> Rect {
    let height: f32 = label
        .lines
        .iter()
        .map(|line| line_height(line.font().size(FONT_SIZE)))
        .sum();
    Rect::centered(label.pos, label.text_width(FONT_SIZE) + 12.0, height + 4.0)
}

/// Move a label that would sit on an earlier one up or down, clear of it.
/// Two relations between the same pair of elements otherwise put their
/// labels on top of each other.
fn separate_labels(edges: &mut [Edge]) {
    const GAP: f32 = 2.0;
    let mut placed: Vec<Rect> = Vec::new();
    for edge in edges.iter_mut() {
        for label in &mut edge.labels {
            // A few rounds: moving off one label can land on another.
            for _ in 0..4 {
                let rect = label_rect(label);
                let Some(other) = placed.iter().find(|r| r.intersects(&rect)) else {
                    break;
                };
                let dy = if rect.center().y <= other.center().y {
                    other.y - GAP - rect.bottom()
                } else {
                    other.bottom() + GAP - rect.y
                };
                label.pos = label.pos.offset(0.0, dy);
            }
            placed.push(label_rect(label));
        }
    }
}

/// How far a U-shaped route bulges out of the side it leaves.
const U_DEPTH: f32 = 52.0;

/// The sides a relation leaves `from` and enters `to` by.
///
/// A directional relation leaves along its hint's axis. Otherwise elements
/// in one row join sideways — or, when another element sits between them,
/// by a U under the row rather than through that element — and elements in
/// different rows join top to bottom, crossing the gap between rows.
fn relation_sides(relation: &Relation, nodes: &[Node]) -> (Side, Side) {
    let (from, to) = (nodes[relation.from].rect, nodes[relation.to].rect);
    if relation.from == relation.to {
        return (Side::Right, Side::Right);
    }
    match relation.direction {
        Some(Direction::Left | Direction::Right) => return route::facing(from, to, true),
        Some(Direction::Up | Direction::Down) => return route::facing(from, to, false),
        None => {}
    }
    let overlaps_vertically = |a: Rect, b: Rect| a.y < b.bottom() && b.y < a.bottom();
    if !overlaps_vertically(from, to) {
        // Nearly level but far apart (elements in neighbouring nested
        // boxes): join sideways, so the curve does not run along the
        // gap between rows, through the boxes' headers.
        let gap = |a0: f32, a1: f32, b0: f32, b1: f32| (b0 - a1).max(a0 - b1);
        let across = gap(from.x, from.right(), to.x, to.right());
        let down = gap(from.y, from.bottom(), to.y, to.bottom());
        // Rows of one grid are a full `ROW_GAP` apart, so this only
        // happens between grids.
        let sideways = down < ROW_GAP / 2.0 && across > 2.0 * down.max(0.0) && across > 0.0;
        return route::facing(from, to, sideways);
    }
    let (left, right) = if from.x <= to.x {
        (from, to)
    } else {
        (to, from)
    };
    let blocked = nodes.iter().enumerate().any(|(index, node)| {
        index != relation.from
            && index != relation.to
            && (overlaps_vertically(node.rect, from) || overlaps_vertically(node.rect, to))
            && node.rect.x < right.x
            && node.rect.right() > left.right()
    });
    if blocked {
        (Side::Bottom, Side::Bottom)
    } else {
        route::facing(from, to, true)
    }
}

/// One end of a relation on a side: the other end's position along the
/// side (the sort key), the relation, and whether this is its start.
type End = (f32, usize, bool);

/// Where each relation starts and ends. Ends sharing a side of one element
/// are spread along it, in the order their other ends lie, so lines and
/// arrowheads do not sit on top of one another.
fn spread_ports(model: &C4, nodes: &[Node], sides: &[(Side, Side)]) -> Vec<(Point, Point)> {
    let mut groups: HashMap<(usize, Side), Vec<End>> = HashMap::new();
    for (index, relation) in model.relations.iter().enumerate() {
        let (side_from, side_to) = sides[index];
        for (element, side, other, start) in [
            (relation.from, side_from, relation.to, true),
            (relation.to, side_to, relation.from, false),
        ] {
            let centre = nodes[other].rect.center();
            let key = match side {
                Side::Top | Side::Bottom => centre.x,
                Side::Left | Side::Right => centre.y,
            };
            groups
                .entry((element, side))
                .or_default()
                .push((key, index, start));
        }
    }
    let mut ports = vec![(Point::new(0.0, 0.0), Point::new(0.0, 0.0)); model.relations.len()];
    for ((element, side), mut ends) in groups {
        ends.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(b.2.cmp(&a.2)));
        let rect = nodes[element].rect;
        let count = ends.len() as f32;
        for (slot, &(_, relation, start)) in ends.iter().enumerate() {
            // Within the middle 60% of the side, clear of rounded corners.
            let t = 0.2 + 0.6 * (slot as f32 + 1.0) / (count + 1.0);
            let point = match side {
                Side::Top => Point::new(rect.x + rect.width * t, rect.y),
                Side::Bottom => Point::new(rect.x + rect.width * t, rect.bottom()),
                Side::Left => Point::new(rect.x, rect.y + rect.height * t),
                Side::Right => Point::new(rect.right(), rect.y + rect.height * t),
            };
            if start {
                ports[relation].0 = point;
            } else {
                ports[relation].1 = point;
            }
        }
    }
    ports
}

/// Places elements and boundaries, scope by scope.
struct Grid<'a> {
    model: &'a C4,
    measure: &'a dyn Measure,
    nodes: Vec<Node>,
    heights: Vec<f32>,
    labels: &'a [Vec<TextLine>],
    boundary_rects: Vec<Rect>,
}

impl Grid<'_> {
    /// Lay out what `scope` holds with its top-left at `(x, y)`, and return
    /// the size it takes.
    fn place_scope(&mut self, scope: Option<usize>, x: f32, y: f32) -> (f32, f32) {
        let model = self.model;
        let elements: Vec<usize> = (0..model.elements.len())
            .filter(|&i| model.elements[i].boundary == scope)
            .collect();
        let boundaries: Vec<usize> = (0..model.boundaries.len())
            .filter(|&i| model.boundaries[i].parent == scope)
            .collect();

        let mut width = 0.0_f32;
        let mut row_y = y;
        // Rows are as tall as their tallest element.
        for row in elements.chunks(model.shapes_per_row) {
            let row_height = row.iter().map(|&i| self.heights[i]).fold(0.0_f32, f32::max);
            for (column, &i) in row.iter().enumerate() {
                let left = x + column as f32 * (BOX_WIDTH + COLUMN_GAP);
                self.nodes[i].rect = Rect::new(left, row_y, BOX_WIDTH, row_height);
            }
            let used = row.len() as f32;
            width = width.max(used * BOX_WIDTH + (used - 1.0) * COLUMN_GAP);
            row_y += row_height + ROW_GAP;
        }
        for row in boundaries.chunks(model.boundaries_per_row) {
            let mut left = x;
            let mut row_height = 0.0_f32;
            for &b in row {
                let rect = self.place_boundary(b, left, row_y);
                left = rect.right() + COLUMN_GAP;
                row_height = row_height.max(rect.height);
            }
            width = width.max(left - COLUMN_GAP - x);
            row_y += row_height + ROW_GAP;
        }
        let height = if row_y > y { row_y - ROW_GAP - y } else { 0.0 };
        (width, height)
    }

    fn place_boundary(&mut self, boundary: usize, x: f32, y: f32) -> Rect {
        let label = &self.labels[boundary];
        // The painter puts the first baseline 1.25 em down and steps by
        // each line's height; leave a gap under the last.
        let header = FONT_SIZE * 1.25
            + label[1..]
                .iter()
                .map(|line| line_height(line.font().size(FONT_SIZE)))
                .sum::<f32>()
            + 14.0;
        let label_width = label
            .iter()
            .map(|line| line_width(self.measure, line, FONT_SIZE))
            .fold(0.0_f32, f32::max);
        let (content_width, content_height) =
            self.place_scope(Some(boundary), x + BOUNDARY_PAD, y + header);
        let width = (content_width + 2.0 * BOUNDARY_PAD)
            .max(label_width + 20.0 + BOUNDARY_PAD)
            .max(BOUNDARY_MIN_WIDTH);
        let rect = Rect::new(x, y, width, header + content_height + BOUNDARY_PAD);
        self.boundary_rects[boundary] = rect;
        rect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::c4::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const NESTED: &str = "C4Container\n\
        title Nested\n\
        Person(user, \"User\", \"Banks online\")\n\
        System_Ext(mail, \"Mail\", \"Sends email\")\n\
        Enterprise_Boundary(bank, \"Big Bank\") {\n\
          System_Boundary(core, \"Internet Banking\") {\n\
            Container(web, \"Web App\", \"Rust\", \"Serves pages\")\n\
            ContainerDb(db, \"Database\", \"Postgres\", \"Accounts\")\n\
            ContainerQueue(jobs, \"Jobs\", \"SQS\")\n\
            Container(api, \"API\", \"Rust\")\n\
          }\n\
          Boundary(ops, \"Operations\", \"team\") {\n\
            Component(mon, \"Monitor\", \"Prometheus\")\n\
          }\n\
          System(mainframe, \"Mainframe\")\n\
        }\n\
        Rel(user, web, \"Uses\", \"HTTPS\")\n\
        BiRel(web, api, \"Calls\", \"JSON\")\n\
        Rel_Back(db, api, \"Reads\")\n\
        Rel_D(api, jobs, \"Enqueues\")\n\
        Rel(api, mail, \"Sends\", \"SMTP\", \"Receipts\")\n";

    fn scene() -> Scene {
        layout(&parse(NESTED).unwrap(), &Fixed)
    }

    #[test]
    fn every_element_boundary_and_relation_reaches_the_scene() {
        let texts = scene().texts();
        for expected in [
            "Nested",
            "User",
            "Banks online",
            "Mail",
            "Big Bank",
            "[ENTERPRISE]",
            "Internet Banking",
            "[SYSTEM]",
            "Operations",
            "[team]",
            "Web App",
            "[Rust]",
            "Serves pages",
            "Database",
            "[Postgres]",
            "Jobs",
            "[SQS]",
            "API",
            "Monitor",
            "[Prometheus]",
            "Mainframe",
            "Uses",
            "[HTTPS]",
            "Calls",
            "[JSON]",
            "Reads",
            "Enqueues",
            "Sends",
            "[SMTP]",
            "Receipts",
            "\u{ab}Container DB\u{bb}",
            "\u{ab}Container Queue\u{bb}",
            "\u{ab}External System\u{bb}",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
    }

    #[test]
    fn boundaries_contain_their_children_and_nothing_else() {
        let scene = scene();
        let model = parse(NESTED).unwrap();
        for (index, boundary) in model.boundaries.iter().enumerate() {
            let rect = scene.containers[index].rect;
            for element in &model.elements {
                let node = scene.node(&element.alias).unwrap().rect;
                // Inside if declared in this boundary or one inside it.
                let mut scope = element.boundary;
                let mut inside = false;
                while let Some(b) = scope {
                    inside |= b == index;
                    scope = model.boundaries[b].parent;
                }
                if inside {
                    assert!(
                        rect.encloses(&node, 0.01),
                        "{} outside {}",
                        element.alias,
                        boundary.alias
                    );
                } else {
                    assert!(
                        !rect.intersects(&node),
                        "{} inside {}",
                        element.alias,
                        boundary.alias
                    );
                }
            }
            if let Some(parent) = boundary.parent {
                assert!(scene.containers[parent].rect.encloses(&rect, 0.01));
            }
        }
    }

    #[test]
    fn sibling_boundaries_and_elements_never_overlap() {
        let scene = scene();
        let boxes: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
            }
        }
        // `core` and `ops` share a parent.
        assert!(
            !scene.containers[1]
                .rect
                .intersects(&scene.containers[2].rect)
        );
    }

    #[test]
    fn everything_stays_inside_the_bounds() {
        let scene = scene();
        let bounds = scene.bounds();
        for rect in scene
            .nodes
            .iter()
            .map(|n| n.rect)
            .chain(scene.containers.iter().map(|c| c.rect))
        {
            assert!(bounds.encloses(&rect, 0.01), "{rect:?} escapes {bounds:?}");
        }
    }

    #[test]
    fn arrowheads_follow_the_relation_kind() {
        let scene = scene();
        let ends: Vec<_> = scene.edges.iter().map(|e| (e.start, e.end)).collect();
        assert_eq!(ends[0], (Marker::None, Marker::Arrow));
        assert_eq!(ends[1], (Marker::Arrow, Marker::Arrow));
        assert_eq!(ends[2], (Marker::Arrow, Marker::None));
    }

    #[test]
    fn shapes_follow_the_element_form() {
        let scene = scene();
        let shape = |id: &str| scene.node(id).unwrap().shape;
        assert_eq!(shape("user"), Shape::Person);
        assert_eq!(shape("db"), Shape::Cylinder);
        assert_eq!(shape("jobs"), Shape::Queue);
        assert_eq!(shape("web"), Shape::Rounded);
        let fill = |id: &str| scene.node(id).unwrap().style.fill;
        assert_eq!(fill("mail"), Paint::Role(Role::External));
        assert_eq!(fill("web"), Paint::Role(Role::Container));
        assert_eq!(fill("mon"), Paint::Role(Role::Component));
    }

    #[test]
    fn layout_config_sets_the_row_length() {
        let source = "C4Context\nUpdateLayoutConfig($c4ShapeInRow=\"1\")\n\
            System(a, \"A\")\nSystem(b, \"B\")\n";
        let scene = layout(&parse(source).unwrap(), &Fixed);
        let (a, b) = (scene.node("a").unwrap().rect, scene.node("b").unwrap().rect);
        assert_eq!(a.x, b.x);
        assert!(b.y > a.bottom());
    }

    #[test]
    fn curves_go_around_elements_not_through_them() {
        // a, b and c in one row: a -> c must not cross b; it goes under.
        let source = "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\nSystem(c, \"C\")\n\
            Rel(a, c, \"skips b\")\nRel(a, b, \"next\")\nRel(c, c, \"self\")\n";
        let scene = layout(&parse(source).unwrap(), &Fixed);
        let b = scene.node("b").unwrap().rect;
        for poly in diagram_scene::flatten::flatten(&scene.edges[0].path, 0.5) {
            for p in &poly.points {
                assert!(!b.inflate(-0.5).contains(*p), "{p:?} inside b");
            }
        }
        // Both of a's edges leave it, at different points.
        let start = |i: usize| {
            diagram_scene::flatten::start(&scene.edges[i].path)
                .unwrap()
                .0
        };
        assert_ne!(start(0), start(1));
        // The U below the row and the self-loop stay inside the bounds.
        let bounds = scene.bounds();
        for edge in &scene.edges {
            for poly in diagram_scene::flatten::flatten(&edge.path, 0.5) {
                for p in &poly.points {
                    assert!(bounds.contains(*p), "{p:?} escapes {bounds:?}");
                }
            }
        }
    }

    #[test]
    fn an_empty_boundary_still_draws() {
        let scene = layout(
            &parse("C4Context\nBoundary(b, \"Empty\") {\n}\n").unwrap(),
            &Fixed,
        );
        assert_eq!(scene.containers.len(), 1);
        assert!(scene.bounds().encloses(&scene.containers[0].rect, 0.01));
    }
}
