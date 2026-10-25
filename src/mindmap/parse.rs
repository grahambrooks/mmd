//! The mindmap grammar: `mindmap/parser/mindmap.jison` upstream, with
//! levels and shapes assigned as `mindmapDb.ts` assigns them.

use crate::Error;
use crate::meta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeShape {
    /// No brackets: upstream draws text over an underline.
    Default,
    /// `[square]`
    Rect,
    /// `(rounded)`
    Rounded,
    /// `((circle))`
    Circle,
    /// `)cloud(`
    Cloud,
    /// `))bang((`
    Bang,
    /// `{{hexagon}}`
    Hexagon,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    /// The label was a markdown string (`` "`…`" ``).
    pub markdown: bool,
    pub shape: NodeShape,
    /// `::icon(fa fa-book)`: recorded, not drawn (no icon fonts).
    pub icon: Option<String>,
    /// `:::urgent large`: CSS classes upstream leaves to the page's
    /// stylesheet; recorded, not drawn.
    pub classes: Vec<String>,
    pub level: usize,
    pub children: Vec<usize>,
}

/// Node 0 is the root.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mindmap {
    pub nodes: Vec<Node>,
}

pub fn parse(source: &str) -> Result<Mindmap, Error> {
    let lines = meta::lines(source);
    meta::header(&lines, &["mindmap"])?;
    let mut map = Mindmap::default();
    let mut base = 0;
    for &(number, line) in &lines[1..] {
        let indent = meta::indent(line);
        let mut text = line.trim();
        if let Some(rest) = decoration(text) {
            let node = map.nodes.last_mut().ok_or_else(|| {
                Error::new(number, "an icon or class must follow the node it decorates")
            })?;
            decorate(node, rest, number)?;
            continue;
        }
        // A trailing decoration on the node's own line.
        // The first `:::`/`::icon(` that follows a whole node, so one inside
        // a label stays part of it.
        let mut trailing = None;
        let split = text
            .char_indices()
            .map(|(i, _)| i)
            .filter(|&i| text[i..].starts_with(":::") || text[i..].starts_with("::icon("))
            .find(|&i| i > 0 && node(text[..i].trim_end(), number).is_ok());
        if let Some(at) = split {
            trailing = Some(&text[at..]);
            text = text[..at].trim_end();
        }
        let mut node = node(text, number)?;
        if let Some(rest) = trailing {
            decorate(&mut node, rest, number)?;
        }
        if map.nodes.is_empty() {
            base = indent;
        }
        node.level = indent.saturating_sub(base);
        if !map.nodes.is_empty() {
            let parent = map
                .nodes
                .iter()
                .rposition(|n| n.level < node.level)
                .ok_or_else(|| {
                    Error::new(
                        number,
                        format!(
                            "`{}` is not indented under the root; a mindmap has one root",
                            node.label
                        ),
                    )
                })?;
            let index = map.nodes.len();
            map.nodes[parent].children.push(index);
        }
        map.nodes.push(node);
    }
    if map.nodes.is_empty() {
        return Err(Error::new(
            lines.first().map_or(1, |l| l.0),
            "a mindmap needs a root node",
        ));
    }
    Ok(map)
}

/// The rest of a line that is only an icon or a class list.
fn decoration(text: &str) -> Option<&str> {
    (text.starts_with("::icon(") || text.starts_with(":::")).then_some(text)
}

/// `::icon(…)` or `:::a b`, possibly several in a row.
fn decorate(node: &mut Node, mut text: &str, number: usize) -> Result<(), Error> {
    loop {
        text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        if let Some(rest) = text.strip_prefix("::icon(") {
            let (icon, after) = rest
                .split_once(')')
                .ok_or_else(|| Error::new(number, "`::icon(` has no closing `)`"))?;
            node.icon = Some(icon.trim().to_string());
            text = after;
        } else if let Some(rest) = text.strip_prefix(":::") {
            let end = rest.find("::").unwrap_or(rest.len());
            node.classes
                .extend(rest[..end].split_whitespace().map(str::to_string));
            text = &rest[end..];
        } else {
            return Err(Error::new(
                number,
                format!("unexpected `{text}`; expected `::icon(…)` or `:::class`"),
            ));
        }
    }
}

/// `id`, `id[label]`, `[label]`, `id(("label"))`, `` id["`**md**`"] ``…
fn node(text: &str, number: usize) -> Result<Node, Error> {
    let plain = |id: &str, label: String, markdown, shape| Node {
        id: id.to_string(),
        label,
        markdown,
        shape,
        icon: None,
        classes: Vec::new(),
        level: 0,
        children: Vec::new(),
    };
    let Some(open_at) = text.find(['(', '[', ')', '{']) else {
        return Ok(plain(text, text.to_string(), false, NodeShape::Default));
    };
    let id = text[..open_at].trim();
    let rest = &text[open_at..];
    let (opener, closer, shape) = [
        ("((", "))", NodeShape::Circle),
        ("))", "((", NodeShape::Bang),
        ("{{", "}}", NodeShape::Hexagon),
        ("(", ")", NodeShape::Rounded),
        (")", "(", NodeShape::Cloud),
        ("[", "]", NodeShape::Rect),
    ]
    .into_iter()
    .find(|(o, _, _)| rest.starts_with(o))
    .ok_or_else(|| {
        Error::new(
            number,
            format!("`{rest}` is not a mindmap node shape (see DESIGN.md)"),
        )
    })?;
    let body = &rest[opener.len()..];
    let unclosed = || {
        Error::new(
            number,
            format!("`{opener}` is not closed by `{closer}` in `{text}`"),
        )
    };
    let (label, markdown, after) = if let Some(md) = body.strip_prefix("\"`") {
        let end = md.find("`\"").ok_or_else(unclosed)?;
        (md[..end].to_string(), true, &md[end + 2..])
    } else if let Some((inside, after)) = meta::quoted(body).filter(|_| body.starts_with('"')) {
        (inside.to_string(), false, after)
    } else {
        let end = body.rfind(closer).ok_or_else(unclosed)?;
        (body[..end].to_string(), false, &body[end..])
    };
    let after = after
        .trim_start()
        .strip_prefix(closer)
        .ok_or_else(unclosed)?;
    if !after.trim().is_empty() {
        return Err(Error::new(
            number,
            format!("unexpected `{}` after the node", after.trim()),
        ));
    }
    let label = label
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let id = if id.is_empty() { label.as_str() } else { id };
    Ok(plain(id, label.trim().to_string(), markdown, shape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_ids_and_labels() {
        let m = parse(
            "mindmap\n  root((Release))\n    a[Square]\n    b(Rounded)\n    c))Bang((\n    d)Cloud(\n    e{{Hex}}\n    plain text\n    [No id]\n    f[\"Quoted (with) brackets\"]\n    g[\"`**bold** md`\"]\n",
        )
        .unwrap();
        let got: Vec<(&str, &str, NodeShape)> = m
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.label.as_str(), n.shape))
            .collect();
        assert_eq!(
            got,
            vec![
                ("root", "Release", NodeShape::Circle),
                ("a", "Square", NodeShape::Rect),
                ("b", "Rounded", NodeShape::Rounded),
                ("c", "Bang", NodeShape::Bang),
                ("d", "Cloud", NodeShape::Cloud),
                ("e", "Hex", NodeShape::Hexagon),
                ("plain text", "plain text", NodeShape::Default),
                ("No id", "No id", NodeShape::Rect),
                ("f", "Quoted (with) brackets", NodeShape::Rect),
                ("g", "**bold** md", NodeShape::Rect),
            ]
        );
        assert!(m.nodes[9].markdown);
        assert_eq!(m.nodes[0].children.len(), 9);
    }

    #[test]
    fn indentation_nests_from_the_roots_indent() {
        let m = parse("mindmap\n    root\n      a\n        a1\n      b\n").unwrap();
        assert_eq!(m.nodes[0].children, vec![1, 3]);
        assert_eq!(m.nodes[1].children, vec![2]);
    }

    #[test]
    fn icons_and_classes_decorate_the_node_before() {
        let m = parse(
            "mindmap\nroot\n  a\n  ::icon(fa fa-book)\n  :::urgent large\n  b :::hot\n  c[std::vec] ::icon(x)\n",
        )
        .unwrap();
        assert_eq!(m.nodes[1].icon.as_deref(), Some("fa fa-book"));
        assert_eq!(m.nodes[1].classes, vec!["urgent", "large"]);
        assert_eq!(m.nodes[2].label, "b");
        assert_eq!(m.nodes[2].classes, vec!["hot"]);
        assert_eq!(m.nodes[3].label, "std::vec");
        assert_eq!(m.nodes[3].icon.as_deref(), Some("x"));
    }

    #[test]
    fn mistakes_name_their_line() {
        for (source, line, needle) in [
            ("mindmap\n  root\n  second\n", 3, "one root"),
            ("mindmap\n  root\n    a[open\n", 3, "not closed"),
            ("mindmap\n  root\n    a[x] y\n", 3, "unexpected"),
            ("mindmap\n  ::icon(x)\n", 2, "follow"),
            ("mindmap\n", 1, "root"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
