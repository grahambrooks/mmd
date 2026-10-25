//! kanban parsing.
//!
//! Grammar per mermaid's `packages/mermaid/src/diagrams/kanban/parser/
//! kanban.jison` and `kanbanDb.ts`: the first node's indentation is the
//! column level; deeper nodes are cards in the column above them. A node is
//! `id`, `id[label]` (any of mindmap's brackets) or `[label]`, optionally
//! followed by `@{ key: value, … }` metadata, which may span lines.
//!
//! `::icon(name)` and `:::class …` are statements of their own, on the
//! line after the node they decorate (as in mindmap). Kanban has no
//! `classDef`, so a class is only a CSS hook; it is kept, not drawn.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    VeryHigh,
    High,
    Low,
    VeryLow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub id: String,
    pub label: String,
    pub ticket: Option<String>,
    pub assigned: Option<String>,
    pub priority: Option<Priority>,
    /// `icon:` metadata or `::icon(…)`: a Font Awesome (or other icon
    /// pack) name.
    pub icon: Option<String>,
    /// `:::a b`: CSS class names, which nothing in the board styles.
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub id: String,
    pub label: String,
    pub cards: Vec<Card>,
    /// Kept as given; mermaid draws neither on a column.
    pub icon: Option<String>,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Kanban {
    pub columns: Vec<Column>,
}

/// Parse a kanban source.
pub fn parse(source: &str) -> Result<Kanban, Error> {
    let mut kanban = Kanban::default();
    let mut column_level: Option<usize> = None;
    let mut seen_header = false;
    let lines: Vec<&str> = source.lines().collect();
    let mut index = 0;

    while index < lines.len() {
        let line_number = index + 1;
        let raw = strip_comment(lines[index]);
        index += 1;
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            if line == "kanban" {
                seen_header = true;
                continue;
            }
            return Err(Error::new(line_number, "expected `kanban`"));
        }
        if let Some(body) = line.strip_prefix("::icon(") {
            let Some((name, tail)) = body.split_once(')') else {
                return Err(Error::new(
                    line_number,
                    "`::icon(` is never closed with `)`",
                ));
            };
            if !tail.trim().is_empty() {
                return Err(Error::new(
                    line_number,
                    format!("unexpected `{}` after `::icon(…)`", tail.trim()),
                ));
            }
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::new(line_number, "`::icon()` names no icon"));
            }
            *decorated(&mut kanban, line_number, "::icon()")?.0 = Some(name.to_string());
            continue;
        }
        if let Some(names) = line.strip_prefix(":::") {
            let classes = decorated(&mut kanban, line_number, ":::")?.1;
            classes.extend(names.split_whitespace().map(str::to_string));
            continue;
        }
        let level = raw.chars().take_while(|c| c.is_whitespace()).count();

        let (id, label, rest) = node(line, line_number)?;
        let mut card = Card {
            id,
            label,
            ticket: None,
            assigned: None,
            priority: None,
            icon: None,
            classes: Vec::new(),
        };
        let mut rest = rest.trim_start();
        if let Some(body) = rest.strip_prefix("@{") {
            // Metadata runs to the closing brace, on this line or a later one.
            let mut data = body.to_string();
            while !data.contains('}') {
                let Some(next) = lines.get(index) else {
                    return Err(Error::new(line_number, "`@{` is never closed with `}`"));
                };
                data.push('\n');
                data.push_str(strip_comment(next));
                index += 1;
            }
            let close = data.find('}').expect("found above");
            let tail = data[close + 1..].trim().to_string();
            metadata(&data[..close], &mut card, line_number)?;
            if !tail.is_empty() {
                return Err(Error::new(
                    line_number,
                    format!("unexpected `{tail}` after `@{{…}}`"),
                ));
            }
            rest = "";
        }
        let rest = rest.trim();
        if rest.starts_with(":::") || rest.starts_with("::icon(") {
            let what = if rest.starts_with(":::") {
                ":::"
            } else {
                "::icon()"
            };
            return Err(Error::new(
                line_number,
                format!("`{what}` goes on its own line, under the node it decorates, not after it"),
            ));
        }
        if !rest.is_empty() {
            return Err(Error::new(
                line_number,
                format!("unexpected `{rest}` after `{}`", card.label),
            ));
        }

        let column_level = *column_level.get_or_insert(level);
        if level < column_level {
            return Err(Error::new(
                line_number,
                format!(
                    "`{}` is indented less than the first column; columns must line up",
                    card.label
                ),
            ));
        }
        if level == column_level {
            kanban.columns.push(Column {
                id: card.id,
                label: card.label,
                cards: Vec::new(),
                icon: card.icon,
                classes: Vec::new(),
            });
        } else {
            kanban
                .columns
                .last_mut()
                .expect("the first node is a column")
                .cards
                .push(card);
        }
    }

    if !seen_header {
        return Err(Error::new(1, "expected `kanban`"));
    }
    Ok(kanban)
}

/// The icon and classes of the node `::icon()` or `:::` decorates: the
/// last one, card or column.
fn decorated<'a>(
    kanban: &'a mut Kanban,
    line: usize,
    what: &str,
) -> Result<(&'a mut Option<String>, &'a mut Vec<String>), Error> {
    let Some(column) = kanban.columns.last_mut() else {
        return Err(Error::new(
            line,
            format!("`{what}` decorates the node above it, and there is none"),
        ));
    };
    Ok(match column.cards.last_mut() {
        Some(card) => (&mut card.icon, &mut card.classes),
        None => (&mut column.icon, &mut column.classes),
    })
}

/// `id`, `id[label]`, `[label]` or another mindmap bracket form; returns the
/// id, the label and whatever follows.
fn node(line: &str, line_number: usize) -> Result<(String, String, &str), Error> {
    const BRACKETS: [(&str, &str); 8] = [
        ("((", "))"),
        ("{{", "}}"),
        ("))", "(("),
        ("(-", "-)"),
        ("-)", "(-"),
        ("[", "]"),
        ("(", ")"),
        (")", "("),
    ];
    let id_end = line
        .find(|c: char| "([{)}@".contains(c))
        .unwrap_or(line.len());
    let id = line[..id_end].trim();
    let after = &line[id_end..];
    let Some((open, close)) = BRACKETS.iter().find(|(open, _)| after.starts_with(open)) else {
        if id.is_empty() {
            return Err(Error::new(line_number, format!("unexpected `{line}`")));
        }
        return Ok((id.to_string(), id.to_string(), after));
    };
    let inner = &after[open.len()..];
    let (label, rest) = if let Some(quoted) = inner.strip_prefix('"') {
        let end = quoted
            .find('"')
            .ok_or_else(|| Error::new(line_number, "an unclosed `\"` in a label"))?;
        let rest = quoted[end + 1..].trim_start();
        let rest = rest.strip_prefix(close).ok_or_else(|| {
            Error::new(line_number, format!("expected `{close}` after the label"))
        })?;
        (quoted[..end].trim_matches('`').to_string(), rest)
    } else {
        let end = inner
            .find(close)
            .ok_or_else(|| Error::new(line_number, format!("`{open}` is never closed")))?;
        (inner[..end].trim().to_string(), &inner[end + close.len()..])
    };
    let id = if id.is_empty() { label.as_str() } else { id };
    Ok((id.to_string(), label.clone(), rest))
}

/// `key: value` pairs separated by commas or newlines.
fn metadata(body: &str, card: &mut Card, line: usize) -> Result<(), Error> {
    for entry in split_entries(body) {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((key, value)) = entry.split_once(':') else {
            return Err(Error::new(
                line,
                format!("expected `key: value` in `@{{…}}`, got `{entry}`"),
            ));
        };
        let value = unquote(value.trim()).to_string();
        match key.trim() {
            "assigned" => card.assigned = Some(value),
            "ticket" => card.ticket = Some(value),
            "label" => card.label = value,
            "priority" => {
                card.priority = Some(match value.as_str() {
                    "Very High" => Priority::VeryHigh,
                    "High" => Priority::High,
                    "Low" => Priority::Low,
                    "Very Low" => Priority::VeryLow,
                    other => {
                        return Err(Error::new(
                            line,
                            format!(
                                "priority `{other}`: expected `Very High`, `High`, `Low` or \
                                 `Very Low`"
                            ),
                        ));
                    }
                })
            }
            // Every card is drawn as a card, whichever shape is named.
            "shape" => {}
            "icon" => card.icon = Some(value).filter(|v| !v.is_empty()),
            other => {
                return Err(Error::new(
                    line,
                    format!(
                        "unknown card metadata `{other}`; expected assigned, ticket, priority, \
                         icon or label"
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Split on commas and newlines outside double quotes.
fn split_entries(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for c in body.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            ',' | '\n' if !quoted => out.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    out.push(current);
    out
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indentation_makes_columns_and_cards() {
        let source = "kanban
  Todo
    Write the grammar
    id2[Update the database]
  id3[In progress]
    [Design]
";
        let kanban = parse(source).unwrap();
        assert_eq!(kanban.columns.len(), 2);
        assert_eq!(kanban.columns[0].label, "Todo");
        assert_eq!(kanban.columns[0].cards[1].id, "id2");
        assert_eq!(kanban.columns[0].cards[1].label, "Update the database");
        assert_eq!(kanban.columns[1].label, "In progress");
        assert_eq!(kanban.columns[1].cards[0].label, "Design");
    }

    #[test]
    fn metadata_on_one_line_or_many() {
        let source = "kanban
  todo[Todo]
    a[Fix it]@{ ticket: MC-1, assigned: 'knsv', priority: 'Very High' }
    b[\"Ship, then rest\"]@{
      assigned: alice
      priority: Low
    }
";
        let kanban = parse(source).unwrap();
        let cards = &kanban.columns[0].cards;
        assert_eq!(cards[0].ticket.as_deref(), Some("MC-1"));
        assert_eq!(cards[0].assigned.as_deref(), Some("knsv"));
        assert_eq!(cards[0].priority, Some(Priority::VeryHigh));
        assert_eq!(cards[1].label, "Ship, then rest");
        assert_eq!(cards[1].priority, Some(Priority::Low));
    }

    #[test]
    fn icons_and_classes_decorate_the_node_above() {
        let source = "kanban
  todo[Todo]
  ::icon(fa fa-list)
    a[Fix it]@{ icon: heart, assigned: graham }
    b[Ship it]
    ::icon(fa fa-rocket)
    :::urgent large
";
        let kanban = parse(source).unwrap();
        let column = &kanban.columns[0];
        assert_eq!(column.icon.as_deref(), Some("fa fa-list"));
        assert_eq!(column.cards[0].icon.as_deref(), Some("heart"));
        assert_eq!(column.cards[0].assigned.as_deref(), Some("graham"));
        assert_eq!(column.cards[1].icon.as_deref(), Some("fa fa-rocket"));
        assert_eq!(column.cards[1].classes, ["urgent", "large"]);
    }

    #[test]
    fn errors_carry_lines() {
        for (source, line, needle) in [
            ("kanban\n  a\n    b@{ priority: Urgent }\n", 3, "Urgent"),
            ("kanban\n  ::icon(heart)\n  a\n", 2, "there is none"),
            ("kanban\n  a\n    b[x]:::hot\n", 3, "own line"),
            ("kanban\n  a\n    b\n    ::icon(heart\n", 4, "never closed"),
            ("kanban\n  a\n    b\n    ::icon(heart) x\n", 4, "unexpected"),
            ("kanban\n    a\n  b\n", 3, "indented less"),
            ("kanban\n  a\n    b[open\n", 3, "never closed"),
            ("kanban\n  a\n    b@{ ticket: 1\n", 3, "never closed"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source:?}: {error}");
            assert!(error.message.contains(needle), "{error}");
        }
    }
}
