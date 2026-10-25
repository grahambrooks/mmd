//! gantt layout: one row per task against a time axis, rows banded by
//! section, section names in a gutter on the left.

use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge, Font, Marker, Node, Paint, Pen, Point, Rect,
    Role, Scene, Seg, Shape, Style, Text, TextLine, Weight,
};

use super::date::{self, DAY, HOUR, MINUTE, SECOND};
use super::parse::{Gantt, Task, TickUnit};
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const ROW: f32 = 28.0;
const BAR: f32 = 20.0;
const CHART_WIDTH: f32 = 640.0;
const SECTION_WIDTH: f32 = 150.0;
const LABEL_GAP: f32 = 5.0;
const AXIS_HEIGHT: f32 = 22.0;
/// The series colour critical tasks are drawn in; sections skip it.
const CRIT: u8 = 3;

/// Lay a parsed chart out.
pub fn layout(gantt: &Gantt, measure: &dyn Measure) -> Scene {
    let small = Font::Small.size(FONT_SIZE);
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;

    // One row per bar, in source order; a section with no bars (only `vert`
    // markers, say) keeps one empty row so its name has a place.
    let mut rows: Vec<Option<&Task>> = gantt
        .tasks
        .iter()
        .filter(|task| !task.vert && task.section.is_none())
        .map(Some)
        .collect();
    let mut bands: Vec<(usize, usize, usize)> = Vec::new();
    for section in 0..gantt.sections.len() {
        let first = rows.len();
        rows.extend(
            gantt
                .tasks
                .iter()
                .filter(|task| !task.vert && task.section == Some(section))
                .map(Some),
        );
        if rows.len() == first {
            rows.push(None);
        }
        bands.push((section, first, rows.len() - first));
    }
    let t0 = gantt.tasks.iter().map(|t| t.start).min().unwrap_or(0);
    let t1 = gantt
        .tasks
        .iter()
        .map(|t| t.end.max(t.render_end))
        .max()
        .unwrap_or(0);
    let t1 = if t1 <= t0 { t0 + DAY } else { t1 };

    // Section names wrap into a gutter left of the chart.
    let section_lines: Vec<Vec<String>> = gantt
        .sections
        .iter()
        .map(|name| wrap(measure, name, FONT_SIZE, true, SECTION_WIDTH))
        .collect();
    let gutter = section_lines
        .iter()
        .flatten()
        .map(|line| measure.width(line, FONT_SIZE, true))
        .fold(0.0_f32, f32::max);
    let chart_x = PADDING + if gutter > 0.0 { gutter + 16.0 } else { 0.0 };
    let chart_right = chart_x + CHART_WIDTH;
    let x_of = |t: i64| chart_x + ((t - t0) as f64 / (t1 - t0) as f64 * CHART_WIDTH as f64) as f32;

    let title_height = if gantt.title.is_some() {
        Font::Title.size(FONT_SIZE) * 1.35 + 10.0
    } else {
        0.0
    };
    let mut chart_top = PADDING + title_height;
    if gantt.top_axis {
        chart_top += AXIS_HEIGHT;
    }
    let chart_bottom = chart_top + rows.len().max(1) as f32 * ROW;
    let row_top = |row: usize| chart_top + row as f32 * ROW;

    // Section bands, alternately shaded, under everything else.
    let bands: Vec<(usize, f32, f32)> = bands
        .into_iter()
        .map(|(section, first, count)| (section, row_top(first), count as f32 * ROW))
        .collect();
    for (section, top, height) in &bands {
        let fill = if section % 2 == 0 {
            Paint::Role(Role::Surface)
        } else {
            Paint::None
        };
        scene.containers.push(Container {
            rect: Rect::new(PADDING / 2.0, *top, chart_right - PADDING / 2.0, *height),
            kind: ContainerKind::Swimlane,
            label: Vec::new(),
            style: Style {
                fill,
                stroke: Paint::None,
                ..Style::for_container(&ContainerKind::Swimlane)
            },
        });
    }

    // Excluded days, shaded across every row.
    if gantt.calendar.is_active() && t1 - t0 <= 5 * 366 * DAY {
        let mut day = date::start_of_day(t0);
        let mut range: Option<(i64, i64)> = None;
        let mut ranges = Vec::new();
        while day <= t1 {
            if gantt.calendar.excludes(day) {
                range = Some(range.map_or((day, day + DAY), |(s, _)| (s, day + DAY)));
            } else if let Some(r) = range.take() {
                ranges.push(r);
            }
            day += DAY;
        }
        ranges.extend(range);
        for (start, end) in ranges {
            let (left, right) = (x_of(start.max(t0)), x_of(end.min(t1)));
            if right - left < 0.5 {
                continue;
            }
            scene.containers.push(Container {
                rect: Rect::new(left, chart_top, right - left, chart_bottom - chart_top),
                kind: ContainerKind::Swimlane,
                label: Vec::new(),
                style: Style {
                    fill: Paint::Role(Role::Zone(3)),
                    stroke: Paint::None,
                    ..Style::for_container(&ContainerKind::Swimlane)
                },
            });
        }
    }

    // Grid lines and the axis.
    let ticks = ticks(gantt, t0, t1, measure);
    let rule = |from: Point, to: Point, pen: Pen| Edge {
        pen,
        end: Marker::None,
        ..Edge::new(vec![Seg::Move(from), Seg::Line(to)])
    };
    let grid = Pen {
        dash: Dash::Dotted,
        weight: Weight::Thin,
        paint: Paint::Role(Role::Line),
    };
    let axis_pen = Pen {
        dash: Dash::Solid,
        weight: Weight::Thin,
        paint: Paint::Role(Role::Line),
    };
    for (at, _) in &ticks {
        let x = x_of(*at);
        scene.edges.push(rule(
            Point::new(x, chart_top),
            Point::new(x, chart_bottom),
            grid,
        ));
    }
    let axis_y = if gantt.top_axis {
        chart_top
    } else {
        chart_bottom
    };
    scene.edges.push(rule(
        Point::new(chart_x, axis_y),
        Point::new(chart_right, axis_y),
        axis_pen,
    ));
    let label_y = if gantt.top_axis {
        chart_top - 8.0
    } else {
        chart_bottom + 6.0 + small
    };
    for (at, label) in &ticks {
        scene.texts.push(Text {
            pos: Point::new(x_of(*at), label_y),
            anchor: Anchor::Middle,
            line: TextLine::new(label.clone(), Font::Small),
            paint: Paint::Role(Role::Muted),
            // A `vert` marker's line passes behind its date.
            backed: true,
            rotate: 0.0,
        });
    }

    // Section names, centred on their band.
    for ((section, top, height), lines) in bands.iter().zip(&section_lines) {
        let line_height = FONT_SIZE * 1.25;
        let block = lines.len() as f32 * line_height;
        let mut y = top + height / 2.0 - block / 2.0 + FONT_SIZE * 0.95;
        for line in lines {
            scene.texts.push(Text {
                pos: Point::new(PADDING, y),
                anchor: Anchor::Start,
                line: TextLine::new(line.clone(), Font::Bold),
                paint: Paint::Role(Role::Series(section_series(*section))),
                backed: false,
                rotate: 0.0,
            });
            y += line_height;
        }
    }

    // Bars and milestones, with their names inside or beside them.
    let mut right_edge = chart_right;
    for (row, task) in rows.iter().enumerate() {
        let Some(task) = task else { continue };
        let center_y = row_top(row) + ROW / 2.0;
        let look = look(task);
        let (left, right) = if task.milestone {
            let mid = task.start + (task.end - task.start) / 2;
            let x = x_of(mid);
            let half = BAR / 2.0 + 1.0;
            let mut node = Node::new(
                format!("task:{}", task.id),
                Shape::Diamond,
                Rect::centered(Point::new(x, center_y), half * 2.0, half * 2.0),
            );
            node.style = look.style;
            scene.nodes.push(node);
            (x - half, x + half)
        } else {
            let (left, right) = (x_of(task.start), x_of(task.render_end));
            let width = (right - left).max(2.0);
            let mut node = Node::new(
                format!("task:{}", task.id),
                Shape::Rect,
                Rect::new(left, center_y - BAR / 2.0, width, BAR),
            );
            node.style = look.style;
            scene.nodes.push(node);
            (left, left + width)
        };

        let text_width = measure.width(&task.name, small, false);
        let baseline = center_y + small * 0.35;
        let inside = !task.milestone && text_width + 8.0 <= right - left;
        let (x, anchor, paint) = if inside {
            ((left + right) / 2.0, Anchor::Middle, look.ink)
        } else if right + LABEL_GAP + text_width <= chart_right
            || left - LABEL_GAP - text_width < chart_x
        {
            (right + LABEL_GAP, Anchor::Start, Paint::Role(Role::Text))
        } else {
            (left - LABEL_GAP, Anchor::End, Paint::Role(Role::Text))
        };
        if anchor == Anchor::Start {
            right_edge = right_edge.max(x + text_width + 4.0);
        }
        right_edge = right_edge.max(right);
        scene.texts.push(Text {
            pos: Point::new(x, baseline),
            anchor,
            line: TextLine::new(task.name.clone(), Font::Small),
            paint,
            backed: false,
            rotate: 0.0,
        });
    }

    // `vert` markers: a line across every row, named under the axis.
    let mut vert_rows: Vec<f32> = Vec::new();
    let mut bottom = chart_bottom + if gantt.top_axis { 0.0 } else { AXIS_HEIGHT };
    for task in gantt.tasks.iter().filter(|task| task.vert) {
        let x = x_of(task.start);
        let width = measure.width(&task.name, small, false);
        let half = width / 2.0;
        // The first row whose last label ends before this one starts.
        let row = vert_rows
            .iter()
            .position(|&end| end + 8.0 < x - half)
            .unwrap_or_else(|| {
                vert_rows.push(f32::MIN);
                vert_rows.len() - 1
            });
        vert_rows[row] = x + half;
        let label_y = bottom + 4.0 + small + row as f32 * (small + 6.0);
        scene.edges.push(rule(
            Point::new(x, chart_top),
            Point::new(x, label_y - small),
            Pen {
                dash: Dash::Solid,
                weight: Weight::Normal,
                paint: Paint::Role(Role::Accent),
            },
        ));
        scene.texts.push(Text {
            pos: Point::new(x, label_y),
            anchor: Anchor::Middle,
            line: TextLine::new(task.name.clone(), Font::Small),
            paint: Paint::Role(Role::Accent),
            backed: false,
            rotate: 0.0,
        });
        right_edge = right_edge.max(x + half);
    }
    if !vert_rows.is_empty() {
        bottom += 8.0 + vert_rows.len() as f32 * (small + 6.0);
    }

    // Tick labels hang half their width past the last tick.
    for (at, label) in &ticks {
        right_edge = right_edge.max(x_of(*at) + measure.width(label, small, false) / 2.0);
    }

    scene.width = right_edge + PADDING;
    scene.height = bottom + PADDING;
    if let Some(title) = &gantt.title {
        scene.title = Some(Text {
            pos: Point::new(scene.width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
        let title_width = measure.width(title, Font::Title.size(FONT_SIZE), true);
        if title_width + 2.0 * PADDING > scene.width {
            scene.width = title_width + 2.0 * PADDING;
            if let Some(title) = &mut scene.title {
                title.pos.x = scene.width / 2.0;
            }
        }
    }
    scene
}

/// Sections cycle through the series colours, skipping the critical one.
fn section_series(section: usize) -> u8 {
    const SERIES: [u8; 5] = [0, 1, 2, 4, 5];
    SERIES[section % SERIES.len()]
}

struct Look {
    style: Style,
    /// Text drawn on the bar.
    ink: Paint,
}

/// A task's fill and outline: solid in its section's colour; hollow and
/// heavy while active; greyed when done; red when critical.
fn look(task: &Task) -> Look {
    let series = Paint::Role(Role::Series(match task.section {
        Some(section) => section_series(section),
        None => 0,
    }));
    let crit = Paint::Role(Role::Series(CRIT));
    let colour = if task.crit { crit } else { series };
    let base = Style::for_shape(Shape::Rect);
    if task.done {
        Look {
            style: Style {
                fill: Paint::Role(Role::Surface),
                stroke: if task.crit {
                    crit
                } else {
                    Paint::Role(Role::Muted)
                },
                emphasis: task.crit,
                ..base
            },
            ink: Paint::Role(Role::Muted),
        }
    } else if task.active {
        Look {
            style: Style {
                fill: Paint::Role(Role::Background),
                stroke: colour,
                emphasis: true,
                ..base
            },
            ink: Paint::Role(Role::Text),
        }
    } else {
        Look {
            style: Style {
                fill: colour,
                stroke: colour,
                ..base
            },
            ink: Paint::Role(Role::Background),
        }
    }
}

/// Axis ticks and their labels: the `tickInterval` if one is set, otherwise
/// the finest calendar step whose labels do not collide.
fn ticks(gantt: &Gantt, t0: i64, t1: i64, measure: &dyn Measure) -> Vec<(i64, String)> {
    let small = Font::Small.size(FONT_SIZE);
    let labelled = |ticks: Vec<i64>| -> Vec<(i64, String)> {
        ticks
            .into_iter()
            .map(|at| {
                // Checked when parsed, so formatting cannot fail here.
                let label = date::format_axis(at, &gantt.axis_format).unwrap_or_default();
                (at, label)
            })
            .collect()
    };
    if let Some((count, unit)) = gantt.tick_interval {
        let ticks = generate(t0, t1, count, unit, gantt.week_start);
        if !ticks.is_empty() && ticks.len() <= 400 {
            return labelled(ticks);
        }
    }
    const STEPS: &[(i64, TickUnit)] = &[
        (1, TickUnit::Second),
        (5, TickUnit::Second),
        (15, TickUnit::Second),
        (30, TickUnit::Second),
        (1, TickUnit::Minute),
        (5, TickUnit::Minute),
        (15, TickUnit::Minute),
        (30, TickUnit::Minute),
        (1, TickUnit::Hour),
        (3, TickUnit::Hour),
        (6, TickUnit::Hour),
        (12, TickUnit::Hour),
        (1, TickUnit::Day),
        (2, TickUnit::Day),
        (1, TickUnit::Week),
        (2, TickUnit::Week),
        (1, TickUnit::Month),
        (3, TickUnit::Month),
        (6, TickUnit::Month),
        (12, TickUnit::Month),
        (60, TickUnit::Month),
        (120, TickUnit::Month),
    ];
    let span = (t1 - t0) as f32;
    let mut fallback = Vec::new();
    for &(count, unit) in STEPS {
        // Skip steps that would make hundreds of ticks without generating them.
        if (t1 - t0) / approx_ms(count, unit) > 200 {
            continue;
        }
        let ticks = labelled(generate(t0, t1, count, unit, gantt.week_start));
        let widest = ticks
            .iter()
            .map(|(_, label)| measure.width(label, small, false))
            .fold(0.0_f32, f32::max);
        let spacing = CHART_WIDTH * approx_ms(count, unit) as f32 / span;
        if ticks.len() >= 2 && spacing >= widest + 14.0 {
            return ticks;
        }
        if !ticks.is_empty() {
            fallback = ticks;
        }
    }
    fallback
}

fn approx_ms(count: i64, unit: TickUnit) -> i64 {
    count
        * match unit {
            TickUnit::Millisecond => 1,
            TickUnit::Second => SECOND,
            TickUnit::Minute => MINUTE,
            TickUnit::Hour => HOUR,
            TickUnit::Day => DAY,
            TickUnit::Week => 7 * DAY,
            TickUnit::Month => 30 * DAY,
        }
}

/// Ticks every `count` units within `t0..=t1`, on calendar boundaries.
fn generate(t0: i64, t1: i64, count: i64, unit: TickUnit, week_start: u32) -> Vec<i64> {
    let mut out = Vec::new();
    let mut push_while = |mut at: i64, next: &dyn Fn(i64) -> i64| {
        while at <= t1 && out.len() <= 1000 {
            if at >= t0 {
                out.push(at);
            }
            at = next(at);
        }
    };
    match unit {
        TickUnit::Month => {
            let civil = date::Civil::of(t0);
            let mut month = civil.year * 12 + civil.month as i64 - 1;
            if count > 1 && 12 % count == 0 {
                month -= month.rem_euclid(count);
            }
            let first =
                date::days_from_civil(month.div_euclid(12), (month.rem_euclid(12) + 1) as u32, 1)
                    * DAY;
            push_while(first, &|at| date::add_months(at, count));
        }
        TickUnit::Week => {
            let mut first = date::start_of_day(t0);
            while date::iso_weekday(first.div_euclid(DAY)) != week_start {
                first -= DAY;
            }
            push_while(first, &|at| at + count * 7 * DAY);
        }
        _ => {
            let step = approx_ms(count, unit);
            let first = t0.div_euclid(step) * step;
            push_while(first, &|at| at + step);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gantt::parse::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const SOURCE: &str = "gantt
    title Release plan
    dateFormat YYYY-MM-DD
    excludes weekends
    section Build
    Design the thing        :done, a1, 2024-01-01, 3d
    Implement               :active, a2, after a1, 10d
    section Ship
    Freeze                  :milestone, m1, after a2, 0d
    Release candidate tests :crit, after m1, 2d
    Cutover                 :vert, v1, after m1, 0d
";

    #[test]
    fn every_task_section_and_title_is_drawn() {
        let gantt = parse(SOURCE).unwrap();
        let scene = layout(&gantt, &Fixed);
        let texts = scene.texts();
        for expected in [
            "Release plan",
            "Build",
            "Ship",
            "Design the thing",
            "Implement",
            "Freeze",
            "Release candidate tests",
            "Cutover",
        ] {
            assert!(texts.iter().any(|t| t == expected), "missing {expected}");
        }
        // Four bars (the vert marker is a line, not a bar).
        assert_eq!(
            scene
                .nodes
                .iter()
                .filter(|n| n.id.starts_with("task:"))
                .count(),
            4
        );
        assert!(scene.node("task:m1").unwrap().shape == Shape::Diamond);
    }

    #[test]
    fn bars_follow_time_and_stay_inside_the_bounds() {
        let gantt = parse(SOURCE).unwrap();
        let scene = layout(&gantt, &Fixed);
        let design = scene.node("task:a1").unwrap().rect;
        let implement = scene.node("task:a2").unwrap().rect;
        assert!(design.right() <= implement.x + 0.5);
        assert!(implement.y > design.y, "one row per task");
        let bounds = scene.bounds();
        for node in &scene.nodes {
            assert!(bounds.encloses(&node.rect, 0.01), "{}", node.id);
        }
        for text in &scene.texts {
            assert!(text.pos.x >= 0.0 && text.pos.x <= scene.width);
            assert!(text.pos.y >= 0.0 && text.pos.y <= scene.height);
        }
    }

    #[test]
    fn ticks_land_on_calendar_boundaries() {
        let t0 = date::days_from_civil(2024, 1, 10) * DAY;
        let t1 = date::days_from_civil(2024, 6, 1) * DAY;
        let months = generate(t0, t1, 1, TickUnit::Month, 7);
        assert_eq!(months.len(), 5);
        assert_eq!(date::Civil::of(months[0]).day, 1);
        let weeks = generate(t0, t1, 1, TickUnit::Week, 1);
        assert!(
            weeks
                .iter()
                .all(|w| date::iso_weekday(w.div_euclid(DAY)) == 1)
        );
    }
}
