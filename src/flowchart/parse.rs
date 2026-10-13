//! Flowchart syntax: `flowchart`/`graph` with a direction, nodes in any of
//! Mermaid's shapes, links of every weight with labels, chains and `&`
//! lists, nested subgraphs, and `classDef`/`class`/`:::`/`style` colours.

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
    Asymmetric,
    Rhombus,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Solid,
    Dotted,
    Thick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    None,
    Arrow,
    Circle,
    Cross,
}

/// Colours set in the source, as `#rrggbb` triples.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Colors {
    pub fill: Option<(u8, u8, u8)>,
    pub stroke: Option<(u8, u8, u8)>,
    pub text: Option<(u8, u8, u8)>,
    pub dashed: bool,
}

impl Colors {
    /// `other`'s colours over these.
    fn merge(&mut self, other: &Colors) {
        self.fill = other.fill.or(self.fill);
        self.stroke = other.stroke.or(self.stroke);
        self.text = other.text.or(self.text);
        self.dashed |= other.dashed;
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: NodeShape,
    /// Innermost subgraph, if any.
    pub parent: Option<usize>,
    pub classes: Vec<String>,
    pub style: Colors,
}

#[derive(Debug, Clone)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
    pub line: LineKind,
    pub start: End,
    pub end: End,
}

#[derive(Debug, Clone)]
pub struct Subgraph {
    pub id: String,
    pub title: String,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Flowchart {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub subgraphs: Vec<Subgraph>,
    pub classes: Vec<(String, Colors)>,
}

impl Flowchart {
    /// A node's colours: the `default` class, its classes, then `style`.
    pub fn colors(&self, node: &Node) -> Colors {
        let mut out = Colors::default();
        let class = |name: &str| {
            self.classes
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, c)| *c)
        };
        if let Some(c) = class("default") {
            out.merge(&c);
        }
        for name in &node.classes {
            if let Some(c) = class(name) {
                out.merge(&c);
            }
        }
        out.merge(&node.style);
        out
    }
}

pub fn parse(source: &str) -> Result<Flowchart, Error> {
    let mut chart = Flowchart {
        direction: Direction::TopDown,
        nodes: Vec::new(),
        links: Vec::new(),
        subgraphs: Vec::new(),
        classes: Vec::new(),
    };
    let mut header = false;
    let mut stack: Vec<usize> = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if !header {
            let mut words = line.split_whitespace();
            match words.next() {
                Some("flowchart" | "graph") => {}
                _ => return Err(Error::new(number, "expected `flowchart` or `graph`")),
            }
            if let Some(dir) = words.next() {
                chart.direction = direction(dir)
                    .ok_or_else(|| Error::new(number, format!("unknown direction `{dir}`")))?;
            }
            header = true;
            continue;
        }
        for statement in split_statements(line) {
            statement_into(&mut chart, &mut stack, statement.trim(), number)?;
        }
    }
    if !header {
        return Err(Error::new(1, "expected `flowchart` or `graph`"));
    }
    if let Some(&open) = stack.last() {
        return Err(Error::new(
            source.lines().count(),
            format!("subgraph `{}` has no `end`", chart.subgraphs[open].id),
        ));
    }
    Ok(chart)
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

fn statement_into(
    chart: &mut Flowchart,
    stack: &mut Vec<usize>,
    statement: &str,
    number: usize,
) -> Result<(), Error> {
    let keyword = statement.split_whitespace().next().unwrap_or("");
    let rest = statement[keyword.len()..].trim();
    match keyword {
        "subgraph" => {
            let (id, title) = subgraph_title(rest, chart.subgraphs.len());
            chart.subgraphs.push(Subgraph {
                id,
                title,
                parent: stack.last().copied(),
            });
            stack.push(chart.subgraphs.len() - 1);
            Ok(())
        }
        "end" => stack
            .pop()
            .map(|_| ())
            .ok_or_else(|| Error::new(number, "`end` without a `subgraph`")),
        // Layout hints and interactions the drawing doesn't use.
        "direction" | "linkStyle" | "click" | "accTitle" | "accDescr" => Ok(()),
        "classDef" => {
            let mut words = rest.splitn(2, char::is_whitespace);
            let names = words.next().unwrap_or("");
            let colors = css(words.next().unwrap_or(""));
            for name in names.split(',') {
                chart.classes.push((name.trim().to_string(), colors));
            }
            Ok(())
        }
        "class" => {
            let mut words = rest.rsplitn(2, char::is_whitespace);
            let class = words.next().unwrap_or("").to_string();
            for id in words.next().unwrap_or("").split(',') {
                let node = node_index(chart, id.trim(), stack);
                chart.nodes[node].classes.push(class.clone());
            }
            Ok(())
        }
        "style" => {
            let mut words = rest.splitn(2, char::is_whitespace);
            let id = words.next().unwrap_or("");
            let colors = css(words.next().unwrap_or(""));
            if let Some(sub) = chart.subgraphs.iter().position(|s| s.id == id) {
                let _ = sub; // subgraph styles aren't drawn
                return Ok(());
            }
            let node = node_index(chart, id, stack);
            chart.nodes[node].style.merge(&colors);
            Ok(())
        }
        _ => chain(chart, stack, statement, number),
    }
}

/// `id [title]`, `id["title"]` or just `title`.
fn subgraph_title(rest: &str, index: usize) -> (String, String) {
    if rest.is_empty() {
        let id = format!("subgraph{index}");
        return (id.clone(), String::new());
    }
    if let Some(open) = rest.find('[') {
        let id = rest[..open].trim().to_string();
        let title = rest[open + 1..].trim_end_matches(']').trim();
        return (id, unquote(title));
    }
    let title = unquote(rest);
    let id = if rest.contains(char::is_whitespace) || rest.starts_with('"') {
        format!("subgraph{index}")
    } else {
        rest.to_string()
    };
    (id, title)
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
        let link = p.link()?;
        let next = group(chart, stack, &mut p)?;
        for &from in &previous {
            for &to in &next {
                chart.links.push(Link {
                    from,
                    to,
                    label: link.label.clone(),
                    line: link.line,
                    start: link.start,
                    end: link.end,
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

const SHAPES: [(&str, &str, NodeShape); 14] = [
    ("(((", ")))", NodeShape::DoubleCircle),
    ("((", "))", NodeShape::Circle),
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

/// A node: an id, then perhaps a shape with its label and `:::class`.
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
    let index = node_index(chart, &id, stack);
    // A shape: the longest opener that has its closer (`[[x]]` is a
    // subroutine, not `[` `]`); of shapes sharing an opener, the one whose
    // closer comes first (`[/a\]` is a trapezoid even with `/]` later on).
    let rest = &p.s[p.i..];
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
        chart.nodes[index].label = label_text(&rest[open.len()..open.len() + end]);
        chart.nodes[index].shape = shape;
        p.i += finish;
    }
    if p.eat(":::") {
        let start = p.i;
        while let Some(c) = p.peek() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                p.i += c.len_utf8();
            } else {
                break;
            }
        }
        chart.nodes[index].classes.push(p.s[start..p.i].to_string());
    }
    Ok(index)
}

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

fn label_text(raw: &str) -> String {
    unquote(raw.trim())
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace("<br />", "\n")
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let s = s
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s);
    let s = s
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(s);
    s.to_string()
}

/// The node with `id`, made (in the innermost open subgraph) if new.
fn node_index(chart: &mut Flowchart, id: &str, stack: &[usize]) -> usize {
    if let Some(i) = chart.nodes.iter().position(|n| n.id == id) {
        return i;
    }
    chart.nodes.push(Node {
        id: id.to_string(),
        label: id.to_string(),
        shape: NodeShape::Rect,
        parent: stack.last().copied(),
        classes: Vec::new(),
        style: Colors::default(),
    });
    chart.nodes.len() - 1
}

/// CSS-ish `fill:#f9f,stroke:#333,color:#fff,stroke-dasharray:5 5`.
fn css(text: &str) -> Colors {
    let mut out = Colors::default();
    for part in text.split([',', ';']) {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "fill" | "background" => out.fill = color(value),
            "stroke" => out.stroke = color(value),
            "color" => out.text = color(value),
            "stroke-dasharray" => out.dashed = true,
            _ => {}
        }
    }
    out
}

/// `#rgb`, `#rrggbb` or a few names.
fn color(value: &str) -> Option<(u8, u8, u8)> {
    let hex = |s: &str| u8::from_str_radix(s, 16).ok();
    if let Some(h) = value.strip_prefix('#') {
        return match h.len() {
            3 | 4 => {
                let d = |i: usize| hex(&h[i..i + 1]).map(|v| v * 17);
                Some((d(0)?, d(1)?, d(2)?))
            }
            6 | 8 => Some((hex(&h[0..2])?, hex(&h[2..4])?, hex(&h[4..6])?)),
            _ => None,
        };
    }
    Some(match value.to_ascii_lowercase().as_str() {
        "white" => (255, 255, 255),
        "black" => (0, 0, 0),
        "red" => (220, 38, 38),
        "green" => (22, 163, 74),
        "blue" => (37, 99, 235),
        "yellow" => (250, 204, 21),
        "orange" => (249, 115, 22),
        "purple" => (147, 51, 234),
        "gray" | "grey" => (128, 128, 128),
        _ => return None,
    })
}

struct Cursor<'a> {
    s: &'a str,
    i: usize,
    number: usize,
}

struct ParsedLink {
    label: Option<String>,
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
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn error(&self, message: &str) -> Error {
        let near: String = self.s[self.i..].chars().take(12).collect();
        Error::new(self.number, format!("{message} near `{near}`"))
    }

    /// A link: `-->`, `---`, `-.->`, `==>`, `--o`, `--x`, `<-->`, with
    /// `|label|` after or `-- label -->` inside.
    fn link(&mut self) -> Result<ParsedLink, Error> {
        let rest = &self.s[self.i..];
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
        let mut consumed = body_end;
        let mut whole = body.to_string();
        // `-- text -->`: an opener, text, then the rest of the link.
        if matches!(body, "--" | "==" | "-.") && rest[body_end..].starts_with(char::is_whitespace) {
            let after = &rest[body_end..];
            let close = find_link_end(after).ok_or_else(|| self.error("unterminated link text"))?;
            label = Some(after[..close.0].trim().to_string());
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
            label = Some(label_text(&self.s[start..start + close]));
            self.i = start + close + 1;
        }
        Ok(ParsedLink {
            label: label.filter(|l| !l.is_empty()),
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

    #[test]
    fn shapes_and_labels() {
        let chart = parse(
            "flowchart LR\n  A[Rect] --> B(Round) --> C([Stadium]) --> D[[Sub]]\n  E[(DB)] --> F((Circle)) --> G{Decide} --> H{{Hex}}\n  K[/Trap\\] --> I[/In/] --> J[\\Out\\] --> L>Flag]\n  M[\"Quoted [x]\"]\n",
        )
        .unwrap();
        assert_eq!(chart.direction, Direction::LeftRight);
        let shape = |id: &str| chart.nodes.iter().find(|n| n.id == id).unwrap().shape;
        assert_eq!(shape("A"), NodeShape::Rect);
        assert_eq!(shape("C"), NodeShape::Stadium);
        assert_eq!(shape("E"), NodeShape::Cylinder);
        assert_eq!(shape("F"), NodeShape::Circle);
        assert_eq!(shape("G"), NodeShape::Rhombus);
        assert_eq!(shape("H"), NodeShape::Hexagon);
        assert_eq!(shape("I"), NodeShape::Parallelogram);
        assert_eq!(shape("K"), NodeShape::Trapezoid);
        assert_eq!(shape("L"), NodeShape::Asymmetric);
        let m = chart.nodes.iter().find(|n| n.id == "M").unwrap();
        assert_eq!(m.label, "Quoted [x]");
        assert_eq!(chart.links.len(), 9);
    }

    #[test]
    fn links_of_every_kind() {
        let chart = parse(
            "graph TD\n  A --> B\n  A --- C\n  A -.-> D\n  A ==> E\n  A --o F\n  A --x G\n  A <--> H\n  A -->|yes| I\n  A -- no --> J\n  A -. maybe .-> K\n",
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
        let target = |i: usize| chart.nodes[l[i].to].id.as_str();
        assert_eq!(target(8), "J");
        assert_eq!(target(9), "K");
    }

    #[test]
    fn ampersands_chains_and_semicolons() {
        let chart = parse("flowchart TD\n  A & B --> C & D --> E; F --> A\n").unwrap();
        // 2×2 + 2×1 + 1
        assert_eq!(chart.links.len(), 7);
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
        let parent = |id: &str| chart.nodes.iter().find(|n| n.id == id).unwrap().parent;
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
        let colors = |id: &str| chart.colors(chart.nodes.iter().find(|n| n.id == id).unwrap());
        assert_eq!(colors("A").fill, Some((0xff, 0x99, 0x66)));
        assert_eq!(colors("B").text, Some((0xff, 0xff, 0xff)));
        let c = colors("C");
        assert_eq!(c.fill, Some((0, 255, 0)));
        assert!(c.dashed);
    }

    #[test]
    fn mistakes_name_their_line() {
        let e = parse("flowchart TD\n  A --> \n").unwrap_err();
        assert_eq!(e.line, 2);
        let e = parse("flowchart TD\n  subgraph x\n  A\n").unwrap_err();
        assert!(e.message.contains("no `end`"), "{e:?}");
        let e = parse("flowchart XY\n").unwrap_err();
        assert!(e.message.contains("direction"));
    }
}
