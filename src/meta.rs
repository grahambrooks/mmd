//! Statements several grammars share: `title`, `accTitle` and `accDescr`
//! (langium's `TitleAndAccessibilities` fragment), and the indentation of
//! a line for the outline-shaped diagrams.

use crate::{Error, strip_comment};

/// A source line: its 1-based number and its text without a `%%` comment.
pub(crate) type Line<'a> = (usize, &'a str);

/// Every line of `source` with its comment stripped, blank lines dropped.
pub(crate) fn lines(source: &str) -> Vec<Line<'_>> {
    source
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, strip_comment(line).trim_end()))
        .filter(|(_, line)| !line.trim().is_empty())
        .collect()
}

/// Leading whitespace, in characters (a tab counts as one, as upstream).
pub(crate) fn indent(line: &str) -> usize {
    line.chars().take_while(|c| c.is_whitespace()).count()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Meta {
    Title(String),
    AccTitle(String),
    AccDescr(String),
}

/// If `lines[*at]` is a `title`, `accTitle` or `accDescr` statement, read
/// it — and the further lines of an `accDescr { … }` — and move `at` past it.
pub(crate) fn meta(lines: &[Line<'_>], at: &mut usize) -> Result<Option<Meta>, Error> {
    let (number, raw) = lines[*at];
    let line = raw.trim();
    if let Some(rest) = word(line, "title") {
        *at += 1;
        return Ok(Some(Meta::Title(rest.trim().to_string())));
    }
    if let Some(rest) = word(line, "accTitle") {
        let value = rest
            .trim_start()
            .strip_prefix(':')
            .ok_or_else(|| Error::new(number, "expected `accTitle: <text>`"))?;
        *at += 1;
        return Ok(Some(Meta::AccTitle(value.trim().to_string())));
    }
    if let Some(rest) = word(line, "accDescr") {
        let rest = rest.trim_start();
        if let Some(value) = rest.strip_prefix(':') {
            *at += 1;
            return Ok(Some(Meta::AccDescr(value.trim().to_string())));
        }
        let body = rest
            .strip_prefix('{')
            .ok_or_else(|| Error::new(number, "expected `accDescr: <text>` or `accDescr { … }`"))?;
        let mut text = Vec::new();
        let mut current = body;
        loop {
            if let Some((inside, _)) = current.split_once('}') {
                text.push(inside.trim());
                *at += 1;
                break;
            }
            text.push(current.trim());
            *at += 1;
            if *at >= lines.len() {
                return Err(Error::new(number, "`accDescr {` has no closing `}`"));
            }
            current = lines[*at].1;
        }
        let text: Vec<&str> = text.into_iter().filter(|t| !t.is_empty()).collect();
        return Ok(Some(Meta::AccDescr(text.join("\n"))));
    }
    Ok(None)
}

/// The rest of `line` if it starts with the keyword `word`.
fn word<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    line.strip_prefix(word)
        .filter(|rest| rest.is_empty() || rest.starts_with([' ', '\t', ':', '{']))
}

/// The diagram's first line, which must be `keyword` or one of `aliases`;
/// anything after the keyword on that line is an error.
pub(crate) fn header(lines: &[Line<'_>], keywords: &[&str]) -> Result<usize, Error> {
    let Some(&(number, line)) = lines.first() else {
        return Err(Error::new(1, format!("expected `{}`", keywords[0])));
    };
    let line = line.trim();
    for keyword in keywords {
        if let Some(rest) = line.strip_prefix(keyword) {
            let rest = rest.trim().trim_start_matches(':').trim();
            if rest.is_empty() {
                return Ok(number);
            }
            return Err(Error::new(
                number,
                format!("unexpected `{rest}` after `{keyword}`"),
            ));
        }
    }
    Err(Error::new(number, format!("expected `{}`", keywords[0])))
}

/// A `"double"` or `'single'` quoted string at the start of `text`: its
/// contents and what follows.
pub(crate) fn quoted(text: &str) -> Option<(&str, &str)> {
    let quote = text.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &text[1..];
    let end = body.find(quote)?;
    Some((&body[..end], &body[end + 1..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_titles_and_accessibility() {
        let source = "x\ntitle  Hello\naccTitle: T\naccDescr {\n  one\n  two\n}\naccDescr: d\n";
        let lines = lines(source);
        let mut at = 1;
        let mut out = Vec::new();
        while at < lines.len() {
            out.push(meta(&lines, &mut at).unwrap().unwrap());
        }
        assert_eq!(
            out,
            vec![
                Meta::Title("Hello".into()),
                Meta::AccTitle("T".into()),
                Meta::AccDescr("one\ntwo".into()),
                Meta::AccDescr("d".into()),
            ]
        );
    }

    #[test]
    fn an_unclosed_description_is_an_error() {
        let lines = lines("x\naccDescr {\n  one\n");
        let error = meta(&lines, &mut 1).unwrap_err();
        assert_eq!(error.line, 2);
    }

    #[test]
    fn other_words_are_not_meta() {
        let lines = lines("x\ntitles\n");
        assert_eq!(meta(&lines, &mut 1).unwrap(), None);
    }

    #[test]
    fn quotes() {
        assert_eq!(quoted("\"a b\": 1"), Some(("a b", ": 1")));
        assert_eq!(quoted("'a'"), Some(("a", "")));
        assert_eq!(quoted("\"open"), None);
    }
}
