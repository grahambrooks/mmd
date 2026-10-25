//! `xychart` source to a model.
//!
//! Grammar: `packages/mermaid/src/diagrams/xychart/parser/xychart.jison`.

use crate::Error;
use crate::chart::{self, Statement};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XyChart {
    pub title: Option<String>,
    /// Categories down the left and values along the top.
    pub horizontal: bool,
    pub x: XAxis,
    pub y: YAxis,
    pub series: Vec<Series>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct XAxis {
    pub title: Option<String>,
    pub data: XData,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum XData {
    /// None given: the points are numbered from 1.
    #[default]
    Auto,
    Bands(Vec<String>),
    Range(f64, f64),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct YAxis {
    pub title: Option<String>,
    pub range: Option<(f64, f64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plot {
    Bar,
    Line,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    pub plot: Plot,
    pub title: Option<String>,
    pub values: Vec<f64>,
}

/// Parse an `xychart` (or `xychart-beta`) source.
pub fn parse(source: &str) -> Result<XyChart, Error> {
    let (header, body) = chart::statements(source, &["xychart", "xychart-beta"])?;
    let mut chart = XyChart::default();
    match header.text.to_ascii_lowercase().as_str() {
        "" | "vertical" => {}
        "horizontal" => chart.horizontal = true,
        other => {
            return Err(Error::new(
                header.line,
                format!("expected `horizontal` or `vertical` after `xychart`, found `{other}`"),
            ));
        }
    }
    let mut series_lines = Vec::new();
    for s in body {
        let text = s.text.trim_end_matches(';').trim();
        let (word, rest) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(w, r)| (w, r.trim()));
        match word.to_ascii_lowercase().as_str() {
            "title" => chart.title = Some(chart::unquote(rest).to_string()),
            "x-axis" => chart.x = x_axis(rest, s)?,
            "y-axis" => chart.y = y_axis(rest, s)?,
            "bar" | "line" => {
                let plot = if word.eq_ignore_ascii_case("bar") {
                    Plot::Bar
                } else {
                    Plot::Line
                };
                let (title, data) = leading_text(rest);
                let values = numbers(data, s)?;
                chart.series.push(Series {
                    plot,
                    title,
                    values,
                });
                series_lines.push(s.line);
            }
            _ => {
                return Err(Error::new(
                    s.line,
                    format!(
                        "`{word}` is not supported in an xychart (see DESIGN.md); expected \
                         `title`, `x-axis`, `y-axis`, `bar` or `line`"
                    ),
                ));
            }
        }
    }
    if chart.series.is_empty() {
        return Err(Error::new(
            header.line,
            "an xychart needs at least one `bar [...]` or `line [...]`",
        ));
    }
    if let XData::Bands(bands) = &chart.x.data {
        for (series, line) in chart.series.iter().zip(&series_lines) {
            if series.values.len() > bands.len() {
                return Err(Error::new(
                    *line,
                    format!(
                        "{} values but the x-axis has {} categories",
                        series.values.len(),
                        bands.len()
                    ),
                ));
            }
        }
    }
    Ok(chart)
}

/// A leading title (quoted, or the words before `[` or a number), and what
/// follows it.
fn leading_text(rest: &str) -> (Option<String>, &str) {
    let rest = rest.trim();
    if let Some(q) = rest.chars().next().filter(|c| *c == '"')
        && let Some(close) = rest[1..].find(q)
    {
        let title = chart::unquote(&rest[..close + 2]).to_string();
        return (Some(title), rest[close + 2..].trim());
    }
    let end = rest.find('[').unwrap_or(rest.len());
    let title = rest[..end].trim();
    let title = (!title.is_empty()).then(|| title.to_string());
    (title, &rest[end..])
}

fn x_axis(rest: &str, s: Statement) -> Result<XAxis, Error> {
    let (title, data) = axis_title(rest);
    let data = if data.starts_with('[') {
        XData::Bands(
            list(data, s)?
                .into_iter()
                .map(|t| chart::unquote(&t).to_string())
                .collect(),
        )
    } else if data.contains("-->") {
        let (a, b) = range(data, s)?;
        XData::Range(a, b)
    } else if data.is_empty() {
        XData::Auto
    } else {
        return Err(Error::new(
            s.line,
            format!("expected `[a, b, …]` or `min --> max` on the x-axis, found `{data}`"),
        ));
    };
    Ok(XAxis { title, data })
}

fn y_axis(rest: &str, s: Statement) -> Result<YAxis, Error> {
    let (title, data) = axis_title(rest);
    let range = if data.is_empty() {
        None
    } else if data.contains("-->") {
        Some(range(data, s)?)
    } else {
        return Err(Error::new(
            s.line,
            format!("expected `min --> max` on the y-axis, found `{data}`"),
        ));
    };
    Ok(YAxis { title, range })
}

/// An axis's title and its data: `[…]`, `min --> max`, or nothing.
fn axis_title(rest: &str) -> (Option<String>, &str) {
    let (title, data) = leading_text(rest);
    if !data.is_empty() {
        return (title, data);
    }
    // No brackets: maybe `title 1 --> 10`.
    let Some(title_text) = title else {
        return (None, "");
    };
    let Some(arrow) = rest.find("-->") else {
        return (Some(title_text), "");
    };
    let left = rest[..arrow].trim_end();
    let start = left.rfind(char::is_whitespace).map_or(0, |i| i + 1);
    let title = left[..start].trim();
    let title = (!title.is_empty()).then(|| chart::unquote(title).to_string());
    (title, rest[start..].trim())
}

fn range(text: &str, s: Statement) -> Result<(f64, f64), Error> {
    let (a, b) = text.split_once("-->").unwrap_or((text, ""));
    let parse = |v: &str| {
        chart::number(v).ok_or_else(|| {
            Error::new(
                s.line,
                format!("`{}` is not a number in `min --> max`", v.trim()),
            )
        })
    };
    let (a, b) = (parse(a)?, parse(b)?);
    if a == b {
        return Err(Error::new(
            s.line,
            format!("the range {a} --> {b} is empty"),
        ));
    }
    Ok((a, b))
}

/// `[a, "b, c", d]` as its items.
fn list(text: &str, s: Statement) -> Result<Vec<String>, Error> {
    let inner = text
        .strip_prefix('[')
        .and_then(|t| t.trim_end().strip_suffix(']'))
        .ok_or_else(|| Error::new(s.line, format!("expected `[…]`, found `{text}`")))?;
    let (mut out, mut current, mut quoted) = (Vec::new(), String::new(), false);
    for c in inner.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            ',' if !quoted => out.push(std::mem::take(&mut current).trim().to_string()),
            _ => current.push(c),
        }
    }
    out.push(current.trim().to_string());
    if out.iter().any(String::is_empty) {
        return Err(Error::new(s.line, format!("an empty item in `{text}`")));
    }
    Ok(out)
}

fn numbers(text: &str, s: Statement) -> Result<Vec<f64>, Error> {
    if text.is_empty() {
        return Err(Error::new(s.line, "expected data as `[1, 2, 3]`"));
    }
    list(text, s)?
        .iter()
        .map(|v| {
            chart::number(v).ok_or_else(|| Error::new(s.line, format!("`{v}` is not a number")))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_axes_and_series() {
        let c = parse(
            "xychart-beta horizontal\n  title \"Sales Revenue\"\n  x-axis [jan, \"feb, early\", mar]\n  \
             y-axis \"Revenue (in $)\" 4000 --> 11000\n  bar [5000, 6000, 7500]\n  line Trend [+5, .5, -7]\n",
        )
        .unwrap();
        assert!(c.horizontal);
        assert_eq!(c.title.as_deref(), Some("Sales Revenue"));
        assert_eq!(
            c.x.data,
            XData::Bands(vec!["jan".into(), "feb, early".into(), "mar".into()])
        );
        assert_eq!(c.y.title.as_deref(), Some("Revenue (in $)"));
        assert_eq!(c.y.range, Some((4000.0, 11000.0)));
        assert_eq!(c.series[1].title.as_deref(), Some("Trend"));
        assert_eq!(c.series[1].values, vec![5.0, 0.5, -7.0]);
    }

    #[test]
    fn axis_titles_and_ranges_are_each_optional() {
        let c = parse("xychart\n  x-axis Year 2010 --> 2020\n  y-axis 0 --> 5\n  line [1, 2]\n")
            .unwrap();
        assert_eq!(c.x.title.as_deref(), Some("Year"));
        assert_eq!(c.x.data, XData::Range(2010.0, 2020.0));
        assert_eq!(c.y.title, None);
        let c = parse("xychart\n  x-axis \"Quarter\"\n  y-axis Units\n  bar [1]\n").unwrap();
        assert_eq!(c.x.title.as_deref(), Some("Quarter"));
        assert_eq!(c.x.data, XData::Auto);
        assert_eq!(c.y.title.as_deref(), Some("Units"));
    }

    #[test]
    fn bad_input_is_an_error_with_its_line() {
        for (source, line, needle) in [
            ("xychart\n", 1, "at least one"),
            ("xychart\n  x-axis [a]\n  bar [1, 2]\n", 3, "1 categories"),
            ("xychart\n  bar [1, x]\n", 2, "`x`"),
            ("xychart\n  y-axis 3 --> 3\n  bar [1]\n", 2, "empty"),
            ("xychart\n  area [1]\n", 2, "`area`"),
            ("xychart diagonal\n  bar [1]\n", 1, "diagonal"),
            ("xychart\n  bar [1,,2]\n", 2, "empty item"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
