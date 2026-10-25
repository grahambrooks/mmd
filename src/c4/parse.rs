//! C4 parsing.
//!
//! Grammar subset per `DESIGN.md`, checked against mermaid's
//! `packages/mermaid/src/diagrams/c4/parser/c4Diagram.jison` and the
//! argument handling in `c4Db.js`. A few C4-PlantUML statements mermaid's
//! grammar lacks (`AddElementTag`, `AddRelTag`, `UpdateBoundaryStyle`,
//! `SHOW_LEGEND()`, …) are accepted too, because real diagrams carry them.

use crate::style::color;
pub use crate::style::{Colors, Rgb};
use crate::{Error, strip_comment};

/// Which C4 view the header names. The view does not restrict which
/// elements may appear — mermaid does not either.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum View {
    #[default]
    Context,
    Container,
    Component,
    /// Relations are numbered in the order they are declared.
    Dynamic,
    Deployment,
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

/// The colours and outline a style statement or tag gives an element or
/// boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ElementStyle {
    /// `bgColor`, `borderColor`, `fontColor`, and a dashed `borderStyle`.
    pub colors: Colors,
    /// `$shape="EightSidedShape()"`.
    pub octagon: bool,
}

impl ElementStyle {
    fn merge(&mut self, other: &ElementStyle) {
        self.colors.merge(&other.colors);
        self.octagon |= other.octagon;
    }
}

/// How a relation's line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
    Bold,
}

/// What `UpdateRelStyle` and relation tags set.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RelStyle {
    pub line_color: Option<Rgb>,
    /// `textColor`. Parsed and kept, but the scene has no per-label colour.
    pub text_color: Option<Rgb>,
    pub line: Option<LineStyle>,
    /// `offsetX`/`offsetY`: where the label moves from the curve's middle.
    pub offset: (f32, f32),
}

impl RelStyle {
    fn merge(&mut self, other: &RelStyle) {
        self.line_color = other.line_color.or(self.line_color);
        self.text_color = other.text_color.or(self.text_color);
        self.line = other.line.or(self.line);
        if other.offset != (0.0, 0.0) {
            self.offset = other.offset;
        }
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
    /// `$tags`, split on `+`.
    pub tags: Vec<String>,
    /// `$sprite`: kept, not drawn (icons are out of scope).
    pub sprite: Option<String>,
    /// `$link`: kept, not drawn (a static SVG has no click target).
    pub link: Option<String>,
    /// From its tags, then `UpdateElementStyle`.
    pub style: ElementStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoundaryKind {
    /// `Boundary`, whose type is the third argument.
    Generic,
    Enterprise,
    System,
    Container,
    /// `Deployment_Node`, `Node`, `Node_L` or `Node_R`: a solid box.
    Node,
}

#[derive(Debug, Clone)]
pub struct Boundary {
    pub alias: String,
    pub label: String,
    pub kind: BoundaryKind,
    /// Shown under the label as `[type]`, as mermaid does.
    pub type_label: String,
    /// Deployment nodes only.
    pub description: Option<String>,
    /// The enclosing boundary, as an index into [`C4::boundaries`]. A
    /// parent always has a lower index than its children.
    pub parent: Option<usize>,
    pub tags: Vec<String>,
    pub link: Option<String>,
    pub style: ElementStyle,
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
    pub tags: Vec<String>,
    pub sprite: Option<String>,
    pub link: Option<String>,
    /// From its tags, then `UpdateRelStyle`.
    pub style: RelStyle,
}

/// An `AddElementTag`/`AddRelTag`/`AddBoundaryTag` definition.
#[derive(Debug, Clone, Default)]
pub struct Tag {
    pub name: String,
    pub element: ElementStyle,
    pub relation: RelStyle,
    /// `$techn`, shown on an element that has no technology of its own.
    pub technology: Option<String>,
    /// `$legendText`, the tag's entry in the legend.
    pub legend: Option<String>,
}

#[derive(Debug, Clone)]
pub struct C4 {
    pub view: View,
    pub title: Option<String>,
    /// `accDescr`; stored, not drawn.
    pub acc_descr: Option<String>,
    pub elements: Vec<Element>,
    pub boundaries: Vec<Boundary>,
    pub relations: Vec<Relation>,
    pub tags: Vec<Tag>,
    /// Elements per grid row (`UpdateLayoutConfig`'s `c4ShapeInRow`).
    pub shapes_per_row: usize,
    /// Boundaries per row (`c4BoundaryInRow`).
    pub boundaries_per_row: usize,
    /// `SHOW_LEGEND()` or `LAYOUT_WITH_LEGEND()`.
    pub legend: bool,
    /// `HIDE_STEREOTYPE()`: no «Person»/«Container» line on elements.
    pub hide_stereotype: bool,
}

impl Default for C4 {
    fn default() -> Self {
        C4 {
            view: View::default(),
            title: None,
            acc_descr: None,
            elements: Vec::new(),
            boundaries: Vec::new(),
            relations: Vec::new(),
            tags: Vec::new(),
            // Mermaid's default is 4; 3 keeps a diagram legible at the width
            // of a printed page.
            shapes_per_row: 3,
            boundaries_per_row: 2,
            legend: false,
            hide_stereotype: false,
        }
    }
}

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
    tags: Vec<String>,
    sprite: Option<String>,
    link: Option<String>,
}

/// A style statement, applied once everything is declared.
enum PendingStyle {
    Element {
        line: usize,
        name: String,
        style: ElementStyle,
        technology: Option<String>,
    },
    /// `UpdateBoundaryStyle`: by alias or by boundary type; `None` for all.
    Boundary {
        line: usize,
        name: Option<String>,
        style: ElementStyle,
    },
    Relation {
        line: usize,
        from: String,
        to: String,
        style: RelStyle,
    },
}

/// C4-PlantUML macros that take no arguments. The `LAYOUT_*` ones only
/// steer PlantUML's layout engine; they are accepted and do nothing here.
const LAYOUT_MACROS: &[&str] = &[
    "LAYOUT_TOP_DOWN",
    "LAYOUT_LEFT_RIGHT",
    "LAYOUT_LANDSCAPE",
    "LAYOUT_AS_SKETCH",
    "HIDE_PERSON_SPRITE",
    "SHOW_PERSON_SPRITE",
    "SHOW_PERSON_OUTLINE",
    "SHOW_PERSON_PORTRAIT",
];

/// Parse a `C4Context`, `C4Container`, `C4Component`, `C4Dynamic` or
/// `C4Deployment` source.
pub fn parse(source: &str) -> Result<C4, Error> {
    let mut model = C4::default();
    let mut seen_header = false;
    let mut pending: Vec<PendingRel> = Vec::new();
    let mut styles: Vec<PendingStyle> = Vec::new();
    // Open boundaries, innermost last, with the line that opened each.
    let mut open: Vec<(usize, usize)> = Vec::new();
    // A boundary whose `{` is expected on the next line.
    let mut awaiting_brace: Option<(usize, usize)> = None;
    // An `accDescr {` block still open: its text so far and first line.
    let mut acc_block: Option<(String, usize)> = None;

    for (index, raw) in source.lines().enumerate() {
        let line_number = index + 1;

        if let Some((text, _)) = acc_block.as_mut() {
            match raw.split_once('}') {
                Some((last, _)) => {
                    text.push_str(last.trim());
                    model.acc_descr = Some(text.trim().to_string());
                    acc_block = None;
                }
                None => {
                    text.push_str(raw.trim());
                    text.push('\n');
                }
            }
            continue;
        }

        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if !seen_header {
            model.view = match line {
                "C4Context" => View::Context,
                "C4Container" => View::Container,
                "C4Component" => View::Component,
                "C4Dynamic" => View::Dynamic,
                "C4Deployment" => View::Deployment,
                _ => {
                    return Err(Error::new(
                        line_number,
                        "expected `C4Context`, `C4Container`, `C4Component`, `C4Dynamic` or \
                         `C4Deployment`",
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
        // Mermaid's C4 grammar sets the drawn title from `accTitle`.
        if let Some(rest) = line.strip_prefix("accTitle") {
            let value = rest
                .trim_start()
                .strip_prefix(':')
                .ok_or_else(|| Error::new(line_number, "expected `accTitle: <text>`"))?;
            model.title = Some(value.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("accDescription ") {
            model.acc_descr = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("accDescr") {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix(':') {
                model.acc_descr = Some(value.trim().to_string());
                continue;
            }
            let Some(body) = rest.strip_prefix('{') else {
                return Err(Error::new(
                    line_number,
                    "expected `accDescr: <text>` or `accDescr { … }`",
                ));
            };
            match body.split_once('}') {
                Some((inside, _)) => model.acc_descr = Some(inside.trim().to_string()),
                None => acc_block = Some((format!("{}\n", body.trim()), line_number)),
            }
            continue;
        }

        let Some((keyword, args, rest)) = split_call(line) else {
            return Err(Error::new(
                line_number,
                format!("expected `Name(...)`, got `{line}`"),
            ));
        };
        let parent = open.last().map(|&(boundary, _)| boundary);
        let args = split_args(args);

        if let Some(boundary_kind) = boundary_keyword(keyword) {
            // `System_Boundary` and friends fix the type themselves; only
            // the generic `Boundary` and deployment nodes take it.
            let (names, kv): (&[&str], &[&str]) = match boundary_kind {
                BoundaryKind::Generic => (
                    &["alias", "label", "type", "tags", "link"],
                    &["type", "tags", "link"],
                ),
                BoundaryKind::Node => (
                    &["alias", "label", "type", "descr", "sprite", "tags", "link"],
                    &["type", "descr", "sprite", "tags", "link"],
                ),
                _ => (&["alias", "label", "tags", "link"], &["tags", "link"]),
            };
            let fields = fields(keyword, &args, names, kv, line_number)?;
            let [alias, label] = required(keyword, &args, &fields, line_number)?;
            check_new_alias(&model, &alias, line_number)?;
            let field = |name: &str| field(names, &fields, name);
            let type_label = match boundary_kind {
                BoundaryKind::Generic => field("type").unwrap_or_else(|| "system".into()),
                BoundaryKind::Node => field("type").unwrap_or_else(|| "node".into()),
                BoundaryKind::Enterprise => "ENTERPRISE".into(),
                BoundaryKind::System => "SYSTEM".into(),
                BoundaryKind::Container => "CONTAINER".into(),
            };
            model.boundaries.push(Boundary {
                alias,
                label,
                kind: boundary_kind,
                type_label,
                description: field("descr"),
                parent,
                tags: split_tags(field("tags")),
                link: field("link"),
                style: ElementStyle::default(),
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

        match keyword {
            "UpdateLayoutConfig" => {
                let names = ["c4ShapeInRow", "c4BoundaryInRow"];
                let fields = fields(keyword, &args, &names, &names, line_number)?;
                for (value, (name, target)) in fields.iter().zip([
                    ("c4ShapeInRow", &mut model.shapes_per_row),
                    ("c4BoundaryInRow", &mut model.boundaries_per_row),
                ]) {
                    if let Some(value) = value {
                        *target =
                            value
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
            "UpdateElementStyle" | "UpdateBoundaryStyle" => {
                let names = ELEMENT_STYLE;
                let fields = fields(keyword, &args, names, names, line_number)?;
                let field = |name: &str| field(names, &fields, name);
                let style = element_style(&field, line_number)?;
                let name = field("elementName");
                if keyword == "UpdateElementStyle" {
                    let name = name.ok_or_else(|| {
                        Error::new(line_number, "`UpdateElementStyle` needs an element alias")
                    })?;
                    styles.push(PendingStyle::Element {
                        line: line_number,
                        name,
                        style,
                        technology: field("techn"),
                    });
                } else {
                    styles.push(PendingStyle::Boundary {
                        line: line_number,
                        name,
                        style,
                    });
                }
                continue;
            }
            "UpdateRelStyle" => {
                let names = [
                    "from",
                    "to",
                    "textColor",
                    "lineColor",
                    "offsetX",
                    "offsetY",
                    "lineStyle",
                    "lineThickness",
                ];
                let fields = fields(keyword, &args, &names, &names, line_number)?;
                let field = |name: &str| field(&names, &fields, name);
                let (Some(from), Some(to)) = (field("from"), field("to")) else {
                    return Err(Error::new(
                        line_number,
                        "`UpdateRelStyle` needs (from, to) at least",
                    ));
                };
                styles.push(PendingStyle::Relation {
                    line: line_number,
                    from,
                    to,
                    style: rel_style(&field, line_number)?,
                });
                continue;
            }
            "AddElementTag"
            | "AddPersonTag"
            | "AddSystemTag"
            | "AddContainerTag"
            | "AddComponentTag"
            | "AddExternalPersonTag"
            | "AddExternalSystemTag"
            | "AddExternalContainerTag"
            | "AddExternalComponentTag"
            | "AddNodeTag"
            | "AddBoundaryTag" => {
                let mut names = vec!["tagStereo"];
                names.extend_from_slice(&ELEMENT_STYLE[1..]);
                let fields = fields(keyword, &args, &names, &names, line_number)?;
                let field = |name: &str| field(&names, &fields, name);
                let name = field("tagStereo")
                    .ok_or_else(|| Error::new(line_number, format!("`{keyword}` needs a tag")))?;
                let tag = Tag {
                    name,
                    element: element_style(&field, line_number)?,
                    technology: field("techn"),
                    legend: field("legendText"),
                    ..Tag::default()
                };
                add_tag(&mut model, tag);
                continue;
            }
            "AddRelTag" => {
                let names = [
                    "tagStereo",
                    "textColor",
                    "lineColor",
                    "lineStyle",
                    "sprite",
                    "techn",
                    "legendText",
                    "legendSprite",
                    "lineThickness",
                ];
                let fields = fields(keyword, &args, &names, &names, line_number)?;
                let field = |name: &str| field(&names, &fields, name);
                let name = field("tagStereo")
                    .ok_or_else(|| Error::new(line_number, "`AddRelTag` needs a tag"))?;
                let tag = Tag {
                    name,
                    relation: rel_style(&field, line_number)?,
                    technology: field("techn"),
                    legend: field("legendText"),
                    ..Tag::default()
                };
                add_tag(&mut model, tag);
                continue;
            }
            "SHOW_LEGEND"
            | "LAYOUT_WITH_LEGEND"
            | "SHOW_FLOATING_LEGEND"
            | "HIDE_STEREOTYPE"
            | "SHOW_STEREOTYPE" => {
                // `SHOW_LEGEND(hideStereotype)` takes an optional flag.
                if args
                    .iter()
                    .any(|a| matches!(a, Arg::Positional(v) if !matches!(v.as_str(), "" | "true" | "false")))
                {
                    return Err(Error::new(
                        line_number,
                        format!("`{keyword}()` takes no arguments here"),
                    ));
                }
                match keyword {
                    "HIDE_STEREOTYPE" => model.hide_stereotype = true,
                    "SHOW_STEREOTYPE" => model.hide_stereotype = false,
                    _ => model.legend = true,
                }
                continue;
            }
            _ if LAYOUT_MACROS.contains(&keyword) => {
                if !args.is_empty() {
                    return Err(Error::new(
                        line_number,
                        format!("`{keyword}()` takes no arguments"),
                    ));
                }
                continue;
            }
            _ => {}
        }

        if let Some((arrows, direction)) = relation_keyword(keyword) {
            let mut args = args;
            if keyword == "RelIndex" {
                // Mermaid drops the index and numbers relations in order.
                let Some(first) = args.iter().position(|a| matches!(a, Arg::Positional(_))) else {
                    return Err(Error::new(
                        line_number,
                        "`RelIndex` needs (index, from, to, \"label\") at least",
                    ));
                };
                args.remove(first);
            }
            let names = [
                "from", "to", "label", "techn", "descr", "sprite", "tags", "link",
            ];
            let kv = &names[3..];
            let fields = fields(keyword, &args, &names, kv, line_number)?;
            let [from, to, label] = required(keyword, &args, &fields, line_number)?;
            let field = |name: &str| field(&names, &fields, name);
            pending.push(PendingRel {
                line: line_number,
                from,
                to,
                label,
                technology: field("techn"),
                description: field("descr"),
                arrows,
                direction,
                tags: split_tags(field("tags")),
                sprite: field("sprite"),
                link: field("link"),
            });
            continue;
        }

        let Some(kind) = ElementKind::from_keyword(keyword) else {
            return Err(Error::new(
                line_number,
                format!("unknown element `{keyword}`"),
            ));
        };
        let names: &[&str] = if kind.has_technology() {
            &["alias", "label", "techn", "descr", "sprite", "tags", "link"]
        } else {
            &["alias", "label", "descr", "sprite", "tags", "link"]
        };
        let kv = &names[2..];
        let fields = fields(keyword, &args, names, kv, line_number)?;
        let [alias, label] = required(keyword, &args, &fields, line_number)?;
        check_new_alias(&model, &alias, line_number)?;
        let field = |name: &str| field(names, &fields, name);
        model.elements.push(Element {
            alias,
            label,
            technology: field("techn"),
            description: field("descr"),
            kind,
            boundary: parent,
            tags: split_tags(field("tags")),
            sprite: field("sprite"),
            link: field("link"),
            style: ElementStyle::default(),
        });
    }

    if !seen_header {
        return Err(Error::new(
            1,
            "expected `C4Context`, `C4Container`, `C4Component`, `C4Dynamic` or `C4Deployment`",
        ));
    }
    if let Some((_, line)) = acc_block {
        return Err(Error::new(line, "`accDescr {` has no closing `}`"));
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
            tags: rel.tags,
            sprite: rel.sprite,
            link: rel.link,
            style: RelStyle::default(),
        });
    }

    apply_styles(&mut model, styles)?;
    Ok(model)
}

/// The arguments `UpdateElementStyle`, `UpdateBoundaryStyle` and the
/// element tags take, mermaid's first and C4-PlantUML's after.
const ELEMENT_STYLE: &[&str] = &[
    "elementName",
    "bgColor",
    "fontColor",
    "borderColor",
    "shadowing",
    "shape",
    "sprite",
    "techn",
    "legendText",
    "legendSprite",
    "borderStyle",
    "borderThickness",
    "type",
];

/// The value of the field called `name`.
fn field(names: &[&str], fields: &[Option<String>], name: &str) -> Option<String> {
    names
        .iter()
        .position(|n| *n == name)
        .and_then(|i| fields[i].clone())
}

/// `$tags="a+b"`, as tag names.
fn split_tags(tags: Option<String>) -> Vec<String> {
    tags.map(|t| {
        t.split('+')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(String::from)
            .collect()
    })
    .unwrap_or_default()
}

/// A later definition of a tag replaces an earlier one.
fn add_tag(model: &mut C4, tag: Tag) {
    model.tags.retain(|t| t.name != tag.name);
    model.tags.push(tag);
}

fn element_style(
    field: &dyn Fn(&str) -> Option<String>,
    line: usize,
) -> Result<ElementStyle, Error> {
    let mut style = ElementStyle::default();
    let rgb = |name: &str| field(name).map(|v| color(&v, line)).transpose();
    style.colors.fill = rgb("bgColor")?;
    style.colors.stroke = rgb("borderColor")?;
    style.colors.text = rgb("fontColor")?;
    if let Some(border) = field("borderStyle") {
        style.colors.dashed = matches!(
            line_style(&border, line)?,
            LineStyle::Dashed | LineStyle::Dotted
        );
    }
    if let Some(shape) = field("shape") {
        style.octagon = match shape.as_str() {
            "EightSidedShape()" => true,
            "RoundedBoxShape()" => false,
            other => {
                return Err(Error::new(
                    line,
                    format!("unknown shape `{other}` (RoundedBoxShape() or EightSidedShape())"),
                ));
            }
        };
    }
    Ok(style)
}

fn rel_style(field: &dyn Fn(&str) -> Option<String>, line: usize) -> Result<RelStyle, Error> {
    let rgb = |name: &str| field(name).map(|v| color(&v, line)).transpose();
    let offset = |name: &str| {
        field(name)
            .map(|v| {
                v.trim()
                    .parse::<f32>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| {
                        Error::new(line, format!("`{name}` must be a number, found `{v}`"))
                    })
            })
            .transpose()
    };
    Ok(RelStyle {
        line_color: rgb("lineColor")?,
        text_color: rgb("textColor")?,
        line: field("lineStyle")
            .map(|v| line_style(&v, line))
            .transpose()?,
        offset: (
            offset("offsetX")?.unwrap_or(0.0),
            offset("offsetY")?.unwrap_or(0.0),
        ),
    })
}

/// C4-PlantUML's `DashedLine()`, `DottedLine()`, `BoldLine()`, `SolidLine()`.
fn line_style(value: &str, line: usize) -> Result<LineStyle, Error> {
    Ok(match value {
        "SolidLine()" => LineStyle::Solid,
        "DashedLine()" => LineStyle::Dashed,
        "DottedLine()" => LineStyle::Dotted,
        "BoldLine()" => LineStyle::Bold,
        other => {
            return Err(Error::new(
                line,
                format!(
                    "unknown line style `{other}` (SolidLine(), DashedLine(), DottedLine() or \
                     BoldLine())"
                ),
            ));
        }
    })
}

/// Tag styles first, in the order each element lists its tags, then the
/// `Update*Style` statements in source order.
fn apply_styles(model: &mut C4, styles: Vec<PendingStyle>) -> Result<(), Error> {
    let tag = |name: &str| model.tags.iter().find(|t| t.name == name);
    let mut element_updates = Vec::new();
    for (index, element) in model.elements.iter().enumerate() {
        let mut style = ElementStyle::default();
        let mut technology = None;
        for name in &element.tags {
            if let Some(t) = tag(name) {
                style.merge(&t.element);
                technology = t.technology.clone().or(technology);
            }
        }
        element_updates.push((index, style, technology));
    }
    let boundary_styles: Vec<ElementStyle> = model
        .boundaries
        .iter()
        .map(|b| {
            let mut style = ElementStyle::default();
            for name in &b.tags {
                if let Some(t) = tag(name) {
                    style.merge(&t.element);
                }
            }
            style
        })
        .collect();
    let rel_styles: Vec<RelStyle> = model
        .relations
        .iter()
        .map(|r| {
            let mut style = RelStyle::default();
            for name in &r.tags {
                if let Some(t) = tag(name) {
                    style.merge(&t.relation);
                }
            }
            style
        })
        .collect();
    for (index, style, technology) in element_updates {
        let element = &mut model.elements[index];
        element.style = style;
        if element.technology.is_none() {
            element.technology = technology;
        }
    }
    for (boundary, style) in model.boundaries.iter_mut().zip(boundary_styles) {
        boundary.style = style;
    }
    for (relation, style) in model.relations.iter_mut().zip(rel_styles) {
        relation.style = style;
    }

    for pending in styles {
        match pending {
            PendingStyle::Element {
                line,
                name,
                style,
                technology,
            } => {
                if let Some(element) = model.elements.iter_mut().find(|e| e.alias == name) {
                    element.style.merge(&style);
                    if technology.is_some() {
                        element.technology = technology;
                    }
                } else if let Some(boundary) = model.boundaries.iter_mut().find(|b| b.alias == name)
                {
                    boundary.style.merge(&style);
                } else {
                    return Err(Error::new(
                        line,
                        format!("`UpdateElementStyle` names `{name}`, which is not defined"),
                    ));
                }
            }
            PendingStyle::Boundary { line, name, style } => {
                let matches = |b: &Boundary| match &name {
                    None => true,
                    Some(name) => &b.alias == name || b.type_label.eq_ignore_ascii_case(name),
                };
                let mut any = false;
                for boundary in model.boundaries.iter_mut().filter(|b| matches(b)) {
                    boundary.style.merge(&style);
                    any = true;
                }
                if !any {
                    return Err(Error::new(
                        line,
                        format!(
                            "`UpdateBoundaryStyle` names `{}`, which is no boundary's alias or \
                             type",
                            name.unwrap_or_default()
                        ),
                    ));
                }
            }
            PendingStyle::Relation {
                line,
                from,
                to,
                style,
            } => {
                let mut any = false;
                for relation in &mut model.relations {
                    if model.elements[relation.from].alias == from
                        && model.elements[relation.to].alias == to
                    {
                        relation.style.merge(&style);
                        any = true;
                    }
                }
                if !any {
                    return Err(Error::new(
                        line,
                        format!("`UpdateRelStyle`: there is no relation from `{from}` to `{to}`"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn boundary_keyword(keyword: &str) -> Option<BoundaryKind> {
    Some(match keyword {
        "Boundary" => BoundaryKind::Generic,
        "Enterprise_Boundary" => BoundaryKind::Enterprise,
        "System_Boundary" => BoundaryKind::System,
        "Container_Boundary" => BoundaryKind::Container,
        "Deployment_Node" | "Node" | "Node_L" | "Node_R" => BoundaryKind::Node,
        _ => return None,
    })
}

fn relation_keyword(keyword: &str) -> Option<(Arrows, Option<Direction>)> {
    Some(match keyword {
        "Rel" | "RelIndex" => (Arrows::Forward, None),
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
/// for the names in `keywords`. Empty arguments are `None`. An argument
/// past the last name is an error rather than being dropped.
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
                            "`{keyword}` takes ({}); `{value}` is one argument too many",
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
                        format!("`${key}` is not an argument of `{keyword}`"),
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
                "C4Context\nSystem(a, \"A\", \"d\", \"s\", \"t\", \"l\", \"extra\")\n",
                "extra",
            ),
            ("C4Context\nSystem(a, \"A\", $colour=\"red\")\n", "$colour"),
            (
                "C4Context\nUpdateElementStyle(ghost, $bgColor=\"red\")\n",
                "ghost",
            ),
            (
                "C4Context\nSystem(a, \"A\")\nUpdateRelStyle(a, a, $lineColor=\"red\")\n",
                "no relation",
            ),
            (
                "C4Context\nSystem(a, \"A\")\nUpdateElementStyle(a, $bgColor=\"hsl(1,2,3)\")\n",
                "hsl",
            ),
            (
                "C4Context\nAddRelTag(t, $lineStyle=\"WavyLine()\")\n",
                "WavyLine()",
            ),
            ("C4Context\nSHOW_LEGEND(sideways)\n", "SHOW_LEGEND"),
            ("C4Context\nRelIndex()\n", "RelIndex"),
        ] {
            let lines = source.lines().count();
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, lines, "{source}");
            assert!(error.message.contains(name), "{source}: {error:?}");
        }

        let error = parse("C4Landscape\n").unwrap_err();
        assert_eq!(error.line, 1);
        assert!(error.message.contains("C4Deployment"), "{error:?}");
    }

    #[test]
    fn dynamic_views_keep_relations_in_order() {
        let source = "C4Dynamic\nContainer(a, \"A\")\nContainer(b, \"B\")\n\
            RelIndex(7, a, b, \"first\")\nRel(b, a, \"second\", \"JSON\")\n\
            RelIndex(1, a, b, \"third\")\n";
        let model = parse(source).unwrap();
        assert_eq!(model.view, View::Dynamic);
        let labels: Vec<_> = model.relations.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["first", "second", "third"]);
        assert_eq!(model.relations[0].from, 0);
        assert_eq!(model.relations[1].technology.as_deref(), Some("JSON"));
    }

    #[test]
    fn deployment_nodes_nest_and_carry_type_and_description() {
        let source = "C4Deployment\n\
            Deployment_Node(cloud, \"AWS\", \"Cloud\", \"eu-west-1\") {\n\
              Node(k8s, \"Cluster\") {\n\
                Container(api, \"API\", \"Rust\")\n\
              }\n\
              Node_L(db, \"RDS\", \"Postgres 16\") {\n\
                ContainerDb(store, \"Store\", \"SQL\")\n\
              }\n\
              Node_R(edge, \"CDN\", $descr=\"Static files\") {\n\
              }\n\
            }\n\
            Rel(api, store, \"Reads\")\n";
        let model = parse(source).unwrap();
        assert_eq!(model.view, View::Deployment);
        let b = &model.boundaries;
        assert_eq!(b.len(), 4);
        assert!(b.iter().all(|b| b.kind == BoundaryKind::Node));
        assert_eq!(b[0].type_label, "Cloud");
        assert_eq!(b[0].description.as_deref(), Some("eu-west-1"));
        assert_eq!(b[1].type_label, "node");
        assert_eq!(b[1].parent, Some(0));
        assert_eq!(b[3].description.as_deref(), Some("Static files"));
        assert_eq!(model.elements[0].boundary, Some(1));
        assert_eq!(model.elements[1].boundary, Some(2));
    }

    #[test]
    fn sprites_tags_and_links_are_accepted() {
        let source = "C4Context\n\
            AddElementTag(\"v1\", $bgColor=\"#d73027\", $fontColor=\"white\", $techn=\"Java\", \
            $legendText=\"Legacy\")\n\
            AddRelTag(async, $lineColor=\"blue\", $lineStyle=DashedLine())\n\
            Person(u, \"User\", \"d\", \"person_sprite\", \"v1\", \"https://x\")\n\
            System(s, \"Sys\", $sprite=\"server\", $tags=\"v1+other\", $link=\"https://y\")\n\
            System_Boundary(b, \"B\", \"v1\") {\n\
            }\n\
            Rel(u, s, \"Uses\", $tags=\"async\", $link=\"https://z\")\n";
        let model = parse(source).unwrap();
        let (u, s) = (&model.elements[0], &model.elements[1]);
        assert_eq!(u.sprite.as_deref(), Some("person_sprite"));
        assert_eq!(u.tags, ["v1"]);
        assert_eq!(u.link.as_deref(), Some("https://x"));
        assert_eq!(s.tags, ["v1", "other"]);
        assert_eq!(s.style.colors.fill, Some((0xd7, 0x30, 0x27)));
        assert_eq!(s.style.colors.text, Some((255, 255, 255)));
        assert_eq!(s.technology.as_deref(), Some("Java"));
        assert_eq!(model.boundaries[0].tags, ["v1"]);
        assert_eq!(
            model.boundaries[0].style.colors.fill,
            Some((0xd7, 0x30, 0x27))
        );
        let rel = &model.relations[0];
        assert_eq!(rel.style.line_color, Some((0, 0, 255)));
        assert_eq!(rel.style.line, Some(LineStyle::Dashed));
        assert_eq!(model.tags[0].legend.as_deref(), Some("Legacy"));
    }

    #[test]
    fn style_updates_override_tags() {
        let source = "C4Context\n\
            AddElementTag(t, $bgColor=\"red\")\n\
            System(a, \"A\", $tags=\"t\")\n\
            System(b, \"B\")\n\
            Boundary(g, \"G\", \"team\") {\n\
            }\n\
            UpdateElementStyle(a, $bgColor=\"#00ff00\", $borderColor=\"blue\", \
            $shape=\"EightSidedShape()\")\n\
            UpdateElementStyle(b, \"#123\", \"#456\")\n\
            UpdateBoundaryStyle(team, $bgColor=\"ivory\", $borderStyle=\"DashedLine()\")\n\
            Rel(a, b, \"x\")\n\
            UpdateRelStyle(a, b, $textColor=\"red\", $lineColor=\"teal\", $offsetX=\"10\", \
            $offsetY=\"-20\")\n";
        let model = parse(source).unwrap();
        let a = &model.elements[0].style;
        assert_eq!(a.colors.fill, Some((0, 255, 0)));
        assert_eq!(a.colors.stroke, Some((0, 0, 255)));
        assert!(a.octagon);
        let b = &model.elements[1].style;
        assert_eq!(b.colors.fill, Some((0x11, 0x22, 0x33)));
        assert_eq!(b.colors.text, Some((0x44, 0x55, 0x66)));
        let g = &model.boundaries[0].style;
        assert_eq!(g.colors.fill, Some((255, 255, 240)));
        assert!(g.colors.dashed);
        let r = &model.relations[0].style;
        assert_eq!(r.line_color, Some((0, 128, 128)));
        assert_eq!(r.text_color, Some((255, 0, 0)));
        assert_eq!(r.offset, (10.0, -20.0));
    }

    #[test]
    fn legend_stereotype_and_layout_macros() {
        let source = "C4Context\nLAYOUT_LEFT_RIGHT()\nSHOW_LEGEND()\nHIDE_STEREOTYPE()\n\
            System(a, \"A\")\n";
        let model = parse(source).unwrap();
        assert!(model.legend && model.hide_stereotype);
    }

    #[test]
    fn accessibility_statements() {
        let source = "C4Context\naccTitle: Shown as the title\naccDescr {\n  Two\n  lines\n}\n\
            System(a, \"A\")\n";
        let model = parse(source).unwrap();
        assert_eq!(model.title.as_deref(), Some("Shown as the title"));
        assert_eq!(model.acc_descr.as_deref(), Some("Two\nlines"));
        let model = parse("C4Context\naccDescr: one line\n").unwrap();
        assert_eq!(model.acc_descr.as_deref(), Some("one line"));
        let error = parse("C4Context\naccDescr {\n  open\n").unwrap_err();
        assert_eq!(error.line, 2);
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
