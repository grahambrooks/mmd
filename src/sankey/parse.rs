//! `sankey` source to a model: CSV rows of `source,target,value`.
//!
//! Grammar: `packages/mermaid/src/diagrams/sankey/parser/sankey.jison`
//! (RFC 4180 fields, quoted with `"` and `""` for a quote inside).

use crate::Error;
use crate::chart;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Sankey {
    /// Node names, in order of first appearance.
    pub nodes: Vec<String>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub source: usize,
    pub target: usize,
    pub value: f64,
    pub line: usize,
}

/// Parse a `sankey` (or `sankey-beta`) source.
pub fn parse(source: &str) -> Result<Sankey, Error> {
    let (header, body) = chart::statements(source, &["sankey", "sankey-beta"])?;
    if !header.text.is_empty() {
        return Err(Error::new(
            header.line,
            format!("unexpected `{}` after `sankey`", header.text),
        ));
    }
    let mut sankey = Sankey::default();
    for s in body {
        let fields = fields(s.text, s.line)?;
        let [source, target, value] = fields.as_slice() else {
            return Err(Error::new(
                s.line,
                format!(
                    "expected three fields `source,target,value`, found {}",
                    fields.len()
                ),
            ));
        };
        let value = chart::number(value)
            .ok_or_else(|| Error::new(s.line, format!("`{value}` is not a number")))?;
        if value < 0.0 {
            return Err(Error::new(
                s.line,
                format!("the flow {source} → {target} is negative ({value})"),
            ));
        }
        let mut node = |name: &str| match sankey.nodes.iter().position(|n| n == name) {
            Some(i) => i,
            None => {
                sankey.nodes.push(name.to_string());
                sankey.nodes.len() - 1
            }
        };
        let (source_i, target_i) = (node(source), node(target));
        if source_i == target_i {
            return Err(Error::new(
                s.line,
                format!("`{source}` flows into itself; sankey flows cannot loop"),
            ));
        }
        sankey.links.push(Link {
            source: source_i,
            target: target_i,
            value,
            line: s.line,
        });
    }
    if sankey.links.is_empty() {
        return Err(Error::new(
            header.line,
            "a sankey needs at least one `source,target,value` row",
        ));
    }
    if let Some(link) = cycle(&sankey) {
        return Err(Error::new(
            link.line,
            format!(
                "the flow {} → {} closes a loop; sankey flows cannot loop",
                sankey.nodes[link.source], sankey.nodes[link.target]
            ),
        ));
    }
    Ok(sankey)
}

/// A link on a cycle, if the flows have one.
fn cycle(s: &Sankey) -> Option<&Link> {
    let mut indegree = vec![0; s.nodes.len()];
    for l in &s.links {
        indegree[l.target] += 1;
    }
    let mut ready: Vec<usize> = (0..s.nodes.len()).filter(|n| indegree[*n] == 0).collect();
    let mut done = vec![false; s.nodes.len()];
    while let Some(n) = ready.pop() {
        done[n] = true;
        for l in s.links.iter().filter(|l| l.source == n) {
            indegree[l.target] -= 1;
            if indegree[l.target] == 0 {
                ready.push(l.target);
            }
        }
    }
    s.links
        .iter()
        .filter(|l| !done[l.source] && !done[l.target])
        .max_by_key(|l| l.line)
}

/// One CSV record's fields, unquoted and trimmed.
fn fields(text: &str, line: usize) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    loop {
        let mut field = String::new();
        while chars.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
            chars.next();
        }
        if chars.peek() == Some(&'"') {
            chars.next();
            loop {
                match chars.next() {
                    Some('"') if chars.peek() == Some(&'"') => {
                        chars.next();
                        field.push('"');
                    }
                    Some('"') => break,
                    Some(c) => field.push(c),
                    None => return Err(Error::new(line, "unterminated quoted field")),
                }
            }
            while chars.peek().is_some_and(|c| *c != ',') {
                let c = chars.next().unwrap_or(' ');
                if !c.is_whitespace() {
                    return Err(Error::new(
                        line,
                        format!("unexpected `{c}` after a quoted field"),
                    ));
                }
            }
        } else {
            while let Some(c) = chars.peek().copied().filter(|c| *c != ',') {
                field.push(c);
                chars.next();
            }
        }
        out.push(field.trim().to_string());
        if chars.next().is_none() {
            return Ok(out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_rows_with_quoted_fields() {
        let s = parse(
            "sankey-beta\n\n%% source,target,value\nAgricultural 'waste',Bio-conversion,124.729\n\
             \"Solar, rooftop\",\"The \"\"grid\"\"\",5\nBio-conversion,Losses,26.862\n",
        )
        .unwrap();
        assert_eq!(
            s.nodes,
            vec![
                "Agricultural 'waste'",
                "Bio-conversion",
                "Solar, rooftop",
                "The \"grid\"",
                "Losses"
            ]
        );
        assert_eq!(s.links.len(), 3);
        assert_eq!(s.links[0].value, 124.729);
        assert_eq!(s.links[2].line, 6);
    }

    #[test]
    fn bad_input_is_an_error_with_its_line() {
        for (source, line, needle) in [
            ("sankey\n  a,b\n", 2, "three fields"),
            ("sankey\n  a,b,c\n", 2, "`c` is not a number"),
            ("sankey\n  a,b,-1\n", 2, "negative"),
            ("sankey\n  a,a,1\n", 2, "itself"),
            ("sankey\n  a,b,1\n  b,c,1\n  c,a,1\n", 4, "loop"),
            ("sankey\n  \"a,b,1\n", 2, "unterminated"),
            ("sankey\n", 1, "at least one"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
