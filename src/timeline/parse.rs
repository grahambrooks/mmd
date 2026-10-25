//! timeline parsing.
//!
//! Grammar per mermaid's
//! `packages/mermaid/src/diagrams/timeline/parser/timeline.jison`: a period
//! is the text before the first `: `, and each further `: ` starts an event.
//! A line that starts with `:` adds events to the period above it.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// Periods left to right, events stacked beneath each.
    #[default]
    LeftRight,
    /// Periods top to bottom, events in a row beside each.
    TopDown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub text: String,
    /// Index into [`Timeline::sections`]; `None` before the first `section`.
    pub section: Option<usize>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timeline {
    pub title: Option<String>,
    pub direction: Direction,
    pub sections: Vec<String>,
    pub periods: Vec<Period>,
}

/// Parse a timeline source.
pub fn parse(source: &str) -> Result<Timeline, Error> {
    let mut timeline = Timeline::default();
    let mut seen_header = false;
    let mut in_acc_descr = false;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw).trim();
        if in_acc_descr {
            if line.contains('}') {
                in_acc_descr = false;
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            let Some(rest) = keyword(line, "timeline") else {
                return Err(Error::new(line_number, "expected `timeline`"));
            };
            timeline.direction = match rest {
                "" | "LR" => Direction::LeftRight,
                "TD" => Direction::TopDown,
                other => {
                    return Err(Error::new(
                        line_number,
                        format!("timeline direction `{other}` is not supported; use LR or TD"),
                    ));
                }
            };
            seen_header = true;
            continue;
        }

        if let Some(rest) = keyword(line, "title") {
            timeline.title = Some(rest.to_string());
        } else if is_acc(line, "accTitle") || is_acc(line, "accDescr") {
            // Accessibility text is not drawn.
        } else if line
            .strip_prefix("accDescr")
            .is_some_and(|rest| rest.trim_start().starts_with('{'))
        {
            in_acc_descr = !line.contains('}');
        } else if let Some(rest) = keyword(line, "section") {
            if rest.is_empty() {
                return Err(Error::new(line_number, "`section` needs a name"));
            }
            timeline.sections.push(text(rest));
        } else if line.starts_with(':') {
            let events = events(line);
            let Some(period) = timeline.periods.last_mut() else {
                return Err(Error::new(
                    line_number,
                    "an event line (`: …`) needs a time period above it",
                ));
            };
            if events.is_empty() {
                return Err(Error::new(line_number, "an empty event"));
            }
            period.events.extend(events);
        } else {
            let (period, rest) = match split_event(line) {
                Some(at) => (&line[..at], &line[at..]),
                None => (line, ""),
            };
            timeline.periods.push(Period {
                text: text(period.trim()),
                section: timeline.sections.len().checked_sub(1),
                events: events(rest),
            });
        }
    }

    if !seen_header {
        return Err(Error::new(1, "expected `timeline`"));
    }
    Ok(timeline)
}

/// The byte offset of the first `:` that starts an event: one followed by
/// whitespace or ending the line.
fn split_event(line: &str) -> Option<usize> {
    line.char_indices()
        .find(|&(at, c)| {
            c == ':'
                && line[at + 1..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace)
        })
        .map(|(at, _)| at)
}

/// Events in `: a : b`.
fn events(mut rest: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(at) = split_event(rest) {
        let tail = &rest[at + 1..];
        let end = split_event(tail).unwrap_or(tail.len());
        let event = tail[..end].trim();
        if !event.is_empty() {
            out.push(text(event));
        }
        rest = &tail[end..];
    }
    out
}

/// Display text: `<br>` becomes a line break.
fn text(raw: &str) -> String {
    let mut out = raw.to_string();
    for br in ["<br/>", "<br />", "<br>", "<BR>", "<BR/>"] {
        out = out.replace(br, "\n");
    }
    out
}

fn keyword<'a>(line: &'a str, word: &str) -> Option<&'a str> {
    let head = line.get(..word.len())?;
    if !head.eq_ignore_ascii_case(word) {
        return None;
    }
    let rest = &line[word.len()..];
    if rest.is_empty() {
        Some("")
    } else {
        rest.starts_with(char::is_whitespace).then(|| rest.trim())
    }
}

fn is_acc(line: &str, word: &str) -> bool {
    line.strip_prefix(word)
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn periods_take_inline_and_continued_events() {
        let source = "timeline
    title History of social media
    2002 : LinkedIn
    2004 : Facebook : Google
         : Orkut
    2005 : YouTube
";
        let timeline = parse(source).unwrap();
        assert_eq!(timeline.title.as_deref(), Some("History of social media"));
        assert_eq!(timeline.periods.len(), 3);
        assert_eq!(timeline.periods[1].events, ["Facebook", "Google", "Orkut"]);
        assert_eq!(timeline.periods[0].section, None);
    }

    #[test]
    fn sections_group_the_periods_after_them() {
        let source = "timeline
    section Early
      1990 : Web
    section Later
      2007 : iPhone
      2010
";
        let timeline = parse(source).unwrap();
        assert_eq!(timeline.sections, ["Early", "Later"]);
        assert_eq!(timeline.periods[1].section, Some(1));
        assert!(timeline.periods[2].events.is_empty());
    }

    #[test]
    fn colons_inside_an_event_survive() {
        let timeline = parse("timeline\n  2024 : ratio 3:2 : done\n").unwrap();
        assert_eq!(timeline.periods[0].events, ["ratio 3:2", "done"]);
    }

    #[test]
    fn direction_and_breaks() {
        let timeline = parse("timeline TD\n  Q1 : plan<br>and build\n").unwrap();
        assert_eq!(timeline.direction, Direction::TopDown);
        assert_eq!(timeline.periods[0].events[0], "plan\nand build");
    }

    #[test]
    fn errors_carry_lines() {
        let error = parse("timeline\n  : orphan\n").unwrap_err();
        assert_eq!(error.line, 2);
        let error = parse("timeline RL\n").unwrap_err();
        assert!(error.message.contains("RL"));
    }
}
