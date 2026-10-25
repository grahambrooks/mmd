//! The thin base the data charts (pie, xychart, quadrant, radar, sankey)
//! share: reading statements, numbers and quoted text, and drawing what
//! `diagram_scene` has no primitive for.
//!
//! The scene strokes edges but never fills them, so a filled region (a pie
//! wedge, a radar area, a sankey ribbon) is drawn as a hatch of strokes:
//! dense and heavy where mermaid fills solid, sparse and thin where it fills
//! translucent. A painter needs nothing new to draw it.

use diagram_scene::{
    Anchor, Dash, Edge, Font, Legend, LegendEntry, Marker, Paint, Pen, Point, Rect, Role, Seg,
    Shape, Style, Text, TextLine, Weight,
};

use crate::text::Measure;
use crate::{Error, FONT_SIZE, PADDING, strip_comment};

/// A statement: its 1-based line and its text, comment stripped and
/// trimmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Statement<'a> {
    pub line: usize,
    pub text: &'a str,
}

/// The header (with the keyword removed from its text) and the statements
/// after it. Blank lines are skipped, and so are `accTitle`/`accDescr`,
/// which describe the diagram to screen readers and never draw anything.
pub(crate) fn statements<'a>(
    source: &'a str,
    keywords: &[&str],
) -> Result<(Statement<'a>, Vec<Statement<'a>>), Error> {
    let mut lines = source
        .lines()
        .enumerate()
        .map(|(i, l)| Statement {
            line: i + 1,
            text: strip_comment(l).trim(),
        })
        .filter(|s| !s.text.is_empty());
    let first = lines
        .next()
        .ok_or_else(|| Error::new(1, format!("expected `{}`", keywords[0])))?;
    let mut sorted: Vec<&&str> = keywords.iter().collect();
    sorted.sort_by_key(|k| std::cmp::Reverse(k.len()));
    let rest = sorted
        .iter()
        .find_map(|k| {
            first
                .text
                .strip_prefix(**k)
                .filter(|r| r.is_empty() || r.starts_with([' ', '\t', ':', ';']))
        })
        .ok_or_else(|| Error::new(first.line, format!("expected `{}`", keywords[0])))?;
    let header = Statement {
        line: first.line,
        text: rest.trim(),
    };
    let mut out = Vec::new();
    let mut in_description = false;
    for s in lines {
        if in_description {
            in_description = !s.text.contains('}');
            continue;
        }
        if let Some(rest) = keyword(s.text, "accTitle") {
            if !rest.starts_with(':') {
                return Err(Error::new(s.line, "expected `accTitle: <text>`"));
            }
            continue;
        }
        if let Some(rest) = keyword(s.text, "accDescr") {
            if rest.starts_with('{') {
                in_description = !rest.contains('}');
            } else if !rest.starts_with(':') {
                return Err(Error::new(
                    s.line,
                    "expected `accDescr: <text>` or `accDescr { … }`",
                ));
            }
            continue;
        }
        out.push(s);
    }
    Ok((header, out))
}

/// `text` after `word`, if it starts with that word as a whole word.
pub(crate) fn keyword<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(word)?;
    let boundary = rest
        .chars()
        .next()
        .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-'));
    boundary.then(|| rest.trim_start())
}

/// A title: the rest of a `title` statement, unquoted.
pub(crate) fn title(text: &str) -> Option<String> {
    keyword(text, "title").map(|t| unquote(t.trim()).to_string())
}

/// `"text"`, `'text'` or `` "`markdown`" ``, as plain text.
pub(crate) fn unquote(text: &str) -> &str {
    let t = text.trim();
    for q in ['"', '\''] {
        if t.len() >= 2 && t.starts_with(q) && t.ends_with(q) {
            let inner = &t[1..t.len() - 1];
            return inner
                .strip_prefix('`')
                .and_then(|i| i.strip_suffix('`'))
                .unwrap_or(inner);
        }
    }
    t
}

/// A finite decimal number.
pub(crate) fn number(text: &str) -> Option<f64> {
    let t = text.trim();
    let ok = !t.is_empty()
        && t.chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+'));
    t.parse::<f64>().ok().filter(|v| ok && v.is_finite())
}

/// A value as it reads in a label: whole numbers without a point, others
/// with at most two decimals.
pub(crate) fn format(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        let s = format!("{rounded:.2}");
        s.trim_end_matches('0').to_string()
    }
}

/// Evenly spaced round tick values (1, 2 or 5 × 10ⁿ apart) covering
/// `lo..=hi`, about `count` of them. With `widen`, the range grows to the
/// enclosing ticks.
pub(crate) fn ticks(lo: f64, hi: f64, count: usize, widen: bool) -> (f64, f64, Vec<f64>) {
    let (lo, hi) = if hi - lo < 1e-12 {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    };
    let raw = (hi - lo) / count.max(1) as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * magnitude);
    let (lo, hi) = if widen {
        ((lo / step).floor() * step, (hi / step).ceil() * step)
    } else {
        (lo, hi)
    };
    let first = (lo / step - 1e-9).ceil() as i64;
    let last = (hi / step + 1e-9).floor() as i64;
    // Rounded to snap away float noise such as 0.30000000000000004.
    let out = (first..=last)
        .map(|i| (i as f64 * step * 1e9).round() / 1e9)
        .collect();
    (lo, hi, out)
}

/// Free text.
pub(crate) fn text(pos: Point, anchor: Anchor, line: &str, font: Font, paint: Paint) -> Text {
    Text {
        pos,
        anchor,
        line: TextLine::new(line, font),
        paint,
        backed: false,
    }
}

/// The scene's title, centred over `width`, and the height it takes.
pub(crate) fn place_title(title: &Option<String>, width: f32) -> (Option<Text>, f32) {
    match title {
        Some(t) if !t.is_empty() => (
            Some(text(
                Point::new(width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
                Anchor::Middle,
                t,
                Font::Title,
                Paint::Role(Role::Text),
            )),
            Font::Title.size(FONT_SIZE) * 1.35 + PADDING,
        ),
        _ => (None, 0.0),
    }
}

/// Width a title needs.
pub(crate) fn title_width(title: &Option<String>, measure: &dyn Measure) -> f32 {
    title.as_ref().map_or(0.0, |t| {
        measure.width(t, Font::Title.size(FONT_SIZE), true) + PADDING * 2.0
    })
}

/// A legend of coloured swatches with its top-left at `at`, sized for its
/// labels as `svg::to_svg` draws them.
pub(crate) fn legend(entries: Vec<(Paint, String)>, at: Point, measure: &dyn Measure) -> Legend {
    let row = FONT_SIZE * 1.6;
    let widest = entries
        .iter()
        .map(|(_, l)| measure.width(l, Font::Small.size(FONT_SIZE), false))
        .fold(0.0_f32, f32::max);
    let rect = Rect::new(
        at.x,
        at.y,
        10.0 + row * 1.2 + 8.0 + widest + 12.0,
        16.0 + row * entries.len() as f32 - 4.0,
    );
    let entries = entries
        .into_iter()
        .map(|(paint, label)| LegendEntry {
            shape: Shape::Rect,
            style: Style {
                fill: paint,
                stroke: paint,
                ..Style::for_shape(Shape::Rect)
            },
            label,
        })
        .collect();
    Legend { rect, entries }
}

/// A line from `a` to `b`.
pub(crate) fn segment(a: Point, b: Point) -> Vec<Seg> {
    vec![Seg::Move(a), Seg::Line(b)]
}

/// An edge drawing `path` with no markers.
pub(crate) fn stroke(path: Vec<Seg>, paint: Paint, weight: Weight) -> Edge {
    Edge {
        pen: Pen {
            dash: Dash::Solid,
            weight,
            paint,
        },
        start: Marker::None,
        end: Marker::None,
        ..Edge::new(path)
    }
}

/// The point at `angle` (radians clockwise from twelve o'clock) on a circle.
pub(crate) fn polar(center: Point, radius: f32, angle: f32) -> Point {
    Point::new(
        center.x + radius * angle.sin(),
        center.y - radius * angle.cos(),
    )
}

/// An arc as cubic curves, clockwise from `a0` to `a1` (radians from twelve
/// o'clock), starting with a move to its first point.
pub(crate) fn arc(center: Point, radius: f32, a0: f32, a1: f32) -> Vec<Seg> {
    let pieces = ((a1 - a0).abs() / std::f32::consts::FRAC_PI_2)
        .ceil()
        .max(1.0) as usize;
    let step = (a1 - a0) / pieces as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan() * radius;
    let mut out = vec![Seg::Move(polar(center, radius, a0))];
    for i in 0..pieces {
        let (s, e) = (a0 + step * i as f32, a0 + step * (i + 1) as f32);
        let (p0, p3) = (polar(center, radius, s), polar(center, radius, e));
        // Tangents point along increasing angle: (cos, sin) in screen space.
        let c1 = Point::new(p0.x + k * s.cos(), p0.y + k * s.sin());
        let c2 = Point::new(p3.x - k * e.cos(), p3.y - k * e.sin());
        out.push(Seg::Cubic(c1, c2, p3));
    }
    out
}

/// Points along an arc, about every two degrees.
pub(crate) fn arc_points(center: Point, radius: f32, a0: f32, a1: f32) -> Vec<Point> {
    let n = (((a1 - a0).abs() / 2f32.to_radians()).ceil() as usize).max(1);
    (0..=n)
        .map(|i| polar(center, radius, a0 + (a1 - a0) * i as f32 / n as f32))
        .collect()
}

/// Horizontal strokes `pitch` apart filling the polygon `poly`, each pulled
/// in by `inset` (half the stroke width, so round caps stay inside).
pub(crate) fn hatch(poly: &[Point], pitch: f32, inset: f32) -> Vec<Seg> {
    hatch_within(poly, pitch, inset, inset)
}

/// [`hatch`] with the strokes' ends pulled in by `side` rather than
/// `inset`: less, so a fill reaches under an outline drawn over its edge.
pub(crate) fn hatch_within(poly: &[Point], pitch: f32, inset: f32, side: f32) -> Vec<Seg> {
    let mut out = Vec::new();
    if poly.len() < 3 {
        return out;
    }
    let (ymin, ymax) = poly.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| {
        (lo.min(p.y), hi.max(p.y))
    });
    let rows: Vec<f32> = if ymax - ymin <= 2.0 * inset {
        vec![(ymin + ymax) / 2.0]
    } else {
        let n = ((ymax - ymin - 2.0 * inset) / pitch).ceil().max(1.0) as usize;
        (0..=n)
            .map(|i| ymin + inset + (ymax - ymin - 2.0 * inset) * i as f32 / n as f32)
            .collect()
    };
    for y in rows {
        let mut xs: Vec<f32> = Vec::new();
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a.y <= y && y < b.y) || (b.y <= y && y < a.y) {
                xs.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
            }
        }
        xs.sort_by(f32::total_cmp);
        for pair in xs.as_chunks::<2>().0 {
            let (mut x0, mut x1) = (pair[0] + side, pair[1] - side);
            if x1 < x0 {
                let mid = (pair[0] + pair[1]) / 2.0;
                (x0, x1) = (mid, mid);
            }
            out.push(Seg::Move(Point::new(x0, y)));
            out.push(Seg::Line(Point::new(x1, y)));
        }
    }
    out
}

/// The points of a path, flattened, for hatching a curved outline.
pub(crate) fn outline_points(path: &[Seg]) -> Vec<Point> {
    diagram_scene::flatten::flatten(path, 0.5)
        .into_iter()
        .flat_map(|p| p.points)
        .collect()
}

/// The bounding box of some points.
#[cfg(test)]
pub(crate) fn bounds(points: impl IntoIterator<Item = Point>) -> Option<Rect> {
    points
        .into_iter()
        .map(|p| Rect::new(p.x, p.y, 0.0, 0.0))
        .reduce(|a, b| a.union(&b))
}

/// A text's extent as drawn: `pos` is its baseline at `anchor`.
pub(crate) fn text_rect(t: &Text, measure: &dyn Measure) -> Rect {
    let size = t.line.font().size(FONT_SIZE);
    let w = diagram_scene::line_width(measure, &t.line, FONT_SIZE);
    let x = match t.anchor {
        Anchor::Start => t.pos.x,
        Anchor::Middle => t.pos.x - w / 2.0,
        Anchor::End => t.pos.x - w,
    };
    Rect::new(x, t.pos.y - size * 0.8, w, size * 1.05)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A measure with a fixed advance, so tests need no fonts.
    pub(crate) struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.55
        }
    }

    /// Every text, title and legend entry lies inside the scene.
    pub(crate) fn assert_inside(scene: &diagram_scene::Scene) {
        let b = scene.bounds().inflate(0.5);
        let mut all: Vec<Rect> = scene.texts.iter().map(|t| text_rect(t, &Fixed)).collect();
        all.extend(scene.title.iter().map(|t| text_rect(t, &Fixed)));
        all.extend(scene.legend.iter().map(|l| l.rect));
        all.extend(scene.nodes.iter().map(|n| n.rect));
        all.extend(scene.containers.iter().map(|c| c.rect));
        for e in &scene.edges {
            all.extend(
                outline_points(&e.path)
                    .into_iter()
                    .map(|p| Rect::new(p.x, p.y, 0.0, 0.0)),
            );
        }
        for r in all {
            assert!(b.encloses(&r, 0.5), "{r:?} escapes {b:?}");
        }
    }

    #[test]
    fn ticks_are_round_and_cover_the_range() {
        let (lo, hi, t) = ticks(3.0, 97.0, 5, true);
        assert_eq!((lo, hi), (0.0, 100.0));
        assert_eq!(t, vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        let (_, _, t) = ticks(0.0, 0.9, 3, true);
        assert!(t.iter().all(|v| format(*v).len() <= 4), "{t:?}");
        let (lo, hi, _) = ticks(5.0, 5.0, 5, true);
        assert!(lo < 5.0 && hi > 5.0);
    }

    #[test]
    fn numbers_read_and_format_plainly() {
        assert_eq!(number("-1.5"), Some(-1.5));
        assert_eq!(number("abc"), None);
        assert_eq!(number("inf"), None);
        assert_eq!(format(3.0), "3");
        assert_eq!(format(2.345), "2.35");
        assert_eq!(format(0.5), "0.5");
    }

    #[test]
    fn hatching_stays_inside_its_polygon() {
        let square = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ];
        let segs = hatch(&square, 4.0, 2.5);
        assert!(!segs.is_empty());
        for p in outline_points(&segs) {
            assert!(
                (2.5..=7.5).contains(&p.x) && (2.5..=7.5).contains(&p.y),
                "{p:?}"
            );
        }
    }

    #[test]
    fn accessibility_statements_are_skipped() {
        let (header, body) = statements(
            "pie showData\naccTitle: t\naccDescr {\n  long\n}\n\"a\" : 1\n",
            &["pie"],
        )
        .unwrap();
        assert_eq!(header.text, "showData");
        assert_eq!(body.len(), 1);
        assert_eq!(body[0].line, 6);
    }
}
