//! `pie` source to a model.
//!
//! Grammar: `packages/parser/src/language/pie/pie.langium`.

use crate::Error;
use crate::chart::{self, Statement};

/// A pie chart.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pie {
    pub title: Option<String>,
    /// Show each slice's value beside its legend entry.
    pub show_data: bool,
    pub slices: Vec<Slice>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    pub label: String,
    pub value: f64,
}

/// Parse a `pie` source.
pub fn parse(source: &str) -> Result<Pie, Error> {
    let (header, body) = chart::statements(source, &["pie"])?;
    let mut pie = Pie::default();
    let mut rest = header.text;
    if let Some(after) = chart::keyword(rest, "showData") {
        pie.show_data = true;
        rest = after;
    }
    if !rest.is_empty() {
        pie.title = Some(chart::title(rest).ok_or_else(|| {
            Error::new(
                header.line,
                format!("expected `showData` or `title` after `pie`, found `{rest}`"),
            )
        })?);
    }
    for statement in body {
        if let Some(t) = chart::title(statement.text) {
            pie.title = Some(t);
            continue;
        }
        let slice = slice(statement)?;
        if pie.slices.iter().any(|s| s.label == slice.label) {
            return Err(Error::new(
                statement.line,
                format!("slice \"{}\" is declared twice", slice.label),
            ));
        }
        pie.slices.push(slice);
    }
    if pie.slices.is_empty() {
        return Err(Error::new(
            header.line,
            "a pie needs at least one `\"label\" : value` slice",
        ));
    }
    if pie.slices.iter().all(|s| s.value == 0.0) {
        return Err(Error::new(
            header.line,
            "every slice of this pie is zero, so there is nothing to draw",
        ));
    }
    Ok(pie)
}

/// `"label" : value`.
fn slice(statement: Statement) -> Result<Slice, Error> {
    let text = statement.text;
    let line = statement.line;
    let quote = text.chars().next().filter(|c| matches!(c, '"' | '\''));
    let Some(quote) = quote else {
        let word = text.split_whitespace().next().unwrap_or(text);
        return Err(Error::new(
            line,
            format!(
                "`{word}` is not supported in a pie (see DESIGN.md); expected `\"label\" : value`"
            ),
        ));
    };
    let close = text[1..]
        .find(quote)
        .map(|i| i + 1)
        .ok_or_else(|| Error::new(line, "unterminated quoted label"))?;
    let label = text[1..close].to_string();
    let value = text[close + 1..]
        .trim_start()
        .strip_prefix(':')
        .ok_or_else(|| Error::new(line, format!("expected `:` and a value after \"{label}\"")))?
        .trim();
    let value = chart::number(value)
        .ok_or_else(|| Error::new(line, format!("`{value}` is not a number")))?;
    if value < 0.0 {
        return Err(Error::new(
            line,
            format!("\"{label}\" has the negative value {value}; pie slices must be zero or more"),
        ));
    }
    Ok(Slice { label, value })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_title_show_data_and_slices() {
        let pie =
            parse("pie showData title Pets adopted\n  \"Dogs\" : 386\n  'Cats' : 85.5\n").unwrap();
        assert!(pie.show_data);
        assert_eq!(pie.title.as_deref(), Some("Pets adopted"));
        assert_eq!(pie.slices.len(), 2);
        assert_eq!(pie.slices[1].value, 85.5);
    }

    #[test]
    fn title_may_be_its_own_statement() {
        let pie = parse("pie\n  title Key elements\n  \"Iron\" : 5\n").unwrap();
        assert_eq!(pie.title.as_deref(), Some("Key elements"));
    }

    #[test]
    fn bad_input_is_an_error_with_its_line() {
        for (source, line, needle) in [
            ("pie\n  \"a\" : -1\n", 2, "negative"),
            ("pie\n  \"a\" : x\n", 2, "not a number"),
            ("pie\n  \"a\" : 1\n  \"a\" : 2\n", 3, "twice"),
            ("pie\n", 1, "at least one"),
            ("pie\n  \"a\" : 0\n", 1, "zero"),
            ("pie\n  section x\n", 2, "`section`"),
            ("pie showDatum\n  \"a\" : 1\n", 1, "showDatum"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
