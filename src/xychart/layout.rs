//! An xychart: a category (or numeric) axis, a value axis with round ticks,
//! bars grouped side by side within each category, and lines over them.

use diagram_scene::{Anchor, Font, Paint, Point, Rect, Role, Scene, Seg, Weight};

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
        let y_title_h = if c.y.title.is_some() {
            FONT_SIZE * 1.6
        } else {
            0.0
        };
        let left = PADDING + tick_width + TICK + 4.0;
        plot = Rect::new(left, top + y_title_h + small, cat_length, VALUE_LENGTH);
        if let Some(t) = &c.y.title {
            scene.texts.push(chart::text(
                Point::new(PADDING, top + FONT_SIZE),
                Anchor::Start,
                t,
                Font::Bold,
                Paint::Role(Role::Text),
            ));
            right = PADDING + measure.width(t, FONT_SIZE, true);
        } else {
            right = 0.0;
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
        let left = PADDING + label_width + TICK + 4.0;
        let tick_row = small * 1.3 + TICK;
        let rows = wrapped.iter().map(Vec::len).max().unwrap_or(1) as f32;
        let band = MIN_SLOT.max(rows * line_h + 8.0);
        plot = Rect::new(
            left,
            top + y_title_h + tick_row,
            VALUE_LENGTH * 1.6,
            (slots as f32 * band).max(VALUE_LENGTH),
        );
        right = plot.right();
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
        if let Some(t) = &c.x.title {
            scene.texts.push(chart::text(
                Point::new(PADDING, bottom + 8.0 + FONT_SIZE),
                Anchor::Start,
                t,
                Font::Bold,
                Paint::Role(Role::Text),
            ));
            right = right.max(PADDING + measure.width(t, FONT_SIZE, true));
            bottom += 8.0 + FONT_SIZE * 1.35;
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

    // Bars grouped within each slot, and lines, in source order so a line
    // declared after the bars is drawn over them, as mermaid does. Bars are
    // hatched edges rather than nodes because nodes always paint last.
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
                scene.edges.push(bar_edge(rect, paint));
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

/// A bar, filled with heavy strokes; a hairline-thin bar is still drawn as
/// one stroke so a small value stays visible.
fn bar_edge(rect: Rect, paint: Paint) -> diagram_scene::Edge {
    let corners = [
        Point::new(rect.x, rect.y),
        Point::new(rect.right(), rect.y),
        Point::new(rect.right(), rect.bottom()),
        Point::new(rect.x, rect.bottom()),
    ];
    // Close strokes, so the round caps barely scallop the bar's sides.
    let mut path = chart::hatch(&corners, 2.5, 2.5);
    if path.is_empty() {
        path = chart::segment(rect.center(), rect.center());
    }
    chart::stroke(path, paint, Weight::Heavy)
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
            let bars = s
                .edges
                .iter()
                .filter(|e| e.pen.weight == Weight::Heavy)
                .count();
            assert_eq!(bars, 4, "one bar per value");
            assert_inside(&s);
        }
    }

    #[test]
    fn bars_rise_from_zero_and_negative_bars_hang_below_it() {
        let s = scene("xychart\n  bar [3, -2]\n");
        let bars: Vec<Rect> = s
            .edges
            .iter()
            .filter(|e| e.pen.weight == Weight::Heavy)
            .filter_map(|e| chart::bounds(chart::outline_points(&e.path)))
            .collect();
        let (up, down) = (bars[0], bars[1]);
        // Hatching is inset half a stroke from the bar's true edge.
        assert!(
            (up.bottom() + 2.5 - (down.y - 2.5)).abs() < 0.6,
            "{up:?} {down:?}"
        );
        assert!(s.texts().iter().any(|t| t == "-2"));
        assert_inside(&s);
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
