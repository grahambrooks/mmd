//! C4 parsing.
//!
//! Grammar subset per `DESIGN.md`, checked against mermaid's
//! `packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison`.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Person,
    PersonExt,
    System,
    SystemExt,
    SystemDb,
}

impl ElementKind {
    fn from_keyword(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "Person" => ElementKind::Person,
            "Person_Ext" => ElementKind::PersonExt,
            "System" => ElementKind::System,
            "System_Ext" => ElementKind::SystemExt,
            "SystemDb" => ElementKind::SystemDb,
            _ => return None,
        })
    }

    /// The «stereotype» shown above the label.
    pub fn stereotype(self) -> &'static str {
        match self {
            ElementKind::Person => "Person",
            ElementKind::PersonExt => "External Person",
            ElementKind::System => "System",
            ElementKind::SystemExt => "External System",
            ElementKind::SystemDb => "Database",
        }
    }

    pub fn is_external(self) -> bool {
        matches!(self, ElementKind::PersonExt | ElementKind::SystemExt)
    }

    pub fn is_person(self) -> bool {
        matches!(self, ElementKind::Person | ElementKind::PersonExt)
    }
}

#[derive(Debug, Clone)]
pub struct Element {
    pub alias: String,
    pub label: String,
    pub description: Option<String>,
    pub kind: ElementKind,
}

#[derive(Debug, Clone)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub technology: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct C4 {
    pub title: Option<String>,
    pub elements: Vec<Element>,
    pub relations: Vec<Relation>,
}

/// Names accepted by mermaid but outside this crate's subset, reported by name
/// so the error says what to do rather than just "unexpected".
const UNSUPPORTED: &[&str] = &[
    "Container",
    "ContainerDb",
    "ContainerQueue",
    "Container_Ext",
    "Component",
    "ComponentDb",
    "Node",
    "SystemQueue",
    "SystemQueue_Ext",
    "SystemDb_Ext",
    "BiRel",
    "Rel_U",
    "Rel_D",
    "Rel_L",
    "Rel_R",
    "Rel_Up",
    "Rel_Down",
    "Rel_Back",
    "Boundary",
    "System_Boundary",
    "Container_Boundary",
    "Enterprise_Boundary",
    "UpdateElementStyle",
    "UpdateLayoutConfig",
    "UpdateRelStyle",
];

/// Parse a C4Context source.
pub fn parse(source: &str) -> Result<C4, Error> {
    let mut model = C4::default();
    let mut seen_header = false;
    // Relations are resolved after parsing, so forward references work.
    let mut pending: Vec<(usize, String, String, String, Option<String>)> = Vec::new();

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if !seen_header {
            if line == "C4Context" {
                seen_header = true;
                continue;
            }
            if line.starts_with("C4") {
                return Err(Error::new(
                    line_number,
                    format!("`{line}` is not supported; only `C4Context` (see DESIGN.md)"),
                ));
            }
            return Err(Error::new(line_number, "expected `C4Context`"));
        }

        if let Some(title) = line.strip_prefix("title ") {
            model.title = Some(title.trim().to_string());
            continue;
        }

        let Some((keyword, args)) = split_call(line) else {
            return Err(Error::new(
                line_number,
                format!("expected `Name(...)`, got `{line}`"),
            ));
        };

        if UNSUPPORTED.contains(&keyword) {
            return Err(Error::new(
                line_number,
                format!("`{keyword}` is not supported (see DESIGN.md)"),
            ));
        }

        let args = split_args(args);
        if keyword == "Rel" {
            if args.len() < 3 {
                return Err(Error::new(
                    line_number,
                    "`Rel` needs at least (from, to, \"label\")",
                ));
            }
            pending.push((
                line_number,
                args[0].clone(),
                args[1].clone(),
                args[2].clone(),
                args.get(3).cloned().filter(|tech| !tech.is_empty()),
            ));
            continue;
        }

        let Some(kind) = ElementKind::from_keyword(keyword) else {
            return Err(Error::new(
                line_number,
                format!("unknown element `{keyword}`"),
            ));
        };
        if args.len() < 2 {
            return Err(Error::new(
                line_number,
                format!("`{keyword}` needs (alias, \"label\") at least"),
            ));
        }
        if model
            .elements
            .iter()
            .any(|element| element.alias == args[0])
        {
            return Err(Error::new(
                line_number,
                format!("`{}` is already defined", args[0]),
            ));
        }
        model.elements.push(Element {
            alias: args[0].clone(),
            label: args[1].clone(),
            description: args.get(2).cloned().filter(|text| !text.is_empty()),
            kind,
        });
    }

    if !seen_header {
        return Err(Error::new(1, "expected `C4Context`"));
    }

    for (line, from, to, label, technology) in pending {
        let resolve = |alias: &str| {
            model
                .elements
                .iter()
                .position(|element| element.alias == alias)
        };
        let from_index =
            resolve(&from).ok_or_else(|| Error::new(line, format!("unknown element `{from}`")))?;
        let to_index =
            resolve(&to).ok_or_else(|| Error::new(line, format!("unknown element `{to}`")))?;
        model.relations.push(Relation {
            from: from_index,
            to: to_index,
            label,
            technology,
        });
    }

    Ok(model)
}

/// Split `Name(args)` into its parts.
fn split_call(line: &str) -> Option<(&str, &str)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }
    Some((line[..open].trim(), &line[open + 1..close]))
}

/// Split on commas outside quotes, unquoting each argument.
fn split_args(input: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in input.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => args.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    args.push(current);
    args.into_iter().map(|arg| arg.trim().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "C4Context\n\
        title Sample\n\
        Person(user, \"User\", \"a person\")\n\
        System(app, \"App\")\n\
        Rel(user, app, \"uses\", \"HTTPS\")\n";

    #[test]
    fn parses_elements_relations_and_title() {
        let model = parse(SAMPLE).unwrap();
        assert_eq!(model.title.as_deref(), Some("Sample"));
        assert_eq!(model.elements.len(), 2);
        assert_eq!(model.elements[0].description.as_deref(), Some("a person"));
        assert_eq!(model.elements[1].description, None);
        assert_eq!(model.relations.len(), 1);
        assert_eq!(model.relations[0].technology.as_deref(), Some("HTTPS"));
    }

    #[test]
    fn relations_may_reference_elements_declared_later() {
        let source = "C4Context\nRel(a, b, \"x\")\nSystem(a, \"A\")\nSystem(b, \"B\")\n";
        let model = parse(source).unwrap();
        assert_eq!(model.relations[0].from, 0);
        assert_eq!(model.relations[0].to, 1);
    }

    #[test]
    fn arguments_may_be_unquoted() {
        let model = parse("C4Context\nSystem(app, App)\n").unwrap();
        assert_eq!(model.elements[0].label, "App");
    }

    #[test]
    fn commas_inside_quotes_are_not_separators() {
        let model = parse("C4Context\nSystem(app, \"One, two\", \"three, four\")\n").unwrap();
        assert_eq!(model.elements[0].label, "One, two");
        assert_eq!(
            model.elements[0].description.as_deref(),
            Some("three, four")
        );
    }

    #[test]
    fn out_of_subset_constructs_name_themselves() {
        let error = parse("C4Context\nSystem_Boundary(b, \"B\")\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("System_Boundary"), "{error:?}");
        assert!(error.message.contains("DESIGN.md"));

        let error = parse("C4Container\n").unwrap_err();
        assert!(error.message.contains("C4Context"), "{error:?}");
    }

    #[test]
    fn unknown_element_references_are_rejected() {
        let error = parse("C4Context\nSystem(a, \"A\")\nRel(a, ghost, \"x\")\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("ghost"));
    }

    #[test]
    fn duplicate_aliases_are_rejected() {
        let error = parse("C4Context\nSystem(a, \"A\")\nSystem(a, \"B\")\n").unwrap_err();
        assert!(error.message.contains("already defined"));
    }
}
