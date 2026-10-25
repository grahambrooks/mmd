//! An xychart: a category (or numeric) axis, a value axis with round ticks,
//! bars grouped side by side within each category, and lines over them.

use diagram_scene::{Anchor, Font, Paint, Point, Rect, Role, Scene, Seg, Text, Weight};

use super::parse::{Plot, XData, XyChart};
use crate::chart;
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

/// The value axis's length.
const VALUE_LENGTH: f32 = 280.0;
/// The category axis's least length, and the least room per category.
const CATEGORY_LENGTH: f32 = 460.0;
const MIN_SLOT: f32 = 30.0;
const TICK: f32 = 5.0;

pub(crate) fn layout(c: &XyChart, measure: &dyn Measure) -> Scene {
    let small = Font::Small.size(FONT_SIZE);
    let line_h = small * 1.3;
    let mut scene = Scene::new(0.0, 0.0);

    // The category axis: labels and where each data index sits, as a
    // fraction of the axis.
    let longest = c
        .series
        .iter()
        .map(|s| s.values.len())
        .max()
        .unwrap_or(1)
        .max(1);
    let (slots, labels): (usize, Vec<(f32, String)>) = match &c.x.data {
        XData::Bands(bands) => (
            bands.len().max(1),
            bands
                .iter()
                .enumerate()
                .map(|(i, b)| ((i as f32 + 0.5) / bands.len() as f32, b.clone()))
                .collect(),
        ),
        XData::Auto => (
            longest,
            (0..longest)
                .map(|i| ((i as f32 + 0.5) / longest as f32, (i + 1).to_string()))
                .collect(),
        ),
        XData::Range(a, b) => {
            let (lo, hi) = (a.min(*b), a.max(*b));
            let (_, _, ticks) = chart::ticks(lo, hi, 6, false);
            let labels = ticks
                .into_iter()
                .map(|t| (linear(t, *a, *b, longest), chart::format(t)))
                .collect();
            (longest, labels)
        }
    };
    // Where series `s`'s `i`th value sits on the category axis (0..1).
    let at = |values: usize, i: usize| -> f32 {
        match &c.x.data {
            XData::Range(a, b) => {
                let v = if values <= 1 {
                    *a
                } else {
                    a + (b - a) * i as f64 / (values - 1) as f64
                };
                linear(v, *a, *b, longest)
            }
            _ => (i as f32 + 0.5) / slots as f32,
        }
    };

    // The value axis.
    let data = c.series.iter().flat_map(|s| s.values.iter().copied());
    let (dmin, dmax) = data.fold((f64::MAX, f64::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let has_bars = c.series.iter().any(|s| s.plot == Plot::Bar);
    let (lo, hi, ticks) = match c.y.range {
        Some((a, b)) => chart::ticks(a.min(b), a.max(b), 5, false),
        None => {
            let (mut lo, mut hi) = (dmin, dmax);
            if has_bars {
                lo = lo.min(0.0);
                hi = hi.max(0.0);
            }
            chart::ticks(lo, hi, 5, true)
        }
    };
    let tick_labels: Vec<String> = ticks.iter().map(|t| chart::format(*t)).collect();
    let tick_width = tick_labels
        .iter()
        .map(|t| measure.width(t, small, false))
        .fold(0.0_f32, f32::max);

    let title_h = chart::place_title(&c.title, 0.0).1;
    let top = PADDING + title_h;
    let cat_length = CATEGORY_LENGTH.max(slots as f32 * MIN_SLOT);
    let slot = cat_length / slots as f32;

    let (plot, mut right, mut bottom);
    if !c.horizontal {
        // Categories along the bottom, values up the left.
        let wrapped: Vec<Vec<String>> = labels
            .iter()
            .map(|(_, l)| wrap(measure, l, small, false, (slot - 4.0).max(24.0)))
            .collect();
        let rows = wrapped.iter().map(Vec::len).max().unwrap_or(1) as f32;
        // The value axis's title runs up its left, as mermaid rotates it.
        let (room, reach) = side_title(&c.y.title, VALUE_LENGTH, small, measure);
        let left = PADDING + room + tick_width + TICK + 4.0;
        plot = Rect::new(left, top + small + reach, cat_length, VALUE_LENGTH);
        right = 0.0_f32;
        if let Some(t) = side_title_text(&c.y.title, plot) {
            scene.texts.push(t);
        }
        for (((frac, _), lines), _) in labels.iter().zip(&wrapped).zip(0..) {
            let x = plot.x + frac * plot.width;
            scene.edges.push(chart::stroke(
                chart::segment(
                    Point::new(x, plot.bottom()),
                    Point::new(x, plot.bottom() + TICK),
                ),
                Paint::Role(Role::Line),
                Weight::Thin,
            ));
            for (j, l) in lines.iter().enumerate() {
                scene.texts.push(chart::text(
                    Point::new(x, plot.bottom() + TICK + 4.0 + small + j as f32 * line_h),
                    Anchor::Middle,
                    l,
                    Font::Small,
                    Paint::Role(Role::Text),
                ));
                right = right.max(x + measure.width(l, small, false) / 2.0);
            }
        }
        bottom = plot.bottom() + TICK + 4.0 + rows * line_h;
        if let Some(t) = &c.x.title {
            bottom += 6.0;
            scene.texts.push(chart::text(
                Point::new(plot.center().x, bottom + FONT_SIZE),
                Anchor::Middle,
                t,
                Font::Bold,
                Paint::Role(Role::Text),
            ));
            right = right.max(plot.center().x + measure.width(t, FONT_SIZE, true) / 2.0);
            bottom += FONT_SIZE * 1.35;
        }
        for (t, label) in ticks.iter().zip(&tick_labels) {
            let y = value_y(*t, lo, hi, plot);
            scene.edges.push(chart::stroke(
                chart::segment(Point::new(plot.x - TICK, y), Point::new(plot.x, y)),
                Paint::Role(Role::Line),
                Weight::Thin,
            ));
            scene.texts.push(chart::text(
                Point::new(plot.x - TICK - 3.0, y + small * 0.35),
                Anchor::End,
                label,
                Font::Small,
                Paint::Role(Role::Muted),
            ));
        }
    } else {
        // Categories down the left, values along the top.
        let wrapped: Vec<Vec<String>> = labels
            .iter()
            .map(|(_, l)| wrap(measure, l, small, false, 160.0))
            .collect();
        let label_width = wrapped
            .iter()
            .flatten()
            .map(|l| measure.width(l, small, false))
            .fold(0.0_f32, f32::max);
        let y_title_h = if c.y.title.is_some() {
            FONT_SIZE * 1.6
        } else {
            0.0
        };
        let tick_row = small * 1.3 + TICK;
        let rows = wrapped.iter().map(Vec::len).max().unwrap_or(1) as f32;
        let band = MIN_SLOT.max(rows * line_h + 8.0);
        let height = (slots as f32 * band).max(VALUE_LENGTH);
        // The category axis is the left one here, and mermaid rotates the
        // left axis's title whichever axis it is.
        let (room, reach) = side_title(&c.x.title, height, 0.0, measure);
        let left = PADDING + room + label_width + TICK + 4.0;
        plot = Rect::new(
            left,
            top + y_title_h + tick_row + reach,
            VALUE_LENGTH * 1.6,
            height,
        );
        right = plot.right();
        if let Some(t) = side_title_text(&c.x.title, plot) {
            scene.texts.push(t);
        }
        if let Some(t) = &c.y.title {
            scene.texts.push(chart::text(
                Point::new(plot.center().x, top + FONT_SIZE),
                Anchor::Middle,
                t,
                Font::Bold,
                Paint::Role(Role::Text),
            ));
            right = right.max(plot.center().x + measure.width(t, FONT_SIZE, true) / 2.0);
        }
        for ((frac, _), lines) in labels.iter().zip(&wrapped) {
            let y = plot.y + frac * plot.height;
            scene.edges.push(chart::stroke(
                chart::segment(Point::new(plot.x - TICK, y), Point::new(plot.x, y)),
                Paint::Role(Role::Line),
                Weight::Thin,
            ));
            let first = y - (lines.len() as f32 - 1.0) * line_h / 2.0 + small * 0.35;
            for (j, l) in lines.iter().enumerate() {
                scene.texts.push(chart::text(
                    Point::new(plot.x - TICK - 3.0, first + j as f32 * line_h),
                    Anchor::End,
                    l,
                    Font::Small,
                    Paint::Role(Role::Text),
                ));
            }
        }
        for (t, label) in ticks.iter().zip(&tick_labels) {
            let x = value_x(*t, lo, hi, plot);
            scene.edges.push(chart::stroke(
                chart::segment(Point::new(x, plot.y - TICK), Point::new(x, plot.y)),
                Paint::Role(Role::Line),
                Weight::Thin,
            ));
            scene.texts.push(chart::text(
                Point::new(x, plot.y - TICK - 3.0),
                Anchor::Middle,
                label,
                Font::Small,
                Paint::Role(Role::Muted),
            ));
            right = right.max(x + measure.width(label, small, false) / 2.0);
        }
        bottom = plot.bottom();
    }
    // A side title longer than the plot hangs below it.
    for t in &scene.texts {
        if t.rotate != 0.0 {
            bottom = bottom.max(chart::text_rect(t, measure).bottom());
        }
    }

    // Axis lines along the plot's edges.
    let axes = if c.horizontal {
        [
            chart::segment(
                Point::new(plot.x, plot.y),
                Point::new(plot.x, plot.bottom()),
            ),
            chart::segment(Point::new(plot.x, plot.y), Point::new(plot.right(), plot.y)),
        ]
    } else {
        [
            chart::segment(
                Point::new(plot.x, plot.y),
                Point::new(plot.x, plot.bottom()),
            ),
            chart::segment(
                Point::new(plot.x, plot.bottom()),
                Point::new(plot.right(), plot.bottom()),
            ),
        ]
    };
    for a in axes {
        scene
            .edges
            .push(chart::stroke(a, Paint::Role(Role::Line), Weight::Normal));
    }

    // Bars grouped within each slot, and lines. Bars are areas and lines
    // edges, which paint over areas, so a line declared after the bars is
    // drawn over them, as mermaid does.
    let bars: Vec<usize> = (0..c.series.len())
        .filter(|i| c.series[*i].plot == Plot::Bar)
        .collect();
    let along_axis = if c.horizontal {
        plot.height
    } else {
        plot.width
    };
    let group = along_axis / slots as f32 * 0.8;
    let bar = group / bars.len().max(1) as f32;
    let base = 0f64.clamp(lo, hi);
    for (si, s) in c.series.iter().enumerate() {
        let paint = Paint::Role(Role::Series(si as u8));
        if s.plot == Plot::Bar {
            let k = bars.iter().position(|b| *b == si).unwrap_or(0);
            for (i, value) in s.values.iter().enumerate() {
                let along = at(s.values.len(), i);
                let offset = -group / 2.0 + bar * k as f32;
                let v = value.clamp(lo, hi);
                let rect = if c.horizontal {
                    let (x0, x1) = (value_x(base, lo, hi, plot), value_x(v, lo, hi, plot));
                    let y = plot.y + along * plot.height + offset;
                    Rect::new(x0.min(x1), y, (x1 - x0).abs(), bar)
                } else {
                    let (y0, y1) = (value_y(base, lo, hi, plot), value_y(v, lo, hi, plot));
                    let x = plot.x + along * plot.width + offset;
                    Rect::new(x, y0.min(y1), bar, (y1 - y0).abs())
                };
                scene
                    .areas
                    .push(chart::area(chart::rect_path(rect), paint, 1.0, None));
            }
            continue;
        }
        let points: Vec<Point> = s
            .values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                let v = value.clamp(lo, hi);
                let along = at(s.values.len(), i);
                if c.horizontal {
                    Point::new(value_x(v, lo, hi, plot), plot.y + along * plot.height)
                } else {
                    Point::new(plot.x + along * plot.width, value_y(v, lo, hi, plot))
                }
            })
            .collect();
        let mut path = vec![Seg::Move(points[0])];
        path.extend(points.iter().skip(1).map(|p| Seg::Line(*p)));
        if points.len() == 1 {
            path.push(Seg::Line(points[0]));
        }
        scene.edges.push(chart::stroke(path, paint, Weight::Thick));
    }

    right = right.max(plot.right());
    let entries: Vec<(Paint, String)> = c
        .series
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            s.title
                .clone()
                .map(|t| (Paint::Role(Role::Series(i as u8)), t))
        })
        .collect();
    if !entries.is_empty() {
        let legend = chart::legend(entries, Point::new(right + 20.0, plot.y), measure);
        right = legend.rect.right();
        bottom = bottom.max(legend.rect.bottom());
        scene.legend = Some(legend);
    }

    let width = (right + PADDING).max(chart::title_width(&c.title, measure));
    scene.title = chart::place_title(&c.title, width).0;
    scene.width = width;
    scene.height = bottom + PADDING;
    scene
}

/// The room a left-side title takes across (its line height), and how far
/// the plot must move down so a title longer than `length` (the plot's
/// height, with `margin` above it already) is not cut off at the top.
fn side_title(
    title: &Option<String>,
    length: f32,
    margin: f32,
    measure: &dyn Measure,
) -> (f32, f32) {
    match title {
        Some(t) => {
            let w = measure.width(t, Font::Bold.size(FONT_SIZE), true);
            (
                Font::Bold.size(FONT_SIZE) * 1.35,
                (w / 2.0 - length / 2.0 - margin).max(0.0),
            )
        }
        None => (0.0, 0.0),
    }
}

/// A left-side title, rotated to read upwards, centred on the plot.
fn side_title_text(title: &Option<String>, plot: Rect) -> Option<Text> {
    title.as_ref().map(|t| Text {
        rotate: -90.0,
        ..chart::text(
            // The ascent lies to the left of the baseline once rotated.
            Point::new(PADDING + Font::Bold.size(FONT_SIZE) * 0.8, plot.center().y),
            Anchor::Middle,
            t,
            Font::Bold,
            Paint::Role(Role::Text),
        )
    })
}

/// Where `v` falls on a numeric category axis from `a` to `b`, as a
/// fraction, inset half a slot at each end so bars fit.
fn linear(v: f64, a: f64, b: f64, slots: usize) -> f32 {
    let pad = 0.5 / slots.max(1) as f64;
    (pad + (v - a) / (b - a) * (1.0 - 2.0 * pad)) as f32
}

fn value_y(v: f64, lo: f64, hi: f64, plot: Rect) -> f32 {
    plot.bottom() - ((v - lo) / (hi - lo)) as f32 * plot.height
}

fn value_x(v: f64, lo: f64, hi: f64, plot: Rect) -> f32 {
    plot.x + ((v - lo) / (hi - lo)) as f32 * plot.width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::tests::{Fixed, assert_inside};

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Fixed)
    }

    const SALES: &str = "xychart-beta\n  title \"Sales Revenue\"\n  \
        x-axis \"Month\" [jan, feb, mar, apr]\n  y-axis \"Revenue (in $)\" 4000 --> 11000\n  \
        bar Actual [5000, 6000, 7500, 8200]\n  line Plan [5500, 6500, 7000, 9000]\n";

    #[test]
    fn every_label_and_title_reaches_the_scene() {
        for header in ["xychart-beta", "xychart-beta horizontal"] {
            let s = scene(&SALES.replacen("xychart-beta", header, 1));
            let texts = s.texts();
            for expected in [
                "Sales Revenue",
                "Month",
                "Revenue (in $)",
                "jan",
                "apr",
                "Actual",
                "Plan",
                "4000",
            ] {
                assert!(
                    texts.iter().any(|t| t == expected),
                    "{header}: {expected}: {texts:?}"
                );
            }
            assert_eq!(s.areas.len(), 4, "one bar per value");
            for a in &s.areas {
                assert_eq!(a.fill, Paint::Role(Role::Series(0)));
                assert_eq!(a.opacity, 1.0);
            }
            assert!(
                s.edges.iter().all(|e| e.pen.weight != Weight::Heavy),
                "no hatching"
            );
            // The left axis's title reads upwards, as mermaid rotates it.
            let left = if header.ends_with("horizontal") {
                "Month"
            } else {
                "Revenue (in $)"
            };
            for t in &s.texts {
                let rotated = t.line.plain() == left;
                assert_eq!(t.rotate, if rotated { -90.0 } else { 0.0 }, "{header}");
            }
            assert_inside(&s);
        }
    }

    #[test]
    fn bars_rise_from_zero_and_negative_bars_hang_below_it() {
        let s = scene("xychart\n  bar [3, -2]\n");
        let bars: Vec<Rect> = s
            .areas
            .iter()
            .filter_map(|a| chart::bounds(chart::outline_points(&a.path)))
            .collect();
        let (up, down) = (bars[0], bars[1]);
        assert!((up.bottom() - down.y).abs() < 1e-3, "{up:?} {down:?}");
        assert!(s.texts().iter().any(|t| t == "-2"));
        assert_inside(&s);
    }

    #[test]
    fn a_line_after_the_bars_draws_over_them() {
        let s = scene("xychart\n  bar [3, 4]\n  line [2, 5]\n");
        assert_eq!(s.areas.len(), 2);
        // Areas paint under every edge.
        assert!(
            s.edges
                .iter()
                .any(|e| e.pen.paint == Paint::Role(Role::Series(1)))
        );
    }

    #[test]
    fn a_long_side_title_stays_inside() {
        let long = "a value axis title far longer than the plot is tall, so it hangs off both ends";
        // The left axis is the value axis upright, the category axis
        // horizontal.
        for source in [
            format!("xychart\n  y-axis \"{long}\"\n  bar [1]\n"),
            format!("xychart horizontal\n  x-axis \"{long}\" [a]\n  bar [1]\n"),
        ] {
            assert_inside(&scene(&source));
        }
    }

    #[test]
    fn a_numeric_x_axis_and_a_lone_point_render() {
        let s = scene("xychart\n  x-axis 0 --> 100\n  line [7]\n  line [1, 2, 3]\n");
        assert!(s.texts().iter().any(|t| t == "100"));
        assert_inside(&s);
    }

    #[test]
    fn many_long_categories_wrap_rather_than_collide() {
        let cats: Vec<String> = (0..20)
            .map(|i| format!("\"Category number {i}\""))
            .collect();
        let s = scene(&format!(
            "xychart\n  x-axis [{}]\n  bar [1]\n",
            cats.join(", ")
        ));
        assert_inside(&s);
    }
}
