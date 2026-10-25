//! `info` parsing, per mermaid's `packages/parser/src/language/info/info.langium`:
//! the keyword, an optional `showInfo`, then `title`/`accTitle`/`accDescr`.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    /// `showInfo`; accepted, as mermaid draws nothing different for it.
    pub show_info: bool,
    pub title: Option<String>,
    /// `accTitle:`; stored, not drawn.
    pub acc_title: Option<String>,
    /// `accDescr`; stored, not drawn.
    pub acc_descr: Option<String>,
}

pub fn parse(source: &str) -> Result<Info, Error> {
    let mut info = Info::default();
    let mut seen_header = false;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            if line != "info" {
                return Err(Error::new(number, "expected `info`"));
            }
            seen_header = true;
            continue;
        }
        if line == "showInfo" {
            info.show_info = true;
        } else if let Some(rest) = keyword(line, "title") {
            info.title = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("accTitle") {
            let rest = rest
                .trim_start()
                .strip_prefix(':')
                .ok_or_else(|| Error::new(number, "expected `accTitle: text`"))?;
            info.acc_title = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("accDescr") {
            let rest = rest.trim_start().strip_prefix(':').ok_or_else(|| {
                Error::new(
                    number,
                    "only the one-line `accDescr: text` is supported in `info`",
                )
            })?;
            info.acc_descr = Some(rest.trim().to_string());
        } else {
            return Err(Error::new(
                number,
                format!("`{line}` is not part of an `info` diagram (see DESIGN.md)"),
            ));
        }
    }
    if !seen_header {
        return Err(Error::new(1, "expected `info`"));
    }
    Ok(info)
}

/// The rest of `line` after `word` and a space, or `""` if it is all.
fn keyword<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(word)?;
    if rest.is_empty() {
        Some("")
    } else if rest.starts_with([' ', '\t']) {
        Some(rest.trim())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_show_info_and_title() {
        let info = parse("info\n  showInfo\n  title About\n").unwrap();
        assert!(info.show_info);
        assert_eq!(info.title.as_deref(), Some("About"));
    }

    #[test]
    fn anything_else_is_an_error_with_its_line() {
        let error = parse("info\n\n  commit\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("commit"));
    }
}
