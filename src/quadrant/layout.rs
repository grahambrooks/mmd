//! A quadrant chart: a square of four tinted quadrants, axis labels outside
//! it, and points placed by their 0..1 coordinates.

use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Font, Node, Paint, Point, Rect, Role, Scene, Shape,
    Style, Weight,
};

use super::parse::Quadrant;
use crate::chart;
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const SIDE: f32 = 400.0;
const RADIUS: f32 = 5.0;
/// The widest a y-axis label may be before it wraps.
const Y_LABEL_WIDTH: f32 = 110.0;

pub(crate) fn layout(q: &Quadrant, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    let line_h = FONT_SIZE * 1.3;
    let small = Font::Small.size(FONT_SIZE);
    let title_h = chart::place_title(&q.title, 0.0).1;

    // The y-axis labels sit left of the square, wrapped.
    let y_labels: Vec<(f32, Vec<String>)> = {
        let mid = q.y_top.is_some();
        [
            (&q.y_bottom, if mid { 0.75 } else { 0.95 }),
            (&q.y_top, 0.25),
        ]
        .into_iter()
        .filter_map(|(t, at)| {
            t.as_ref()
                .map(|t| (at, wrap(measure, t, FONT_SIZE, false, Y_LABEL_WIDTH)))
        })
        .collect()
    };
    let y_width = y_labels
        .iter()
        .flat_map(|(_, lines)| lines)
        .map(|l| measure.width(l, FONT_SIZE, false))
        .fold(0.0_f32, f32::max);
    let has_x = q.x_left.is_some() || q.x_right.is_some();
    // With points the x-axis labels go underneath, as in mermaid.
    let x_on_top = has_x && q.points.is_empty();
    let left = PADDING + if y_width > 0.0 { y_width + 12.0 } else { 0.0 };
    let top = PADDING + title_h + if x_on_top { line_h + 8.0 } else { 0.0 };
    let square = Rect::new(left, top, SIDE, SIDE);
    let half = SIDE / 2.0;

    // Quadrants as containers, so the borders and points draw over them.
    let fills = [Role::System, Role::Container, Role::Component, Role::Person];
    let origins = [
        Point::new(left + half, top),
        Point::new(left, top),
        Point::new(left, top + half),
        Point::new(left + half, top + half),
    ];
    for (origin, fill) in origins.iter().zip(fills) {
        let kind = ContainerKind::Swimlane;
        scene.containers.push(Container {
            rect: Rect::new(origin.x, origin.y, half, half),
            style: Style {
                fill: Paint::Role(fill),
                stroke: Paint::None,
                ..Style::for_container(&kind)
            },
            kind,
            label: Vec::new(),
        });
    }
    let (r, b) = (square.right(), square.bottom());
    let border = vec![
        diagram_scene::Seg::Move(Point::new(left, top)),
        diagram_scene::Seg::Line(Point::new(r, top)),
        diagram_scene::Seg::Line(Point::new(r, b)),
        diagram_scene::Seg::Line(Point::new(left, b)),
        diagram_scene::Seg::Close,
    ];
    scene.edges.push(chart::stroke(
        border,
        Paint::Role(Role::Line),
        Weight::Normal,
    ));
    for line in [
        chart::segment(Point::new(left + half, top), Point::new(left + half, b)),
        chart::segment(Point::new(left, top + half), Point::new(r, top + half)),
    ] {
        scene
            .edges
            .push(chart::stroke(line, Paint::Role(Role::Line), Weight::Thin));
    }

    // Quadrant labels: centred, or near the top once there are points.
    for (origin, label) in origins.iter().zip(&q.quadrants) {
        let Some(label) = label else { continue };
        let lines = wrap(measure, label, FONT_SIZE, true, half - 16.0);
        let first = if q.points.is_empty() {
            origin.y + half / 2.0 - (lines.len() as f32 - 1.0) * line_h / 2.0 + FONT_SIZE * 0.35
        } else {
            origin.y + 10.0 + FONT_SIZE
        };
        for (i, l) in lines.iter().enumerate() {
            scene.texts.push(chart::text(
                Point::new(origin.x + half / 2.0, first + i as f32 * line_h),
                Anchor::Middle,
                l,
                Font::Bold,
                Paint::Role(Role::Text),
            ));
        }
    }

    // Axis labels.
    let x_y = if x_on_top {
        top - 8.0 - FONT_SIZE * 0.3
    } else {
        b + 8.0 + FONT_SIZE
    };
    let mut right = r;
    match (&q.x_left, &q.x_right) {
        (Some(l), Some(rt)) => {
            for (t, x) in [(l, left + half / 2.0), (rt, left + half * 1.5)] {
                scene
                    .texts
                    .push(axis_text(Point::new(x, x_y), Anchor::Middle, t));
            }
        }
        (Some(l), None) => {
            scene
                .texts
                .push(axis_text(Point::new(left, x_y), Anchor::Start, l));
            right = right.max(left + measure.width(l, FONT_SIZE, false));
        }
        (None, Some(rt)) => {
            scene.texts.push(axis_text(
                Point::new(left + half * 1.5, x_y),
                Anchor::Middle,
                rt,
            ));
        }
        (None, None) => {}
    }
    for (at, lines) in &y_labels {
        let centre = top + SIDE * at;
        let first = centre - (lines.len() as f32 - 1.0) * line_h / 2.0 + FONT_SIZE * 0.35;
        for (i, l) in lines.iter().enumerate() {
            scene.texts.push(axis_text(
                Point::new(left - 12.0, first + i as f32 * line_h),
                Anchor::End,
                l,
            ));
        }
    }

    // Points, each labelled underneath.
    let mut bottom = if x_on_top || !has_x {
        b
    } else {
        x_y + FONT_SIZE * 0.4
    };
    for (i, p) in q.points.iter().enumerate() {
        let radius = p.style.radius.unwrap_or(RADIUS).max(1.0);
        let at = Point::new(left + p.x * SIDE, top + (1.0 - p.y) * SIDE);
        let mut node = Node::new(
            format!("point-{i}"),
            Shape::Circle,
            Rect::centered(at, radius * 2.0, radius * 2.0),
        );
        let fill = p
            .style
            .color
            .map_or(Paint::Role(Role::Accent), |(r, g, b)| Paint::Rgb(r, g, b));
        let width = p.style.stroke_width.unwrap_or(0.0);
        let stroke = match (p.style.stroke, width > 0.0) {
            (Some((r, g, b)), true) => Paint::Rgb(r, g, b),
            (None, true) => fill,
            _ => Paint::None,
        };
        node.style = Style {
            fill,
            stroke,
            dash: Dash::Solid,
            emphasis: width >= 2.0,
            ..Style::for_shape(Shape::Circle)
        };
        scene.nodes.push(node);
        let label_y = at.y + radius + 4.0 + small;
        scene.texts.push(chart::text(
            Point::new(at.x, label_y),
            Anchor::Middle,
            &p.name,
            Font::Small,
            Paint::Role(Role::Text),
        ));
        let half_w = measure.width(&p.name, small, false) / 2.0;
        right = right.max(at.x + half_w).max(at.x + radius);
        bottom = bottom.max(label_y + small * 0.3).max(at.y + radius);
    }
    let left_edge = q
        .points
        .iter()
        .map(|p| left + p.x * SIDE - measure.width(&p.name, small, false) / 2.0)
        .fold(PADDING, f32::min);
    // A label hanging off the left edge moves everything right.
    if left_edge < PADDING {
        shift(&mut scene, PADDING - left_edge);
        right += PADDING - left_edge;
    }

    let width = (right + PADDING).max(chart::title_width(&q.title, measure));
    scene.title = chart::place_title(&q.title, width).0;
    scene.width = width;
    scene.height = bottom + PADDING;
    scene
}

fn axis_text(pos: Point, anchor: Anchor, text: &str) -> diagram_scene::Text {
    chart::text(pos, anchor, text, Font::Body, Paint::Role(Role::Muted))
}

/// Move everything drawn `dx` to the right.
fn shift(scene: &mut Scene, dx: f32) {
    for c in &mut scene.containers {
        c.rect.x += dx;
    }
    for n in &mut scene.nodes {
        n.rect.x += dx;
    }
    for t in &mut scene.texts {
        t.pos.x += dx;
    }
    for e in &mut scene.edges {
        for seg in &mut e.path {
            match seg {
                diagram_scene::Seg::Move(p) | diagram_scene::Seg::Line(p) => p.x += dx,
                diagram_scene::Seg::Cubic(a, b, p) => {
                    a.x += dx;
                    b.x += dx;
                    p.x += dx;
                }
                diagram_scene::Seg::Close => {}
            }
        }
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
    fn every_label_and_point_reaches_the_scene() {
        let s = scene(
            "quadrantChart\n  title Priorities\n  x-axis Low effort --> High effort\n  \
             y-axis Low value --> High value\n  quadrant-1 Plan\n  quadrant-2 Do now\n  \
             quadrant-3 Drop\n  quadrant-4 Delegate\n  Fonts: [0.2, 0.9]\n  \
             Edge point: [0, 0] color: #ff0000\n  Far: [1, 1] radius: 9\n",
        );
        let texts = s.texts();
        for expected in [
            "Priorities",
            "Low effort",
            "High effort",
            "Low value",
            "High value",
            "Plan",
            "Do now",
            "Drop",
            "Delegate",
            "Fonts",
            "Edge point",
            "Far",
        ] {
            assert!(texts.iter().any(|t| t == expected), "{expected}: {texts:?}");
        }
        assert_eq!(s.nodes.len(), 3);
        assert_eq!(s.nodes[1].style.fill, Paint::Rgb(255, 0, 0));
        assert_inside(&s);
    }

    #[test]
    fn an_empty_chart_still_draws_its_quadrants() {
        let s = scene("quadrantChart\n");
        assert_eq!(s.containers.len(), 4);
        assert_inside(&s);
        let s = scene("quadrantChart\n  x-axis Only left\n  quadrant-1 One\n");
        assert_inside(&s);
    }

    #[test]
    fn a_point_label_at_the_left_edge_stays_inside() {
        let s = scene("quadrantChart\n  A very long point name indeed: [0, 0.5]\n");
        assert_inside(&s);
    }
}
