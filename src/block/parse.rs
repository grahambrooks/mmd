//! The block grammar: `block/parser/block.jison` upstream, with shapes
//! named as `blockDB.ts`'s `typeStr2Type` names them.
//!
//! Statements are separated by whitespace as well as newlines, so one
//! line can hold several blocks (`a b:2 c`), a chain of links
//! (`A --> B --> C`), or both.

use std::collections::HashMap;

use crate::Error;
use crate::meta::{self, Meta};
use crate::style::{self, Colors};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockShape {
    /// `id` or `id["label"]`
    Square,
    /// `("…")`
    Round,
    /// `(("…"))`
    Circle,
    /// `((("…")))`
    DoubleCircle,
    /// `>"…"]`
    Asymmetric,
    /// `{"…"}`
    Diamond,
    /// `{{"…"}}`
    Hexagon,
    /// `(["…"])`
    Stadium,
    /// `[["…"]]`
    Subroutine,
    /// `[("…")]`
    Cylinder,
    /// `[/"…"/]`
    LeanRight,
    /// `[\"…"\]`
    LeanLeft,
    /// `[/"…"\]`
    Trapezoid,
    /// `[\"…"/]`
    InvTrapezoid,
}

/// A block arrow's direction: `<["label"]>(right, down)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Right,
    Left,
    Up,
    Down,
    /// Both ways horizontally.
    X,
    /// Both ways vertically.
    Y,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Leaf(BlockShape),
    /// `space` or `space:N`: an empty cell.
    Space,
    /// `block … end` or `block:id … end`.
    Composite {
        children: Vec<usize>,
        /// `columns N`; `None` for `columns auto`, one row.
        columns: Option<usize>,
    },
    Arrow(Vec<Dir>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: String,
    /// The label as written; a block without one shows its id.
    pub label: Option<String>,
    pub markdown: bool,
    pub kind: BlockKind,
    /// Columns spanned, from `:N`.
    pub span: usize,
    pub colors: Colors,
    pub line: usize,
}

impl Block {
    pub fn text(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    None,
    Arrow,
    Cross,
    Circle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    Solid,
    Thick,
    Dotted,
    /// `~~~`: places nothing, draws nothing; accepted for compatibility.
    Invisible,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
    pub start: End,
    pub end: End,
    pub stroke: Stroke,
}

/// Block 0 is the diagram itself: a composite that is not drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BlockDiagram {
    pub blocks: Vec<Block>,
    pub links: Vec<Link>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

impl BlockDiagram {
    pub fn index(&self, id: &str) -> Option<usize> {
        self.blocks
            .iter()
            .skip(1)
            .position(|b| b.id == id)
            .map(|i| i + 1)
    }
}

pub fn parse(source: &str) -> Result<BlockDiagram, Error> {
    let lines = meta::lines(source);
    let header = meta::header(&lines, &["block-beta", "block"])?;
    let mut p = Parser {
        diagram: BlockDiagram {
            blocks: vec![Block {
                id: String::new(),
                label: None,
                markdown: false,
                kind: BlockKind::Composite {
                    children: Vec::new(),
                    columns: None,
                },
                span: 1,
                colors: Colors::default(),
                line: header,
            }],
            ..BlockDiagram::default()
        },
        stack: vec![0],
        classes: HashMap::new(),
        applied: Vec::new(),
        styled: Vec::new(),
    };
    let mut at = 1;
    while at < lines.len() {
        if let Some(m) = meta::meta(&lines, &mut at)? {
            match m {
                Meta::AccTitle(t) => p.diagram.acc_title = Some(t),
                Meta::AccDescr(t) => p.diagram.acc_descr = Some(t),
                // Upstream's block grammar has no title; `title x` there is
                // two blocks. Two blocks it is.
                Meta::Title(_) => p.statements(lines[at - 1].1.trim(), lines[at - 1].0)?,
            }
            continue;
        }
        let (number, line) = lines[at];
        at += 1;
        p.line(line.trim(), number)?;
    }
    if p.stack.len() > 1 {
        let open = &p.diagram.blocks[*p.stack.last().unwrap()];
        return Err(Error::new(
            open.line,
            format!(
                "`block{}` is never closed by `end`",
                if open.id.is_empty() {
                    String::new()
                } else {
                    format!(":{}", open.id)
                }
            ),
        ));
    }
    p.resolve_styles()?;
    Ok(p.diagram)
}

struct Parser {
    diagram: BlockDiagram,
    /// The open composites, outermost first.
    stack: Vec<usize>,
    classes: HashMap<String, Colors>,
    /// `class ids name`, in order, with the line.
    applied: Vec<(Vec<String>, String, usize)>,
    /// `style ids css`, in order.
    styled: Vec<(Vec<String>, Colors, usize)>,
}

/// One lexed piece of a statement line.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Node(NodeRef),
    Link(Link),
    Space(usize),
    Columns(Option<usize>),
    /// `block` or `block:id…`, opening a composite.
    Open(Option<NodeRef>),
    End,
}

#[derive(Debug, Clone, PartialEq)]
struct NodeRef {
    id: String,
    label: Option<(String, bool)>,
    kind: Option<BlockKind>,
    span: Option<usize>,
}

impl Parser {
    fn line(&mut self, line: &str, number: usize) -> Result<(), Error> {
        let keyword = |w: &str| {
            line.strip_prefix(w)
                .filter(|r| r.starts_with([' ', '\t']))
                .map(str::trim)
        };
        if let Some(rest) = keyword("classDef") {
            let rest = rest.trim_end_matches(';');
            let (name, css) = rest
                .split_once([' ', '\t'])
                .ok_or_else(|| Error::new(number, "expected `classDef <name> <styles>`"))?;
            let colors = style::colors(&style::declarations(css, number)?, number)?;
            self.classes
                .entry(name.to_string())
                .or_default()
                .merge(&colors);
            return Ok(());
        }
        if let Some(rest) = keyword("class") {
            let (ids, name) = id_list(rest, number, "class")?;
            self.applied
                .push((ids, name.trim_end_matches(';').to_string(), number));
            return Ok(());
        }
        if let Some(rest) = keyword("style") {
            let (ids, css) = id_list(rest, number, "style")?;
            let colors = style::colors(&style::declarations(css, number)?, number)?;
            self.styled.push((ids, colors, number));
            return Ok(());
        }
        for word in ["linkStyle", "click"] {
            if keyword(word).is_some() {
                return Err(Error::new(
                    number,
                    format!("`{word}` is not supported in block diagrams (see DESIGN.md)"),
                ));
            }
        }
        self.statements(line, number)
    }

    fn statements(&mut self, line: &str, number: usize) -> Result<(), Error> {
        let tokens = tokens(line, number)?;
        let mut previous: Option<String> = None;
        let mut pending: Option<Link> = None;
        for token in tokens {
            if !matches!(token, Token::Node(_)) && pending.is_some() {
                return Err(Error::new(number, "a link must end at a block"));
            }
            match token {
                Token::Node(node) => {
                    let id = node.id.clone();
                    self.node(node, number)?;
                    if let Some(mut link) = pending.take() {
                        link.from = previous.clone().unwrap_or_default();
                        link.to = id.clone();
                        self.diagram.links.push(link);
                    }
                    previous = Some(id);
                }
                Token::Link(link) => {
                    if previous.is_none() {
                        return Err(Error::new(number, "a link must start at a block"));
                    }
                    pending = Some(link);
                }
                Token::Space(n) => {
                    previous = None;
                    let id = format!("space-{}", self.diagram.blocks.len());
                    self.push(Block {
                        id,
                        label: None,
                        markdown: false,
                        kind: BlockKind::Space,
                        span: n,
                        colors: Colors::default(),
                        line: number,
                    });
                }
                Token::Columns(n) => {
                    previous = None;
                    let current = *self.stack.last().unwrap();
                    if let BlockKind::Composite { columns, .. } =
                        &mut self.diagram.blocks[current].kind
                    {
                        *columns = n;
                    }
                }
                Token::Open(node) => {
                    previous = None;
                    let (id, label, span) = match node {
                        Some(n) => {
                            if n.kind
                                .as_ref()
                                .is_some_and(|k| !matches!(k, BlockKind::Leaf(BlockShape::Square)))
                            {
                                return Err(Error::new(
                                    number,
                                    format!(
                                        "a composite block takes a `[\"label\"]`, not another shape, in `block:{}`",
                                        n.id
                                    ),
                                ));
                            }
                            (n.id, n.label, n.span.unwrap_or(1))
                        }
                        None => (format!("block-{}", self.diagram.blocks.len()), None, 1),
                    };
                    if self.diagram.index(&id).is_some() {
                        return Err(Error::new(
                            number,
                            format!("`{id}` is already a block; a composite needs its own id"),
                        ));
                    }
                    let (label, markdown) = match label {
                        Some((l, m)) => (Some(l), m),
                        None => (None, false),
                    };
                    let index = self.push(Block {
                        id,
                        label,
                        markdown,
                        kind: BlockKind::Composite {
                            children: Vec::new(),
                            columns: None,
                        },
                        span,
                        colors: Colors::default(),
                        line: number,
                    });
                    self.stack.push(index);
                }
                Token::End => {
                    previous = None;
                    if self.stack.len() == 1 {
                        return Err(Error::new(number, "`end` without an open `block`"));
                    }
                    self.stack.pop();
                }
            }
        }
        if pending.is_some() {
            return Err(Error::new(number, "a link must end at a block"));
        }
        Ok(())
    }

    fn push(&mut self, block: Block) -> usize {
        let index = self.diagram.blocks.len();
        self.diagram.blocks.push(block);
        let parent = *self.stack.last().unwrap();
        if let BlockKind::Composite { children, .. } = &mut self.diagram.blocks[parent].kind {
            children.push(index);
        }
        index
    }

    /// Declare a block, or update one already declared.
    fn node(&mut self, node: NodeRef, number: usize) -> Result<(), Error> {
        if let Some(index) = self.diagram.index(&node.id) {
            let block = &mut self.diagram.blocks[index];
            if let Some((label, markdown)) = node.label {
                block.label = Some(label);
                block.markdown = markdown;
            }
            if let Some(kind) = node.kind {
                if matches!(block.kind, BlockKind::Composite { .. }) {
                    return Err(Error::new(
                        number,
                        format!("`{}` is a composite block and cannot take a shape", node.id),
                    ));
                }
                block.kind = kind;
            }
            if let Some(span) = node.span {
                block.span = span;
            }
            return Ok(());
        }
        let (label, markdown) = match node.label {
            Some((l, m)) => (Some(l), m),
            None => (None, false),
        };
        self.push(Block {
            id: node.id,
            label,
            markdown,
            kind: node.kind.unwrap_or(BlockKind::Leaf(BlockShape::Square)),
            span: node.span.unwrap_or(1),
            colors: Colors::default(),
            line: number,
        });
        Ok(())
    }

    fn resolve_styles(&mut self) -> Result<(), Error> {
        let default = self.classes.get("default").copied();
        if let Some(d) = default {
            for block in self.diagram.blocks.iter_mut().skip(1) {
                if matches!(block.kind, BlockKind::Leaf(_)) {
                    block.colors.merge(&d);
                }
            }
        }
        for (ids, name, number) in std::mem::take(&mut self.applied) {
            let colors = *self.classes.get(&name).ok_or_else(|| {
                Error::new(
                    number,
                    format!("class `{name}` is not defined by a `classDef`"),
                )
            })?;
            for id in ids {
                let index = self.known(&id, number)?;
                self.diagram.blocks[index].colors.merge(&colors);
            }
        }
        for (ids, colors, number) in std::mem::take(&mut self.styled) {
            for id in ids {
                let index = self.known(&id, number)?;
                self.diagram.blocks[index].colors.merge(&colors);
            }
        }
        Ok(())
    }

    fn known(&self, id: &str, number: usize) -> Result<usize, Error> {
        self.diagram
            .index(id)
            .ok_or_else(|| Error::new(number, format!("no block is called `{id}`")))
    }
}

/// `A,B rest` for `class` and `style`.
fn id_list<'a>(rest: &'a str, number: usize, what: &str) -> Result<(Vec<String>, &'a str), Error> {
    // Ids may be written `a, b`; the list ends at the first space not
    // next to a comma.
    let mut end = 0;
    let chars: Vec<(usize, char)> = rest.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        if c.is_whitespace() {
            let before = rest[..at].trim_end().ends_with(',');
            let after = rest[at..].trim_start().starts_with(',');
            if !before && !after {
                end = at;
                break;
            }
        }
        i += 1;
        end = rest.len();
    }
    let (ids, tail) = rest.split_at(end);
    let tail = tail.trim();
    if tail.is_empty() {
        return Err(Error::new(
            number,
            format!("expected `{what} <ids> <value>`"),
        ));
    }
    let ids = ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    Ok((ids, tail))
}

fn is_id_char(c: char) -> bool {
    !c.is_whitespace() && !"()[]{}<>:=-~\"".contains(c)
}

/// The tokens of one statement line.
fn tokens(line: &str, number: usize) -> Result<Vec<Token>, Error> {
    let mut lx = Lexer {
        s: line,
        at: 0,
        number,
    };
    let mut out = Vec::new();
    loop {
        lx.skip_space();
        if lx.done() {
            return Ok(out);
        }
        if let Some(link) = lx.link()? {
            out.push(Token::Link(link));
            continue;
        }
        let rest = lx.rest();
        let start = lx.at;
        let word: String = rest.chars().take_while(|&c| is_id_char(c)).collect();
        if word.is_empty() {
            return Err(lx.error(&format!("unexpected `{}`", preview(rest))));
        }
        lx.at += word.len();
        let next = lx.rest();
        let boundary = next.is_empty() || next.starts_with(char::is_whitespace);
        match word.as_str() {
            "end" if boundary => out.push(Token::End),
            "block" if boundary => out.push(Token::Open(None)),
            "block" if next.starts_with(':') => {
                lx.at += 1;
                let node = lx.node_ref()?;
                out.push(Token::Open(Some(node)));
            }
            "space" if boundary => out.push(Token::Space(1)),
            "space" if next.starts_with(':') => {
                lx.at += 1;
                let n = lx.number()?;
                if n == 0 {
                    return Err(lx.error("`space:0` takes no room; leave it out"));
                }
                out.push(Token::Space(n));
            }
            "columns" if boundary => {
                lx.skip_space();
                if lx.rest().starts_with("auto") {
                    lx.at += 4;
                    out.push(Token::Columns(None));
                } else {
                    let n = lx.number()?;
                    if n == 0 {
                        return Err(lx.error("`columns 0` leaves no room for a block"));
                    }
                    out.push(Token::Columns(Some(n)));
                }
            }
            _ => {
                lx.at = start;
                let node = lx.node_ref()?;
                out.push(Token::Node(node));
            }
        }
    }
}

fn preview(s: &str) -> String {
    s.chars().take(16).collect()
}

struct Lexer<'a> {
    s: &'a str,
    at: usize,
    number: usize,
}

impl<'a> Lexer<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.at..]
    }

    fn done(&self) -> bool {
        self.at >= self.s.len()
    }

    fn skip_space(&mut self) {
        let rest = self.rest();
        self.at += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.at += token.len();
            true
        } else {
            false
        }
    }

    fn error(&self, message: &str) -> Error {
        Error::new(self.number, message.to_string())
    }

    fn number(&mut self) -> Result<usize, Error> {
        let digits: String = self
            .rest()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if digits.is_empty() {
            return Err(self.error(&format!(
                "expected a number, found `{}`",
                preview(self.rest())
            )));
        }
        self.at += digits.len();
        digits
            .parse()
            .map_err(|_| self.error(&format!("`{digits}` is too large")))
    }

    /// An id, then an optional shape with its label, then an optional `:N`.
    fn node_ref(&mut self) -> Result<NodeRef, Error> {
        let id: String = self.rest().chars().take_while(|&c| is_id_char(c)).collect();
        if id.is_empty() {
            return Err(self.error(&format!(
                "expected a block id, found `{}`",
                preview(self.rest())
            )));
        }
        self.at += id.len();
        let mut node = NodeRef {
            id,
            label: None,
            kind: None,
            span: None,
        };
        if self.eat("<[") {
            let label = self.label(&["]>"])?;
            self.eat("]>");
            self.skip_space();
            if !self.eat("(") {
                return Err(self.error("a block arrow needs a direction, as in `<[\"x\"]>(right)`"));
            }
            let close = self
                .rest()
                .find(')')
                .ok_or_else(|| self.error("a block arrow's `(` has no closing `)`"))?;
            let inside = &self.rest()[..close];
            let mut dirs = Vec::new();
            for word in inside.split([',', ' ', '\t']).filter(|w| !w.is_empty()) {
                dirs.push(match word {
                    "right" => Dir::Right,
                    "left" => Dir::Left,
                    "up" => Dir::Up,
                    "down" => Dir::Down,
                    "x" => Dir::X,
                    "y" => Dir::Y,
                    other => {
                        return Err(self.error(&format!(
                            "`{other}` is not a block arrow direction; use right, left, up, down, x or y"
                        )));
                    }
                });
            }
            if dirs.is_empty() {
                return Err(self.error("a block arrow needs a direction"));
            }
            self.at += close + 1;
            node.label = Some(label);
            node.kind = Some(BlockKind::Arrow(dirs));
        } else if let Some((open, closers)) = SHAPES
            .iter()
            .find(|(open, _)| self.rest().starts_with(open))
            .map(|(o, c)| (*o, *c))
        {
            self.at += open.len();
            let ends: Vec<&str> = closers.iter().map(|(c, _)| *c).collect();
            let label = self.label(&ends)?;
            let (close, shape) = closers
                .iter()
                .find(|(c, _)| self.rest().starts_with(c))
                .copied()
                .ok_or_else(|| {
                    self.error(&format!(
                        "`{open}` is not closed by {} in `{}`",
                        ends.iter()
                            .map(|e| format!("`{e}`"))
                            .collect::<Vec<_>>()
                            .join(" or "),
                        node.id
                    ))
                })?;
            self.at += close.len();
            node.label = Some(label);
            node.kind = Some(BlockKind::Leaf(shape));
        } else if self.rest().starts_with([')', '(', '[', '{', '>', '<']) {
            return Err(self.error(&format!(
                "`{}` is not a block shape (see DESIGN.md)",
                preview(self.rest())
            )));
        }
        if self.rest().starts_with(':') {
            self.at += 1;
            let n = self.number()?;
            if n == 0 {
                return Err(self.error(&format!("`{}:0` spans no columns", node.id)));
            }
            node.span = Some(n);
        }
        Ok(node)
    }

    /// A label up to one of `ends`: quoted, a markdown string, or bare.
    fn label(&mut self, ends: &[&str]) -> Result<(String, bool), Error> {
        let rest = self.rest();
        if let Some(md) = rest.strip_prefix("\"`") {
            let end = md
                .find("`\"")
                .ok_or_else(|| self.error("a markdown string has no closing `` ` ``\""))?;
            self.at += 2 + end + 2;
            return Ok((md[..end].to_string(), true));
        }
        if let Some(body) = rest.strip_prefix('"') {
            let end = body
                .find('"')
                .ok_or_else(|| self.error("a label has no closing `\"`"))?;
            self.at += 1 + end + 1;
            return Ok((breaks(&body[..end]), false));
        }
        // Bare: up to the first closer.
        let end = ends
            .iter()
            .filter_map(|e| rest.find(e))
            .min()
            .ok_or_else(|| self.error(&format!("`{}` is not closed", preview(rest))))?;
        self.at += end;
        Ok((breaks(rest[..end].trim()), false))
    }

    /// A link here, if one starts here.
    fn link(&mut self) -> Result<Option<Link>, Error> {
        let start = self.at;
        let Some(first) = self.run()? else {
            return Ok(None);
        };
        let link = match first {
            Run::Link(link) => link,
            Run::Open(start_end) => {
                self.skip_space();
                let rest = self.rest();
                let Some(body) = rest.strip_prefix('"') else {
                    self.at = start;
                    return Err(self.error(&format!(
                        "expected a label in quotes after `{}`",
                        preview(&self.s[start..]).trim()
                    )));
                };
                let end = body
                    .find('"')
                    .ok_or_else(|| self.error("a link label has no closing `\"`"))?;
                let label = body[..end].to_string();
                self.at += 1 + end + 1;
                self.skip_space();
                let Some(Run::Link(mut link)) = self.run()? else {
                    return Err(self.error(&format!(
                        "the link labelled \"{label}\" needs an arrow after its label, as in `-- \"{label}\" -->`"
                    )));
                };
                link.label = Some(label);
                if link.start == End::None {
                    link.start = start_end;
                }
                link
            }
        };
        Ok(Some(link))
    }

    /// `-->`, `---`, `==>`, `-.->`, `~~~`, `<-->`, `--x`, `--o`…; or the
    /// opening `--`, `==`, `-.` of a labelled link.
    fn run(&mut self) -> Result<Option<Run>, Error> {
        let rest = self.rest();
        let begin = self.at;
        let mut chars = rest.chars().peekable();
        let start = if chars.peek() == Some(&'<') {
            chars.next();
            End::Arrow
        } else {
            End::None
        };
        let body: String = {
            let mut b = String::new();
            while let Some(&c) = chars.peek() {
                if matches!(c, '-' | '=' | '.' | '~') {
                    b.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            b
        };
        let suffix = chars
            .peek()
            .copied()
            .filter(|c| matches!(c, '>' | 'x' | 'o'));
        let end = match suffix {
            Some('>') => End::Arrow,
            Some('x') => End::Cross,
            Some('o') => End::Circle,
            _ => End::None,
        };
        // `-?\.+-`, upstream's dotted link.
        let dotted = {
            let b = body.strip_prefix('-').unwrap_or(&body);
            let dots = b.len() - b.trim_start_matches('.').len();
            dots > 0 && &b[dots..] == "-"
        };
        let stroke = if body.len() >= 3 && body.chars().all(|c| c == '~') {
            Stroke::Invisible
        } else if body.len() >= 2 && body.chars().all(|c| c == '-') {
            Stroke::Solid
        } else if body.len() >= 2 && body.chars().all(|c| c == '=') {
            Stroke::Thick
        } else if dotted {
            Stroke::Dotted
        } else if body == "-." {
            return Ok(Some(self.open(
                begin,
                usize::from(start != End::None) + 2,
                start,
            )));
        } else {
            if start == End::Arrow {
                return Err(self.error(&format!("`{}` is not a link", preview(rest))));
            }
            return Ok(None);
        };
        let len = usize::from(start != End::None) + body.len() + usize::from(suffix.is_some());
        // `--` and `==` with nothing after open a labelled link.
        if suffix.is_none() && body.len() == 2 && stroke != Stroke::Invisible {
            return Ok(Some(self.open(begin, len, start)));
        }
        self.at = begin + len;
        Ok(Some(Run::Link(Link {
            from: String::new(),
            to: String::new(),
            label: None,
            start,
            end,
            stroke,
        })))
    }

    fn open(&mut self, begin: usize, len: usize, start: End) -> Run {
        self.at = begin + len;
        Run::Open(start)
    }
}

enum Run {
    Link(Link),
    /// The start of a labelled link, and its start marker. The stroke and
    /// end come from the link after the label, as upstream reads them.
    Open(End),
}

/// Bounding shapes, longest opener first, with each closer and the shape
/// that pair makes.
const SHAPES: &[(&str, &[(&str, BlockShape)])] = &[
    ("(((", &[(")))", BlockShape::DoubleCircle)]),
    ("((", &[("))", BlockShape::Circle)]),
    ("([", &[("])", BlockShape::Stadium)]),
    ("(", &[(")", BlockShape::Round)]),
    ("[[", &[("]]", BlockShape::Subroutine)]),
    ("[(", &[(")]", BlockShape::Cylinder)]),
    (
        "[/",
        &[
            ("/]", BlockShape::LeanRight),
            ("\\]", BlockShape::Trapezoid),
        ],
    ),
    (
        "[\\",
        &[
            ("\\]", BlockShape::LeanLeft),
            ("/]", BlockShape::InvTrapezoid),
        ],
    ),
    ("[", &[("]", BlockShape::Square)]),
    ("{{", &[("}}", BlockShape::Hexagon)]),
    ("{", &[("}", BlockShape::Diamond)]),
    (">", &[("]", BlockShape::Asymmetric)]),
];

fn breaks(text: &str) -> String {
    text.replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("&nbsp;", " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(d: &BlockDiagram) -> Vec<(String, String)> {
        d.blocks[1..]
            .iter()
            .map(|b| (b.id.clone(), format!("{:?}", b.kind)))
            .collect()
    }

    fn children(d: &BlockDiagram, i: usize) -> Vec<&str> {
        match &d.blocks[i].kind {
            BlockKind::Composite { children, .. } => {
                children.iter().map(|&c| d.blocks[c].id.as_str()).collect()
            }
            _ => vec![],
        }
    }

    #[test]
    fn several_blocks_on_a_line_with_spans_and_columns() {
        let d = parse("block-beta\n  columns 3\n  a[\"A label\"] b:2 c:2 d\n").unwrap();
        assert_eq!(children(&d, 0), vec!["a", "b", "c", "d"]);
        assert_eq!(d.blocks[1].label.as_deref(), Some("A label"));
        assert_eq!(d.blocks[2].span, 2);
        assert!(matches!(
            d.blocks[0].kind,
            BlockKind::Composite {
                columns: Some(3),
                ..
            }
        ));
    }

    #[test]
    fn every_shape() {
        let d = parse(
            "block\n a(\"r\") b((\"c\")) c(((\"dc\"))) d>\"asym\"] e{\"dia\"} f{{\"hex\"}} g([\"st\"]) h[[\"sub\"]] i[(\"cyl\")] j[/\"lr\"/] k[\\\"ll\"\\] l[/\"tr\"\\] m[\\\"it\"/] n[bare words]\n",
        )
        .unwrap();
        let shapes: Vec<BlockShape> = d.blocks[1..]
            .iter()
            .map(|b| match b.kind {
                BlockKind::Leaf(s) => s,
                _ => panic!(),
            })
            .collect();
        use BlockShape::*;
        assert_eq!(
            shapes,
            vec![
                Round,
                Circle,
                DoubleCircle,
                Asymmetric,
                Diamond,
                Hexagon,
                Stadium,
                Subroutine,
                Cylinder,
                LeanRight,
                LeanLeft,
                Trapezoid,
                InvTrapezoid,
                Square
            ]
        );
        assert_eq!(d.blocks[14].label.as_deref(), Some("bare words"));
    }

    #[test]
    fn composites_nest_and_take_ids_and_spans() {
        let d = parse("block\n  a:3\n  block:group1:2\n    columns 2\n    h i\n    block\n      D\n    end\n  end\n  g\n").unwrap();
        assert_eq!(children(&d, 0), vec!["a", "group1", "g"]);
        let g1 = d.index("group1").unwrap();
        assert_eq!(d.blocks[g1].span, 2);
        assert_eq!(children(&d, g1).len(), 3);
        assert!(kinds(&d).iter().any(|(id, _)| id == "D"));
    }

    #[test]
    fn links_of_every_kind() {
        let d = parse(
            "block\n A space B\n A-->B\n A --- B\n A ==> B\n A -.-> B\n A <--> B\n A --x B\n A--o B\n A-- \"X\" -->B\n A ~~~ B\n C --> D --> E\n",
        )
        .unwrap();
        let got: Vec<(Stroke, End, End, Option<&str>)> = d
            .links
            .iter()
            .map(|l| (l.stroke, l.start, l.end, l.label.as_deref()))
            .collect();
        use End::*;
        use Stroke::*;
        assert_eq!(
            got,
            vec![
                (Solid, None, Arrow, Option::None),
                (Solid, None, None, Option::None),
                (Thick, None, Arrow, Option::None),
                (Dotted, None, Arrow, Option::None),
                (Solid, Arrow, Arrow, Option::None),
                (Solid, None, Cross, Option::None),
                (Solid, None, Circle, Option::None),
                (Solid, None, Arrow, Some("X")),
                (Invisible, None, None, Option::None),
                (Solid, None, Arrow, Option::None),
                (Solid, None, Arrow, Option::None),
            ]
        );
        assert_eq!(
            (d.links[9].from.as_str(), d.links[10].to.as_str()),
            ("C", "E")
        );
        // C, D and E were declared by the chain.
        assert_eq!(children(&d, 0), vec!["A", "space-2", "B", "C", "D", "E"]);
    }

    #[test]
    fn a_link_updates_labels_of_known_blocks() {
        let d = parse("block\n id1 space id2\n id1(\"Start\")-->id2(\"Stop\")\n").unwrap();
        assert_eq!(d.blocks[1].label.as_deref(), Some("Start"));
        assert_eq!(d.blocks[1].kind, BlockKind::Leaf(BlockShape::Round));
        assert_eq!(d.blocks.len(), 4);
    }

    #[test]
    fn block_arrows() {
        let d = parse("block\n a<[\"Go\"]>(right)\n b<[\" \"]>(x, down)\n").unwrap();
        assert_eq!(d.blocks[1].kind, BlockKind::Arrow(vec![Dir::Right]));
        assert_eq!(d.blocks[2].kind, BlockKind::Arrow(vec![Dir::X, Dir::Down]));
    }

    #[test]
    fn styles_and_classes() {
        let d = parse(
            "block\n A B C\n classDef blue fill:#00f,stroke:#333;\n class A,B blue\n style C fill:#0f0,color:#fff\n",
        )
        .unwrap();
        assert_eq!(d.blocks[1].colors.fill, Some((0, 0, 255)));
        assert_eq!(d.blocks[2].colors.stroke, Some((0x33, 0x33, 0x33)));
        assert_eq!(d.blocks[3].colors.text, Some((255, 255, 255)));
    }

    #[test]
    fn mistakes_name_their_line() {
        for (source, line, needle) in [
            ("block\n  A\n  block:g\n  B\n", 3, "never closed"),
            ("block\n  A\n  end\n", 3, "without"),
            ("block\n  A[open\n", 2, "not closed"),
            ("block\n  A -- B\n", 2, "label in quotes"),
            ("block\n  A -->\n", 2, "end at a block"),
            ("block\n  --> B\n", 2, "start at a block"),
            ("block\n  A\n  class A nope\n", 3, "nope"),
            ("block\n  A\n  style Z fill:#fff\n", 3, "`Z`"),
            ("block\n  A<[\"x\"]>(sideways)\n", 2, "sideways"),
            ("block\n  A)x(\n", 2, "not a block shape"),
            ("block\n  A\n  linkStyle 0 stroke:#f00\n", 3, "linkStyle"),
            ("block\n  columns 0\n", 2, "columns 0"),
            ("block\n  a:0\n", 2, "spans no"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
