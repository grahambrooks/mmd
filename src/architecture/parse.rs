//! `architecture-beta` parsing, per mermaid's
//! `packages/parser/src/language/architecture/architecture.langium` (and
//! `arch.langium`) and the checks in `architectureDb.ts`.
//!
//! ```text
//! architecture-beta
//!     group api(cloud)[API]
//!     group data[Data] in api
//!     service db(database)[Database] in data
//!     service server(server)[Server] in api
//!     junction hub in api
//!     db:L -- R:server
//!     server:T -[calls]-> B:hub
//!     server{group}:B <--> T:db{group}
//! ```

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    fn parse(c: char) -> Option<Side> {
        Some(match c {
            'L' => Side::Left,
            'R' => Side::Right,
            'T' => Side::Top,
            'B' => Side::Bottom,
            _ => return None,
        })
    }

    pub fn horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: String,
    /// `(cloud)`: accepted, not drawn.
    pub icon: Option<String>,
    pub title: Option<String>,
    /// Index into [`Architecture::groups`].
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Service {
        /// `(database)`, `(logos:aws-s3)`.
        icon: Option<String>,
        /// `"text"` in place of an icon.
        icon_text: Option<String>,
        title: Option<String>,
    },
    Junction,
}

/// A service or a junction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub kind: ItemKind,
    /// Index into [`Architecture::groups`].
    pub parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edge {
    /// Indices into [`Architecture::items`].
    pub from: usize,
    pub to: usize,
    pub from_side: Side,
    pub to_side: Side,
    /// `<` before the line: an arrowhead at `from`.
    pub from_arrow: bool,
    /// `>` after the line: an arrowhead at `to`.
    pub to_arrow: bool,
    /// `{group}`: the edge meets the boundary of the end's group.
    pub from_group: bool,
    pub to_group: bool,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Architecture {
    pub title: Option<String>,
    pub groups: Vec<Group>,
    pub items: Vec<Item>,
    pub edges: Vec<Edge>,
}

impl Architecture {
    /// The groups holding `group`, innermost first, starting with itself.
    pub fn group_chain(&self, group: Option<usize>) -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = group;
        while let Some(g) = at {
            out.push(g);
            at = self.groups[g].parent;
        }
        out
    }
}

pub fn parse(source: &str) -> Result<Architecture, Error> {
    let mut model = Architecture::default();
    let mut seen_header = false;
    // Every id, services, junctions and groups alike, is unique.
    let mut ids: Vec<(String, &'static str)> = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            if line != "architecture-beta" && line != "architecture" {
                return Err(Error::new(number, "expected `architecture-beta`"));
            }
            seen_header = true;
            continue;
        }
        let (word, rest) = match line.find([' ', '\t']) {
            Some(at) => (&line[..at], line[at..].trim()),
            None => (line, ""),
        };
        match word {
            "title" => model.title = Some(rest.to_string()),
            "accTitle" | "accTitle:" | "accDescr" | "accDescr:" => {}
            "group" | "service" | "junction" => {
                let mut cursor = Cursor::new(rest, number);
                let id = cursor.id()?;
                let mut icon = None;
                let mut icon_text = None;
                let mut title = None;
                if word == "service" && cursor.peek() == Some('"') {
                    icon_text = Some(cursor.string()?);
                } else if word != "junction" && cursor.peek() == Some('(') {
                    icon = Some(cursor.icon()?);
                }
                if word != "junction" && cursor.peek() == Some('[') {
                    title = Some(cursor.title()?);
                }
                let mut parent = None;
                if cursor.keyword("in") {
                    let name = cursor.id()?;
                    let found = ids.iter().find(|(id, _)| *id == name).map(|(_, k)| *k);
                    match found {
                        None => {
                            return Err(Error::new(
                                number,
                                format!(
                                    "the {word} `{id}`'s parent `{name}` does not exist; \
                                     declare a group before what it holds"
                                ),
                            ));
                        }
                        Some("group") => {
                            parent = model.groups.iter().position(|g| g.id == name);
                        }
                        Some(_) => {
                            return Err(Error::new(
                                number,
                                format!("the {word} `{id}`'s parent `{name}` is not a group"),
                            ));
                        }
                    }
                    if name == id {
                        return Err(Error::new(
                            number,
                            format!("the {word} `{id}` cannot be placed within itself"),
                        ));
                    }
                }
                cursor.end()?;
                if let Some((_, kind)) = ids.iter().find(|(existing, _)| *existing == id) {
                    return Err(Error::new(
                        number,
                        format!("the id `{id}` is already in use by another {kind}"),
                    ));
                }
                if word == "group" {
                    ids.push((id.clone(), "group"));
                    model.groups.push(Group {
                        id,
                        icon,
                        title,
                        parent,
                    });
                } else {
                    ids.push((id.clone(), "node"));
                    let kind = if word == "service" {
                        ItemKind::Service {
                            icon,
                            icon_text,
                            title,
                        }
                    } else {
                        ItemKind::Junction
                    };
                    model.items.push(Item { id, kind, parent });
                }
            }
            _ => {
                let edge = edge(&model, line, number)?;
                model.edges.push(edge);
            }
        }
    }
    if !seen_header {
        return Err(Error::new(1, "expected `architecture-beta`"));
    }
    Ok(model)
}

/// `a{group}:R <-[title]-> L:b{group}`.
fn edge(model: &Architecture, line: &str, number: usize) -> Result<Edge, Error> {
    let mut cursor = Cursor::new(line, number);
    let from_id = cursor.id().map_err(|_| unsupported(line, number))?;
    let from_group = cursor.literal("{group}");
    if !cursor.literal(":") {
        return Err(unsupported(line, number));
    }
    let from_side = cursor.side()?;
    let from_arrow = cursor.literal("<");
    let title = if cursor.literal("--") {
        None
    } else if cursor.literal("-") {
        let title = cursor.title()?;
        if !cursor.literal("-") {
            return Err(Error::new(number, "expected `-[title]-` between the sides"));
        }
        Some(title)
    } else {
        return Err(Error::new(
            number,
            "expected `--` or `-[title]-` between the sides of an edge",
        ));
    };
    let to_arrow = cursor.literal(">");
    let to_side = cursor.side()?;
    if !cursor.literal(":") {
        return Err(Error::new(number, "expected `:` after the right-hand side"));
    }
    let to_id = cursor.id()?;
    let to_group = cursor.literal("{group}");
    cursor.end()?;
    let find = |id: &str| -> Result<usize, Error> {
        if model.groups.iter().any(|g| g.id == id) {
            return Err(Error::new(
                number,
                format!("`{id}` is a group; an edge joins services or junctions (use `{{group}}`)"),
            ));
        }
        model.items.iter().position(|i| i.id == id).ok_or_else(|| {
            Error::new(
                number,
                format!("`{id}` does not exist yet; declare it before an edge to it"),
            )
        })
    };
    let from = find(&from_id)?;
    let to = find(&to_id)?;
    let (pa, pb) = (model.items[from].parent, model.items[to].parent);
    for (modified, id) in [(from_group, &from_id), (to_group, &to_id)] {
        if modified && pa.is_some() && pa == pb {
            return Err(Error::new(
                number,
                format!(
                    "`{id}{{group}}` must cross a group boundary, but the edge does not \
                     pass between two groups"
                ),
            ));
        }
    }
    Ok(Edge {
        from,
        to,
        from_side,
        to_side,
        from_arrow,
        to_arrow,
        from_group,
        to_group,
        title,
    })
}

fn unsupported(line: &str, number: usize) -> Error {
    let word = line.split_whitespace().next().unwrap_or(line);
    Error::new(
        number,
        format!("`{word}` is not supported in an architecture diagram (see DESIGN.md)"),
    )
}

/// A cursor over one statement, skipping spaces between tokens.
struct Cursor<'a> {
    rest: &'a str,
    line: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str, line: usize) -> Self {
        Cursor { rest: text, line }
    }

    fn skip(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.rest.chars().next()
    }

    fn literal(&mut self, token: &str) -> bool {
        self.skip();
        match self.rest.strip_prefix(token) {
            Some(r) => {
                self.rest = r;
                true
            }
            None => false,
        }
    }

    /// A whole word, such as `in`.
    fn keyword(&mut self, word: &str) -> bool {
        self.skip();
        match self.rest.strip_prefix(word) {
            Some(r) if r.is_empty() || r.starts_with([' ', '\t']) => {
                self.rest = r;
                true
            }
            _ => false,
        }
    }

    /// `[\w]([-\w]*\w)?`.
    fn id(&mut self) -> Result<String, Error> {
        self.skip();
        let word = |c: char| c.is_alphanumeric() || c == '_';
        let mut end = 0;
        for (i, c) in self.rest.char_indices() {
            if word(c) || (c == '-' && i > 0) {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        let mut id = &self.rest[..end];
        // An id can't end with a dash: `a--` is `a` then `--`.
        while id.ends_with('-') {
            id = &id[..id.len() - 1];
        }
        if id.is_empty() {
            return Err(Error::new(
                self.line,
                format!("expected an id, found `{}`", self.rest),
            ));
        }
        self.rest = &self.rest[id.len()..];
        Ok(id.to_string())
    }

    fn side(&mut self) -> Result<Side, Error> {
        self.skip();
        let side = self
            .rest
            .chars()
            .next()
            .and_then(Side::parse)
            .ok_or_else(|| {
                Error::new(
                    self.line,
                    format!(
                        "expected a side `L`, `R`, `T` or `B`, found `{}`",
                        self.rest
                    ),
                )
            })?;
        self.rest = &self.rest[1..];
        Ok(side)
    }

    /// `(name)`, letters, digits, `-` and `:`.
    fn icon(&mut self) -> Result<String, Error> {
        self.literal("(");
        let end = self
            .rest
            .find(')')
            .ok_or_else(|| Error::new(self.line, "expected `)` after the icon"))?;
        let icon = self.rest[..end].trim();
        if icon.is_empty()
            || !icon
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':'))
        {
            return Err(Error::new(
                self.line,
                format!("`({icon})` is not an icon name"),
            ));
        }
        self.rest = &self.rest[end + 1..];
        Ok(icon.to_string())
    }

    fn string(&mut self) -> Result<String, Error> {
        self.literal("\"");
        let end = self
            .rest
            .find('"')
            .ok_or_else(|| Error::new(self.line, "unterminated string"))?;
        let text = self.rest[..end].to_string();
        self.rest = &self.rest[end + 1..];
        Ok(text)
    }

    /// `[Title]`, `["Title"]` or `['Title']`.
    fn title(&mut self) -> Result<String, Error> {
        if !self.literal("[") {
            return Err(Error::new(self.line, "expected `[title]`"));
        }
        let quote = self.rest.chars().next().filter(|c| *c == '"' || *c == '\'');
        let text = if let Some(q) = quote {
            let inner = &self.rest[1..];
            let end = inner
                .find(q)
                .ok_or_else(|| Error::new(self.line, "unterminated string in a title"))?;
            let text = inner[..end].to_string();
            self.rest = inner[end + 1..].trim_start();
            text
        } else {
            let end = self
                .rest
                .find(']')
                .ok_or_else(|| Error::new(self.line, "expected `]` after the title"))?;
            let text = self.rest[..end].trim().to_string();
            self.rest = &self.rest[end..];
            text
        };
        if !self.literal("]") {
            return Err(Error::new(self.line, "expected `]` after the title"));
        }
        Ok(text.trim().to_string())
    }

    fn end(&mut self) -> Result<(), Error> {
        self.skip();
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(Error::new(self.line, format!("unexpected `{}`", self.rest)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_groups_services_junctions_and_edges() {
        let model = parse(
            "architecture-beta\n  title Web\n  group api(cloud)[API]\n  group data[\"Data tier\"] in api\n\
             service db(database)[Database] in data\n  service server(server)[Server] in api\n\
             service ext \"X\"[External]\n  junction hub in api\n  db:L -- R:server\n\
             server:T -[calls]-> B:hub\n  ext:R <--> L:server{group}\n  hub:B<--T:db\n",
        )
        .unwrap();
        assert_eq!(model.title.as_deref(), Some("Web"));
        assert_eq!(model.groups[1].title.as_deref(), Some("Data tier"));
        assert_eq!(model.groups[1].parent, Some(0));
        assert_eq!(model.items.len(), 4);
        assert_eq!(
            model.items[2].kind,
            ItemKind::Service {
                icon: None,
                icon_text: Some("X".into()),
                title: Some("External".into())
            }
        );
        assert_eq!(model.edges.len(), 4);
        let e = &model.edges[1];
        assert_eq!((e.from_side, e.to_side), (Side::Top, Side::Bottom));
        assert_eq!(e.title.as_deref(), Some("calls"));
        assert!(e.to_arrow && !e.from_arrow);
        let e = &model.edges[2];
        assert!(e.from_arrow && e.to_arrow && e.to_group && !e.from_group);
        assert!(model.edges[3].from_arrow);
    }

    #[test]
    fn checks_follow_mermaids_db() {
        let error = parse("architecture-beta\n service a in g\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("does not exist"));
        let error = parse("architecture-beta\n service a\n service a\n").unwrap_err();
        assert!(error.message.contains("already in use"));
        let error = parse("architecture-beta\n service a\n a:R -- L:b\n").unwrap_err();
        assert_eq!(error.line, 3);
        let error = parse(
            "architecture-beta\n group g\n service a in g\n service b in g\n a{group}:R -- L:b\n",
        )
        .unwrap_err();
        assert!(error.message.contains("group boundary"));
        let error = parse("architecture-beta\n style a fill:red\n").unwrap_err();
        assert!(error.message.contains("`style`"), "{error}");
    }
}
