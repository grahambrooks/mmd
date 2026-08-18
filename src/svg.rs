//! SVG output.
//!
//! Every escape and every attribute is written here, so no diagram module
//! builds markup by hand. Styling is emitted as presentation attributes — the
//! crate never produces a CSS cascade, because SVG consumers vary in how much
//! CSS they support.

use std::fmt::Write;

use crate::geom::Rect;

/// Builds an SVG document.
pub struct Svg {
    body: String,
    open: Vec<&'static str>,
}

/// One attribute. Numbers are written with a fixed, compact precision so
/// output is stable across runs and platforms.
pub enum Value<'a> {
    Str(&'a str),
    Num(f32),
}

impl<'a> From<&'a str> for Value<'a> {
    fn from(value: &'a str) -> Self {
        Value::Str(value)
    }
}

impl From<f32> for Value<'_> {
    fn from(value: f32) -> Self {
        Value::Num(value)
    }
}

impl Svg {
    pub fn new() -> Self {
        Self {
            body: String::new(),
            open: Vec::new(),
        }
    }

    /// Write a self-closing element.
    pub fn element<'a>(&mut self, name: &str, attrs: &[(&str, Value<'a>)]) {
        self.indent();
        let _ = write!(self.body, "<{name}");
        self.attrs(attrs);
        self.body.push_str("/>\n");
    }

    /// Write an element with escaped text content.
    pub fn text_element<'a>(&mut self, name: &str, attrs: &[(&str, Value<'a>)], text: &str) {
        self.indent();
        let _ = write!(self.body, "<{name}");
        self.attrs(attrs);
        self.body.push('>');
        escape_into(&mut self.body, text);
        let _ = writeln!(self.body, "</{name}>");
    }

    /// Open a container element; close it with [`Svg::end`].
    pub fn begin<'a>(&mut self, name: &'static str, attrs: &[(&str, Value<'a>)]) {
        self.indent();
        let _ = write!(self.body, "<{name}");
        self.attrs(attrs);
        self.body.push_str(">\n");
        self.open.push(name);
    }

    pub fn end(&mut self) {
        if let Some(name) = self.open.pop() {
            self.indent();
            let _ = writeln!(self.body, "</{name}>");
        }
    }

    /// Finish the document, sizing the viewBox to `bounds`.
    pub fn finish(mut self, bounds: Rect) -> String {
        while !self.open.is_empty() {
            self.end();
        }
        let mut out = String::with_capacity(self.body.len() + 256);
        out.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\"");
        let _ = write!(out, " width=\"{}\"", Number(bounds.width));
        let _ = write!(out, " height=\"{}\"", Number(bounds.height));
        let _ = write!(
            out,
            " viewBox=\"{} {} {} {}\"",
            Number(bounds.x),
            Number(bounds.y),
            Number(bounds.width),
            Number(bounds.height)
        );
        out.push_str(">\n");
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        out
    }

    fn attrs<'a>(&mut self, attrs: &[(&str, Value<'a>)]) {
        for (name, value) in attrs {
            let _ = write!(self.body, " {name}=\"");
            match value {
                Value::Str(text) => escape_into(&mut self.body, text),
                Value::Num(number) => {
                    let _ = write!(self.body, "{}", Number(*number));
                }
            }
            self.body.push('"');
        }
    }

    fn indent(&mut self) {
        for _ in 0..self.open.len() {
            self.body.push_str("  ");
        }
    }
}

impl Default for Svg {
    fn default() -> Self {
        Self::new()
    }
}

/// A number formatted for SVG: at most three decimals, no trailing zeros, and
/// no `-0`.
pub struct Number(pub f32);

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let rounded = (self.0 * 1000.0).round() / 1000.0;
        let rounded = if rounded == 0.0 { 0.0 } else { rounded };
        if rounded.fract() == 0.0 {
            write!(f, "{}", rounded as i64)
        } else {
            let text = format!("{rounded:.3}");
            write!(f, "{}", text.trim_end_matches('0').trim_end_matches('.'))
        }
    }
}

fn escape_into(out: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup_in_text_and_attributes() {
        let mut svg = Svg::new();
        svg.text_element("text", &[("id", "a\"b".into())], "<script> & 'x'");
        let out = svg.finish(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(out.contains("&lt;script&gt; &amp; &apos;x&apos;"), "{out}");
        assert!(out.contains("id=\"a&quot;b\""), "{out}");
        assert!(!out.contains("<script>"));
    }

    #[test]
    fn numbers_are_compact_and_stable() {
        assert_eq!(Number(1.0).to_string(), "1");
        assert_eq!(Number(1.5).to_string(), "1.5");
        assert_eq!(Number(1.23456).to_string(), "1.235");
        assert_eq!(Number(-0.0).to_string(), "0");
    }

    #[test]
    fn nesting_closes_every_open_element() {
        let mut svg = Svg::new();
        svg.begin("g", &[]);
        svg.begin("g", &[]);
        svg.element("rect", &[("x", 1.0.into())]);
        let out = svg.finish(Rect::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(out.matches("<g").count(), 2);
        assert_eq!(out.matches("</g>").count(), 2);
    }

    #[test]
    fn viewbox_matches_the_bounds() {
        let out = Svg::new().finish(Rect::new(-5.0, -5.0, 100.0, 50.0));
        assert!(out.contains("viewBox=\"-5 -5 100 50\""), "{out}");
        assert!(out.contains("width=\"100\""), "{out}");
    }
}
