//! `venn-beta` parsing, per mermaid's
//! `packages/mermaid/src/diagrams/venn/parser/venn.jison` and `vennDB.ts`.
//!
//! ```text
//! venn-beta
//!     title Skills
//!     set A["Frontend"]:20
//!         text A1["React"]
//!     set B["Backend"]
//!     union A,B["APIs"]:3
//!     text A,B note["shared"]
//!     style A fill:#ff6b6b, color:#333
//! ```

use crate::style::{self, Colors};
use crate::{Error, strip_comment};

/// The most sets mmd draws: regions are tracked as bit masks.
pub const MAX_SETS: usize = 64;

#[derive(Debug, Clone, Default)]
pub struct Venn {
    pub title: Option<String>,
    pub sets: Vec<Set>,
    pub unions: Vec<Union>,
    pub texts: Vec<TextNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Set {
    pub id: String,
    pub label: Option<String>,
    /// `:20`. Mermaid defaults a set to 10.
    pub size: Option<f32>,
    pub colors: Colors,
    /// The source line declaring it.
    pub line: usize,
}

impl Set {
    pub fn name(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.id)
    }

    pub fn size(&self) -> f32 {
        self.size.unwrap_or(10.0)
    }
}

/// An intersection, `union A,B`.
#[derive(Debug, Clone, PartialEq)]
pub struct Union {
    /// Indices into [`Venn::sets`], ascending.
    pub sets: Vec<usize>,
    pub label: Option<String>,
    pub size: Option<f32>,
    pub colors: Colors,
    /// The source line declaring (or first styling) it.
    pub line: usize,
}

/// A `text` item placed in the region of its sets.
#[derive(Debug, Clone, PartialEq)]
pub struct TextNode {
    /// Indices into [`Venn::sets`], ascending.
    pub sets: Vec<usize>,
    pub id: String,
    pub label: Option<String>,
    pub colors: Colors,
    /// The source line declaring it.
    pub line: usize,
}

impl TextNode {
    pub fn name(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.id)
    }
}

impl Venn {
    /// The union declared for exactly these sets.
    pub fn union(&self, sets: &[usize]) -> Option<&Union> {
        self.unions.iter().find(|u| u.sets == sets)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Str(String),
    Bracket(String),
    Number(f32),
    Comma,
    Colon,
}

pub fn parse(source: &str) -> Result<Venn, Error> {
    let mut venn = Venn::default();
    let mut seen_header = false;
    // The sets of the last `set`/`union`, which indented `text` joins.
    let mut current: Option<Vec<usize>> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let stripped = strip_comment(raw);
        let line = stripped.trim();
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            if !line.eq_ignore_ascii_case("venn-beta") {
                return Err(Error::new(number, "expected `venn-beta`"));
            }
            seen_header = true;
            continue;
        }
        let indented = stripped.starts_with([' ', '\t']);
        let (word, rest) = match line.find([' ', '\t']) {
            Some(at) => (&line[..at], line[at..].trim()),
            None => (line, ""),
        };
        let lower = word.to_ascii_lowercase();
        if !indented {
            // A line at the margin ends the indented `text` block.
            if lower != "set" && lower != "union" {
                current = None;
            }
        }
        match lower.as_str() {
            "title" => venn.title = Some(rest.to_string()),
            "set" => {
                let tokens = lex(rest, number)?;
                let (ids, after) = identifiers(&tokens, number)?;
                if ids.len() != 1 {
                    return Err(Error::new(
                        number,
                        "`set` takes one identifier; use `union` for an intersection",
                    ));
                }
                let (label, size) = label_and_size(&tokens[after..], number)?;
                let id = ids.into_iter().next().unwrap_or_default();
                if venn.sets.iter().any(|s| s.id == id) {
                    return Err(Error::new(
                        number,
                        format!("set `{id}` is already declared"),
                    ));
                }
                if venn.sets.len() == MAX_SETS {
                    return Err(Error::new(
                        number,
                        format!(
                            "a venn diagram of more than {MAX_SETS} sets is not supported \
                             (see DESIGN.md)"
                        ),
                    ));
                }
                venn.sets.push(Set {
                    id,
                    label,
                    size,
                    colors: Colors::default(),
                    line: number,
                });
                current = Some(vec![venn.sets.len() - 1]);
            }
            "union" => {
                let tokens = lex(rest, number)?;
                let (ids, after) = identifiers(&tokens, number)?;
                if ids.len() < 2 {
                    return Err(Error::new(number, "union requires multiple identifiers"));
                }
                let sets = resolve(&venn, &ids, number)?;
                let (label, size) = label_and_size(&tokens[after..], number)?;
                if venn.union(&sets).is_some() {
                    return Err(Error::new(
                        number,
                        format!("union `{}` is already declared", ids.join(",")),
                    ));
                }
                venn.unions.push(Union {
                    sets: sets.clone(),
                    label,
                    size,
                    colors: Colors::default(),
                    line: number,
                });
                current = Some(sets);
            }
            "text" => {
                let tokens = lex(rest, number)?;
                let (sets, tail) = match (&current, indented) {
                    (Some(sets), true) => (sets.clone(), &tokens[..]),
                    _ => {
                        let (ids, after) = identifiers(&tokens, number)?;
                        (resolve(&venn, &ids, number)?, &tokens[after..])
                    }
                };
                let (id, label) = match tail {
                    [Token::Ident(id) | Token::Str(id)] => (id.clone(), None),
                    [Token::Number(n)] => (format_number(*n), None),
                    [Token::Ident(id) | Token::Str(id), Token::Bracket(label)] => {
                        (id.clone(), Some(label.clone()))
                    }
                    _ => {
                        return Err(Error::new(
                            number,
                            "expected `text <sets> <id>` or `text <id>[\"label\"]`",
                        ));
                    }
                };
                venn.texts.push(TextNode {
                    sets,
                    id,
                    label,
                    colors: Colors::default(),
                    line: number,
                });
            }
            "style" => style_statement(&mut venn, rest, number)?,
            _ => {
                return Err(Error::new(
                    number,
                    format!("`{word}` is not supported in a venn diagram (see DESIGN.md)"),
                ));
            }
        }
    }
    if !seen_header {
        return Err(Error::new(1, "expected `venn-beta`"));
    }
    if venn.sets.is_empty() {
        return Err(Error::new(1, "a venn diagram needs at least one `set`"));
    }
    Ok(venn)
}

fn format_number(n: f32) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Set indices for `ids`, ascending; every one must be declared.
fn resolve(venn: &Venn, ids: &[String], line: usize) -> Result<Vec<usize>, Error> {
    let mut out = Vec::new();
    for id in ids {
        let index = venn
            .sets
            .iter()
            .position(|s| &s.id == id)
            .ok_or_else(|| Error::new(line, format!("unknown set identifier `{id}`")))?;
        if !out.contains(&index) {
            out.push(index);
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// A comma-separated identifier list at the start, and where it ends.
fn identifiers(tokens: &[Token], line: usize) -> Result<(Vec<String>, usize), Error> {
    let mut ids = Vec::new();
    let mut at = 0;
    loop {
        match tokens.get(at) {
            Some(Token::Ident(id) | Token::Str(id)) => ids.push(id.clone()),
            _ => return Err(Error::new(line, "expected a set identifier")),
        }
        at += 1;
        if tokens.get(at) == Some(&Token::Comma) {
            at += 1;
        } else {
            return Ok((ids, at));
        }
    }
}

/// `["label"]` and `:size`, both optional, in that order.
fn label_and_size(tokens: &[Token], line: usize) -> Result<(Option<String>, Option<f32>), Error> {
    match tokens {
        [] => Ok((None, None)),
        [Token::Bracket(label)] => Ok((Some(label.clone()), None)),
        [Token::Colon, Token::Number(n)] => Ok((None, Some(*n))),
        [Token::Bracket(label), Token::Colon, Token::Number(n)] => {
            Ok((Some(label.clone()), Some(*n)))
        }
        _ => Err(Error::new(
            line,
            "expected an optional `[\"label\"]` and `:size` after the identifiers",
        )),
    }
}

fn style_statement(venn: &mut Venn, rest: &str, line: usize) -> Result<(), Error> {
    // The identifier list runs until the first gap not followed by a comma.
    let mut ids = Vec::new();
    let mut chars = rest;
    loop {
        chars = chars.trim_start();
        let (id, after) = if let Some(quoted) = chars.strip_prefix('"') {
            let end = quoted
                .find('"')
                .ok_or_else(|| Error::new(line, "unterminated string"))?;
            (quoted[..end].to_string(), &quoted[end + 1..])
        } else {
            let end = chars
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(chars.len());
            if end == 0 {
                return Err(Error::new(
                    line,
                    "expected `style <targets> <property>:<value>`",
                ));
            }
            (chars[..end].to_string(), &chars[end..])
        };
        ids.push(id);
        let trimmed = after.trim_start();
        if let Some(next) = trimmed.strip_prefix(',') {
            chars = next;
        } else {
            chars = trimmed;
            break;
        }
    }
    let declarations = style::declarations(chars, line)?;
    if declarations.is_empty() {
        return Err(Error::new(
            line,
            "expected `style <targets> <property>:<value>`",
        ));
    }
    let colors = style::colors(&declarations, line)?;
    // A single id may be a set or a text node; several are an intersection.
    if ids.len() == 1 {
        let id = &ids[0];
        if let Some(set) = venn.sets.iter_mut().find(|s| &s.id == id) {
            set.colors.merge(&colors);
            return Ok(());
        }
        if let Some(text) = venn.texts.iter_mut().find(|t| &t.id == id) {
            text.colors.merge(&colors);
            return Ok(());
        }
        return Err(Error::new(line, format!("unknown style target `{id}`")));
    }
    let sets = resolve(venn, &ids, line)?;
    if venn.union(&sets).is_none() {
        // Styling an undeclared intersection still names a region.
        venn.unions.push(Union {
            sets: sets.clone(),
            label: None,
            size: None,
            colors: Colors::default(),
            line,
        });
    }
    if let Some(union) = venn.unions.iter_mut().find(|u| u.sets == sets) {
        union.colors.merge(&colors);
    }
    Ok(())
}

fn lex(text: &str, line: usize) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            ' ' | '\t' => i += 1,
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            ':' => {
                tokens.push(Token::Colon);
                i += 1;
            }
            '"' => {
                let end = bytes[i + 1..]
                    .iter()
                    .position(|&c| c == '"')
                    .ok_or_else(|| Error::new(line, "unterminated string"))?;
                tokens.push(Token::Str(bytes[i + 1..i + 1 + end].iter().collect()));
                i += end + 2;
            }
            '[' => {
                let end = bytes[i + 1..]
                    .iter()
                    .position(|&c| c == ']')
                    .ok_or_else(|| Error::new(line, "unterminated `[label]`"))?;
                let inner: String = bytes[i + 1..i + 1 + end].iter().collect();
                let inner = inner.trim();
                let inner = inner
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .unwrap_or(inner);
                tokens.push(Token::Bracket(inner.to_string()));
                i += end + 2;
            }
            c if c.is_ascii_digit() || c == '.' || c == '+' || c == '-' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
                    i += 1;
                }
                let s: String = bytes[start..i].iter().collect();
                let n = s
                    .parse::<f32>()
                    .map_err(|_| Error::new(line, format!("`{s}` is not a number")))?;
                tokens.push(Token::Number(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || bytes[i] == '_' || bytes[i] == '-')
                {
                    i += 1;
                }
                tokens.push(Token::Ident(bytes[start..i].iter().collect()));
            }
            other => {
                return Err(Error::new(
                    line,
                    format!("unexpected `{other}` in a venn diagram"),
                ));
            }
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sets_unions_texts_and_styles() {
        let venn = parse(
            "venn-beta\n  title Skills\n  set A[\"Frontend\"]:20\n    text A1[\"React\"]\n  \
             set B[Backend]\n  union A,B[\"APIs\"]:3\n    text AB1\ntext B b2\n  \
             style A fill:#ff6b6b\n  style A,B color:#333\n  style A1 color:red\n",
        )
        .unwrap();
        assert_eq!(venn.title.as_deref(), Some("Skills"));
        assert_eq!(venn.sets[0].name(), "Frontend");
        assert_eq!(venn.sets[0].size, Some(20.0));
        assert_eq!(venn.sets[1].name(), "Backend");
        assert_eq!(venn.unions[0].sets, vec![0, 1]);
        assert_eq!(venn.unions[0].label.as_deref(), Some("APIs"));
        let texts: Vec<_> = venn
            .texts
            .iter()
            .map(|t| (t.name(), t.sets.clone()))
            .collect();
        assert_eq!(
            texts,
            vec![("React", vec![0]), ("AB1", vec![0, 1]), ("b2", vec![1])]
        );
        assert_eq!(venn.sets[0].colors.fill, Some((0xff, 0x6b, 0x6b)));
        assert_eq!(venn.unions[0].colors.text, Some((0x33, 0x33, 0x33)));
        assert_eq!(venn.texts[0].colors.text, Some((0xff, 0, 0)));
    }

    #[test]
    fn quoted_identifiers_and_sorted_unions() {
        let venn =
            parse("venn-beta\n set \"Foo Bar\"\n set Buz\n union Buz,\"Foo Bar\"\n").unwrap();
        assert_eq!(venn.sets[0].id, "Foo Bar");
        assert_eq!(venn.unions[0].sets, vec![0, 1]);
    }

    #[test]
    fn errors_name_the_problem_and_its_line() {
        let error = parse("venn-beta\n set A\n union A\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("multiple identifiers"));
        let error = parse("venn-beta\n set Foo\n union Foo,Buz\n").unwrap_err();
        assert!(error.message.contains("unknown set identifier"));
        let many: String = (0..=MAX_SETS).map(|i| format!(" set S{i}\n")).collect();
        let error = parse(&format!("venn-beta\n{many}")).unwrap_err();
        assert_eq!(error.line, MAX_SETS + 2);
        assert!(error.message.contains("more than 64 sets"));
        let error = parse("venn-beta\n set A,B\n").unwrap_err();
        assert_eq!(error.line, 2);
    }
}
