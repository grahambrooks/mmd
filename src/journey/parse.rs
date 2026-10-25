//! User-journey parsing.
//!
//! Grammar per mermaid's
//! `packages/mermaid/src/diagrams/user-journey/parser/journey.jison`:
//! `title`, `section <name>`, and tasks `name: score: actor, actor`.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub name: String,
    /// Index into [`Journey::sections`]; `None` before the first `section`.
    pub section: Option<usize>,
    /// 0–5; higher is happier.
    pub score: f32,
    /// Indexes into [`Journey::actors`].
    pub actors: Vec<usize>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Journey {
    pub title: Option<String>,
    pub sections: Vec<String>,
    pub tasks: Vec<Task>,
    /// Everyone named in a task, sorted, as mermaid's `getActors` returns
    /// them: an actor's colour follows this order.
    pub actors: Vec<String>,
}

/// Parse a journey source.
pub fn parse(source: &str) -> Result<Journey, Error> {
    let mut journey = Journey::default();
    let mut people: Vec<Vec<String>> = Vec::new();
    let mut seen_header = false;
    let mut in_acc_descr = false;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        // `#` starts a comment in journey's grammar, as `%%` does.
        let line = strip_comment(raw);
        let line = line.split('#').next().unwrap_or("").trim();
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
            if keyword(line, "journey").is_some_and(str::is_empty) {
                seen_header = true;
                continue;
            }
            return Err(Error::new(line_number, "expected `journey`"));
        }

        if let Some(rest) = keyword(line, "title") {
            journey.title = Some(rest.to_string());
        } else if ["accTitle", "accDescr"].iter().any(|word| {
            line.strip_prefix(word)
                .is_some_and(|rest| rest.trim_start().starts_with(':'))
        }) {
            // Accessibility text is not drawn.
        } else if line
            .strip_prefix("accDescr")
            .is_some_and(|rest| rest.trim_start().starts_with('{'))
        {
            in_acc_descr = !line.contains('}');
        } else if let Some(rest) = keyword(line, "section") {
            if rest.is_empty() || rest.contains(':') {
                return Err(Error::new(
                    line_number,
                    "`section` takes a name, without `:`",
                ));
            }
            journey.sections.push(rest.to_string());
        } else {
            let Some((name, data)) = line.split_once(':') else {
                return Err(Error::new(
                    line_number,
                    format!("unexpected `{line}`; expected a task as `name: score: actors`"),
                ));
            };
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::new(line_number, "a task needs a name before `:`"));
            }
            let mut pieces = data.split(':');
            let score_text = pieces.next().unwrap_or("").trim();
            let score: f32 = score_text
                .parse()
                .ok()
                .filter(|s: &f32| (0.0..=5.0).contains(s))
                .ok_or_else(|| {
                    Error::new(
                        line_number,
                        format!("task `{name}`: score `{score_text}` is not a number from 0 to 5"),
                    )
                })?;
            let actors: Vec<String> = pieces
                .next()
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|actor| !actor.is_empty())
                .map(String::from)
                .collect();
            if pieces.next().is_some() {
                return Err(Error::new(
                    line_number,
                    format!("task `{name}` has more than `score: actors` after its name"),
                ));
            }
            people.push(actors);
            journey.tasks.push(Task {
                name: name.to_string(),
                section: journey.sections.len().checked_sub(1),
                score,
                actors: Vec::new(),
            });
        }
    }

    if !seen_header {
        return Err(Error::new(1, "expected `journey`"));
    }
    let mut actors: Vec<String> = people.iter().flatten().cloned().collect();
    actors.sort();
    actors.dedup();
    for (task, names) in journey.tasks.iter_mut().zip(&people) {
        task.actors = names
            .iter()
            .filter_map(|name| actors.iter().position(|a| a == name))
            .collect();
        task.actors.dedup();
    }
    journey.actors = actors;
    Ok(journey)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_take_a_score_and_actors() {
        let source = "journey
    title My working day
    section Go to work
      Make tea: 5: Me
      Go upstairs: 3: Me, Cat
    section Go home
      Sit down: 1
";
        let journey = parse(source).unwrap();
        assert_eq!(journey.title.as_deref(), Some("My working day"));
        assert_eq!(journey.actors, ["Cat", "Me"]);
        assert_eq!(journey.tasks[1].actors, [1, 0]);
        assert_eq!(journey.tasks[2].score, 1.0);
        assert!(journey.tasks[2].actors.is_empty());
        assert_eq!(journey.tasks[2].section, Some(1));
    }

    #[test]
    fn errors_carry_lines() {
        for (source, line, needle) in [
            ("journey\n  Tea: lots: Me\n", 2, "lots"),
            ("journey\n  Tea: 9: Me\n", 2, "9"),
            ("journey\n  just words\n", 2, "expected a task"),
            ("journey\n  Tea: 5: Me: extra\n", 2, "more than"),
            ("pie\n", 1, "journey"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source:?}");
            assert!(error.message.contains(needle), "{error}");
        }
    }
}
