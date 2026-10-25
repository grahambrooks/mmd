//! Requirement diagram syntax: `requirementDiagram` with the six
//! requirement kinds (`id`, `text`, `risk`, `verifyMethod`), `element`
//! blocks (`type`, `docRef`), the seven relationships in either arrow
//! direction, `direction` and `classDef`/`class`/`:::`/`style`.
//!
//! Grammar checked against mermaid's
//! `packages/mermaid/src/diagrams/requirement/parser/requirementDiagram.jison`.
//! Keywords are case-insensitive, as there.

pub use crate::graph::Direction;
pub use crate::style::Colors;
use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementKind {
    Requirement,
    Functional,
    Interface,
    Performance,
    Physical,
    DesignConstraint,
}

impl RequirementKind {
    const ALL: [(&'static str, RequirementKind); 6] = [
        ("requirement", RequirementKind::Requirement),
        ("functionalrequirement", RequirementKind::Functional),
        ("interfacerequirement", RequirementKind::Interface),
        ("performancerequirement", RequirementKind::Performance),
        ("physicalrequirement", RequirementKind::Physical),
        ("designconstraint", RequirementKind::DesignConstraint),
    ];

    /// As mermaid shows it in the box's `<<…>>` line.
    pub fn title(self) -> &'static str {
        match self {
            RequirementKind::Requirement => "Requirement",
            RequirementKind::Functional => "Functional Requirement",
            RequirementKind::Interface => "Interface Requirement",
            RequirementKind::Performance => "Performance Requirement",
            RequirementKind::Physical => "Physical Requirement",
            RequirementKind::DesignConstraint => "Design Constraint",
        }
    }
}

/// What a box holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Requirement {
        kind: RequirementKind,
        id: Option<String>,
        text: Option<String>,
        /// `Low`, `Medium` or `High`.
        risk: Option<&'static str>,
        /// `Analysis`, `Demonstration`, `Inspection` or `Test`.
        verify: Option<&'static str>,
    },
    Element {
        kind: Option<String>,
        docref: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct Block {
    pub name: String,
    pub body: Body,
    pub classes: Vec<String>,
    pub style: Colors,
}

impl Block {
    /// The box's `<<…>>` line.
    pub fn title(&self) -> &'static str {
        match &self.body {
            Body::Requirement { kind, .. } => kind.title(),
            Body::Element { .. } => "Element",
        }
    }

    /// The box's `Key: value` rows, in mermaid's order.
    pub fn rows(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut row = |key: &str, value: Option<&str>| {
            if let Some(v) = value {
                out.push(format!("{key}: {v}"));
            }
        };
        match &self.body {
            Body::Requirement {
                id,
                text,
                risk,
                verify,
                ..
            } => {
                row("ID", id.as_deref());
                row("Text", text.as_deref());
                row("Risk", *risk);
                row("Verification", *verify);
            }
            Body::Element { kind, docref } => {
                row("Type", kind.as_deref());
                row("Doc Ref", docref.as_deref());
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationKind {
    Contains,
    Copies,
    Derives,
    Satisfies,
    Verifies,
    Refines,
    Traces,
}

impl RelationKind {
    const ALL: [(&'static str, RelationKind); 7] = [
        ("contains", RelationKind::Contains),
        ("copies", RelationKind::Copies),
        ("derives", RelationKind::Derives),
        ("satisfies", RelationKind::Satisfies),
        ("verifies", RelationKind::Verifies),
        ("refines", RelationKind::Refines),
        ("traces", RelationKind::Traces),
    ];

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, k)| *k == self)
            .map(|(n, _)| *n)
            .unwrap_or("traces")
    }
}

#[derive(Debug, Clone)]
pub struct Relation {
    /// The source and destination, as indexes into the blocks.
    pub from: usize,
    pub to: usize,
    pub kind: RelationKind,
}

#[derive(Debug, Clone)]
pub struct RequirementDiagram {
    pub direction: Direction,
    pub blocks: Vec<Block>,
    pub relations: Vec<Relation>,
    pub styles: Vec<(String, Colors)>,
    /// Stored, not drawn.
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

impl RequirementDiagram {
    /// A block's colours: the `default` class, its classes, then `style`.
    pub fn colors(&self, block: &Block) -> Colors {
        let mut out = Colors::default();
        let style = |name: &str| self.styles.iter().find(|(n, _)| n == name).map(|(_, c)| *c);
        if let Some(c) = style("default") {
            out.merge(&c);
        }
        for name in &block.classes {
            if let Some(c) = style(name) {
                out.merge(&c);
            }
        }
        out.merge(&block.style);
        out
    }

    /// The block called `name`, made a bare element if new.
    fn block(&mut self, name: &str) -> usize {
        if let Some(i) = self.blocks.iter().position(|b| b.name == name) {
            return i;
        }
        self.blocks.push(Block {
            name: name.to_string(),
            body: Body::Element {
                kind: None,
                docref: None,
            },
            classes: Vec::new(),
            style: Colors::default(),
        });
        self.blocks.len() - 1
    }
}

pub fn parse(source: &str) -> Result<RequirementDiagram, Error> {
    let mut d = RequirementDiagram {
        direction: Direction::TopDown,
        blocks: Vec::new(),
        relations: Vec::new(),
        styles: Vec::new(),
        acc_title: None,
        acc_descr: None,
    };
    let mut header = false;
    // The block whose `{ … }` is open, and the line it opened on.
    let mut open: Option<(usize, usize)> = None;
    let mut descr: Option<(String, usize)> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        if let Some((text, _)) = descr.as_mut() {
            match raw.find('}') {
                Some(end) => {
                    text.push_str(&raw[..end]);
                    d.acc_descr = Some(text.trim().to_string());
                    descr = None;
                }
                None => {
                    text.push_str(raw);
                    text.push('\n');
                }
            }
            continue;
        }
        let line = strip_comment(raw).trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !header {
            if line != "requirementDiagram" && line != "requirement" {
                return Err(Error::new(number, "expected `requirementDiagram`"));
            }
            header = true;
            continue;
        }
        if let Some((block, _)) = open {
            if line == "}" {
                open = None;
            } else {
                body_line(&mut d.blocks[block], line, number)?;
            }
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or("");
        let rest = line[keyword.len()..].trim();
        let lower = keyword.to_ascii_lowercase();
        if let Some(&(_, kind)) = RequirementKind::ALL.iter().find(|(k, _)| *k == lower) {
            let body = Body::Requirement {
                kind,
                id: None,
                text: None,
                risk: None,
                verify: None,
            };
            open = Some((block_statement(&mut d, rest, body, number)?, number));
            continue;
        }
        match lower.as_str() {
            "element" => {
                let body = Body::Element {
                    kind: None,
                    docref: None,
                };
                open = Some((block_statement(&mut d, rest, body, number)?, number));
                continue;
            }
            "direction" => {
                d.direction = Direction::from_word(rest).ok_or_else(|| {
                    Error::new(
                        number,
                        format!("unknown direction `{rest}`: expected TB, BT, LR or RL"),
                    )
                })?;
                continue;
            }
            "classdef" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let names = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""), number)?;
                for name in names.split(',') {
                    d.styles.push((name.trim().to_string(), colors));
                }
                continue;
            }
            "class" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let ids = words.next().unwrap_or("");
                let classes = words.next().unwrap_or("").trim();
                if classes.is_empty() {
                    return Err(Error::new(number, "`class` needs ids and a class name"));
                }
                for id in ids.split(',') {
                    let b = d.block(&unquote(id.trim()));
                    for class in classes.split(',') {
                        d.blocks[b].classes.push(class.trim().to_string());
                    }
                }
                continue;
            }
            "style" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let ids = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""), number)?;
                for id in ids.split(',') {
                    let b = d.block(&unquote(id.trim()));
                    d.blocks[b].style.merge(&colors);
                }
                continue;
            }
            "title" => {
                return Err(Error::new(
                    number,
                    "`title` is not supported in requirementDiagram (see DESIGN.md)",
                ));
            }
            _ => {}
        }
        if let Some(value) = acc(line, "accTitle") {
            d.acc_title = Some(value.to_string());
            continue;
        }
        if let Some(value) = acc(line, "accDescr") {
            d.acc_descr = Some(value.to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("accDescr")
            && let Some(body) = rest.trim_start().strip_prefix('{')
        {
            match body.find('}') {
                Some(end) => d.acc_descr = Some(body[..end].trim().to_string()),
                None => descr = Some((format!("{body}\n"), number)),
            }
            continue;
        }
        if let Some((id, classes)) = line.split_once(":::") {
            let b = d.block(&unquote(id.trim()));
            for class in classes.split(',') {
                d.blocks[b].classes.push(class.trim().to_string());
            }
            continue;
        }
        relation(&mut d, line, number)?;
    }
    if !header {
        return Err(Error::new(1, "expected `requirementDiagram`"));
    }
    if let Some((_, line)) = open {
        return Err(Error::new(line, "block is never closed with `}`"));
    }
    if let Some((_, line)) = descr {
        return Err(Error::new(line, "`accDescr {` is never closed with `}`"));
    }
    Ok(d)
}

/// `name [:::class] {` after a block keyword: the block, made or updated.
fn block_statement(
    d: &mut RequirementDiagram,
    rest: &str,
    body: Body,
    number: usize,
) -> Result<usize, Error> {
    let head = rest
        .strip_suffix('{')
        .ok_or_else(|| Error::new(number, "expected `{` after the name"))?
        .trim();
    let (name, classes) = match head.split_once(":::") {
        Some((name, classes)) => (name.trim(), Some(classes.trim())),
        None => (head, None),
    };
    let name = unquote(name);
    if name.is_empty() {
        return Err(Error::new(number, "expected a name"));
    }
    let b = d.block(&name);
    d.blocks[b].body = body;
    if let Some(classes) = classes {
        for class in classes.split(',') {
            d.blocks[b].classes.push(class.trim().to_string());
        }
    }
    Ok(b)
}

/// `key: value` inside a block.
fn body_line(block: &mut Block, line: &str, number: usize) -> Result<(), Error> {
    let (key, value) = line
        .split_once(':')
        .ok_or_else(|| Error::new(number, format!("expected `key: value`, found `{line}`")))?;
    let key = key.trim().to_ascii_lowercase();
    let value = unquote(value.trim());
    let pick = |choices: &[&'static str], what: &str| -> Result<&'static str, Error> {
        choices
            .iter()
            .find(|c| c.eq_ignore_ascii_case(&value))
            .copied()
            .ok_or_else(|| {
                Error::new(
                    number,
                    format!("unknown {what} `{value}`: expected {}", choices.join(", ")),
                )
            })
    };
    match (&mut block.body, key.as_str()) {
        (Body::Requirement { id, .. }, "id") => *id = Some(value),
        (Body::Requirement { text, .. }, "text") => *text = Some(value),
        (Body::Requirement { risk, .. }, "risk") => {
            *risk = Some(pick(&["Low", "Medium", "High"], "risk")?);
        }
        (Body::Requirement { verify, .. }, "verifymethod") => {
            *verify = Some(pick(
                &["Analysis", "Demonstration", "Inspection", "Test"],
                "verifyMethod",
            )?);
        }
        (Body::Element { kind, .. }, "type") => *kind = Some(value),
        (Body::Element { docref, .. }, "docref") => *docref = Some(value),
        (Body::Requirement { .. }, _) => {
            return Err(Error::new(
                number,
                format!(
                    "`{key}` is not a requirement field: expected id, text, risk or verifyMethod"
                ),
            ));
        }
        (Body::Element { .. }, _) => {
            return Err(Error::new(
                number,
                format!("`{key}` is not an element field: expected type or docRef"),
            ));
        }
    }
    Ok(())
}

/// `a - satisfies -> b` or `b <- satisfies - a`.
fn relation(d: &mut RequirementDiagram, line: &str, number: usize) -> Result<(), Error> {
    let unrecognised = || Error::new(number, format!("unrecognised statement `{line}`"));
    let (src, kind, dst) = if let Some((left, right)) = split_outside(line, "<-") {
        let (kind, src) = split_outside(right, "-").ok_or_else(unrecognised)?;
        (src, kind, left)
    } else if let Some((left, right)) = split_outside(line, "->") {
        let (src, kind) = split_outside(left, "-").ok_or_else(unrecognised)?;
        (src, kind, right)
    } else {
        return Err(unrecognised());
    };
    let kind_name = kind.trim().to_ascii_lowercase();
    let kind = RelationKind::ALL
        .iter()
        .find(|(n, _)| *n == kind_name)
        .map(|&(_, k)| k)
        .ok_or_else(|| {
            Error::new(
                number,
                format!(
                    "unknown relationship `{}`: expected contains, copies, derives, satisfies, \
                     verifies, refines or traces",
                    kind.trim()
                ),
            )
        })?;
    let (src, dst) = (unquote(src.trim()), unquote(dst.trim()));
    if src.is_empty() || dst.is_empty() {
        return Err(unrecognised());
    }
    let from = d.block(&src);
    let to = d.block(&dst);
    d.relations.push(Relation { from, to, kind });
    Ok(())
}

/// `text` split at the first `token` outside quotes.
fn split_outside<'a>(text: &'a str, token: &str) -> Option<(&'a str, &'a str)> {
    let mut quoted = false;
    for (i, c) in text.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted && text[i..].starts_with(token) {
            return Some((&text[..i], &text[i + token.len()..]));
        }
    }
    None
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
        .to_string()
}

fn acc<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?.trim_start();
    Some(rest.strip_prefix(':')?.trim())
}

fn css(text: &str, number: usize) -> Result<Colors, Error> {
    crate::style::colors(&crate::style::declarations(text, number)?, number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_relations_and_styles() {
        let d = parse(concat!(
            "requirementDiagram\n",
            "  requirement test_req {\n",
            "    id: 1\n",
            "    text: the test text.\n",
            "    risk: high\n",
            "    verifymethod: test\n",
            "  }\n",
            "  functionalRequirement \"quoted name\" {\n",
            "    id: 1.1\n",
            "    text: \"quoted: text\"\n",
            "    risk: Low\n",
            "    verifyMethod: Inspection\n",
            "  }\n",
            "  interfaceRequirement i {\n  }\n",
            "  performanceRequirement p {\n  }\n",
            "  physicalRequirement ph {\n  }\n",
            "  designConstraint dc:::hot {\n  }\n",
            "  element test_entity {\n",
            "    type: simulation\n",
            "    docRef: reqs/test_entity\n",
            "  }\n",
            "  test_entity - satisfies -> test_req\n",
            "  test_req <- contains - \"quoted name\"\n",
            "  \"quoted name\" - traces -> \"with-dash\"\n",
            "  i - copies -> p\n",
            "  i - derives -> p\n",
            "  i - verifies -> p\n",
            "  i - refines -> p\n",
            "  i - traces -> ghost\n",
            "  classDef hot fill:#f00\n",
            "  class i,p hot\n",
            "  ph:::hot\n",
            "  style test_req stroke:#00f\n",
            "  direction LR\n",
        ))
        .unwrap();
        assert_eq!(
            d.blocks[0].rows(),
            [
                "ID: 1",
                "Text: the test text.",
                "Risk: High",
                "Verification: Test"
            ]
        );
        assert_eq!(d.blocks[1].title(), "Functional Requirement");
        assert_eq!(d.blocks[1].rows()[1], "Text: quoted: text");
        assert_eq!(d.blocks[5].title(), "Design Constraint");
        assert_eq!(
            d.blocks[6].rows(),
            ["Type: simulation", "Doc Ref: reqs/test_entity"]
        );
        let contains = &d.relations[1];
        assert_eq!(contains.kind, RelationKind::Contains);
        assert_eq!(d.blocks[contains.from].name, "quoted name");
        assert_eq!(d.blocks[contains.to].name, "test_req");
        assert_eq!(d.relations.len(), 8);
        assert!(d.blocks.iter().any(|b| b.name == "with-dash"));
        assert!(d.blocks.iter().any(|b| b.name == "ghost"));
        for name in ["i", "p", "ph", "dc"] {
            let b = d.blocks.iter().find(|b| b.name == name).unwrap();
            assert_eq!(d.colors(b).fill, Some((255, 0, 0)), "{name}");
        }
        assert_eq!(d.colors(&d.blocks[0]).stroke, Some((0, 0, 255)));
        assert_eq!(d.direction, Direction::LeftRight);
    }

    #[test]
    fn malformed_input_is_an_error_with_a_line() {
        for (source, line, needle) in [
            (
                "requirementDiagram\n  requirement r {\n  id: 1\n",
                2,
                "never closed",
            ),
            (
                "requirementDiagram\n  requirement r {\n  risk: huge\n}\n",
                3,
                "unknown risk",
            ),
            (
                "requirementDiagram\n  requirement r {\n  type: x\n}\n",
                3,
                "not a requirement field",
            ),
            (
                "requirementDiagram\n  a - blesses -> b\n",
                2,
                "unknown relationship",
            ),
            ("requirementDiagram\n  a b c\n", 2, "unrecognised"),
            ("requirementDiagram\n  title Hi\n", 2, "title"),
            ("requirementDiagram\n  requirement r\n", 2, "`{`"),
            ("requirementDiagram\n  style a fill:blurple\n", 2, "colour"),
            ("graph\n", 1, "requirementDiagram"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
