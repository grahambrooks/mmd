//! A diagram's accessible name and description, from mermaid's `accTitle`
//! and `accDescr` statements.
//!
//! Every diagram type spells them the same way, so they are read here, once,
//! rather than by each type's parser: `accTitle: text`, `accDescr: text`, and
//! the block form `accDescr { … }`, on one line or several.

use diagram_scene::Accessibility;

/// Read `source`'s accessibility statements; `fallback` names the diagram
/// when it has no `accTitle`.
pub(crate) fn of(source: &str, fallback: Option<String>) -> Accessibility {
    let mut out = Accessibility::default();
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(rest) = statement(line, "accTitle")
            && let Some(text) = rest.strip_prefix(':')
        {
            out.title = non_empty(text);
        } else if let Some(rest) = statement(line, "accDescr") {
            if let Some(text) = rest.strip_prefix(':') {
                out.description = non_empty(text);
            } else if let Some(body) = rest.strip_prefix('{') {
                out.description = block(body, &mut lines);
            }
        }
    }
    if out.title.is_none() {
        out.title = fallback;
    }
    out
}

/// What follows `keyword` when `line` is that statement.
fn statement<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?;
    let rest = rest.trim_start();
    rest.starts_with([':', '{']).then_some(rest)
}

/// An `accDescr { … }` body: the rest of this line and following lines, up
/// to the closing brace, joined with newlines.
fn block<'a>(first: &str, lines: &mut impl Iterator<Item = &'a str>) -> Option<String> {
    let mut parts = Vec::new();
    let mut current = first.to_string();
    loop {
        if let Some(end) = current.find('}') {
            parts.push(current[..end].trim().to_string());
            break;
        }
        parts.push(current.trim().to_string());
        match lines.next() {
            Some(next) => current = next.to_string(),
            // Unclosed: the diagram's own parser reports that.
            None => break,
        }
    }
    let text = parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    non_empty(&text)
}

fn non_empty(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(source: &str) -> Accessibility {
        of(source, None)
    }

    #[test]
    fn reads_single_line_statements() {
        let a = read("pie\n  accTitle: Pets\n  accDescr: Dogs outnumber cats\n  \"Dogs\": 3\n");
        assert_eq!(a.title.as_deref(), Some("Pets"));
        assert_eq!(a.description.as_deref(), Some("Dogs outnumber cats"));
    }

    #[test]
    fn reads_block_descriptions_on_one_line_or_several() {
        let a = read("graph\n  accDescr { One line }\n  A\n");
        assert_eq!(a.description.as_deref(), Some("One line"));
        let a = read("graph\n  accDescr {\n    First\n    Second\n  }\n  A\n");
        assert_eq!(a.description.as_deref(), Some("First\nSecond"));
    }

    #[test]
    fn falls_back_without_an_acc_title() {
        let plan = || Some("Release plan".to_string());
        assert_eq!(of("gantt\n", plan()).title.as_deref(), Some("Release plan"));
        let a = of("gantt\n  accTitle: Q3\n", plan());
        assert_eq!(a.title.as_deref(), Some("Q3"), "accTitle wins");
    }

    #[test]
    fn ignores_lookalikes() {
        let a = read("flowchart\n  accTitleBar --> B\n  accDescription[x]\n");
        assert_eq!(a, Accessibility::default());
    }
}
