//! `radar-beta` source to a model.
//!
//! Grammar: `packages/parser/src/language/radar/radar.langium`; defaults
//! from `packages/mermaid/src/diagrams/radar/db.ts`.

use crate::Error;
use crate::chart;

#[derive(Debug, Clone, PartialEq)]
pub struct Radar {
    pub title: Option<String>,
    pub axes: Vec<Axis>,
    pub curves: Vec<Curve>,
    pub show_legend: bool,
    pub ticks: usize,
    pub max: Option<f64>,
    pub min: f64,
    pub graticule: Graticule,
}

impl Default for Radar {
    fn default() -> Self {
        Radar {
            title: None,
            axes: Vec::new(),
            curves: Vec::new(),
            show_legend: true,
            ticks: 5,
            max: None,
            min: 0.0,
            graticule: Graticule::Circle,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Graticule {
    Circle,
    Polygon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub name: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub label: String,
    /// One value per axis, in axis order.
    pub values: Vec<f64>,
}

/// A curve's value, with the axis it names if it names one.
type Entry = (Option<String>, f64);

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Num(f64),
    Punct(char),
    Newline,
}

/// Parse a `radar-beta` source.
pub fn parse(source: &str) -> Result<Radar, Error> {
    let (header, body) = chart::statements(source, &["radar-beta"])?;
    let rest = header.text.trim_start_matches(':').trim();
    if !rest.is_empty() {
        return Err(Error::new(
            header.line,
            format!("unexpected `{rest}` after `radar-beta`"),
        ));
    }
    let mut radar = Radar::default();
    let mut tokens: Vec<(usize, Tok)> = Vec::new();
    for s in body {
        if let Some(t) = chart::title(s.text) {
            radar.title = Some(t);
            continue;
        }
        tokenize(s.text, s.line, &mut tokens)?;
        tokens.push((s.line, Tok::Newline));
    }
    let mut p = Parser {
        tokens,
        at: 0,
        last_line: header.line,
    };
    // Curves are resolved once every axis is known.
    let mut curves: Vec<(usize, String, Vec<Entry>)> = Vec::new();
    while let Some((line, tok)) = p.next() {
        match tok {
            Tok::Newline => {}
            Tok::Word(w) if w == "axis" => loop {
                let name = p.word("an axis name")?;
                let label = p.label()?.unwrap_or_else(|| name.clone());
                if radar.axes.iter().any(|a| a.name == name) {
                    return Err(Error::new(line, format!("axis `{name}` is declared twice")));
                }
                radar.axes.push(Axis { name, label });
                if !p.eat(',') {
                    break;
                }
            },
            Tok::Word(w) if w == "curve" => loop {
                let name = p.word("a curve name")?;
                let label = p.label()?.unwrap_or(name);
                let line = p.line();
                p.expect('{')?;
                let entries = p.entries()?;
                curves.push((line, label, entries));
                if !p.eat(',') {
                    break;
                }
            },
            Tok::Word(w) => {
                let mut name = w;
                loop {
                    option(&mut radar, &name, &mut p, line)?;
                    if !p.eat(',') {
                        break;
                    }
                    name = p.word("an option")?;
                }
            }
            other => {
                return Err(Error::new(
                    line,
                    format!("unexpected {} in a radar chart", describe(&other)),
                ));
            }
        }
        p.end_of_statement()?;
    }
    if radar.axes.is_empty() {
        return Err(Error::new(header.line, "a radar chart needs `axis` names"));
    }
    for (line, label, entries) in curves {
        let values = if entries.iter().all(|(axis, _)| axis.is_none()) {
            if entries.len() != radar.axes.len() {
                return Err(Error::new(
                    line,
                    format!(
                        "curve `{label}` has {} values for {} axes",
                        entries.len(),
                        radar.axes.len()
                    ),
                ));
            }
            entries.into_iter().map(|(_, v)| v).collect()
        } else if entries.iter().any(|(axis, _)| axis.is_none()) {
            return Err(Error::new(
                line,
                format!("curve `{label}` mixes plain values with `axis: value` entries"),
            ));
        } else {
            let mut values = Vec::new();
            for axis in &radar.axes {
                let value = entries
                    .iter()
                    .find(|(a, _)| a.as_deref() == Some(axis.name.as_str()))
                    .ok_or_else(|| {
                        Error::new(
                            line,
                            format!("curve `{label}` has no value for axis `{}`", axis.name),
                        )
                    })?;
                values.push(value.1);
            }
            if let Some((Some(a), _)) = entries
                .iter()
                .find(|(a, _)| !radar.axes.iter().any(|x| Some(&x.name) == a.as_ref()))
            {
                return Err(Error::new(
                    line,
                    format!("curve `{label}` names unknown axis `{a}`"),
                ));
            }
            values
        };
        radar.curves.push(Curve { label, values });
    }
    if radar.max.is_some_and(|m| m <= radar.min) {
        return Err(Error::new(header.line, "`max` must be greater than `min`"));
    }
    Ok(radar)
}

fn option(radar: &mut Radar, name: &str, p: &mut Parser, line: usize) -> Result<(), Error> {
    match name {
        "showLegend" => {
            radar.show_legend = match p.word("`true` or `false`")?.as_str() {
                "true" => true,
                "false" => false,
                other => {
                    return Err(Error::new(
                        line,
                        format!("expected `true` or `false`, found `{other}`"),
                    ));
                }
            }
        }
        "ticks" => {
            let n = p.number()?;
            if n < 1.0 || n.fract() != 0.0 {
                return Err(Error::new(
                    line,
                    "`ticks` must be a whole number of at least 1",
                ));
            }
            radar.ticks = n as usize;
        }
        "max" => radar.max = Some(p.number()?),
        "min" => radar.min = p.number()?,
        "graticule" => {
            radar.graticule = match p.word("`circle` or `polygon`")?.as_str() {
                "circle" => Graticule::Circle,
                "polygon" => Graticule::Polygon,
                other => {
                    return Err(Error::new(
                        line,
                        format!("expected `circle` or `polygon`, found `{other}`"),
                    ));
                }
            }
        }
        other => {
            return Err(Error::new(
                line,
                format!(
                    "`{other}` is not supported in a radar chart (see DESIGN.md); expected \
                     `axis`, `curve`, `title` or an option"
                ),
            ));
        }
    }
    Ok(())
}

fn describe(tok: &Tok) -> String {
    match tok {
        Tok::Word(w) => format!("`{w}`"),
        Tok::Str(s) => format!("\"{s}\""),
        Tok::Num(n) => format!("`{n}`"),
        Tok::Punct(c) => format!("`{c}`"),
        Tok::Newline => "end of line".into(),
    }
}

fn tokenize(text: &str, line: usize, out: &mut Vec<(usize, Tok)>) -> Result<(), Error> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '\'' {
            let start = i + 1;
            i = start;
            while i < chars.len() && chars[i] != c {
                i += 1;
            }
            if i == chars.len() {
                return Err(Error::new(line, "unterminated quoted label"));
            }
            out.push((line, Tok::Str(chars[start..i].iter().collect())));
            i += 1;
        } else if c.is_ascii_digit()
            || (c == '-' && chars.get(i + 1).is_some_and(char::is_ascii_digit))
        {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            // An id may start with a digit (`1st`); only all-numeric is a number.
            if i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-'))
                {
                    i += 1;
                }
                out.push((line, Tok::Word(chars[start..i].iter().collect())));
                continue;
            }
            let n = chart::number(&text)
                .ok_or_else(|| Error::new(line, format!("`{text}` is not a number")))?;
            out.push((line, Tok::Num(n)));
        } else if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '-')) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            out.push((line, Tok::Word(word.trim_end_matches('-').to_string())));
        } else if "[]{},:".contains(c) {
            out.push((line, Tok::Punct(c)));
            i += 1;
        } else {
            return Err(Error::new(
                line,
                format!("unexpected `{c}` in a radar chart"),
            ));
        }
    }
    Ok(())
}

struct Parser {
    tokens: Vec<(usize, Tok)>,
    at: usize,
    last_line: usize,
}

impl Parser {
    fn next(&mut self) -> Option<(usize, Tok)> {
        let t = self.tokens.get(self.at).cloned();
        if let Some((line, _)) = &t {
            self.last_line = *line;
            self.at += 1;
        }
        t
    }

    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at).map(|(_, t)| t)
    }

    fn line(&self) -> usize {
        self.tokens.get(self.at).map_or(self.last_line, |(l, _)| *l)
    }

    fn end(&self, what: &str) -> Error {
        Error::new(self.last_line, format!("expected {what}"))
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Punct(c)) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn skip_newlines(&mut self) {
        while self.peek() == Some(&Tok::Newline) {
            self.at += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), Error> {
        match self.next() {
            Some((_, Tok::Punct(p))) if p == c => Ok(()),
            Some((line, other)) => Err(Error::new(
                line,
                format!("expected `{c}`, found {}", describe(&other)),
            )),
            None => Err(self.end(&format!("`{c}`"))),
        }
    }

    fn word(&mut self, what: &str) -> Result<String, Error> {
        match self.next() {
            Some((_, Tok::Word(w))) => Ok(w),
            Some((line, other)) => Err(Error::new(
                line,
                format!("expected {what}, found {}", describe(&other)),
            )),
            None => Err(self.end(what)),
        }
    }

    fn number(&mut self) -> Result<f64, Error> {
        match self.next() {
            Some((_, Tok::Num(n))) => Ok(n),
            Some((line, other)) => Err(Error::new(
                line,
                format!("expected a number, found {}", describe(&other)),
            )),
            None => Err(self.end("a number")),
        }
    }

    /// An optional `["label"]`.
    fn label(&mut self) -> Result<Option<String>, Error> {
        if !self.eat('[') {
            return Ok(None);
        }
        let label = match self.next() {
            Some((_, Tok::Str(s))) => s,
            Some((line, other)) => {
                return Err(Error::new(
                    line,
                    format!("expected a quoted label, found {}", describe(&other)),
                ));
            }
            None => return Err(self.end("a quoted label")),
        };
        self.expect(']')?;
        Ok(Some(label))
    }

    /// `1, 2, 3 }` or `A: 1, B: 2 }`, possibly across lines.
    fn entries(&mut self) -> Result<Vec<Entry>, Error> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            let axis = if let Some(Tok::Word(_)) = self.peek() {
                let w = self.word("an axis")?;
                self.eat(':');
                Some(w)
            } else {
                None
            };
            out.push((axis, self.number()?));
            self.skip_newlines();
            if self.eat('}') {
                return Ok(out);
            }
            self.expect(',')?;
        }
    }

    fn end_of_statement(&mut self) -> Result<(), Error> {
        match self.next() {
            None | Some((_, Tok::Newline)) => Ok(()),
            Some((line, other)) => Err(Error::new(
                line,
                format!("unexpected {} after a statement", describe(&other)),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_axes_curves_and_options() {
        let r = parse(
            "radar-beta\n  title Grades\n  axis m[\"Math\"], s[\"Science\"], e[\"English\"]\n  \
             axis h[\"History\"]\n  curve a[\"Alice\"]{85, 90, 80, 70}\n  \
             curve b[\"Bob\"]{ h: 1, e: 2,\n    s: 3, m 4 }\n  max 100\n  min 0, ticks 4\n  \
             graticule polygon\n  showLegend false\n",
        )
        .unwrap();
        assert_eq!(r.title.as_deref(), Some("Grades"));
        assert_eq!(r.axes.len(), 4);
        assert_eq!(r.axes[3].label, "History");
        assert_eq!(r.curves[0].label, "Alice");
        // Named entries come back in axis order.
        assert_eq!(r.curves[1].values, vec![4.0, 3.0, 2.0, 1.0]);
        assert_eq!(r.max, Some(100.0));
        assert_eq!(r.ticks, 4);
        assert_eq!(r.graticule, Graticule::Polygon);
        assert!(!r.show_legend);
    }

    #[test]
    fn several_curves_on_one_line() {
        let r = parse("radar-beta:\n  axis A, B, C\n  curve x{1,2,3}, y{3,2,1}\n").unwrap();
        assert_eq!(r.axes[0].label, "A");
        assert_eq!(r.curves.len(), 2);
        assert_eq!(r.curves[1].label, "y");
    }

    #[test]
    fn bad_input_is_an_error_with_its_line() {
        for (source, line, needle) in [
            (
                "radar-beta\n  axis A, B\n  curve c{1}\n",
                3,
                "1 values for 2 axes",
            ),
            (
                "radar-beta\n  axis A, B\n  curve c{A: 1}\n",
                3,
                "no value for axis `B`",
            ),
            (
                "radar-beta\n  axis A\n  curve c{A: 1, Z: 2}\n",
                3,
                "unknown axis `Z`",
            ),
            ("radar-beta\n  curve c{1}\n", 1, "needs `axis`"),
            ("radar-beta\n  axis A, B\n  curve c{1, B 2}\n", 3, "mixes"),
            ("radar-beta\n  axis A\n  legend on\n", 3, "`legend`"),
            ("radar-beta\n  axis A\n  graticule hex\n", 3, "hex"),
            ("radar-beta\n  axis A\n  curve c{1\n", 3, "expected"),
            ("radar-beta\n  axis A\n  max 0\n", 1, "greater"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
