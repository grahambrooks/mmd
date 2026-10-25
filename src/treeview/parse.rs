//! The treeView grammar: `treeView/treeView.langium` upstream. Structure
//! is indentation only; each line is one quoted name.

use crate::Error;
use crate::meta::{self, Meta};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TreeView {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    /// Entry 0 is the implicit root, `/`, as upstream draws it.
    pub entries: Vec<Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub depth: usize,
    pub children: Vec<usize>,
}

pub fn parse(source: &str) -> Result<TreeView, Error> {
    let lines = meta::lines(source);
    meta::header(&lines, &["treeView-beta"])?;
    let mut tree = TreeView {
        entries: vec![Entry {
            name: "/".into(),
            depth: 0,
            children: Vec::new(),
        }],
        ..TreeView::default()
    };
    // (indent, entry) from the root down; the root's indent is below any.
    let mut stack: Vec<(isize, usize)> = vec![(-1, 0)];
    let mut at = 1;
    while at < lines.len() {
        match meta::meta(&lines, &mut at)? {
            Some(Meta::Title(t)) => tree.title = Some(t),
            Some(Meta::AccTitle(t)) => tree.acc_title = Some(t),
            Some(Meta::AccDescr(t)) => tree.acc_descr = Some(t),
            None => {
                let (number, line) = lines[at];
                let level = meta::indent(line) as isize;
                let text = line.trim();
                let name = match meta::quoted(text) {
                    Some((name, rest)) if rest.trim().is_empty() => name.to_string(),
                    Some((_, rest)) => {
                        return Err(Error::new(
                            number,
                            format!("unexpected `{}` after the name", rest.trim()),
                        ));
                    }
                    // Upstream requires quotes; a bare name is read whole.
                    None => text.to_string(),
                };
                while stack.last().is_some_and(|&(indent, _)| indent >= level) {
                    stack.pop();
                }
                let &(_, parent) = stack.last().expect("the root never pops");
                let index = tree.entries.len();
                tree.entries.push(Entry {
                    name,
                    depth: tree.entries[parent].depth + 1,
                    children: Vec::new(),
                });
                tree.entries[parent].children.push(index);
                stack.push((level, index));
                at += 1;
            }
        }
    }
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indentation_nests() {
        let t = parse("treeView-beta\n  \"packages\"\n    \"mermaid\"\n      \"src\"\n    'parser'\n  README.md\n").unwrap();
        let names: Vec<(&str, usize)> = t
            .entries
            .iter()
            .map(|e| (e.name.as_str(), e.depth))
            .collect();
        assert_eq!(
            names,
            vec![
                ("/", 0),
                ("packages", 1),
                ("mermaid", 2),
                ("src", 3),
                ("parser", 2),
                ("README.md", 1)
            ]
        );
        assert_eq!(t.entries[0].children, vec![1, 5]);
        assert_eq!(t.entries[1].children, vec![2, 4]);
    }

    #[test]
    fn mistakes_name_their_line() {
        let error = parse("treeView-beta\n  \"a\" b\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("`b`"));
    }
}
