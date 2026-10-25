//! A radar chart: axes radiating clockwise from twelve o'clock, a circular
//! or polygonal graticule, and one closed curve per data series.

use diagram_scene::{Anchor, Font, Paint, Point, Rect, Role, Scene, Seg, Weight};

use super::parse::{Graticule, Radar};
use crate::chart;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const RADIUS: f32 = 150.0;
/// Mermaid's `curveTension`.
const TENSION: f32 = 0.17;

pub(crate) fn layout(r: &Radar, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    // Laid out around the origin, then moved into place.
    let o = Point::new(0.0, 0.0);
    let n = r.axes.len();
    let angle = |i: usize| i as f32 * std::f32::consts::TAU / n as f32;

    for t in 1..=r.ticks {
        let radius = RADIUS * t as f32 / r.ticks as f32;
        let path = match r.graticule {
            Graticule::Circle => {
                let mut p = chart::arc(o, radius, 0.0, std::f32::consts::TAU);
                p.push(Seg::Close);
                p
            }
            Graticule::Polygon => closed(
                &(0..n)
                    .map(|i| chart::polar(o, radius, angle(i)))
                    .collect::<Vec<_>>(),
            ),
        };
        scene
            .edges
            .push(chart::stroke(path, Paint::Role(Role::Muted), Weight::Thin));
    }
    for i in 0..n {
        scene.edges.push(chart::stroke(
            chart::segment(o, chart::polar(o, RADIUS, angle(i))),
            Paint::Role(Role::Line),
            Weight::Thin,
        ));
    }

    let max = r.max.unwrap_or_else(|| {
        r.curves
            .iter()
            .flat_map(|c| c.values.iter().copied())
            .fold(f64::MIN, f64::max)
    });
    // No data, or all of it at the minimum: any range will do.
    let max = if max > r.min { max } else { r.min + 1.0 };
    for (k, curve) in r.curves.iter().enumerate() {
        let points: Vec<Point> = curve
            .values
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let frac = ((v.clamp(r.min, max) - r.min) / (max - r.min)) as f32;
                chart::polar(o, RADIUS * frac, angle(i))
            })
            .collect();
        let outline = match r.graticule {
            Graticule::Circle if n > 2 => round(&points),
            _ => closed(&points),
        };
        let paint = Paint::Role(Role::Series(k as u8));
        // A sparse hatch reads as mermaid's half-transparent fill and lets
        // the curves under it show through.
        let fill = chart::hatch(&chart::outline_points(&outline), 4.0, 0.5);
        scene.edges.push(chart::stroke(fill, paint, Weight::Thin));
        scene
            .edges
            .push(chart::stroke(outline, paint, Weight::Thick));
    }

    for (i, axis) in r.axes.iter().enumerate() {
        let a = angle(i);
        let at = chart::polar(o, RADIUS + 10.0, a);
        let (sin, cos) = (a.sin(), a.cos());
        let anchor = if sin > 0.2 {
            Anchor::Start
        } else if sin < -0.2 {
            Anchor::End
        } else {
            Anchor::Middle
        };
        // Above the top, below the bottom, centred on the line at the sides.
        let dy = FONT_SIZE * (0.35 - 0.55 * cos);
        scene.texts.push(chart::text(
            at.offset(0.0, dy),
            anchor,
            &axis.label,
            Font::Body,
            Paint::Role(Role::Text),
        ));
    }

    // Move everything so its extent starts at the padding, under the title.
    let title_h = chart::place_title(&r.title, 0.0).1;
    let mut extent = Rect::new(-RADIUS, -RADIUS, RADIUS * 2.0, RADIUS * 2.0);
    for t in &scene.texts {
        extent = extent.union(&chart::text_rect(t, measure));
    }
    let (dx, dy) = (PADDING - extent.x, PADDING + title_h - extent.y);
    for e in &mut scene.edges {
        for seg in &mut e.path {
            move_seg(seg, dx, dy);
        }
    }
    for t in &mut scene.texts {
        t.pos = t.pos.offset(dx, dy);
    }
    let mut right = extent.right() + dx;
    let mut bottom = extent.bottom() + dy;

    if r.show_legend && !r.curves.is_empty() {
        let entries = r
            .curves
            .iter()
            .enumerate()
            .map(|(k, c)| (Paint::Role(Role::Series(k as u8)), c.label.clone()))
            .collect();
        let legend = chart::legend(
            entries,
            Point::new(right + 24.0, PADDING + title_h),
            measure,
        );
        right = legend.rect.right();
        bottom = bottom.max(legend.rect.bottom());
        scene.legend = Some(legend);
    }

    let width = (right + PADDING).max(chart::title_width(&r.title, measure));
    scene.title = chart::place_title(&r.title, width).0;
    scene.width = width;
    scene.height = bottom + PADDING;
    scene
}

fn move_seg(seg: &mut Seg, dx: f32, dy: f32) {
    match seg {
        Seg::Move(p) | Seg::Line(p) => *p = p.offset(dx, dy),
        Seg::Cubic(a, b, p) => {
            *a = a.offset(dx, dy);
            *b = b.offset(dx, dy);
            *p = p.offset(dx, dy);
        }
        Seg::Close => {}
    }
}

/// A closed polygon through `points`.
fn closed(points: &[Point]) -> Vec<Seg> {
    let mut out = vec![Seg::Move(points[0])];
    out.extend(points[1..].iter().map(|p| Seg::Line(*p)));
    out.push(Seg::Close);
    out
}

/// A smooth closed curve through `points`, as mermaid's `closedRoundCurve`.
fn round(points: &[Point]) -> Vec<Seg> {
    let n = points.len();
    let mut out = vec![Seg::Move(points[0])];
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        let c1 = Point::new(
            p1.x + (p2.x - p0.x) * TENSION,
            p1.y + (p2.y - p0.y) * TENSION,
        );
        let c2 = Point::new(
            p2.x - (p3.x - p1.x) * TENSION,
            p2.y - (p3.y - p1.y) * TENSION,
        );
        out.push(Seg::Cubic(c1, c2, p2));
    }
    out.push(Seg::Close);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::tests::{Fixed, assert_inside};

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Fixed)
    }

    #[test]
    fn every_axis_and_curve_label_reaches_the_scene() {
        let s = scene(
            "radar-beta\n  title Skills\n  axis a[\"Rust\"], b[\"Go\"], c[\"Python\"], d[\"Writing\"], e[\"Design\"]\n  \
             curve x[\"Ann\"]{5, 3, 4, 2, 1}\n  curve y[\"Bo\"]{1, 2, 3, 4, 5}\n",
        );
        let texts = s.texts();
        for expected in [
            "Skills", "Rust", "Go", "Python", "Writing", "Design", "Ann", "Bo",
        ] {
            assert!(texts.iter().any(|t| t == expected), "{expected}: {texts:?}");
        }
        assert_inside(&s);
    }

    #[test]
    fn degenerate_data_still_renders() {
        for source in [
            "radar-beta\n  axis A\n",
            "radar-beta\n  axis A, B\n  curve z{0, 0}\n",
            "radar-beta\n  axis A, B, C\n  curve z{5, 9, 2}\n  max 4\n  graticule polygon\n",
        ] {
            assert_inside(&scene(source));
        }
    }
}
