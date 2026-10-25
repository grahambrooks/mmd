//! timeline layout: a column per time period with its events stacked
//! beneath (or, top-down, a row per period with its events beside it),
//! sections as headers spanning their periods.

use diagram_scene::{
    Anchor, Dash, Edge, Font, Marker, Node, Paint, Pen, Point, Rect, Role, Scene, Seg, Shape, Text,
    TextLine, Weight,
};

use super::parse::{Direction, Timeline};
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const BOX_WIDTH: f32 = 150.0;
const GAP: f32 = 20.0;
const EVENT_GAP: f32 = 10.0;
const PAD: f32 = 8.0;
/// Room between the period row and the events for the time arrow.
const LANE: f32 = 40.0;

/// Lay a parsed timeline out.
pub fn layout(timeline: &Timeline, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let title_height = if timeline.title.is_some() {
        Font::Title.size(FONT_SIZE) * 1.35 + 12.0
    } else {
        0.0
    };
    let top = PADDING + title_height;

    // Each period's colour: its section's, or its own when there are none.
    let colour = |index: usize| {
        let period = &timeline.periods[index];
        let n = match period.section {
            Some(section) => section,
            None if timeline.sections.is_empty() => index,
            None => timeline.sections.len() + index,
        };
        Paint::Role(Role::Series((n % 6) as u8))
    };
    let label = |text: &str, font: Font| -> Vec<TextLine> {
        wrap(measure, text, FONT_SIZE, font.bold(), BOX_WIDTH - 2.0 * PAD)
            .into_iter()
            .map(|line| TextLine::new(line, font))
            .collect()
    };
    let height_of = |lines: &[TextLine]| lines.len() as f32 * FONT_SIZE * 1.35 + 2.0 * PAD;

    let periods: Vec<Vec<TextLine>> = timeline
        .periods
        .iter()
        .map(|p| label(&p.text, Font::Bold))
        .collect();
    let events: Vec<Vec<Vec<TextLine>>> = timeline
        .periods
        .iter()
        .map(|p| p.events.iter().map(|e| label(e, Font::Body)).collect())
        .collect();
    let section_labels: Vec<Vec<TextLine>> = timeline
        .sections
        .iter()
        .map(|s| label(s, Font::Bold))
        .collect();
    let section_height = section_labels
        .iter()
        .map(|l| height_of(l))
        .fold(0.0_f32, f32::max);
    let period_height = periods
        .iter()
        .map(|l| height_of(l))
        .fold(0.0_f32, f32::max)
        .max(FONT_SIZE * 1.35 + 2.0 * PAD);

    let filled = |id: String, rect: Rect, lines: Vec<TextLine>, fill: Paint| {
        let mut node = Node::new(id, Shape::Rounded, rect);
        node.label = lines;
        node.style.fill = fill;
        node.style.stroke = fill;
        node.style.text = Paint::Role(Role::Background);
        node
    };
    let outlined = |id: String, rect: Rect, lines: Vec<TextLine>, stroke: Paint| {
        let mut node = Node::new(id, Shape::Rounded, rect);
        node.label = lines;
        node.style.fill = Paint::Role(Role::Surface);
        node.style.stroke = stroke;
        node
    };
    let connector = |from: Point, to: Point, paint: Paint| Edge {
        pen: Pen {
            dash: Dash::Dashed,
            weight: Weight::Thin,
            paint,
        },
        end: Marker::None,
        ..Edge::new(vec![Seg::Move(from), Seg::Line(to)])
    };
    let arrow = |from: Point, to: Point| Edge {
        pen: Pen {
            dash: Dash::Solid,
            weight: Weight::Thick,
            paint: Paint::Role(Role::Line),
        },
        end: Marker::Arrow,
        ..Edge::new(vec![Seg::Move(from), Seg::Line(to)])
    };

    let (width, height) = match timeline.direction {
        Direction::LeftRight => {
            let column = |index: usize| PADDING + index as f32 * (BOX_WIDTH + GAP);
            let mut y = top;
            let mut next_empty = timeline.periods.len();
            if !timeline.sections.is_empty() {
                for (section, lines) in section_labels.iter().enumerate() {
                    let members: Vec<usize> = (0..timeline.periods.len())
                        .filter(|&i| timeline.periods[i].section == Some(section))
                        .collect();
                    let (first, last) = match (members.first(), members.last()) {
                        (Some(&f), Some(&l)) => (f, l),
                        // A section with no periods still shows, after the
                        // last column.
                        _ => {
                            next_empty += 1;
                            (next_empty - 1, next_empty - 1)
                        }
                    };
                    let left = column(first);
                    let right = column(last) + BOX_WIDTH;
                    let fill = Paint::Role(Role::Series((section % 6) as u8));
                    scene.nodes.push(filled(
                        format!("section:{section}"),
                        Rect::new(left, y, right - left, section_height),
                        lines.clone(),
                        fill,
                    ));
                }
                y += section_height + GAP / 2.0;
            }
            let period_top = y;
            let line_y = period_top + period_height + LANE / 2.0;
            let mut bottom = line_y + LANE / 2.0;
            let columns = next_empty;
            for (index, lines) in periods.iter().enumerate() {
                let x = column(index);
                let paint = colour(index);
                scene.nodes.push(filled(
                    format!("period:{index}"),
                    Rect::new(x, period_top, BOX_WIDTH, period_height),
                    lines.clone(),
                    paint,
                ));
                let mut event_y = line_y + LANE / 2.0;
                for (e, lines) in events[index].iter().enumerate() {
                    let h = height_of(lines);
                    scene.nodes.push(outlined(
                        format!("event:{index}:{e}"),
                        Rect::new(x, event_y, BOX_WIDTH, h),
                        lines.clone(),
                        paint,
                    ));
                    event_y += h + EVENT_GAP;
                }
                if !events[index].is_empty() {
                    let cx = x + BOX_WIDTH / 2.0;
                    scene.edges.push(connector(
                        Point::new(cx, period_top + period_height),
                        Point::new(cx, event_y - EVENT_GAP),
                        paint,
                    ));
                    bottom = bottom.max(event_y - EVENT_GAP);
                }
            }
            let right = column(columns.max(1)) - GAP;
            scene.edges.push(arrow(
                Point::new(PADDING, line_y),
                Point::new(right + GAP / 2.0, line_y),
            ));
            (right + GAP / 2.0 + PADDING, bottom + PADDING)
        }
        Direction::TopDown => {
            let section_x = PADDING;
            let period_x = if timeline.sections.is_empty() {
                PADDING
            } else {
                PADDING + BOX_WIDTH + GAP
            };
            let line_x = period_x + BOX_WIDTH + LANE / 2.0;
            let events_x = line_x + LANE / 2.0;
            let mut y = top;
            let mut right = line_x + LANE / 2.0;
            let mut rows: Vec<(f32, f32)> = Vec::new();
            for (index, lines) in periods.iter().enumerate() {
                let paint = colour(index);
                let tallest = events[index]
                    .iter()
                    .map(|l| height_of(l))
                    .fold(height_of(lines), f32::max);
                scene.nodes.push(filled(
                    format!("period:{index}"),
                    Rect::new(period_x, y, BOX_WIDTH, tallest),
                    lines.clone(),
                    paint,
                ));
                let mut x = events_x;
                for (e, lines) in events[index].iter().enumerate() {
                    scene.nodes.push(outlined(
                        format!("event:{index}:{e}"),
                        Rect::new(x, y, BOX_WIDTH, tallest),
                        lines.clone(),
                        paint,
                    ));
                    x += BOX_WIDTH + EVENT_GAP;
                }
                if !events[index].is_empty() {
                    scene.edges.push(connector(
                        Point::new(period_x + BOX_WIDTH, y + tallest / 2.0),
                        Point::new(x - EVENT_GAP, y + tallest / 2.0),
                        paint,
                    ));
                    right = right.max(x - EVENT_GAP);
                }
                rows.push((y, tallest));
                y += tallest + GAP;
            }
            for (section, lines) in section_labels.iter().enumerate() {
                let members: Vec<usize> = (0..timeline.periods.len())
                    .filter(|&i| timeline.periods[i].section == Some(section))
                    .collect();
                let (top, bottom) = match (members.first(), members.last()) {
                    (Some(&f), Some(&l)) => (rows[f].0, rows[l].0 + rows[l].1),
                    _ => {
                        let at = y;
                        y += section_height + GAP;
                        (at, at + section_height)
                    }
                };
                scene.nodes.push(filled(
                    format!("section:{section}"),
                    Rect::new(section_x, top, BOX_WIDTH, bottom - top),
                    lines.clone(),
                    Paint::Role(Role::Series((section % 6) as u8)),
                ));
            }
            let bottom = (y - GAP).max(top + period_height);
            scene.edges.push(arrow(
                Point::new(line_x, top),
                Point::new(line_x, bottom + GAP / 2.0),
            ));
            (right + PADDING, bottom + GAP / 2.0 + PADDING)
        }
    };

    scene.width = width;
    scene.height = height;
    if let Some(title) = &timeline.title {
        let title_width = measure.width(title, Font::Title.size(FONT_SIZE), true);
        scene.width = scene.width.max(title_width + 2.0 * PADDING);
        scene.title = Some(Text {
            pos: Point::new(scene.width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
        });
    }
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::parse::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const SOURCE: &str = "timeline
    title Product history
    section Founding
      2019 : Idea : Prototype
      2020 : Seed round
    section Growth
      2021 : Launch
           : First customers
      2022
";

    #[test]
    fn every_period_event_and_section_is_drawn() {
        for direction in ["", " TD"] {
            let source = SOURCE.replacen("timeline", &format!("timeline{direction}"), 1);
            let scene = layout(&parse(&source).unwrap(), &Fixed);
            let texts = scene.texts();
            for expected in [
                "Product history",
                "Founding",
                "Growth",
                "2019",
                "2022",
                "Idea",
                "Prototype",
                "Seed round",
                "Launch",
                "First customers",
            ] {
                assert!(texts.iter().any(|t| t == expected), "missing {expected}");
            }
        }
    }

    #[test]
    fn boxes_stay_inside_the_bounds_and_apart() {
        for direction in ["", " TD"] {
            let source = SOURCE.replacen("timeline", &format!("timeline{direction}"), 1);
            let scene = layout(&parse(&source).unwrap(), &Fixed);
            let bounds = scene.bounds();
            for node in &scene.nodes {
                assert!(bounds.encloses(&node.rect, 0.01), "{}", node.id);
            }
            let boxes: Vec<_> = scene
                .nodes
                .iter()
                .filter(|n| !n.id.starts_with("section"))
                .collect();
            for (i, a) in boxes.iter().enumerate() {
                for b in &boxes[i + 1..] {
                    assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
                }
            }
        }
    }
}
