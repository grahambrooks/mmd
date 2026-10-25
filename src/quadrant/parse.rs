//! `quadrantChart` source to a model.
//!
//! Grammar: `packages/mermaid/src/diagrams/quadrant-chart/parser/quadrant.jison`;
//! point styles as `quadrantDb.ts` reads them.

use crate::Error;
use crate::chart::{self, Statement};
use crate::style::{self, Rgb};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Quadrant {
    pub title: Option<String>,
    pub x_left: Option<String>,
    pub x_right: Option<String>,
    pub y_bottom: Option<String>,
    pub y_top: Option<String>,
    /// `quadrant-1` (top right) to `quadrant-4` (bottom right),
    /// anticlockwise as mermaid numbers them.
    pub quadrants: [Option<String>; 4],
    pub points: Vec<QPoint>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QPoint {
    pub name: String,
    /// 0..=1 from the left and from the bottom.
    pub x: f32,
    pub y: f32,
    pub style: PointStyle,
}

/// A point's own look; unset parts take the class's, then the theme's.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PointStyle {
    pub radius: Option<f32>,
    pub color: Option<Rgb>,
    pub stroke: Option<Rgb>,
    pub stroke_width: Option<f32>,
}

impl PointStyle {
    fn under(self, class: PointStyle) -> PointStyle {
        PointStyle {
            radius: self.radius.or(class.radius),
            color: self.color.or(class.color),
            stroke: self.stroke.or(class.stroke),
            stroke_width: self.stroke_width.or(class.stroke_width),
        }
    }
}

/// Parse a `quadrantChart` source.
pub fn parse(source: &str) -> Result<Quadrant, Error> {
    let (header, body) = chart::statements(source, &["quadrantChart"])?;
    if !header.text.is_empty() {
        return Err(Error::new(
            header.line,
            format!("unexpected `{}` after `quadrantChart`", header.text),
        ));
    }
    let mut q = Quadrant::default();
    let mut classes: Vec<(String, PointStyle)> = Vec::new();
    let mut classed: Vec<(usize, String)> = Vec::new();
    for s in body {
        let text = s.text.trim_end_matches(';').trim();
        let (word, rest) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(w, r)| (w, r.trim()));
        // Keywords are case-insensitive, as mermaid's lexer reads them.
        match word.to_ascii_lowercase().as_str() {
            "title" => q.title = Some(chart::unquote(rest).to_string()),
            "x-axis" => (q.x_left, q.x_right) = axis(rest),
            "y-axis" => (q.y_bottom, q.y_top) = axis(rest),
            "quadrant-1" | "quadrant-2" | "quadrant-3" | "quadrant-4" => {
                let n = word.as_bytes()[9] - b'1';
                q.quadrants[n as usize] = Some(chart::unquote(rest).to_string());
            }
            "classdef" => {
                let (name, styles) = rest
                    .split_once(char::is_whitespace)
                    .ok_or_else(|| Error::new(s.line, "expected `classDef <name> <styles>`"))?;
                classes.push((name.to_string(), point_style(styles, s)?));
            }
            _ => {
                let (point, class) = point(text, s)?;
                if let Some(class) = class {
                    classed.push((q.points.len(), class));
                }
                q.points.push(point);
            }
        }
    }
    // Classes apply wherever they are defined, as in mermaid.
    for (index, class) in classed {
        if let Some((_, style)) = classes.iter().rev().find(|(name, _)| *name == class) {
            let point = &mut q.points[index];
            point.style = point.style.under(*style);
        }
    }
    Ok(q)
}

/// `Low --> High`, `Low -->` or `Low`.
fn axis(rest: &str) -> (Option<String>, Option<String>) {
    let text = |t: &str| {
        let t = chart::unquote(t.trim());
        (!t.is_empty()).then(|| t.to_string())
    };
    match arrow(rest) {
        Some((left, right)) => match text(right) {
            Some(right) => (text(left), Some(right)),
            None => (text(left).map(|l| format!("{l} ⟶")), None),
        },
        None => (text(rest), None),
    }
}

/// Split at `-->` (two or more dashes then `>`).
fn arrow(text: &str) -> Option<(&str, &str)> {
    let at = text.find("-->")?;
    let start = text[..at].trim_end_matches('-').len();
    let start = start.min(at);
    Some((&text[..start], &text[at + 3..]))
}

/// `Name: [x, y]`, `Name:::class: [x, y]`, either with styles after.
fn point(text: &str, s: Statement) -> Result<(QPoint, Option<String>), Error> {
    let unsupported = || {
        let word = text.split_whitespace().next().unwrap_or(text);
        Error::new(
            s.line,
            format!(
                "`{word}` is not supported in a quadrantChart (see DESIGN.md); expected a \
                 point such as `Name: [0.3, 0.6]`"
            ),
        )
    };
    // A quoted name may hold `:` or `[`; the coordinates start after it.
    let name_end = match text.strip_prefix('"') {
        Some(after) => after.find('"').map_or(0, |i| i + 2),
        None => 0,
    };
    let open = text[name_end..]
        .find('[')
        .map(|i| i + name_end)
        .ok_or_else(unsupported)?;
    let head = text[..open].trim_end();
    let head = head.strip_suffix(':').ok_or_else(unsupported)?.trim();
    let (name, class) = match head.split_once(":::") {
        Some((n, c)) => (n.trim(), Some(c.trim().to_string())),
        None => (head, None),
    };
    let name = chart::unquote(name).to_string();
    if name.is_empty() {
        return Err(Error::new(s.line, "a point needs a name before `:`"));
    }
    let close = text[open..]
        .find(']')
        .map(|i| i + open)
        .ok_or_else(|| Error::new(s.line, "expected `]` after the point's coordinates"))?;
    let coords: Vec<&str> = text[open + 1..close].split(',').collect();
    let coordinate = |v: &str| {
        chart::number(v)
            .filter(|n| (0.0..=1.0).contains(n))
            .map(|n| n as f32)
            .ok_or_else(|| {
                Error::new(
                    s.line,
                    format!(
                        "point coordinate `{}` must be a number from 0 to 1",
                        v.trim()
                    ),
                )
            })
    };
    if coords.len() != 2 {
        return Err(Error::new(s.line, "expected `[x, y]`"));
    }
    let (x, y) = (coordinate(coords[0])?, coordinate(coords[1])?);
    let style = point_style(&text[close + 1..], s)?;
    Ok((QPoint { name, x, y, style }, class))
}

/// `radius: 10, color: #ff3300, stroke-color: #10f0f0, stroke-width: 5px`.
fn point_style(text: &str, s: Statement) -> Result<PointStyle, Error> {
    let mut out = PointStyle::default();
    for (key, value) in style::declarations(text, s.line)? {
        match key.as_str() {
            "radius" => out.radius = Some(style::number(&value, s.line, "radius")?),
            "color" => out.color = Some(style::color(&value, s.line)?),
            "stroke-color" => out.stroke = Some(style::color(&value, s.line)?),
            "stroke-width" => {
                out.stroke_width = Some(style::number(&value, s.line, "stroke-width")?)
            }
            other => {
                return Err(Error::new(
                    s.line,
                    format!(
                        "point style `{other}` is not supported; mermaid takes `radius`, \
                         `color`, `stroke-color` and `stroke-width`"
                    ),
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "quadrantChart
    title Reach and engagement of campaigns
    x-axis Low Reach --> High Reach
    y-axis Low Engagement -->
    quadrant-1 We should expand
    quadrant-2 Need to promote
    quadrant-3 Re-evaluate
    quadrant-4 May be improved
    Campaign A: [0.3, 0.6]
    Campaign B:::hot: [0.45, 0.23] radius: 12
    \"Campaign C\": [0.57, 0.69] color: #ff3300, stroke-color: #10f0f0, stroke-width: 5px
    classDef hot color: #109060, radius : 5, stroke-color: #310085, stroke-width: 10px
";

    #[test]
    fn reads_axes_quadrants_points_and_styles() {
        let q = parse(SOURCE).unwrap();
        assert_eq!(q.x_left.as_deref(), Some("Low Reach"));
        assert_eq!(q.x_right.as_deref(), Some("High Reach"));
        assert_eq!(q.y_bottom.as_deref(), Some("Low Engagement ⟶"));
        assert_eq!(q.y_top, None);
        assert_eq!(q.quadrants[3].as_deref(), Some("May be improved"));
        assert_eq!(q.points.len(), 3);
        let b = &q.points[1].style;
        // The point's own radius wins over its class's.
        assert_eq!(b.radius, Some(12.0));
        assert_eq!(b.color, Some((0x10, 0x90, 0x60)));
        assert_eq!(q.points[2].name, "Campaign C");
        assert_eq!(q.points[2].style.stroke_width, Some(5.0));
    }

    #[test]
    fn keywords_are_case_insensitive_and_quoted_names_hold_anything() {
        let q = parse(
            "quadrantChart\n  TiTle \"Plan (* +=[\"\n  x-AxIs Urgent --> Not Urgent\n  \
             QuaDrant-2 Do\n  \"Point1 : (* +=[❤\": [1, 0]\n",
        )
        .unwrap();
        assert_eq!(q.title.as_deref(), Some("Plan (* +=["));
        assert_eq!(q.x_right.as_deref(), Some("Not Urgent"));
        assert_eq!(q.quadrants[1].as_deref(), Some("Do"));
        assert_eq!(q.points[0].name, "Point1 : (* +=[❤");
        assert_eq!((q.points[0].x, q.points[0].y), (1.0, 0.0));
    }

    #[test]
    fn bad_input_is_an_error_with_its_line() {
        for (source, line, needle) in [
            ("quadrantChart\n  A: [1.5, 0.2]\n", 2, "from 0 to 1"),
            ("quadrantChart\n  A: [0.5]\n", 2, "[x, y]"),
            (
                "quadrantChart\n  A: [0.5, 0.5] opacity: 1\n",
                2,
                "`opacity`",
            ),
            ("quadrantChart\n  A: [0.5, 0.5] color: #zz\n", 2, "colour"),
            ("quadrantChart\n  section A\n", 2, "`section`"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
