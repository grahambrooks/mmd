//! The ishikawa grammar: `ishikawa/parser/ishikawa.jison` upstream, with
//! levels assigned as `ishikawaDb.ts` assigns them. Every line is text;
//! the first is the effect, and indentation relative to the first cause
//! gives the rest their depth.

use crate::Error;
use crate::meta;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Ishikawa {
    /// The effect (the problem) at the head.
    pub effect: String,
    /// Categories, each with its causes.
    pub causes: Vec<Cause>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Cause {
    pub text: String,
    pub causes: Vec<Cause>,
}

pub fn parse(source: &str) -> Result<Ishikawa, Error> {
    let lines = meta::lines(source);
    let header = lines.first().map_or(1, |l| l.0);
    let keyword = lines.first().map(|l| l.1.trim()).unwrap_or("");
    let rest = keyword
        .get(..13)
        .filter(|k| k.eq_ignore_ascii_case("ishikawa-beta"))
        .map(|_| &keyword[13..])
        .or_else(|| {
            keyword
                .get(..8)
                .filter(|k| k.eq_ignore_ascii_case("ishikawa"))
                .map(|_| &keyword[8..])
        })
        .ok_or_else(|| Error::new(header, "expected `ishikawa`"))?;
    if !rest.trim().is_empty() {
        return Err(Error::new(
            header,
            format!("unexpected `{}` after `ishikawa`", rest.trim()),
        ));
    }
    let Some(&(_, effect)) = lines.get(1) else {
        return Err(Error::new(
            header,
            "an ishikawa diagram needs its effect on the next line",
        ));
    };
    let mut diagram = Ishikawa {
        effect: effect.trim().to_string(),
        causes: Vec::new(),
    };
    // The open causes from a category down: (level, index among siblings).
    let mut path: Vec<(isize, usize)> = Vec::new();
    let mut base = None;
    for &(_, line) in &lines[2..] {
        let raw = meta::indent(line) as isize;
        let base = *base.get_or_insert(raw);
        let level = (raw - base + 1).max(1);
        // Pop to the parent: the nearest open cause at a lower level.
        while path.last().is_some_and(|&(l, _)| l >= level) {
            path.pop();
        }
        let mut siblings = &mut diagram.causes;
        for &(_, i) in &path {
            siblings = &mut siblings[i].causes;
        }
        siblings.push(Cause {
            text: line.trim().to_string(),
            causes: Vec::new(),
        });
        path.push((level, siblings.len() - 1));
    }
    Ok(diagram)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(c: &Cause) -> String {
        if c.causes.is_empty() {
            c.text.clone()
        } else {
            let inner: Vec<String> = c.causes.iter().map(shape).collect();
            format!("{}({})", c.text, inner.join(","))
        }
    }

    fn outline(source: &str) -> (String, Vec<String>) {
        let d = parse(source).unwrap();
        (d.effect, d.causes.iter().map(shape).collect())
    }

    #[test]
    fn nests_like_upstream() {
        let (effect, causes) = outline(
            "ishikawa-beta\n    Blurry Photo\n        Process\n            Out of focus\n        User\n            Shaky hands\n",
        );
        assert_eq!(effect, "Blurry Photo");
        assert_eq!(causes, vec!["Process(Out of focus)", "User(Shaky hands)"]);
        let (_, causes) = outline("ishikawa\nProblem\nCause A\n  Sub A1\nCause B\n");
        assert_eq!(causes, vec!["Cause A(Sub A1)", "Cause B"]);
        // The effect may be indented more than the causes.
        let (_, causes) = outline("ISHIKAWA\n    Problem\nA\n  A1\n    A1a\n  A2\nB\n");
        assert_eq!(causes, vec!["A(A1(A1a),A2)", "B"]);
        // A cause indented less than the first is still a category.
        let (_, causes) = outline("ishikawa\nP\n    A\n  B\n");
        assert_eq!(causes, vec!["A", "B"]);
    }

    #[test]
    fn a_jump_of_several_levels_nests_one_deeper() {
        let (_, causes) = outline("ishikawa\nP\nA\n        deep\n    mid\n");
        assert_eq!(causes, vec!["A(deep,mid)"]);
    }

    #[test]
    fn an_empty_diagram_is_an_error() {
        let error = parse("ishikawa\n").unwrap_err();
        assert_eq!(error.line, 1);
        assert!(parse("ishikawa x\nP\n").is_err());
    }
}
