//! C4 parsing.
//!
//! Grammar subset per `DESIGN.md`, checked against mermaid's
//! `packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison` and the
//! argument handling in `c4Db.js`.

use crate::{Error, strip_comment};

/// Which C4 view the header names. The view does not restrict which
/// elements may appear — mermaid does not either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Context,
    Container,
    Component,
}

/// The C4 abstraction an element belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Person,
    System,
    Container,
    Component,
}

/// How an element stores or carries data, which picks its outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Plain,
    /// A cylinder.
    Db,
    /// A horizontal cylinder.
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ElementKind {
    Person,
    PersonExt,
    System,
    SystemExt,
    SystemDb,
    SystemDbExt,
    SystemQueue,
    SystemQueueExt,
    Container,
    ContainerExt,
    ContainerDb,
    ContainerDbExt,
    ContainerQueue,
    ContainerQueueExt,
    Component,
    ComponentExt,
    ComponentDb,
    ComponentDbExt,
    ComponentQueue,
    ComponentQueueExt,
}

impl ElementKind {
    fn from_keyword(keyword: &str) -> Option<Self> {
        use ElementKind::*;
        Some(match keyword {
            "Person" => Person,
            "Person_Ext" => PersonExt,
            "System" => System,
            "System_Ext" => SystemExt,
            "SystemDb" => SystemDb,
            "SystemDb_Ext" => SystemDbExt,
            "SystemQueue" => SystemQueue,
            "SystemQueue_Ext" => SystemQueueExt,
            "Container" => Container,
            "Container_Ext" => ContainerExt,
            "ContainerDb" => ContainerDb,
            "ContainerDb_Ext" => ContainerDbExt,
            "ContainerQueue" => ContainerQueue,
            "ContainerQueue_Ext" => ContainerQueueExt,
            "Component" => Component,
            "Component_Ext" => ComponentExt,
            "ComponentDb" => ComponentDb,
            "ComponentDb_Ext" => ComponentDbExt,
            "ComponentQueue" => ComponentQueue,
            "ComponentQueue_Ext" => ComponentQueueExt,
            _ => return None,
        })
    }

    pub fn level(self) -> Level {
        use ElementKind::*;
        match self {
            Person | PersonExt => Level::Person,
            System | SystemExt | SystemDb | SystemDbExt | SystemQueue | SystemQueueExt => {
                Level::System
            }
            Container | ContainerExt | ContainerDb | ContainerDbExt | ContainerQueue
            | ContainerQueueExt => Level::Container,
            Component | ComponentExt | ComponentDb | ComponentDbExt | ComponentQueue
            | ComponentQueueExt => Level::Component,
        }
    }

    pub fn form(self) -> Form {
        use ElementKind::*;
        match self {
            SystemDb | SystemDbExt | ContainerDb | ContainerDbExt | ComponentDb
            | ComponentDbExt => Form::Db,
            SystemQueue | SystemQueueExt | ContainerQueue | ContainerQueueExt | ComponentQueue
            | ComponentQueueExt => Form::Queue,
            _ => Form::Plain,
        }
    }

    pub fn is_external(self) -> bool {
        use ElementKind::*;
        matches!(
            self,
            PersonExt
                | SystemExt
                | SystemDbExt
                | SystemQueueExt
                | ContainerExt
                | ContainerDbExt
                | ContainerQueueExt
                | ComponentExt
                | ComponentDbExt
                | ComponentQueueExt
        )
    }

    pub fn is_person(self) -> bool {
        self.level() == Level::Person
    }

    /// Containers and components carry a technology between label and
    /// description; people and systems do not.
    pub fn has_technology(self) -> bool {
        matches!(self.level(), Level::Container | Level::Component)
    }

    /// The «stereotype» shown above the label.
    pub fn stereotype(self) -> String {
        let level = match self.level() {
            Level::Person => "Person",
            Level::System => "System",
            Level::Container => "Container",
            Level::Component => "Component",
        };
        let form = match self.form() {
            Form::Plain => "",
            Form::Db => " DB",
            Form::Queue => " Queue",
        };
        let external = if self.is_external() { "External " } else { "" };
        format!("{external}{level}{form}")
    }
}

#[derive(Debug, Clone)]
pub struct Element {
    pub alias: String,
    pub label: String,
    /// Containers and components only; drawn as `[technology]`.
    pub technology: Option<String>,
    pub description: Option<String>,
    pub kind: ElementKind,
    /// The boundary it was declared in, as an index into
    /// [`C4::boundaries`].
    pub boundary: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    /// `Boundary`, whose type is the third argument.
    Generic,
    Enterprise,
    System,
    Container,
}

#[derive(Debug, Clone)]
pub struct Boundary {
    pub alias: String,
    pub label: String,
    pub kind: BoundaryKind,
    /// Shown under the label as `[type]`, as mermaid does.
    pub type_label: String,
    /// The enclosing boundary, as an index into [`C4::boundaries`]. A
    /// parent always has a lower index than its children.
    pub parent: Option<usize>,
}

/// Which ends of a relation carry an arrowhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrows {
    /// `Rel`: at `to`.
    Forward,
    /// `Rel_Back`: at `from`.
    Back,
    /// `BiRel`: at both ends.
    Both,
}

/// The layout hint of `Rel_U`/`Rel_D`/`Rel_L`/`Rel_R`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub label: String,
    pub technology: Option<String>,
    pub description: Option<String>,
    pub arrows: Arrows,
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone)]
pub struct C4 {
    pub view: View,
    pub title: Option<String>,
    pub elements: Vec<Element>,
    pub boundaries: Vec<Boundary>,
    pub relations: Vec<Relation>,
    /// Elements per grid row (`UpdateLayoutConfig`'s `c4ShapeInRow`).
    pub shapes_per_row: usize,
    /// Boundaries per row (`c4BoundaryInRow`).
    pub boundaries_per_row: usize,
}

impl Default for C4 {
    fn default() -> Self {
        C4 {
            view: View::default(),
            title: None,
            elements: Vec::new(),
            boundaries: Vec::new(),
            relations: Vec::new(),
            // Mermaid's default is 4; 3 keeps a diagram legible at the width
            // of a printed page.
            shapes_per_row: 3,
            boundaries_per_row: 2,
        }
    }
}

/// Names mermaid accepts but this crate does not, reported by name so the
/// error says what is missing rather than just "unexpected".
const UNSUPPORTED: &[&str] = &[
    "Deployment_Node",
    "Node",
    "Node_L",
    "Node_R",
    "RelIndex",
    "UpdateElementStyle",
    "UpdateRelStyle",
];

/// One argument of a `Name(...)` call.
#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Positional(String),
    /// `$key="value"`.
    Keyword(String, String),
}

/// A relation waiting for its ends to be resolved, so forward references
/// work.
struct PendingRel {
    line: usize,
    from: String,
    to: String,
    label: String,
    technology: Option<String>,
    description: Option<String>,
    arrows: Arrows,
    direction: Option<Direction>,
}

/// Parse a `C4Context`, `C4Container` or `C4Component` source.
pub fn parse(source: &str) -> Result<C4, Error> {
    let mut model = C4::default();
    let mut seen_header = false;
    let mut pending: Vec<PendingRel> = Vec::new();
    // Open boundaries, innermost last, with the line that opened each.
    let mut open: Vec<(usize, usize)> = Vec::new();
    // A boundary whose `{` is expected on the next line.
    let mut awaiting_brace: Option<(usize, usize)> = None;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if !seen_header {
            model.view = match line {
                "C4Context" => View::Context,
                "C4Container" => View::Container,
                "C4Component" => View::Component,
                _ if line.starts_with("C4") => {
                    return Err(Error::new(
                        line_number,
                        format!(
                            "`{line}` is not supported; only `C4Context`, `C4Container` \
                             and `C4Component` (see DESIGN.md)"
                        ),
                    ));
                }
                _ => {
                    return Err(Error::new(
                        line_number,
                        "expected `C4Context`, `C4Container` or `C4Component`",
                    ));
                }
            };
            seen_header = true;
            continue;
        }

        if let Some((boundary, _)) = awaiting_brace.take() {
            if line == "{" {
                open.push((boundary, line_number));
                continue;
            }
            return Err(Error::new(
                line_number,
                format!(
                    "expected `{{` to open boundary `{}`",
                    model.boundaries[boundary].alias
                ),
            ));
        }

        if line == "}" {
            if open.pop().is_none() {
                return Err(Error::new(line_number, "`}` closes no boundary"));
            }
            continue;
        }

        if let Some(title) = line.strip_prefix("title ") {
            model.title = Some(title.trim().to_string());
            continue;
        }

        let Some((keyword, args, rest)) = split_call(line) else {
            return Err(Error::new(
                line_number,
                format!("expected `Name(...)`, got `{line}`"),
            ));
        };
        let parent = open.last().map(|&(boundary, _)| boundary);

        if UNSUPPORTED.contains(&keyword) {
            return Err(Error::new(
                line_number,
                format!("`{keyword}` is not supported (see DESIGN.md)"),
            ));
        }

        let args = split_args(args);

        if let Some(boundary_kind) = boundary_keyword(keyword) {
            // `System_Boundary` and friends fix the type themselves; only
            // the generic `Boundary` takes it as an argument.
            let (names, kv): (&[&str], &[&str]) = if boundary_kind == BoundaryKind::Generic {
                (&["alias", "label", "type"], &["type"])
            } else {
                (&["alias", "label"], &[])
            };
            let fields = fields(keyword, &args, names, kv, line_number)?;
            let [alias, label] = required(keyword, &args, &fields, line_number)?;
            check_new_alias(&model, &alias, line_number)?;
            let type_label = match boundary_kind {
                BoundaryKind::Generic => fields[2].clone().unwrap_or_else(|| "system".into()),
                BoundaryKind::Enterprise => "ENTERPRISE".into(),
                BoundaryKind::System => "SYSTEM".into(),
                BoundaryKind::Container => "CONTAINER".into(),
            };
            model.boundaries.push(Boundary {
                alias,
                label,
                kind: boundary_kind,
                type_label,
                parent,
            });
            let boundary = model.boundaries.len() - 1;
            match rest {
                "{" => open.push((boundary, line_number)),
                "" => awaiting_brace = Some((boundary, line_number)),
                other => {
                    return Err(Error::new(
                        line_number,
                        format!("expected `{{` after `{keyword}(...)`, got `{other}`"),
                    ));
                }
            }
            continue;
        }

        if !rest.is_empty() {
            return Err(Error::new(
                line_number,
                format!("unexpected `{rest}` after `{keyword}(...)`"),
            ));
        }

        if keyword == "UpdateLayoutConfig" {
            let names = ["c4ShapeInRow", "c4BoundaryInRow"];
            let fields = fields(keyword, &args, &names, &names, line_number)?;
            for (value, (name, target)) in fields.iter().zip([
                ("c4ShapeInRow", &mut model.shapes_per_row),
                ("c4BoundaryInRow", &mut model.boundaries_per_row),
            ]) {
                if let Some(value) = value {
                    *target = value
                        .parse()
                        .ok()
                        .filter(|&n: &usize| n >= 1)
                        .ok_or_else(|| {
                            Error::new(
                                line_number,
                                format!("`{name}` must be a whole number of at least 1"),
                            )
                        })?;
                }
            }
            continue;
        }

        if let Some((arrows, direction)) = relation_keyword(keyword) {
            let names = ["from", "to", "label", "techn", "descr"];
            let fields = fields(keyword, &args, &names, &["techn", "descr"], line_number)?;
            let [from, to, label] = required(keyword, &args, &fields, line_number)?;
            pending.push(PendingRel {
                line: line_number,
                from,
                to,
                label,
                technology: fields[3].clone(),
                description: fields[4].clone(),
                arrows,
                direction,
            });
            continue;
        }

        let Some(kind) = ElementKind::from_keyword(keyword) else {
            return Err(Error::new(
                line_number,
                format!("unknown element `{keyword}`"),
            ));
        };
        let (names, kv): (&[&str], &[&str]) = if kind.has_technology() {
            (&["alias", "label", "techn", "descr"], &["techn", "descr"])
        } else {
            (&["alias", "label", "descr"], &["descr"])
        };
        let fields = fields(keyword, &args, names, kv, line_number)?;
        let [alias, label] = required(keyword, &args, &fields, line_number)?;
        check_new_alias(&model, &alias, line_number)?;
        let (technology, description) = if kind.has_technology() {
            (fields[2].clone(), fields[3].clone())
        } else {
            (None, fields[2].clone())
        };
        model.elements.push(Element {
            alias,
            label,
            technology,
            description,
            kind,
            boundary: parent,
        });
    }

    if !seen_header {
        return Err(Error::new(
            1,
            "expected `C4Context`, `C4Container` or `C4Component`",
        ));
    }
    if let Some((boundary, line)) = awaiting_brace.or_else(|| open.last().copied()) {
        return Err(Error::new(
            line,
            format!(
                "boundary `{}` is never closed with `}}`",
                model.boundaries[boundary].alias
            ),
        ));
    }

    for rel in pending {
        let resolve = |alias: &str| {
            if let Some(index) = model.elements.iter().position(|e| e.alias == alias) {
                return Ok(index);
            }
            let message = if model.boundaries.iter().any(|b| b.alias == alias) {
                format!(
                    "`{alias}` is a boundary; relations between boundaries are not \
                     supported (see DESIGN.md)"
                )
            } else {
                format!("unknown element `{alias}`")
            };
            Err(Error::new(rel.line, message))
        };
        let from = resolve(&rel.from)?;
        let to = resolve(&rel.to)?;
        model.relations.push(Relation {
            from,
            to,
            label: rel.label,
            technology: rel.technology,
            description: rel.description,
            arrows: rel.arrows,
            direction: rel.direction,
        });
    }

    Ok(model)
}

fn boundary_keyword(keyword: &str) -> Option<BoundaryKind> {
    Some(match keyword {
        "Boundary" => BoundaryKind::Generic,
        "Enterprise_Boundary" => BoundaryKind::Enterprise,
        "System_Boundary" => BoundaryKind::System,
        "Container_Boundary" => BoundaryKind::Container,
        _ => return None,
    })
}

fn relation_keyword(keyword: &str) -> Option<(Arrows, Option<Direction>)> {
    Some(match keyword {
        "Rel" => (Arrows::Forward, None),
        "BiRel" => (Arrows::Both, None),
        "Rel_Back" => (Arrows::Back, None),
        "Rel_U" | "Rel_Up" => (Arrows::Forward, Some(Direction::Up)),
        "Rel_D" | "Rel_Down" => (Arrows::Forward, Some(Direction::Down)),
        "Rel_L" | "Rel_Left" => (Arrows::Forward, Some(Direction::Left)),
        "Rel_R" | "Rel_Right" => (Arrows::Forward, Some(Direction::Right)),
        _ => return None,
    })
}

fn check_new_alias(model: &C4, alias: &str, line: usize) -> Result<(), Error> {
    let taken = model.elements.iter().any(|e| e.alias == alias)
        || model.boundaries.iter().any(|b| b.alias == alias);
    if taken {
        return Err(Error::new(line, format!("`{alias}` is already defined")));
    }
    Ok(())
}

/// Assign arguments to `names`: positionally in order, or by `$name=`
/// for the names in `keywords`. Empty arguments are `None`.
///
/// Anything past the last name is mermaid's sprite, tags and link, which
/// this crate does not draw — so a non-empty one is an error rather than
/// being dropped.
fn fields(
    keyword: &str,
    args: &[Arg],
    names: &[&str],
    keywords: &[&str],
    line: usize,
) -> Result<Vec<Option<String>>, Error> {
    let mut out: Vec<Option<String>> = vec![None; names.len()];
    let mut position = 0;
    for arg in args {
        match arg {
            Arg::Positional(value) => {
                if position < names.len() {
                    out[position] = Some(value.clone()).filter(|v| !v.is_empty());
                } else if !value.is_empty() {
                    return Err(Error::new(
                        line,
                        format!(
                            "`{keyword}` takes ({}); a sprite, tags or link argument \
                             is not supported (see DESIGN.md)",
                            names.join(", ")
                        ),
                    ));
                }
                position += 1;
            }
            Arg::Keyword(key, value) => {
                let Some(slot) = names
                    .iter()
                    .position(|name| name == key)
                    .filter(|_| keywords.contains(&key.as_str()))
                else {
                    return Err(Error::new(
                        line,
                        format!("`${key}` is not supported in `{keyword}` (see DESIGN.md)"),
                    ));
                };
                out[slot] = Some(value.clone()).filter(|v| !v.is_empty());
            }
        }
    }
    Ok(out)
}

/// The first `N` fields, which must be given. The last of them (a label)
/// may be written as `""`; the ends before it (aliases) may not be empty.
fn required<const N: usize>(
    keyword: &str,
    args: &[Arg],
    fields: &[Option<String>],
    line: usize,
) -> Result<[String; N], Error> {
    let given = args
        .iter()
        .filter(|arg| matches!(arg, Arg::Positional(_)))
        .count();
    if given < N || fields[..N - 1].iter().any(Option::is_none) {
        let shape = if N == 3 {
            "(from, to, \"label\")"
        } else {
            "(alias, \"label\")"
        };
        return Err(Error::new(
            line,
            format!("`{keyword}` needs {shape} at least"),
        ));
    }
    Ok(std::array::from_fn(|index| {
        fields[index].clone().unwrap_or_default()
    }))
}

/// Split `Name(args) rest` into its parts. `rest` is what follows the last
/// `)` — a boundary's `{`, or nothing.
fn split_call(line: &str) -> Option<(&str, &str, &str)> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }
    Some((
        line[..open].trim(),
        &line[open + 1..close],
        line[close + 1..].trim(),
    ))
}

/// Split on commas outside quotes. A `$key=value` argument is a keyword
/// argument; quotes around a value are removed.
fn split_args(input: &str) -> Vec<Arg> {
    let mut raw = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for character in input.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                current.push(character);
            }
            ',' if !quoted => raw.push(std::mem::take(&mut current)),
            other => current.push(other),
        }
    }
    raw.push(current);
    // `Name()` has no arguments rather than one empty one.
    if raw.len() == 1 && raw[0].trim().is_empty() {
        return Vec::new();
    }
    raw.into_iter()
        .map(|arg| {
            let arg = arg.trim();
            match arg.strip_prefix('$').and_then(|kv| kv.split_once('=')) {
                Some((key, value)) => Arg::Keyword(key.trim().to_string(), unquote(value)),
                None => Arg::Positional(unquote(arg)),
            }
        })
        .collect()
}

fn unquote(value: &str) -> String {
    value.trim().replace('"', "").trim().to_string()
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
        assert_eq!(model.view, View::Context);
        assert_eq!(model.title.as_deref(), Some("Sample"));
        assert_eq!(model.elements.len(), 2);
        assert_eq!(model.elements[0].description.as_deref(), Some("a person"));
        assert_eq!(model.elements[1].description, None);
        assert_eq!(model.relations.len(), 1);
        assert_eq!(model.relations[0].technology.as_deref(), Some("HTTPS"));
        assert_eq!(model.relations[0].arrows, Arrows::Forward);
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
    fn container_and_component_views_are_accepted() {
        for (header, view) in [
            ("C4Container", View::Container),
            ("C4Component", View::Component),
        ] {
            let model = parse(&format!("{header}\nSystem(a, \"A\")\n")).unwrap();
            assert_eq!(model.view, view);
        }
    }

    #[test]
    fn containers_and_components_carry_a_technology() {
        let source = "C4Container\n\
            Container(api, \"API\", \"Rust\", \"Serves requests\")\n\
            ContainerDb_Ext(db, \"Ledger\", \"Postgres\")\n\
            ComponentQueue(q, \"Jobs\", $descr=\"Work to do\", $techn=\"SQS\")\n\
            SystemQueue_Ext(bus, \"Bus\", \"Partner events\")\n";
        let model = parse(source).unwrap();
        let api = &model.elements[0];
        assert_eq!(api.kind, ElementKind::Container);
        assert_eq!(api.technology.as_deref(), Some("Rust"));
        assert_eq!(api.description.as_deref(), Some("Serves requests"));
        let db = &model.elements[1];
        assert_eq!(db.kind.form(), Form::Db);
        assert!(db.kind.is_external());
        assert_eq!(db.description, None);
        let queue = &model.elements[2];
        assert_eq!(queue.kind.level(), Level::Component);
        assert_eq!(queue.kind.form(), Form::Queue);
        assert_eq!(queue.technology.as_deref(), Some("SQS"));
        assert_eq!(queue.description.as_deref(), Some("Work to do"));
        let bus = &model.elements[3];
        assert_eq!(bus.technology, None);
        assert_eq!(bus.description.as_deref(), Some("Partner events"));
        assert_eq!(bus.kind.stereotype(), "External System Queue");
    }

    #[test]
    fn every_element_keyword_is_recognised() {
        for keyword in [
            "Person",
            "Person_Ext",
            "System",
            "System_Ext",
            "SystemDb",
            "SystemDb_Ext",
            "SystemQueue",
            "SystemQueue_Ext",
            "Container",
            "Container_Ext",
            "ContainerDb",
            "ContainerDb_Ext",
            "ContainerQueue",
            "ContainerQueue_Ext",
            "Component",
            "Component_Ext",
            "ComponentDb",
            "ComponentDb_Ext",
            "ComponentQueue",
            "ComponentQueue_Ext",
        ] {
            let kind = ElementKind::from_keyword(keyword).unwrap();
            assert_eq!(kind.is_external(), keyword.ends_with("_Ext"), "{keyword}");
        }
    }

    #[test]
    fn boundaries_nest_and_hold_their_elements() {
        let source = "C4Container\n\
            Person(user, \"User\")\n\
            Enterprise_Boundary(e, \"Bank\") {\n\
              System_Boundary(s, \"Banking\")\n\
              {\n\
                Container(web, \"Web\", \"Rust\")\n\
              }\n\
              Boundary(b, \"Ops\", \"team\") {\n\
                System(mon, \"Monitoring\")\n\
              }\n\
            }\n\
            Rel(user, web, \"uses\")\n";
        let model = parse(source).unwrap();
        let aliases: Vec<_> = model.boundaries.iter().map(|b| b.alias.as_str()).collect();
        assert_eq!(aliases, ["e", "s", "b"]);
        assert_eq!(model.boundaries[0].parent, None);
        assert_eq!(model.boundaries[1].parent, Some(0));
        assert_eq!(model.boundaries[2].parent, Some(0));
        assert_eq!(model.boundaries[0].type_label, "ENTERPRISE");
        assert_eq!(model.boundaries[1].type_label, "SYSTEM");
        assert_eq!(model.boundaries[2].type_label, "team");
        assert_eq!(model.elements[0].boundary, None);
        assert_eq!(model.elements[1].boundary, Some(1));
        assert_eq!(model.elements[2].boundary, Some(2));
    }

    #[test]
    fn a_generic_boundary_defaults_to_system() {
        let model = parse("C4Context\nBoundary(b, \"B\") {\n}\n").unwrap();
        assert_eq!(model.boundaries[0].type_label, "system");
    }

    #[test]
    fn unbalanced_boundaries_are_errors() {
        let error =
            parse("C4Context\nSystem_Boundary(b, \"B\") {\nSystem(a, \"A\")\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("never closed"), "{error:?}");

        let error = parse("C4Context\nSystem_Boundary(b, \"B\")\nSystem(a, \"A\")\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains('{'), "{error:?}");

        let error = parse("C4Context\nSystem(a, \"A\")\n}\n").unwrap_err();
        assert_eq!(error.line, 3);
    }

    #[test]
    fn relation_variants_set_arrows_and_hints() {
        let source = "C4Context\nSystem(a, \"A\")\nSystem(b, \"B\")\n\
            BiRel(a, b, \"syncs\")\n\
            Rel_Back(a, b, \"notifies\")\n\
            Rel_D(a, b, \"down\")\n\
            Rel_Left(a, b, \"left\", \"gRPC\", \"detail\")\n";
        let model = parse(source).unwrap();
        let r = &model.relations;
        assert_eq!(r[0].arrows, Arrows::Both);
        assert_eq!(r[1].arrows, Arrows::Back);
        assert_eq!(
            (r[2].arrows, r[2].direction),
            (Arrows::Forward, Some(Direction::Down))
        );
        assert_eq!(r[3].direction, Some(Direction::Left));
        assert_eq!(r[3].description.as_deref(), Some("detail"));
    }

    #[test]
    fn layout_config_is_read() {
        let model =
            parse("C4Context\nUpdateLayoutConfig($c4ShapeInRow=\"2\", $c4BoundaryInRow=\"1\")\n")
                .unwrap();
        assert_eq!((model.shapes_per_row, model.boundaries_per_row), (2, 1));
        let model = parse("C4Context\nUpdateLayoutConfig(4)\n").unwrap();
        assert_eq!((model.shapes_per_row, model.boundaries_per_row), (4, 2));
        let error = parse("C4Context\nUpdateLayoutConfig(0)\n").unwrap_err();
        assert!(error.message.contains("c4ShapeInRow"), "{error:?}");
    }

    #[test]
    fn out_of_subset_constructs_name_themselves() {
        for (source, name) in [
            (
                "C4Context\nUpdateElementStyle(a, $bgColor=\"red\")\n",
                "UpdateElementStyle",
            ),
            (
                "C4Context\nUpdateRelStyle(a, b, $textColor=\"red\")\n",
                "UpdateRelStyle",
            ),
            ("C4Context\nRelIndex(1, a, b, \"x\")\n", "RelIndex"),
            (
                "C4Context\nDeployment_Node(n, \"N\") {\n}\n",
                "Deployment_Node",
            ),
            (
                "C4Context\nSystem(a, \"A\", \"d\", $tags=\"v1\")\n",
                "$tags",
            ),
            (
                "C4Context\nSystem(a, \"A\", $link=\"https://x\")\n",
                "$link",
            ),
            ("C4Context\nSystem(a, \"A\", \"d\", \"sprite\")\n", "sprite"),
            (
                "C4Context\nSystem_Boundary(b, \"B\", \"tag\") {\n}\n",
                "tags",
            ),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, 2, "{source}");
            assert!(error.message.contains(name), "{source}: {error:?}");
            assert!(error.message.contains("DESIGN.md"), "{source}: {error:?}");
        }

        for header in ["C4Dynamic", "C4Deployment"] {
            let error = parse(&format!("{header}\n")).unwrap_err();
            assert_eq!(error.line, 1);
            assert!(error.message.contains(header), "{error:?}");
        }
    }

    #[test]
    fn empty_trailing_arguments_drop_nothing_and_are_accepted() {
        let model = parse("C4Context\nSystem(a, \"A\", \"d\", \"\")\n").unwrap();
        assert_eq!(model.elements[0].description.as_deref(), Some("d"));
    }

    #[test]
    fn relations_cannot_join_boundaries() {
        let error =
            parse("C4Context\nSystem(a, \"A\")\nBoundary(b, \"B\") {\n}\nRel(a, b, \"x\")\n")
                .unwrap_err();
        assert_eq!(error.line, 5);
        assert!(error.message.contains("boundary"), "{error:?}");
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
        let error = parse("C4Context\nSystem(a, \"A\")\nBoundary(a, \"B\") {\n}\n").unwrap_err();
        assert!(error.message.contains("already defined"));
    }

    #[test]
    fn trailing_text_after_a_call_is_an_error() {
        let error = parse("C4Context\nSystem(a, \"A\") extra\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("extra"));
    }
}
