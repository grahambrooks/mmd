//! User-journey layout: sections over a row of tasks, a time arrow, and
//! under each task a face set higher the better its score, with an actor
//! legend on the left.

use diagram_scene::{
    Anchor, Dash, Edge, Font, Legend, LegendEntry, Marker, Node, Paint, Pen, Point, Rect, Role,
    Scene, Seg, Shape, Style, Text, TextLine, Weight,
};

use super::parse::Journey;
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const TASK_WIDTH: f32 = 130.0;
const GAP: f32 = 14.0;
const PAD: f32 = 8.0;
/// Vertical distance between adjacent scores.
const STEP: f32 = 24.0;
const FACE: f32 = 13.0;
const DOT: f32 = 5.0;

/// Lay a parsed journey out.
pub fn layout(journey: &Journey, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let line_height = FONT_SIZE * 1.35;
    let title_height = if journey.title.is_some() {
        Font::Title.size(FONT_SIZE) * 1.35 + 12.0
    } else {
        0.0
    };
    let top = PADDING + title_height;
    let actor_paint = |actor: usize| Paint::Role(Role::Series((actor % 6) as u8));
    let section_paint = |section: Option<usize>| match section {
        Some(section) => Paint::Role(Role::Series((section % 6) as u8)),
        None => Paint::Role(Role::Muted),
    };

    // The actor legend, left of the tasks.
    let mut x0 = PADDING;
    if !journey.actors.is_empty() {
        let row = FONT_SIZE * 1.6;
        let widest = journey
            .actors
            .iter()
            .map(|a| measure.width(a, Font::Small.size(FONT_SIZE), false))
            .fold(0.0_f32, f32::max);
        let width = 10.0 + row * 1.2 + 8.0 + widest + 12.0;
        let height = 16.0 + row * journey.actors.len() as f32;
        scene.legend = Some(Legend {
            rect: Rect::new(PADDING, top, width, height),
            entries: journey
                .actors
                .iter()
                .enumerate()
                .map(|(i, actor)| LegendEntry {
                    shape: Shape::Circle,
                    style: Style {
                        fill: actor_paint(i),
                        stroke: actor_paint(i),
                        ..Style::for_shape(Shape::Circle)
                    },
                    label: actor.clone(),
                })
                .collect(),
        });
        x0 += width + 24.0;
    }
    let column = |index: usize| x0 + index as f32 * (TASK_WIDTH + GAP);

    let wrapped = |text: &str, bold: bool| -> Vec<String> {
        wrap(measure, text, FONT_SIZE, bold, TASK_WIDTH - 2.0 * PAD)
    };
    let task_lines: Vec<Vec<String>> = journey
        .tasks
        .iter()
        .map(|t| wrapped(&t.name, false))
        .collect();
    let task_height = task_lines
        .iter()
        .map(|l| l.len() as f32 * line_height + 2.0 * PAD + DOT)
        .fold(line_height + 2.0 * PAD, f32::max);

    // Sections: one header over each run of consecutive tasks.
    let mut runs: Vec<(Option<usize>, usize, usize)> = Vec::new();
    for (index, task) in journey.tasks.iter().enumerate() {
        match runs.last_mut() {
            Some((section, _, last)) if *section == task.section => *last = index,
            _ => runs.push((task.section, index, index)),
        }
    }
    // A section with no tasks still appears, after the last task.
    let mut columns = journey.tasks.len();
    for section in 0..journey.sections.len() {
        if !journey.tasks.iter().any(|t| t.section == Some(section)) {
            runs.push((Some(section), columns, columns));
            columns += 1;
        }
    }
    let header_lines: Vec<Vec<String>> = runs
        .iter()
        .map(|(section, first, last)| match section {
            Some(s) => {
                let width = column(*last) + TASK_WIDTH - column(*first);
                wrap(
                    measure,
                    &journey.sections[*s],
                    FONT_SIZE,
                    true,
                    width - 2.0 * PAD,
                )
            }
            None => Vec::new(),
        })
        .collect();
    let has_sections = runs.iter().any(|(section, _, _)| section.is_some());
    let header_height = header_lines
        .iter()
        .map(|l| l.len() as f32 * line_height + 2.0 * PAD)
        .fold(0.0_f32, f32::max);
    let task_top = if has_sections {
        top + header_height + GAP / 2.0
    } else {
        top
    };

    for ((section, first, last), lines) in runs.iter().zip(&header_lines) {
        let Some(section) = section else { continue };
        let left = column(*first);
        let right = column(*last) + TASK_WIDTH;
        let paint = section_paint(Some(*section));
        let mut node = Node::new(
            format!("section:{section}:{first}"),
            Shape::Rounded,
            Rect::new(left, top, right - left, header_height),
        );
        node.label = lines
            .iter()
            .map(|l| TextLine::new(l.clone(), Font::Bold))
            .collect();
        node.style.fill = paint;
        node.style.stroke = paint;
        node.style.text = Paint::Role(Role::Background);
        scene.nodes.push(node);
    }

    let line_y = task_top + task_height + 22.0;
    let score_top = line_y + 22.0;
    let face_y = |score: f32| score_top + FACE + (5.0 - score) * STEP;
    let pen = |dash: Dash, weight: Weight, paint: Paint| Pen {
        dash,
        weight,
        paint,
    };
    let plain = |path: Vec<Seg>, pen: Pen| Edge {
        pen,
        end: Marker::None,
        ..Edge::new(path)
    };

    for (index, task) in journey.tasks.iter().enumerate() {
        let x = column(index);
        let cx = x + TASK_WIDTH / 2.0;
        let paint = section_paint(task.section);
        let mut node = Node::new(
            format!("task:{index}"),
            Shape::Rounded,
            Rect::new(x, task_top, TASK_WIDTH, task_height),
        );
        node.label = task_lines[index]
            .iter()
            .map(|l| TextLine::new(l.clone(), Font::Body))
            .collect();
        node.style.stroke = paint;
        scene.nodes.push(node);

        // Who takes part: dots along the top edge.
        for (slot, &actor) in task.actors.iter().enumerate() {
            let at = Point::new(x + 12.0 + slot as f32 * (DOT * 2.0 + 3.0), task_top);
            let mut dot = Node::new(
                format!("task:{index}:actor:{actor}"),
                Shape::Circle,
                Rect::centered(at, DOT * 2.0, DOT * 2.0),
            );
            dot.style.fill = actor_paint(actor);
            dot.style.stroke = Paint::Role(Role::Background);
            scene.nodes.push(dot);
        }

        // The face, joined to its task by a dotted drop line.
        let cy = face_y(task.score);
        let mood = mood(task.score);
        scene.edges.push(plain(
            vec![
                Seg::Move(Point::new(cx, task_top + task_height)),
                Seg::Line(Point::new(cx, cy - FACE)),
            ],
            pen(Dash::Dotted, Weight::Thin, Paint::Role(Role::Muted)),
        ));
        scene.edges.push(plain(
            circle(Point::new(cx, cy), FACE),
            pen(Dash::Solid, Weight::Thick, mood),
        ));
        let mouth = if task.score > 3.0 {
            // A smile: the arc's middle dips below its ends.
            let (l, r) = (
                Point::new(cx - 6.0, cy + 3.0),
                Point::new(cx + 6.0, cy + 3.0),
            );
            vec![
                Seg::Move(l),
                Seg::Cubic(
                    Point::new(cx - 4.0, cy + 8.0),
                    Point::new(cx + 4.0, cy + 8.0),
                    r,
                ),
            ]
        } else if task.score < 3.0 {
            let (l, r) = (
                Point::new(cx - 6.0, cy + 7.0),
                Point::new(cx + 6.0, cy + 7.0),
            );
            vec![
                Seg::Move(l),
                Seg::Cubic(
                    Point::new(cx - 4.0, cy + 2.0),
                    Point::new(cx + 4.0, cy + 2.0),
                    r,
                ),
            ]
        } else {
            vec![
                Seg::Move(Point::new(cx - 5.0, cy + 5.0)),
                Seg::Line(Point::new(cx + 5.0, cy + 5.0)),
            ]
        };
        scene
            .edges
            .push(plain(mouth, pen(Dash::Solid, Weight::Normal, mood)));
        for dx in [-FACE / 3.0, FACE / 3.0] {
            let mut eye = Node::new(
                format!("task:{index}:eye:{dx}"),
                Shape::Circle,
                Rect::centered(Point::new(cx + dx, cy - FACE / 3.0), 3.4, 3.4),
            );
            eye.style.fill = mood;
            eye.style.stroke = mood;
            scene.nodes.push(eye);
        }
    }

    let right = column(columns.max(1)) - GAP;
    scene.edges.push(Edge {
        pen: pen(Dash::Solid, Weight::Thick, Paint::Role(Role::Line)),
        end: Marker::Arrow,
        ..Edge::new(vec![
            Seg::Move(Point::new(x0, line_y)),
            Seg::Line(Point::new(right, line_y)),
        ])
    });

    let legend_bottom = scene.legend.as_ref().map_or(0.0, |l| l.rect.bottom());
    scene.width = right + PADDING;
    scene.height = (face_y(0.0) + FACE + PADDING).max(legend_bottom + PADDING);
    if let Some(title) = &journey.title {
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

/// Green for a good score, amber for neutral, red for a bad one.
fn mood(score: f32) -> Paint {
    Paint::Role(Role::Series(if score > 3.0 {
        1
    } else if score < 3.0 {
        3
    } else {
        4
    }))
}

/// A circle as four cubic arcs.
fn circle(center: Point, r: f32) -> Vec<Seg> {
    let k = r * 0.552_284_8;
    let (x, y) = (center.x, center.y);
    vec![
        Seg::Move(Point::new(x + r, y)),
        Seg::Cubic(
            Point::new(x + r, y + k),
            Point::new(x + k, y + r),
            Point::new(x, y + r),
        ),
        Seg::Cubic(
            Point::new(x - k, y + r),
            Point::new(x - r, y + k),
            Point::new(x - r, y),
        ),
        Seg::Cubic(
            Point::new(x - r, y - k),
            Point::new(x - k, y - r),
            Point::new(x, y - r),
        ),
        Seg::Cubic(
            Point::new(x + k, y - r),
            Point::new(x + r, y - k),
            Point::new(x + r, y),
        ),
        Seg::Close,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journey::parse::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const SOURCE: &str = "journey
    title My working day
    section Go to work
      Make tea: 5: Me
      Go upstairs: 3: Me
      Do work: 1: Me, Cat
    section Go home
      Go downstairs: 5: Me
      Sit down: 4: Me
";

    #[test]
    fn every_task_section_and_actor_is_drawn() {
        let scene = layout(&parse(SOURCE).unwrap(), &Fixed);
        let texts = scene.texts();
        for expected in [
            "My working day",
            "Go to work",
            "Go home",
            "Make tea",
            "Do work",
            "Sit down",
            "Me",
            "Cat",
        ] {
            assert!(texts.iter().any(|t| t == expected), "missing {expected}");
        }
    }

    #[test]
    fn happier_faces_sit_higher_and_everything_fits() {
        let scene = layout(&parse(SOURCE).unwrap(), &Fixed);
        let eye = |task: usize| {
            scene
                .nodes
                .iter()
                .find(|n| n.id.starts_with(&format!("task:{task}:eye")))
                .unwrap()
                .rect
                .y
        };
        assert!(eye(0) < eye(1) && eye(1) < eye(2));
        let bounds = scene.bounds();
        for node in &scene.nodes {
            assert!(bounds.encloses(&node.rect, 0.01), "{}", node.id);
        }
        assert!(bounds.encloses(&scene.legend.as_ref().unwrap().rect, 0.01));
    }
}
