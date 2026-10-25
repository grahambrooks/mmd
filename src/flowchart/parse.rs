//! Flowchart syntax: `flowchart`/`graph` with a direction, nodes in any of
//! Mermaid's shapes (bracket syntax or v11 `@{ shape: … }`), links of every
//! weight with labels and ids, invisible links, chains and `&` lists, nested
//! subgraphs with their own direction, `classDef`/`class`/`:::`/`style`
//! colours on nodes and subgraphs, `linkStyle`, `click`, accessibility
//! statements and markdown strings.

pub use crate::style::{Colors, Rgb};
use crate::style::{color, colors, dash_length, declarations, number as length};
use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    Rect,
    Round,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Ellipse,
    Asymmetric,
    Rhombus,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
    /// A page with a wavy bottom edge (`doc`).
    Document,
    /// A cylinder on its side (`h-cyl`, direct access storage).
    HorizontalCylinder,
    Cloud,
    /// A page with a folded corner (`note`).
    Note,
    /// Just the label, no outline (`text`).
    Text,
    /// A small filled dot (`sm-circ`, `f-circ`): no label.
    Dot,
    /// A ringed dot (`fr-circ`, a stop): no label.
    RingedDot,
    /// A thick bar (`fork`/`join`): no label.
    Bar,
}

impl NodeShape {
    /// Shapes mermaid draws without their label.
    pub fn unlabelled(self) -> bool {
        matches!(self, NodeShape::Dot | NodeShape::RingedDot | NodeShape::Bar)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Solid,
    Dotted,
    Thick,
    /// `~~~`: placed like a link, never drawn.
    Invisible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    None,
    Arrow,
    Circle,
    Cross,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    Solid,
    Dashed,
    Dotted,
}

/// A `linkStyle`: the parts of the CSS a drawn edge can carry.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LinkStyle {
    pub stroke: Option<Rgb>,
    /// `stroke-width` in px.
    pub width: Option<f32>,
    pub dash: Option<Stroke>,
    /// The label's `color`. Parsed, but the scene has no per-label colour.
    pub color: Option<Rgb>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    /// The label is a markdown string (`` "`…`" `` or an `@{ label }`).
    pub markdown: bool,
    pub shape: NodeShape,
    /// Innermost subgraph, if any.
    pub parent: Option<usize>,
    pub classes: Vec<String>,
    pub style: Colors,
    /// Where it was first mentioned.
    pub source_line: usize,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub markdown: bool,
    pub line: LineKind,
    pub start: End,
    pub end: End,
    /// `A e1@--> B`.
    pub id: Option<String>,
    /// Its own `linkStyle`, which replaces `linkStyle default`.
    pub style: Option<LinkStyle>,
}

#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: String,
    pub title: String,
    pub markdown: bool,
    pub parent: Option<usize>,
    /// Its own `direction`, if it set one.
    pub direction: Option<Direction>,
    pub classes: Vec<String>,
    pub style: Colors,
}

#[derive(Debug, Clone)]
pub struct Flowchart {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub subgraphs: Vec<Subgraph>,
    pub classes: Vec<(String, Colors)>,
    /// `linkStyle default …`.
    pub link_default: Option<LinkStyle>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

impl Flowchart {
    fn class(&self, name: &str) -> Option<Colors> {
        self.classes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| *c)
    }

    /// A node's colours: the `default` class, its classes, then `style`.
    pub fn colors(&self, node: &Node) -> Colors {
        let mut out = self.class("default").unwrap_or_default();
        for name in &node.classes {
            if let Some(c) = self.class(name) {
                out.merge(&c);
            }
        }
        out.merge(&node.style);
        out
    }

    /// A subgraph's colours: its classes, then `style`.
    pub fn subgraph_colors(&self, sub: &Subgraph) -> Colors {
        let mut out = Colors::default();
        for name in &sub.classes {
            if let Some(c) = self.class(name) {
                out.merge(&c);
            }
        }
        out.merge(&sub.style);
        out
    }

    /// A link's style: its own `linkStyle`, else `linkStyle default`.
    pub fn link_style(&self, link: &Link) -> LinkStyle {
        link.style.or(self.link_default).unwrap_or_default()
    }
}

pub fn parse(source: &str) -> Result<Flowchart, Error> {
    let lines = logical_lines(source);
    let mut p = Parser {
        chart: Flowchart {
            direction: Direction::TopDown,
            nodes: Vec::new(),
            links: Vec::new(),
            subgraphs: Vec::new(),
            classes: Vec::new(),
            link_default: None,
            acc_title: None,
            acc_descr: None,
        },
        stack: Vec::new(),
        clicks: Vec::new(),
    };
    let mut header = false;
    for (number, text) in &lines {
        let number = *number;
        let line = text.trim();
        if line.is_empty() {
            continue;
        }
        if !header {
            let (head, rest) = line.split_once(';').unwrap_or((line, ""));
            p.header(head, number)?;
            header = true;
            for statement in split_statements(rest) {
                p.statement(statement.trim(), number)?;
            }
            continue;
        }
        if keyword_is(line, "accTitle") || keyword_is(line, "accDescr") {
            p.accessibility(line, number)?;
            continue;
        }
        for statement in split_statements(line) {
            p.statement(statement.trim(), number)?;
        }
    }
    if !header {
        return Err(Error::new(1, "expected `flowchart` or `graph`"));
    }
    p.finish(source.lines().count())
}

/// Source lines, joined where a markdown string, `@{ … }` data or an
/// `accDescr { … }` block runs on: (first line's number, text).
fn logical_lines(source: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut open = false;
    for (index, raw) in source.lines().enumerate() {
        let line = strip_comment(raw);
        match out.last_mut() {
            Some(last) if open => {
                last.1.push('\n');
                last.1.push_str(line);
            }
            _ => out.push((index + 1, line.to_string())),
        }
        open = out.last().is_some_and(|(_, text)| continues(text));
    }
    out
}

/// Whether a line is still open at its end.
fn continues(text: &str) -> bool {
    let t = text.trim_start();
    if keyword_is(t, "accDescr") {
        let after = t["accDescr".len()..].trim_start();
        return after.starts_with('{') && !after.contains('}');
    }
    let b = text.as_bytes();
    let (mut i, mut md, mut quoted, mut data) = (0, false, false, 0i32);
    while i < b.len() {
        if md {
            if b[i] == b'`' && b.get(i + 1) == Some(&b'"') {
                md = false;
                i += 1;
            }
        } else {
            match b[i] {
                b'"' if !quoted && b.get(i + 1) == Some(&b'`') => {
                    md = true;
                    i += 1;
                }
                b'"' => quoted = !quoted,
                b'@' if !quoted && b.get(i + 1) == Some(&b'{') => {
                    data += 1;
                    i += 1;
                }
                b'{' if !quoted && data > 0 => data += 1,
                b'}' if !quoted && data > 0 => data -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    md || data > 0
}

/// `line` starts with the keyword `word`, not merely a longer id.
fn keyword_is(line: &str, word: &str) -> bool {
    line.strip_prefix(word)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', ':', '{', '\n']))
}

fn direction(word: &str) -> Option<Direction> {
    Some(match word {
        "TB" | "TD" => Direction::TopDown,
        "BT" => Direction::BottomUp,
        "LR" => Direction::LeftRight,
        "RL" => Direction::RightLeft,
        _ => return None,
    })
}

/// A line's statements, split on `;` outside quotes and brackets.
fn split_statements(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut quoted, mut start) = (0i32, false, 0);
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' | '(' | '{' if !quoted => depth += 1,
            ']' | ')' | '}' if !quoted => depth -= 1,
            ';' if !quoted && depth <= 0 => {
                out.push(&line[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&line[start..]);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

struct Parser {
    chart: Flowchart,
    /// Open subgraphs, innermost last.
    stack: Vec<usize>,
    /// Ids named by `click`, checked once every node is known.
    clicks: Vec<(String, usize)>,
}

impl Parser {
    fn header(&mut self, line: &str, number: usize) -> Result<(), Error> {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("flowchart" | "graph") => {}
            _ => return Err(Error::new(number, "expected `flowchart` or `graph`")),
        }
        if let Some(dir) = words.next() {
            self.chart.direction = direction(dir)
                .ok_or_else(|| Error::new(number, format!("unknown direction `{dir}`")))?;
        }
        if let Some(extra) = words.next() {
            return Err(Error::new(
                number,
                format!("unexpected `{extra}` after the direction"),
            ));
        }
        Ok(())
    }

    fn statement(&mut self, statement: &str, number: usize) -> Result<(), Error> {
        let keyword = statement.split_whitespace().next().unwrap_or("");
        let rest = statement[keyword.len()..].trim();
        let chart = &mut self.chart;
        match keyword {
            "subgraph" => {
                let (id, title, markdown) = subgraph_title(rest, chart.subgraphs.len());
                chart.subgraphs.push(Subgraph {
                    id,
                    title,
                    markdown,
                    parent: self.stack.last().copied(),
                    direction: None,
                    classes: Vec::new(),
                    style: Colors::default(),
                });
                self.stack.push(chart.subgraphs.len() - 1);
                Ok(())
            }
            "end" if rest.is_empty() => self
                .stack
                .pop()
                .map(|_| ())
                .ok_or_else(|| Error::new(number, "`end` without a `subgraph`")),
            "direction" => {
                let dir = direction(rest).ok_or_else(|| {
                    Error::new(
                        number,
                        format!("unknown direction `{rest}` (TB, TD, BT, LR or RL)"),
                    )
                })?;
                match self.stack.last() {
                    Some(&sub) => chart.subgraphs[sub].direction = Some(dir),
                    None if dir == chart.direction => {}
                    None => {
                        return Err(Error::new(
                            number,
                            "`direction` outside a subgraph has no effect; set it on the `flowchart` line",
                        ));
                    }
                }
                Ok(())
            }
            "linkStyle" => self.link_style(rest, number),
            "click" => self.click(rest, number),
            "accTitle" | "accDescr" => self.accessibility(statement, number),
            "classDef" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let names = words.next().unwrap_or("");
                if names.is_empty() {
                    return Err(Error::new(number, "`classDef` needs a class name"));
                }
                let colors = colors(&declarations(words.next().unwrap_or(""), number)?, number)?;
                for name in names.split(',') {
                    chart.classes.push((name.trim().to_string(), colors));
                }
                Ok(())
            }
            "class" => {
                let mut words = rest.rsplitn(2, char::is_whitespace);
                let class = words.next().unwrap_or("").to_string();
                let ids = words.next().unwrap_or("").trim();
                if class.is_empty() || ids.is_empty() {
                    return Err(Error::new(number, "expected `class <ids> <class>`"));
                }
                for id in ids.split(',') {
                    let id = id.trim();
                    if let Some(sub) = chart.subgraphs.iter_mut().find(|s| s.id == id) {
                        sub.classes.push(class.clone());
                    } else {
                        let node = node_index(chart, id, &self.stack, number);
                        chart.nodes[node].classes.push(class.clone());
                    }
                }
                Ok(())
            }
            "style" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let id = words.next().unwrap_or("");
                if id.is_empty() {
                    return Err(Error::new(number, "expected `style <id> <styles>`"));
                }
                let colors = colors(&declarations(words.next().unwrap_or(""), number)?, number)?;
                if let Some(sub) = chart.subgraphs.iter_mut().find(|s| s.id == id) {
                    sub.style.merge(&colors);
                } else {
                    let node = node_index(chart, id, &self.stack, number);
                    chart.nodes[node].style.merge(&colors);
                }
                Ok(())
            }
            _ => {
                if self.edge_data(statement, number)? {
                    return Ok(());
                }
                chain(&mut self.chart, &self.stack, statement, number)
            }
        }
    }

    /// `linkStyle 0,2 stroke:#f00,stroke-width:2px` or `linkStyle default …`,
    /// optionally with `interpolate <curve>`.
    fn link_style(&mut self, rest: &str, number: usize) -> Result<(), Error> {
        let mut words = rest.splitn(2, char::is_whitespace);
        let targets = words.next().unwrap_or("");
        let mut styles = words.next().unwrap_or("").trim();
        if targets.is_empty() {
            return Err(Error::new(
                number,
                "`linkStyle` needs link numbers or `default`",
            ));
        }
        let mut curved = false;
        if keyword_is(styles, "interpolate") {
            let mut w = styles["interpolate".len()..]
                .trim_start()
                .splitn(2, char::is_whitespace);
            let curve = w.next().unwrap_or("");
            if !CURVES.contains(&curve) {
                return Err(Error::new(number, format!("unknown curve `{curve}`")));
            }
            styles = w.next().unwrap_or("").trim();
            curved = true;
        }
        if styles.is_empty() && !curved {
            return Err(Error::new(number, "`linkStyle` needs styles"));
        }
        let style = link_style(&declarations(styles, number)?, number)?;
        let apply = !styles.is_empty();
        if targets == "default" {
            if apply {
                self.chart.link_default = Some(style);
            }
            return Ok(());
        }
        for target in targets.split(',') {
            let target = target.trim();
            let n: usize = target.parse().map_err(|_| {
                Error::new(
                    number,
                    format!("`linkStyle` expects link numbers or `default`, found `{target}`"),
                )
            })?;
            let count = self.chart.links.len();
            let link = self.chart.links.get_mut(n).ok_or_else(|| {
                Error::new(
                    number,
                    format!(
                        "`linkStyle {n}`: there is no link {n}; links are numbered from 0 and {count} are defined above this line"
                    ),
                )
            })?;
            if apply {
                link.style = Some(style);
            }
        }
        Ok(())
    }

    /// `click id callback ["tooltip"]`, `click id call fn(args) ["tooltip"]`,
    /// `click id [href] "url" ["tooltip"] [_blank]`. Interactions a static
    /// SVG can't carry, checked so a typo doesn't pass silently.
    fn click(&mut self, rest: &str, number: usize) -> Result<(), Error> {
        let mut words = rest.splitn(2, char::is_whitespace);
        let ids = words.next().unwrap_or("");
        if ids.is_empty() {
            return Err(Error::new(number, "`click` needs a node id"));
        }
        for id in ids.split(',') {
            self.clicks.push((id.trim().to_string(), number));
        }
        let tokens = click_tokens(words.next().unwrap_or(""), number)?;
        let target = |t: &Tok| matches!(t, Tok::Word(w) if LINK_TARGETS.contains(&w.as_str()));
        let link_tail = |rest: &[Tok]| match rest {
            [] | [Tok::Str] => true,
            [t] => target(t),
            [Tok::Str, t] => target(t),
            _ => false,
        };
        let callback = |w: &str| {
            !w.is_empty()
                && w.chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, '_' | '$' | '.'))
        };
        let ok = match tokens.as_slice() {
            [Tok::Word(h), Tok::Str, rest @ ..] if h == "href" => link_tail(rest),
            [Tok::Word(c), Tok::Word(f), rest @ ..] if c == "call" => {
                let name = f.split_once('(').map_or(f.as_str(), |(n, _)| n);
                callback(name)
                    && (!f.contains('(') || f.ends_with(')'))
                    && matches!(rest, [] | [Tok::Str])
            }
            [Tok::Str, rest @ ..] => link_tail(rest),
            [Tok::Word(f), rest @ ..] => {
                f != "href" && f != "call" && callback(f) && matches!(rest, [] | [Tok::Str])
            }
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(Error::new(
                number,
                "malformed `click`: expected `click <id> <callback> [\"tooltip\"]`, `click <id> call <fn>(<args>)` or `click <id> [href] \"<url>\" [\"tooltip\"] [_self|_blank|_parent|_top]`",
            ))
        }
    }

    /// `accTitle: …`, `accDescr: …` or a multi-line `accDescr { … }`.
    fn accessibility(&mut self, line: &str, number: usize) -> Result<(), Error> {
        if let Some(rest) = line.strip_prefix("accTitle") {
            let value = rest
                .trim_start()
                .strip_prefix(':')
                .ok_or_else(|| Error::new(number, "expected `accTitle: <text>`"))?;
            self.chart.acc_title = Some(value.trim().to_string());
            return Ok(());
        }
        let rest = line["accDescr".len()..].trim_start();
        if let Some(value) = rest.strip_prefix(':') {
            self.chart.acc_descr = Some(value.trim().to_string());
            return Ok(());
        }
        let body = rest
            .strip_prefix('{')
            .ok_or_else(|| Error::new(number, "expected `accDescr: <text>` or `accDescr { … }`"))?;
        let (inside, after) = body
            .split_once('}')
            .ok_or_else(|| Error::new(number, "`accDescr {` has no closing `}`"))?;
        if !after.trim().trim_start_matches(';').trim().is_empty() {
            return Err(Error::new(
                number,
                format!("unexpected `{}` after `accDescr {{ … }}`", after.trim()),
            ));
        }
        let text: Vec<&str> = inside.lines().map(str::trim).collect();
        self.chart.acc_descr = Some(text.join("\n").trim().to_string());
        Ok(())
    }

    /// `e1@{ animate: true }` for an edge id already defined. Mermaid
    /// animates the edge or changes its curve; neither is drawn here.
    fn edge_data(&mut self, statement: &str, number: usize) -> Result<bool, Error> {
        let Some((name, after)) = statement.split_once("@{") else {
            return Ok(false);
        };
        if !self
            .chart
            .links
            .iter()
            .any(|l| l.id.as_deref() == Some(name))
        {
            return Ok(false);
        }
        let close = data_end(after)
            .ok_or_else(|| Error::new(number, format!("`{name}@{{` has no closing `}}`")))?;
        if !after[close + 1..].trim().is_empty() {
            return Err(Error::new(
                number,
                format!("unexpected `{}` after edge data", after[close + 1..].trim()),
            ));
        }
        for (key, value) in data_pairs(&after[..close], number)? {
            let valid = match key.as_str() {
                "animate" => matches!(value.as_str(), "true" | "false"),
                "animation" => matches!(value.as_str(), "fast" | "slow"),
                "curve" => CURVES.contains(&value.as_str()),
                _ => {
                    return Err(Error::new(
                        number,
                        format!("unknown edge property `{key}` (animate, animation or curve)"),
                    ));
                }
            };
            if !valid {
                return Err(Error::new(
                    number,
                    format!("unexpected value `{value}` for edge `{key}`"),
                ));
            }
        }
        Ok(true)
    }

    fn finish(mut self, last_line: usize) -> Result<Flowchart, Error> {
        if let Some(&open) = self.stack.last() {
            return Err(Error::new(
                last_line,
                format!("subgraph `{}` has no `end`", self.chart.subgraphs[open].id),
            ));
        }
        let chart = &mut self.chart;
        // A `style` or `class` written before its subgraph made a node of
        // that id: hand its colours to the subgraph. A link to a subgraph is
        // another matter.
        for s in 0..chart.subgraphs.len() {
            let id = chart.subgraphs[s].id.clone();
            let Some(n) = chart.nodes.iter().position(|node| node.id == id) else {
                continue;
            };
            let node = &chart.nodes[n];
            if chart.links.iter().any(|l| l.from == n || l.to == n) {
                return Err(Error::new(
                    node.source_line,
                    format!("a link to subgraph `{id}` is not supported (see DESIGN.md)"),
                ));
            }
            if node.label != node.id || node.shape != NodeShape::Rect {
                return Err(Error::new(
                    node.source_line,
                    format!("node `{id}` has the same id as a subgraph"),
                ));
            }
            let node = chart.nodes.remove(n);
            let sub = &mut chart.subgraphs[s];
            sub.classes.extend(node.classes);
            sub.style.merge(&node.style);
            for l in &mut chart.links {
                l.from -= usize::from(l.from > n);
                l.to -= usize::from(l.to > n);
            }
        }
        for (id, number) in &self.clicks {
            let known = chart.nodes.iter().any(|n| &n.id == id)
                || chart.subgraphs.iter().any(|s| &s.id == id);
            if !known {
                return Err(Error::new(
                    *number,
                    format!("`click` names `{id}`, which is not a node"),
                ));
            }
        }
        for node in &chart.nodes {
            if node.shape.unlabelled() && node.label != node.id {
                return Err(Error::new(
                    node.source_line,
                    format!(
                        "node `{}`: its shape is drawn without a label, so `{}` would be lost",
                        node.id, node.label
                    ),
                ));
            }
        }
        Ok(self.chart)
    }
}

/// d3 curves mermaid accepts for `interpolate` and `curve:`.
const CURVES: [&str; 20] = [
    "basis",
    "basisClosed",
    "basisOpen",
    "bumpX",
    "bumpY",
    "bundle",
    "cardinal",
    "cardinalClosed",
    "cardinalOpen",
    "catmullRom",
    "catmullRomClosed",
    "catmullRomOpen",
    "linear",
    "linearClosed",
    "monotoneX",
    "monotoneY",
    "natural",
    "step",
    "stepAfter",
    "stepBefore",
];

const LINK_TARGETS: [&str; 4] = ["_self", "_blank", "_parent", "_top"];

#[derive(Debug)]
enum Tok {
    Str,
    Word(String),
}

/// Quoted strings and words; a word's `( … )` stays in it whole.
fn click_tokens(text: &str, number: usize) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        if c == '"' {
            chars.next();
            let close = text[start + 1..]
                .find('"')
                .ok_or_else(|| Error::new(number, "unterminated string in `click`"))?;
            out.push(Tok::Str);
            let end = start + close + 2;
            while chars.peek().is_some_and(|&(i, _)| i < end) {
                chars.next();
            }
            continue;
        }
        let mut end = text.len();
        let mut depth = 0;
        let mut quoted = false;
        for (i, c) in text[start..].char_indices() {
            match c {
                '"' if depth > 0 => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => depth -= 1,
                c if c.is_whitespace() && depth <= 0 => {
                    end = start + i;
                    break;
                }
                _ => {}
            }
        }
        out.push(Tok::Word(text[start..end].to_string()));
        while chars.peek().is_some_and(|&(i, _)| i < end) {
            chars.next();
        }
    }
    Ok(out)
}

/// `id [title]`, `id["title"]` or just `title`: (id, title, markdown).
fn subgraph_title(rest: &str, index: usize) -> (String, String, bool) {
    if rest.is_empty() {
        return (format!("subgraph{index}"), String::new(), false);
    }
    if let Some(open) = rest.find('[')
        && !rest[..open].trim().is_empty()
    {
        let id = rest[..open].trim().to_string();
        let (title, markdown) = label_text(rest[open + 1..].trim_end().trim_end_matches(']'));
        return (id, title, markdown);
    }
    let (title, markdown) = label_text(rest);
    let id = if rest.contains(char::is_whitespace) || rest.starts_with('"') {
        format!("subgraph{index}")
    } else {
        rest.to_string()
    };
    (id, title, markdown)
}

/// `A --> B & C -- text --> D`: nodes, joined by links.
fn chain(
    chart: &mut Flowchart,
    stack: &[usize],
    statement: &str,
    number: usize,
) -> Result<(), Error> {
    let mut p = Cursor {
        s: statement,
        i: 0,
        number,
    };
    let mut previous = group(chart, stack, &mut p)?;
    loop {
        p.skip_space();
        if p.done() {
            return Ok(());
        }
        let id = p.edge_id();
        let link = p.link()?;
        let next = group(chart, stack, &mut p)?;
        for &from in &previous {
            for &to in &next {
                // The first link to claim an id keeps it, as in mermaid.
                let id = id
                    .clone()
                    .filter(|id| !chart.links.iter().any(|l| l.id.as_ref() == Some(id)));
                chart.links.push(Link {
                    from,
                    to,
                    label: link.label.clone(),
                    markdown: link.markdown,
                    line: link.line,
                    start: link.start,
                    end: link.end,
                    id,
                    style: None,
                });
            }
        }
        previous = next;
    }
}

/// `A & B[x] & C`: nodes, declared or referred to.
fn group(chart: &mut Flowchart, stack: &[usize], p: &mut Cursor) -> Result<Vec<usize>, Error> {
    let mut out = vec![node(chart, stack, p)?];
    loop {
        let save = p.i;
        p.skip_space();
        if p.eat("&") {
            p.skip_space();
            out.push(node(chart, stack, p)?);
        } else {
            p.i = save;
            return Ok(out);
        }
    }
}

const SHAPES: [(&str, &str, NodeShape); 15] = [
    ("(((", ")))", NodeShape::DoubleCircle),
    ("((", "))", NodeShape::Circle),
    ("(-", "-)", NodeShape::Ellipse),
    ("([", "])", NodeShape::Stadium),
    ("[[", "]]", NodeShape::Subroutine),
    ("[(", ")]", NodeShape::Cylinder),
    ("{{", "}}", NodeShape::Hexagon),
    ("[/", "/]", NodeShape::Parallelogram),
    ("[\\", "\\]", NodeShape::ParallelogramAlt),
    ("[/", "\\]", NodeShape::Trapezoid),
    ("[\\", "/]", NodeShape::TrapezoidAlt),
    ("[", "]", NodeShape::Rect),
    ("(", ")", NodeShape::Round),
    ("{", "}", NodeShape::Rhombus),
    (">", "]", NodeShape::Asymmetric),
];

/// A node: an id, then perhaps a shape with its label, `:::class` and
/// `@{ … }` data.
fn node(chart: &mut Flowchart, stack: &[usize], p: &mut Cursor) -> Result<usize, Error> {
    p.skip_space();
    let start = p.i;
    while let Some(c) = p.peek() {
        if c.is_alphanumeric() || c == '_' {
            p.i += c.len_utf8();
        } else {
            break;
        }
    }
    if p.i == start {
        return Err(p.error("expected a node id"));
    }
    let id = p.s[start..p.i].to_string();
    let index = node_index(chart, &id, stack, p.number);
    let rest = &p.s[p.i..];
    if rest.starts_with("[|") {
        return Err(p.error("`[|field:value| …]` vertex properties are not supported"));
    }
    // A shape: the longest opener that has its closer (`[[x]]` is a
    // subroutine, not `[` `]`); of shapes sharing an opener, the one whose
    // closer comes first (`[/a\]` is a trapezoid even with `/]` later on).
    let found = SHAPES
        .iter()
        .filter(|(open, ..)| rest.starts_with(open))
        .filter_map(|&(open, close, shape)| {
            let end = find_closer(&rest[open.len()..], close)?;
            let finish = open.len() + end + close.len();
            Some((finish, std::cmp::Reverse(open.len()), open, end, shape))
        })
        .min_by_key(|&(finish, longer, ..)| (longer, finish));
    if let Some((finish, _, open, end, shape)) = found {
        let (label, markdown) = label_text(&rest[open.len()..open.len() + end]);
        let n = &mut chart.nodes[index];
        (n.label, n.markdown, n.shape) = (label, markdown, shape);
        p.i += finish;
    }
    for _ in 0..2 {
        if p.eat(":::") {
            let start = p.i;
            while let Some(c) = p.peek() {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    p.i += c.len_utf8();
                } else {
                    break;
                }
            }
            if p.i == start {
                return Err(p.error("expected a class name after `:::`"));
            }
            chart.nodes[index].classes.push(p.s[start..p.i].to_string());
        }
        if p.eat("@{") {
            let body = &p.s[p.i..];
            let close = data_end(body).ok_or_else(|| p.error("`@{` has no closing `}`"))?;
            node_data(chart, index, &body[..close], p.number)?;
            p.i += close + 1;
        }
    }
    Ok(index)
}

/// In `body` (just after `@{`), where its closing `}` is.
fn data_end(body: &str) -> Option<usize> {
    let (mut depth, mut quote) = (1, None);
    for (i, c) in body.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(q)) if c == q => quote = None,
            ('{', None) => depth += 1,
            ('}', None) => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// `shape: rect, label: "x"` (or one per line): keys and unquoted values.
fn data_pairs(body: &str, number: usize) -> Result<Vec<(String, String)>, Error> {
    let mut parts = Vec::new();
    let (mut start, mut quote) = (0, None);
    for (i, c) in body.char_indices() {
        match (c, quote) {
            ('"' | '\'', None) => quote = Some(c),
            (c, Some(q)) if c == q => quote = None,
            (',' | '\n', None) => {
                parts.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&body[start..]);
    let mut out = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part.split_once(':').ok_or_else(|| {
            Error::new(
                number,
                format!("expected `key: value` in `@{{ … }}`, found `{part}`"),
            )
        })?;
        let value = value.trim();
        let value = ['"', '\'']
            .iter()
            .find_map(|&q| value.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
            .unwrap_or(value);
        out.push((key.trim().to_string(), value.to_string()));
    }
    Ok(out)
}

/// A node's `@{ shape: …, label: … }`.
fn node_data(chart: &mut Flowchart, index: usize, body: &str, number: usize) -> Result<(), Error> {
    let mut label = None;
    let mut markdown = true;
    for (key, value) in data_pairs(body, number)? {
        match key.as_str() {
            "shape" => chart.nodes[index].shape = named_shape(&value, number)?,
            "label" => label = Some(value),
            "labelType" => {
                markdown = match value.as_str() {
                    "markdown" => true,
                    "string" | "text" => false,
                    _ => {
                        return Err(Error::new(
                            number,
                            format!("unknown labelType `{value}` (markdown, string or text)"),
                        ));
                    }
                }
            }
            "icon" | "img" | "form" | "pos" | "w" | "h" | "constraint" => {
                return Err(Error::new(
                    number,
                    format!("`{key}`: icon and image nodes are not supported (see DESIGN.md)"),
                ));
            }
            _ => {
                return Err(Error::new(
                    number,
                    format!("unknown node property `{key}` (shape, label or labelType)"),
                ));
            }
        }
    }
    if let Some(label) = label {
        let n = &mut chart.nodes[index];
        n.label = breaks(&label);
        n.markdown = markdown;
    }
    Ok(())
}

/// A v11 shape name or alias (`shapes.ts` upstream).
fn named_shape(name: &str, number: usize) -> Result<NodeShape, Error> {
    use NodeShape::*;
    Ok(match name {
        "rect" | "proc" | "process" | "rectangle" => Rect,
        "rounded" | "event" | "state" => Round,
        "stadium" | "terminal" | "pill" => Stadium,
        "fr-rect" | "subprocess" | "subproc" | "framed-rectangle" | "subroutine" => Subroutine,
        "cyl" | "db" | "database" | "cylinder" => Cylinder,
        "h-cyl" | "das" | "horizontal-cylinder" => HorizontalCylinder,
        "circle" | "circ" => Circle,
        "dbl-circ" | "double-circle" => DoubleCircle,
        "diam" | "decision" | "diamond" | "question" | "choice" => Rhombus,
        "hex" | "hexagon" | "prepare" => Hexagon,
        "lean-r" | "lean-right" | "in-out" => Parallelogram,
        "lean-l" | "lean-left" | "out-in" => ParallelogramAlt,
        "trap-b" | "priority" | "trapezoid-bottom" | "trapezoid" => Trapezoid,
        "trap-t" | "manual" | "trapezoid-top" | "inv-trapezoid" => TrapezoidAlt,
        "odd" => Asymmetric,
        "doc" | "document" => Document,
        "cloud" => Cloud,
        "note" => Note,
        "text" => Text,
        "sm-circ" | "start" | "small-circle" | "f-circ" | "junction" | "filled-circle" => Dot,
        "fr-circ" | "stop" | "framed-circle" => RingedDot,
        "fork" | "join" => Bar,
        _ if UNSUPPORTED_SHAPES.contains(&name) => {
            return Err(Error::new(
                number,
                format!("shape `{name}` is not supported (see DESIGN.md)"),
            ));
        }
        _ => return Err(Error::new(number, format!("unknown shape `{name}`"))),
    })
}

/// Mermaid's shapes with no counterpart here.
const UNSUPPORTED_SHAPES: [&str; 72] = [
    "datastore",
    "data-store",
    "bang",
    "notch-rect",
    "card",
    "notched-rectangle",
    "lin-rect",
    "lined-rectangle",
    "lined-process",
    "lin-proc",
    "shaded-process",
    "hourglass",
    "collate",
    "brace",
    "comment",
    "brace-l",
    "brace-r",
    "braces",
    "bolt",
    "com-link",
    "lightning-bolt",
    "delay",
    "half-rounded-rectangle",
    "lin-cyl",
    "disk",
    "lined-cylinder",
    "curv-trap",
    "curved-trapezoid",
    "display",
    "div-rect",
    "div-proc",
    "divided-rectangle",
    "divided-process",
    "tri",
    "extract",
    "triangle",
    "win-pane",
    "internal-storage",
    "window-pane",
    "notch-pent",
    "loop-limit",
    "notched-pentagon",
    "flip-tri",
    "manual-file",
    "flipped-triangle",
    "sl-rect",
    "manual-input",
    "sloped-rectangle",
    "docs",
    "documents",
    "st-doc",
    "stacked-document",
    "st-rect",
    "procs",
    "processes",
    "stacked-rectangle",
    "bow-rect",
    "stored-data",
    "bow-tie-rectangle",
    "cross-circ",
    "summary",
    "crossed-circle",
    "tag-doc",
    "tagged-document",
    "tag-rect",
    "tagged-rectangle",
    "tag-proc",
    "tagged-process",
    "flag",
    "paper-tape",
    "lin-doc",
    "lined-document",
];

/// Where `close` ends a label, skipping a quoted one.
fn find_closer(body: &str, close: &str) -> Option<usize> {
    let trimmed = body.trim_start();
    if let Some(quoted) = trimmed.strip_prefix('"') {
        let lead = body.len() - trimmed.len();
        let end_quote = quoted.find('"')? + 1;
        let after = lead + end_quote + 1;
        return body[after..].find(close).map(|i| after + i);
    }
    body.find(close)
}

/// A label's text, and whether it is a markdown string (`` "`…`" ``).
fn label_text(raw: &str) -> (String, bool) {
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix("\"`").and_then(|s| s.strip_suffix("`\"")) {
        let lines: Vec<&str> = inner.lines().map(str::trim).collect();
        return (breaks(lines.join("\n").trim()), true);
    }
    (breaks(&unquote(raw)), false)
}

fn breaks(text: &str) -> String {
    text.replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace("<br />", "\n")
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    s.to_string()
}

/// The node with `id`, made (in the innermost open subgraph) if new.
fn node_index(chart: &mut Flowchart, id: &str, stack: &[usize], number: usize) -> usize {
    if let Some(i) = chart.nodes.iter().position(|n| n.id == id) {
        return i;
    }
    chart.nodes.push(Node {
        id: id.to_string(),
        label: id.to_string(),
        markdown: false,
        shape: NodeShape::Rect,
        parent: stack.last().copied(),
        classes: Vec::new(),
        style: Colors::default(),
        source_line: number,
    });
    chart.nodes.len() - 1
}

/// A link's stroke, stroke-width, stroke-dasharray and color. `fill` (always
/// `none` on a link) and font properties have nothing to apply to.
fn link_style(declarations: &[(String, String)], number: usize) -> Result<LinkStyle, Error> {
    let mut out = LinkStyle::default();
    for (key, value) in declarations {
        match key.as_str() {
            "stroke" => out.stroke = Some(color(value, number)?),
            "stroke-width" => out.width = Some(length(value, number, "stroke-width")?),
            "stroke-dasharray" => out.dash = Some(dash(value, number)?),
            "color" => out.color = Some(color(value, number)?),
            _ => {}
        }
    }
    Ok(out)
}

/// `5 5` is dashed, `2 2` or shorter dotted, `0` or `none` solid.
fn dash(value: &str, number: usize) -> Result<Stroke, Error> {
    Ok(match dash_length(value, number)? {
        None | Some(0.0) => Stroke::Solid,
        Some(on) if on <= 2.5 => Stroke::Dotted,
        Some(_) => Stroke::Dashed,
    })
}

struct Cursor<'a> {
    s: &'a str,
    i: usize,
    number: usize,
}

struct ParsedLink {
    label: Option<String>,
    markdown: bool,
    line: LineKind,
    start: End,
    end: End,
}

impl Cursor<'_> {
    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn done(&self) -> bool {
        self.i >= self.s.len()
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.s[self.i..].starts_with(token) {
            self.i += token.len();
            true
        } else {
            false
        }
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek().filter(|c| c.is_whitespace()) {
            self.i += c.len_utf8();
        }
    }

    fn error(&self, message: &str) -> Error {
        let near: String = self.s[self.i..].chars().take(12).collect();
        Error::new(self.number, format!("{message} near `{near}`"))
    }

    /// An edge id before a link: `e1@-->`.
    fn edge_id(&mut self) -> Option<String> {
        let rest = &self.s[self.i..];
        let first = rest.chars().next()?;
        if !(first.is_alphanumeric() || first == '_') {
            return None;
        }
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))?;
        let after = rest[end..].strip_prefix('@')?;
        if after.starts_with(['{', '"']) || after.is_empty() {
            return None;
        }
        self.i += end + 1;
        Some(rest[..end].to_string())
    }

    /// A link: `-->`, `---`, `-.->`, `==>`, `--o`, `--x`, `<-->`, `~~~`,
    /// with `|label|` after or `-- label -->` inside.
    fn link(&mut self) -> Result<ParsedLink, Error> {
        let rest = &self.s[self.i..];
        if rest.starts_with("~~~") {
            self.i += rest.len() - rest.trim_start_matches('~').len();
            let save = self.i;
            self.skip_space();
            if self.peek() == Some('|') {
                return Err(self.error("a label on an invisible link `~~~` is not supported"));
            }
            self.i = save;
            return Ok(ParsedLink {
                label: None,
                markdown: false,
                line: LineKind::Invisible,
                start: End::None,
                end: End::None,
            });
        }
        let mut chars = rest.char_indices().peekable();
        let mut start = End::None;
        if let Some(&(_, c)) = chars.peek()
            && matches!(c, '<' | 'o' | 'x')
            && rest[c.len_utf8()..].starts_with(['-', '=', '.'])
        {
            start = match c {
                '<' => End::Arrow,
                'o' => End::Circle,
                _ => End::Cross,
            };
            chars.next();
        }
        let body_start = chars.peek().map_or(rest.len(), |&(i, _)| i);
        let mut body_end = body_start;
        while let Some(&(i, c)) = chars.peek() {
            if matches!(c, '-' | '.' | '=') {
                body_end = i + 1;
                chars.next();
            } else {
                break;
            }
        }
        let body = &rest[body_start..body_end];
        if body.len() < 2 {
            return Err(self.error("expected a link"));
        }
        let mut label = None;
        let mut markdown = false;
        let mut consumed = body_end;
        let mut whole = body.to_string();
        // `-- text -->`: an opener, text, then the rest of the link.
        if matches!(body, "--" | "==" | "-.") && rest[body_end..].starts_with(char::is_whitespace) {
            let after = &rest[body_end..];
            let close = find_link_end(after).ok_or_else(|| self.error("unterminated link text"))?;
            let (text, md) = label_text(&after[..close.0]);
            (label, markdown) = (Some(text), md);
            whole.push_str(&after[close.0..close.1]);
            consumed = body_end + close.1;
        }
        let tail = &rest[consumed..];
        let mut end = End::None;
        if let Some(c) = tail.chars().next() {
            let next = tail[c.len_utf8()..].chars().next();
            let ends_word = next.is_none_or(|n| !n.is_alphanumeric() && n != '_');
            end = match c {
                '>' => End::Arrow,
                'o' if ends_word => End::Circle,
                'x' if ends_word => End::Cross,
                _ => End::None,
            };
            if end != End::None {
                consumed += c.len_utf8();
            }
        }
        // A closing arrow written into the text form (`-- text -->`).
        if whole.ends_with('>') {
            end = End::Arrow;
        }
        self.i += consumed;
        let line = if whole.contains('.') {
            LineKind::Dotted
        } else if whole.contains('=') {
            LineKind::Thick
        } else {
            LineKind::Solid
        };
        self.skip_space();
        if self.eat("|") {
            let start = self.i;
            let close = self.s[start..]
                .find('|')
                .ok_or_else(|| self.error("unterminated `|label|`"))?;
            let (text, md) = label_text(&self.s[start..start + close]);
            (label, markdown) = (Some(text), md);
            self.i = start + close + 1;
        }
        Ok(ParsedLink {
            label: label.filter(|l| !l.is_empty()),
            markdown,
            line,
            start,
            end,
        })
    }
}

/// In `text -->`, where the text ends and where the link's own characters
/// (including its arrow) end.
fn find_link_end(after: &str) -> Option<(usize, usize)> {
    let bytes = after.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b'-' | b'.' | b'=') {
                j += 1;
            }
            if j - (i + 1) >= 2 {
                let arrow = if j < bytes.len() && bytes[j] == b'>' {
                    1
                } else {
                    0
                };
                return Some((i, j + arrow));
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node<'a>(chart: &'a Flowchart, id: &str) -> &'a Node {
        chart.nodes.iter().find(|n| n.id == id).unwrap()
    }

    fn error(source: &str) -> Error {
        parse(source).unwrap_err()
    }

    #[test]
    fn shapes_and_labels() {
        let chart = parse(
            "flowchart LR\n  A[Rect] --> B(Round) --> C([Stadium]) --> D[[Sub]]\n  E[(DB)] --> F((Circle)) --> G{Decide} --> H{{Hex}}\n  K[/Trap\\] --> I[/In/] --> J[\\Out\\] --> L>Flag]\n  M[\"Quoted [x]\"] --> N(-Ellipse-)\n",
        )
        .unwrap();
        assert_eq!(chart.direction, Direction::LeftRight);
        let shape = |id: &str| node(&chart, id).shape;
        assert_eq!(shape("A"), NodeShape::Rect);
        assert_eq!(shape("C"), NodeShape::Stadium);
        assert_eq!(shape("E"), NodeShape::Cylinder);
        assert_eq!(shape("F"), NodeShape::Circle);
        assert_eq!(shape("G"), NodeShape::Rhombus);
        assert_eq!(shape("H"), NodeShape::Hexagon);
        assert_eq!(shape("I"), NodeShape::Parallelogram);
        assert_eq!(shape("K"), NodeShape::Trapezoid);
        assert_eq!(shape("L"), NodeShape::Asymmetric);
        assert_eq!(shape("N"), NodeShape::Ellipse);
        assert_eq!(node(&chart, "N").label, "Ellipse");
        assert_eq!(node(&chart, "M").label, "Quoted [x]");
        assert_eq!(chart.links.len(), 10);
    }

    #[test]
    fn links_of_every_kind() {
        let chart = parse(
            "graph TD\n  A --> B\n  A --- C\n  A -.-> D\n  A ==> E\n  A --o F\n  A --x G\n  A <--> H\n  A -->|yes| I\n  A -- no --> J\n  A -. maybe .-> K\n  A ~~~ L\n",
        )
        .unwrap();
        let l = &chart.links;
        assert_eq!((l[0].line, l[0].end), (LineKind::Solid, End::Arrow));
        assert_eq!(l[1].end, End::None);
        assert_eq!((l[2].line, l[2].end), (LineKind::Dotted, End::Arrow));
        assert_eq!((l[3].line, l[3].end), (LineKind::Thick, End::Arrow));
        assert_eq!(l[4].end, End::Circle);
        assert_eq!(l[5].end, End::Cross);
        assert_eq!((l[6].start, l[6].end), (End::Arrow, End::Arrow));
        assert_eq!(l[7].label.as_deref(), Some("yes"));
        assert_eq!((l[8].label.as_deref(), l[8].end), (Some("no"), End::Arrow));
        assert_eq!(
            (l[9].label.as_deref(), l[9].line),
            (Some("maybe"), LineKind::Dotted)
        );
        assert_eq!((l[10].line, l[10].end), (LineKind::Invisible, End::None));
        let target = |i: usize| chart.nodes[l[i].to].id.as_str();
        assert_eq!(target(8), "J");
        assert_eq!(target(9), "K");
        assert_eq!(target(10), "L");
    }

    #[test]
    fn invisible_links_chain_and_reject_labels() {
        let chart = parse("flowchart LR\n  A ~~~ B ~~~~ C\n").unwrap();
        assert_eq!(chart.links.len(), 2);
        assert!(chart.links.iter().all(|l| l.line == LineKind::Invisible));
        let e = error("flowchart LR\n  A ~~~|x| B\n");
        assert_eq!(e.line, 2);
        assert!(e.message.contains("invisible"), "{e:?}");
    }

    #[test]
    fn ampersands_chains_and_semicolons() {
        let chart = parse("flowchart TD\n  A & B --> C & D --> E; F --> A\n").unwrap();
        // 2×2 + 2×1 + 1
        assert_eq!(chart.links.len(), 7);
        let chart = parse("graph TD; A-->B\n").unwrap();
        assert_eq!(chart.links.len(), 1);
    }

    #[test]
    fn subgraphs_nest_and_own_their_nodes() {
        let chart = parse(
            "flowchart TB\n  subgraph outer [Outer box]\n    A --> B\n    subgraph inner\n      C\n    end\n  end\n  B --> C --> D\n",
        )
        .unwrap();
        assert_eq!(chart.subgraphs.len(), 2);
        assert_eq!(chart.subgraphs[0].title, "Outer box");
        assert_eq!(chart.subgraphs[1].parent, Some(0));
        let parent = |id: &str| node(&chart, id).parent;
        assert_eq!(parent("A"), Some(0));
        assert_eq!(parent("C"), Some(1));
        assert_eq!(parent("D"), None);
    }

    #[test]
    fn classes_and_styles_colour_nodes() {
        let chart = parse(
            "flowchart TD\n  A:::hot --> B --> C\n  classDef hot fill:#f96,stroke:#333,color:#fff\n  class B hot\n  style C fill:#0f0,stroke-dasharray: 5 5\n",
        )
        .unwrap();
        let colors = |id: &str| chart.colors(node(&chart, id));
        assert_eq!(colors("A").fill, Some((0xff, 0x99, 0x66)));
        assert_eq!(colors("B").text, Some((0xff, 0xff, 0xff)));
        let c = colors("C");
        assert_eq!(c.fill, Some((0, 255, 0)));
        assert!(c.dashed);
    }

    #[test]
    fn subgraphs_take_style_and_classes() {
        let chart = parse(
            "flowchart TD\n  style late fill:#eee\n  subgraph one [One]\n    A\n  end\n  subgraph late\n    B\n  end\n  classDef cool fill:#9cf,stroke:#036\n  class one cool\n  style one stroke-dasharray: 4 4,color:#123\n",
        )
        .unwrap();
        // The early `style late` didn't leave a stray node behind.
        assert_eq!(chart.nodes.len(), 2);
        let one = chart.subgraph_colors(&chart.subgraphs[0]);
        assert_eq!(one.fill, Some((0x99, 0xcc, 0xff)));
        assert_eq!(one.stroke, Some((0, 0x33, 0x66)));
        assert_eq!(one.text, Some((0x11, 0x22, 0x33)));
        assert!(one.dashed);
        let late = chart.subgraph_colors(&chart.subgraphs[1]);
        assert_eq!(late.fill, Some((0xee, 0xee, 0xee)));
    }

    #[test]
    fn a_link_to_a_subgraph_is_an_error() {
        let e = error("flowchart TD\n  subgraph one\n    A\n  end\n  B --> one\n");
        assert_eq!(e.line, 5);
        assert!(e.message.contains("subgraph `one`"), "{e:?}");
    }

    #[test]
    fn link_styles() {
        let chart = parse(
            "flowchart TD\n  A --> B --> C\n  C -.-> D\n  linkStyle 0,2 stroke:#ff3,stroke-width:4px,color:red,fill:none\n  linkStyle default stroke:#999,stroke-dasharray: 2 3\n  linkStyle 1 interpolate basis\n",
        )
        .unwrap();
        let style = |i: usize| chart.link_style(&chart.links[i]);
        assert_eq!(style(0).stroke, Some((0xff, 0xff, 0x33)));
        assert_eq!(style(0).width, Some(4.0));
        assert_eq!(style(0).color, Some((255, 0, 0)));
        assert_eq!(style(2), style(0));
        // `interpolate` alone leaves link 1 on the default.
        assert_eq!(style(1).stroke, Some((0x99, 0x99, 0x99)));
        assert_eq!(style(1).dash, Some(Stroke::Dotted));
    }

    #[test]
    fn an_unreadable_colour_or_width_is_an_error() {
        for (source, names) in [
            ("flowchart TD\n  A\n  style A fill:blurple\n", "blurple"),
            (
                "flowchart TD\n  A\n  classDef x stroke:hsl(0,0%,0%)\n",
                "hsl",
            ),
            (
                "flowchart TD\n  A --> B\n  linkStyle 0 stroke-width:thick\n",
                "thick",
            ),
        ] {
            let e = error(source);
            assert_eq!(e.line, 3, "{source}");
            assert!(e.message.contains(names), "{e}");
        }
    }

    #[test]
    fn link_style_mistakes() {
        let e = error("flowchart TD\n  A --> B\n  linkStyle 1 stroke:#f00\n");
        assert_eq!(e.line, 3);
        assert!(e.message.contains("no link 1"), "{e:?}");
        // Links below the statement don't count, as in mermaid.
        let e = error("flowchart TD\n  linkStyle 0 stroke:#f00\n  A --> B\n");
        assert_eq!(e.line, 2);
        let e = error("flowchart TD\n  A --> B\n  linkStyle first stroke:#f00\n");
        assert!(e.message.contains("`first`"), "{e:?}");
        let e = error("flowchart TD\n  A --> B\n  linkStyle 0\n");
        assert!(e.message.contains("needs styles"), "{e:?}");
        let e = error("flowchart TD\n  A --> B\n  linkStyle 0 stroke #f00\n");
        assert!(e.message.contains("property:value"), "{e:?}");
        let e = error("flowchart TD\n  A --> B\n  linkStyle 0 interpolate wiggly\n");
        assert!(e.message.contains("wiggly"), "{e:?}");
    }

    #[test]
    fn subgraph_direction() {
        let chart = parse(
            "flowchart LR\n  subgraph s\n    direction TB\n    A --> B\n  end\n  subgraph t\n    direction LR\n    C\n  end\n",
        )
        .unwrap();
        assert_eq!(chart.subgraphs[0].direction, Some(Direction::TopDown));
        assert_eq!(chart.subgraphs[1].direction, Some(Direction::LeftRight));
        let e = error("flowchart LR\n  subgraph s\n    direction sideways\n  end\n");
        assert_eq!(e.line, 3);
        // At the top level it would do nothing.
        assert!(parse("flowchart LR\n  direction LR\n  A\n").is_ok());
        let e = error("flowchart LR\n  direction TB\n  A\n");
        assert_eq!(e.line, 2);
    }

    #[test]
    fn clicks_are_checked_but_not_drawn() {
        let ok = [
            "click A callback",
            "click A callback \"Tooltip\"",
            "click A call callback()",
            "click A call callback(\"x y\", 2) \"Tooltip\"",
            "click A href \"https://example.com\"",
            "click A href \"https://example.com\" \"Tooltip\" _blank",
            "click A \"https://example.com\"",
            "click A \"https://example.com\" _self",
            "click A,B \"https://example.com\" \"Tip\" _top",
        ];
        for statement in ok {
            let source = format!("flowchart TD\n  A --> B\n  {statement}\n");
            assert!(parse(&source).is_ok(), "{statement}: {:?}", parse(&source));
        }
        let bad = [
            "click",
            "click A",
            "click A href",
            "click A \"url\" _nowhere",
            "click A callback extra",
            "click A \"url\" \"tip\" _blank more",
            "click A call",
        ];
        for statement in bad {
            let source = format!("flowchart TD\n  A --> B\n  {statement}\n");
            let e = parse(&source).unwrap_err();
            assert_eq!(e.line, 3, "{statement}");
        }
        let e = error("flowchart TD\n  A --> B\n  click Q callback\n");
        assert!(e.message.contains("`Q`"), "{e:?}");
    }

    #[test]
    fn accessibility_statements() {
        let chart = parse(
            "flowchart TD\n  accTitle: The title; with a semicolon\n  accDescr {\n    A longer\n    description\n  }\n  A --> B\n",
        )
        .unwrap();
        assert_eq!(
            chart.acc_title.as_deref(),
            Some("The title; with a semicolon")
        );
        assert_eq!(chart.acc_descr.as_deref(), Some("A longer\ndescription"));
        assert_eq!(chart.nodes.len(), 2);
        let chart = parse("flowchart TD\n  accDescr: short\n  A\n").unwrap();
        assert_eq!(chart.acc_descr.as_deref(), Some("short"));
        let chart = parse("flowchart TD\n  accDescr { one line }\n  A\n").unwrap();
        assert_eq!(chart.acc_descr.as_deref(), Some("one line"));
        let e = error("flowchart TD\n  accDescr {\n  never closed\n");
        assert_eq!(e.line, 2);
        let e = error("flowchart TD\n  accTitle the title\n");
        assert_eq!(e.line, 2);
    }

    #[test]
    fn edge_ids_and_edge_data() {
        let chart = parse(
            "flowchart LR\n  A e1@--> B\n  B e2@-- text --> C\n  e1@{ animate: true }\n  e2@{ animation: slow, curve: linear }\n",
        )
        .unwrap();
        assert_eq!(chart.links.len(), 2);
        assert_eq!(chart.links[0].id.as_deref(), Some("e1"));
        assert_eq!(chart.links[1].id.as_deref(), Some("e2"));
        assert_eq!(chart.links[1].label.as_deref(), Some("text"));
        // No node was made for the edge ids.
        assert_eq!(chart.nodes.len(), 3);
        let e = error("flowchart LR\n  A e1@--> B\n  e1@{ wobble: true }\n");
        assert_eq!(e.line, 3);
        assert!(e.message.contains("wobble"), "{e:?}");
        let e = error("flowchart LR\n  A e1@--> B\n  e1@{ animate: maybe }\n");
        assert!(e.message.contains("maybe"), "{e:?}");
    }

    #[test]
    fn v11_shape_syntax() {
        let chart = parse(
            "flowchart TD\n  A@{ shape: rounded, label: \"Hello\" } --> B@{ shape: cyl }\n  C@{ shape: doc } & D@{ shape: lean-l }\n  E[Old]@{ shape: hex }\n  F@{\n    shape: diamond\n    label: \"Multi-line data\"\n  }\n  G@{ shape: sm-circ } --> H@{ shape: text, label: \"note\" }\n",
        )
        .unwrap();
        let n = |id: &str| node(&chart, id);
        assert_eq!(
            (n("A").shape, n("A").label.as_str()),
            (NodeShape::Round, "Hello")
        );
        assert!(n("A").markdown);
        assert_eq!(n("B").shape, NodeShape::Cylinder);
        assert_eq!(n("C").shape, NodeShape::Document);
        assert_eq!(n("D").shape, NodeShape::ParallelogramAlt);
        assert_eq!(
            (n("E").shape, n("E").label.as_str()),
            (NodeShape::Hexagon, "Old")
        );
        assert_eq!(n("F").shape, NodeShape::Rhombus);
        assert_eq!(n("F").label, "Multi-line data");
        assert_eq!(n("G").shape, NodeShape::Dot);
        assert_eq!(n("H").shape, NodeShape::Text);
        assert_eq!(chart.links.len(), 2);
    }

    #[test]
    fn v11_shape_mistakes() {
        let e = error("flowchart TD\n  A@{ shape: wobbly }\n");
        assert_eq!(e.line, 2);
        assert!(e.message.contains("unknown shape `wobbly`"), "{e:?}");
        let e = error("flowchart TD\n  A@{ shape: bolt }\n");
        assert!(e.message.contains("`bolt` is not supported"), "{e:?}");
        let e = error("flowchart TD\n  A@{ icon: \"fa:user\" }\n");
        assert!(e.message.contains("icon"), "{e:?}");
        let e = error("flowchart TD\n  A@{ colour: red }\n");
        assert!(e.message.contains("colour"), "{e:?}");
        let e = error("flowchart TD\n  A@{ shape: rect\n");
        assert_eq!(e.line, 2);
        let e = error("flowchart TD\n  A@{ shape: sm-circ, label: \"Begin\" }\n");
        assert!(e.message.contains("Begin"), "{e:?}");
        let e = error("flowchart TD\n  A[|a:b|text]\n");
        assert_eq!(e.line, 2);
    }

    #[test]
    fn markdown_strings() {
        let chart = parse(
            "flowchart LR\n  A[\"`The **cat**\n    in the hat`\"] -- \"`a *bat*`\" --> B[\"plain **text**\"]\n  subgraph s [\"`**Two**`\"]\n    C\n  end\n  C -->|\"`x`\"| B\n",
        )
        .unwrap();
        let a = node(&chart, "A");
        assert!(a.markdown);
        assert_eq!(a.label, "The **cat**\nin the hat");
        let b = node(&chart, "B");
        assert!(!b.markdown);
        assert_eq!(b.label, "plain **text**");
        assert!(chart.links[0].markdown);
        assert_eq!(chart.links[0].label.as_deref(), Some("a *bat*"));
        assert!(chart.links[1].markdown);
        assert!(chart.subgraphs[0].markdown);
        assert_eq!(chart.subgraphs[0].title, "**Two**");
    }

    #[test]
    fn mistakes_name_their_line() {
        let e = error("flowchart TD\n  A --> \n");
        assert_eq!(e.line, 2);
        let e = error("flowchart TD\n  subgraph x\n  A\n");
        assert!(e.message.contains("no `end`"), "{e:?}");
        let e = error("flowchart XY\n");
        assert!(e.message.contains("direction"));
        let e = error("flowchart TD extra\n");
        assert!(e.message.contains("extra"));
        let e = error("flowchart TD\n  A\n  title Nope\n");
        assert_eq!(e.line, 3);
        let e = error("flowchart TD\n  A:::\n");
        assert_eq!(e.line, 2);
    }
}
