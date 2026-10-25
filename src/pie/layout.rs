//! A pie: slices clockwise from twelve o'clock in source order, percentages
//! on the slices, and a legend to the right.

use diagram_scene::{Anchor, Font, Node, Paint, Point, Rect, Role, Scene, Shape, Style, Weight};

use super::parse::Pie;
use crate::chart;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const RADIUS: f32 = 130.0;
/// Heavy strokes are 5 px; 2.5 px apart they overlap into a solid fill
/// with no gap at a shallow rim.
const PITCH: f32 = 2.5;
const INSET: f32 = 2.5;

pub(crate) fn layout(pie: &Pie, measure: &dyn Measure) -> Scene {
    let total: f64 = pie.slices.iter().map(|s| s.value).sum();
    let mut scene = Scene::new(0.0, 0.0);
    // Laid out from the top; the title's height is added once its width
    // is known.
    let probe = chart::place_title(&pie.title, 0.0).1;
    let top = PADDING + probe;
    let center = Point::new(PADDING + RADIUS + 4.0, top + RADIUS + 4.0);

    let mut angle = 0.0_f32;
    let mut bounds = Vec::new();
    for (i, slice) in pie.slices.iter().enumerate() {
        let sweep = (slice.value / total) as f32 * std::f32::consts::TAU;
        let (a0, a1) = (angle, angle + sweep);
        angle = a1;
        if sweep <= 0.0 {
            continue;
        }
        let mut wedge = vec![center];
        wedge.extend(chart::arc_points(center, RADIUS, a0, a1));
        scene.edges.push(chart::stroke(
            chart::hatch_within(&wedge, PITCH, INSET, 0.5),
            Paint::Role(Role::Series(i as u8)),
            Weight::Heavy,
        ));
        bounds.push((a0, a1, i));
    }
    // Separators in the background colour, as mermaid strokes its slices.
    if bounds.len() > 1 {
        for &(a0, _, _) in &bounds {
            scene.edges.push(chart::stroke(
                chart::segment(center, chart::polar(center, RADIUS, a0)),
                Paint::Role(Role::Background),
                Weight::Thick,
            ));
        }
    }
    let mut ring = Node::new(
        "pie",
        Shape::Circle,
        Rect::centered(center, RADIUS * 2.0 + 2.0, RADIUS * 2.0 + 2.0),
    );
    ring.style = Style {
        fill: Paint::None,
        emphasis: true,
        ..Style::for_shape(Shape::Circle)
    };
    scene.nodes.push(ring);

    for &(a0, a1, i) in &bounds {
        let percent = pie.slices[i].value / total * 100.0;
        if percent.round() == 0.0 {
            continue;
        }
        let at = chart::polar(center, RADIUS * 0.72, (a0 + a1) / 2.0);
        scene.texts.push(chart::text(
            at.offset(0.0, FONT_SIZE * 0.35),
            Anchor::Middle,
            &format!("{}%", percent.round()),
            Font::Bold,
            Paint::Role(Role::Background),
        ));
    }

    let entries: Vec<(Paint, String)> = pie
        .slices
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = if pie.show_data {
                format!("{} [{}]", s.label, s.value)
            } else {
                s.label.clone()
            };
            (Paint::Role(Role::Series(i as u8)), label)
        })
        .collect();
    let mut legend = chart::legend(entries, Point::new(0.0, 0.0), measure);
    legend.rect.x = center.x + RADIUS + 32.0;
    legend.rect.y = (center.y - legend.rect.height / 2.0).max(top);
    let right = legend.rect.right() + PADDING;
    let bottom = (center.y + RADIUS + 4.0).max(legend.rect.bottom()) + PADDING;
    scene.legend = Some(legend);

    let width = right.max(chart::title_width(&pie.title, measure));
    let (title, _) = chart::place_title(&pie.title, width);
    scene.title = title;
    scene.width = width;
    scene.height = bottom;
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::tests::{Fixed, assert_inside};

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Fixed)
    }

    #[test]
    fn every_label_value_and_percentage_reaches_the_scene() {
        let s = scene("pie showData title Pets\n  \"Dogs\" : 3\n  \"Cats\" : 1\n");
        let texts = s.texts();
        for expected in ["Pets", "Dogs [3]", "Cats [1]", "75%", "25%"] {
            assert!(texts.iter().any(|t| t == expected), "{expected}: {texts:?}");
        }
        assert_inside(&s);
    }

    #[test]
    fn a_zero_slice_keeps_its_legend_entry() {
        let s = scene("pie\n  \"Some\" : 5\n  \"None\" : 0\n");
        assert!(s.texts().iter().any(|t| t == "None"));
        // One wedge, no separators for a lone slice.
        assert_eq!(s.edges.len(), 1);
        assert_inside(&s);
    }

    #[test]
    fn a_long_title_widens_the_scene() {
        let title = "A very long title for a very small pie chart indeed, longer than it";
        let s = scene(&format!("pie title {title}\n  \"a\" : 1\n"));
        assert_inside(&s);
        assert!(s.title.unwrap().pos.x > RADIUS * 2.0);
    }
}
