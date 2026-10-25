//! The treemap grammar: `treemap/treemap.langium` upstream, with the
//! hierarchy built as `treemap/utils.ts` builds it — only sections take
//! children, so an item indented under a leaf belongs to the leaf's section.

use std::collections::HashMap;

use crate::Error;
use crate::meta::{self, Meta};
use crate::style::{self, Colors};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Treemap {
    pub title: Option<String>,
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
    pub items: Vec<Item>,
    /// The outermost items, in order.
    pub roots: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub name: String,
    /// A leaf's own value; `None` for a section.
    pub value: Option<f64>,
    /// The value as written, for display.
    pub shown: String,
    pub children: Vec<usize>,
    pub colors: Colors,
    /// Where it is declared.
    pub line: usize,
}

impl Treemap {
    /// A leaf's value, or the sum of a section's leaves.
    pub fn total(&self, item: usize) -> f64 {
        let it = &self.items[item];
        match it.value {
            Some(v) => v,
            None => it.children.iter().map(|&c| self.total(c)).sum(),
        }
    }
}

pub fn parse(source: &str) -> Result<Treemap, Error> {
    let lines = meta::lines(source);
    meta::header(&lines, &["treemap-beta", "treemap"])?;
    // Classes first, as upstream reads them before any item.
    let mut classes: HashMap<String, Colors> = HashMap::new();
    for &(number, line) in &lines[1..] {
        if let Some(rest) = line.trim().strip_prefix("classDef")
            && rest.starts_with([' ', '\t'])
        {
            let rest = rest.trim().trim_end_matches(';');
            let (name, styles) = rest.split_once([' ', '\t']).unwrap_or((rest, ""));
            let colors = style::colors(&style::declarations(styles, number)?, number)?;
            classes.entry(name.to_string()).or_default().merge(&colors);
        }
    }

    let mut map = Treemap::default();
    // (indent, item) for the open sections.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut at = 1;
    while at < lines.len() {
        if let Some(m) = meta::meta(&lines, &mut at)? {
            match m {
                Meta::Title(t) => map.title = Some(t),
                Meta::AccTitle(t) => map.acc_title = Some(t),
                Meta::AccDescr(t) => map.acc_descr = Some(t),
            }
            continue;
        }
        let (number, line) = lines[at];
        at += 1;
        if line.trim().starts_with("classDef") {
            continue;
        }
        let level = meta::indent(line);
        let item = item(line.trim(), number, &classes)?;
        let is_section = item.value.is_none();
        while stack.last().is_some_and(|&(indent, _)| indent >= level) {
            stack.pop();
        }
        let index = map.items.len();
        map.items.push(item);
        match stack.last() {
            Some(&(_, parent)) => map.items[parent].children.push(index),
            None => map.roots.push(index),
        }
        if is_section {
            stack.push((level, index));
        }
    }
    // An item with nothing to size it by would be drawn as nothing. Leaves
    // first: a zero leaf is the cause of an empty section.
    let mut order: Vec<usize> = (0..map.items.len()).collect();
    order.sort_by_key(|&i| map.items[i].value.is_none());
    for index in order {
        let it = &map.items[index];
        if map.total(index) <= 0.0 {
            let what = if it.value.is_some() {
                "has no area: its value is 0"
            } else {
                "has no leaves with a value, so it has no area"
            };
            return Err(Error::new(it.line, format!("`{}` {what}", it.name)));
        }
    }
    Ok(map)
}

/// `"Name"`, `"Name":::class`, `"Name": 12` or `"Name", 12:::class`.
fn item(text: &str, number: usize, classes: &HashMap<String, Colors>) -> Result<Item, Error> {
    let (name, rest) = meta::quoted(text).ok_or_else(|| {
        Error::new(
            number,
            format!("expected a quoted name such as `\"Item\": 10`, found `{text}`"),
        )
    })?;
    let mut rest = rest.trim();
    let mut class = None;
    if let Some((before, after)) = rest.split_once(":::") {
        class = Some(after.trim());
        rest = before.trim();
    }
    let (value, shown) = if rest.is_empty() {
        (None, String::new())
    } else {
        let digits = rest
            .strip_prefix(':')
            .or_else(|| rest.strip_prefix(','))
            .ok_or_else(|| Error::new(number, format!("unexpected `{rest}` after `\"{name}\"`")))?
            .trim();
        let plain: String = digits.chars().filter(|c| !matches!(c, '_' | ',')).collect();
        let value = plain
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or_else(|| Error::new(number, format!("`{digits}` is not a value")))?;
        (Some(value), digits.to_string())
    };
    let colors = match class {
        Some(c) => *classes.get(c).ok_or_else(|| {
            Error::new(
                number,
                format!("class `{c}` is not defined by a `classDef`"),
            )
        })?,
        None => Colors::default(),
    };
    Ok(Item {
        name: name.to_string(),
        value,
        shown,
        children: Vec::new(),
        colors,
        line: number,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_and_leaves_nest_by_indent() {
        let t = parse(
            "treemap-beta\n\"A\"\n  \"A1\": 10\n  \"A2\", 1,000:::hot\n\"B\":::hot\n    \"B1\": 5\nclassDef hot fill:#f00;\n",
        )
        .unwrap();
        assert_eq!(t.roots, vec![0, 3]);
        assert_eq!(t.items[0].children, vec![1, 2]);
        assert_eq!(t.items[2].value, Some(1000.0));
        assert_eq!(t.items[2].colors.fill, Some((255, 0, 0)));
        assert_eq!(t.items[3].colors.fill, Some((255, 0, 0)));
        assert_eq!(t.total(0), 1010.0);
    }

    #[test]
    fn items_under_a_leaf_belong_to_its_section() {
        let t = parse("treemap\n\"S\"\n  \"L\": 1\n    \"M\": 2\n").unwrap();
        assert_eq!(t.items[0].children, vec![1, 2]);
    }

    #[test]
    fn mistakes_name_their_line() {
        for (source, line, needle) in [
            ("treemap\nA: 1\n", 2, "quoted"),
            ("treemap\n\"A\": x\n", 2, "not a value"),
            ("treemap\n\"A\" 3\n", 2, "unexpected"),
            ("treemap\n\"A\": 3:::nope\n", 2, "nope"),
            ("treemap\n\"A\"\n\"B\": 2\n", 2, "no leaves"),
            ("treemap\n\"S\"\n  \"A\": 0\n", 3, "value is 0"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
