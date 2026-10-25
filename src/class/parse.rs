//! Class diagram syntax: `classDiagram` (and `classDiagram-v2`) with
//! classes and their members (in `class X { … }` blocks or `X : member`
//! lines), generics, annotations, labels, every relation kind in both
//! directions with labels and cardinalities, lollipop interfaces,
//! namespaces, notes, `direction` and `classDef`/`cssClass`/`:::`/`style`.
//! `click`, `link` and `callback` are accepted and not drawn.
//!
//! Grammar checked against mermaid's
//! `packages/mermaid/src/diagrams/class/parser/classDiagram.jison`, member
//! handling against `classTypes.ts`.

pub use crate::graph::Direction;
pub use crate::style::Colors;
use crate::{Error, strip_comment};

/// `$` (static) or `*` (abstract) at the end of a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classifier {
    None,
    Static,
    Abstract,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// As mermaid shows it: visibility, name, `(params)`, ` : return`.
    pub text: String,
    pub classifier: Classifier,
}

#[derive(Debug, Clone)]
pub struct Class {
    pub id: String,
    /// `class X["label"]`, else the id; generics shown as `<T>`.
    pub label: String,
    /// Without the `<< >>`.
    pub annotations: Vec<String>,
    pub attributes: Vec<Member>,
    pub methods: Vec<Member>,
    pub namespace: Option<usize>,
    /// The circle end of a lollipop (`()--`), not a class box.
    pub interface: bool,
    pub classes: Vec<String>,
    pub style: Colors,
}

#[derive(Debug, Clone)]
pub struct Namespace {
    /// Qualified: `a.b`.
    pub id: String,
    pub label: String,
    pub parent: Option<usize>,
}

/// What a relation's end is drawn with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    None,
    /// `<|` / `|>`
    Inheritance,
    /// `*`
    Composition,
    /// `o`
    Aggregation,
    /// `<` / `>`
    Dependency,
    /// `()`
    Lollipop,
}

#[derive(Debug, Clone)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub from_head: Head,
    pub to_head: Head,
    /// `..` rather than `--`.
    pub dashed: bool,
    pub label: Option<String>,
    /// The quoted strings by each end: `"1" --> "*"`.
    pub from_card: Option<String>,
    pub to_card: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub text: String,
    /// `note for X`; `None` for a free note.
    pub class: Option<usize>,
    pub namespace: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ClassDiagram {
    pub direction: Direction,
    pub classes: Vec<Class>,
    pub namespaces: Vec<Namespace>,
    pub relations: Vec<Relation>,
    pub notes: Vec<Note>,
    pub styles: Vec<(String, Colors)>,
    /// Stored, not drawn.
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

impl ClassDiagram {
    /// A class's colours: the `default` class, its classes, then `style`.
    pub fn colors(&self, class: &Class) -> Colors {
        let mut out = Colors::default();
        let style = |name: &str| self.styles.iter().find(|(n, _)| n == name).map(|(_, c)| *c);
        if let Some(c) = style("default") {
            out.merge(&c);
        }
        for name in &class.classes {
            if let Some(c) = style(name) {
                out.merge(&c);
            }
        }
        out.merge(&class.style);
        out
    }
}

struct Parser {
    d: ClassDiagram,
    /// Open namespaces, innermost last: (namespace, line opened).
    namespaces: Vec<(usize, usize)>,
}

impl Parser {
    /// The class `name` (`Name` or `Name~T~`), made if new in the current
    /// namespace.
    fn class(&mut self, name: &Name) -> usize {
        if let Some(i) = self.d.classes.iter().position(|c| c.id == name.id) {
            if let Some(generic) = &name.generic
                && self.d.classes[i].label == name.id
            {
                self.d.classes[i].label = format!("{}<{}>", name.id, generics(generic));
            }
            return i;
        }
        let label = match &name.generic {
            Some(g) => format!("{}<{}>", name.id, generics(g)),
            None => name.id.clone(),
        };
        self.d.classes.push(Class {
            id: name.id.clone(),
            label,
            annotations: Vec::new(),
            attributes: Vec::new(),
            methods: Vec::new(),
            namespace: self.namespaces.last().map(|&(n, _)| n),
            interface: false,
            classes: Vec::new(),
            style: Colors::default(),
        });
        self.d.classes.len() - 1
    }

    fn member(&mut self, class: usize, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if let Some(annotation) = text.strip_prefix("<<").and_then(|t| t.strip_suffix(">>")) {
            self.d.classes[class]
                .annotations
                .push(annotation.trim().to_string());
        } else if text.find(')').is_some_and(|at| at > 0) {
            self.d.classes[class].methods.push(method(text));
        } else {
            self.d.classes[class].attributes.push(attribute(text));
        }
    }

    /// A namespace by its qualified id, made with its ancestors if new.
    fn namespace(&mut self, id: &str, label: Option<String>) -> usize {
        let mut parent: Option<usize> = None;
        let parts: Vec<&str> = id.split('.').collect();
        for k in 0..parts.len() {
            let qualified = parts[..=k].join(".");
            let leaf = k + 1 == parts.len();
            let found = self.d.namespaces.iter().position(|n| n.id == qualified);
            let at = match found {
                Some(at) => at,
                None => {
                    self.d.namespaces.push(Namespace {
                        id: qualified,
                        label: parts[k].to_string(),
                        parent,
                    });
                    self.d.namespaces.len() - 1
                }
            };
            if leaf && let Some(label) = &label {
                self.d.namespaces[at].label = label.clone();
            }
            parent = Some(at);
        }
        parent.expect("a namespace id has a part")
    }
}

pub fn parse(source: &str) -> Result<ClassDiagram, Error> {
    let mut p = Parser {
        d: ClassDiagram {
            direction: Direction::TopDown,
            classes: Vec::new(),
            namespaces: Vec::new(),
            relations: Vec::new(),
            notes: Vec::new(),
            styles: Vec::new(),
            acc_title: None,
            acc_descr: None,
        },
        namespaces: Vec::new(),
    };
    let mut header = false;
    // The class whose `{ … }` body is open, and the line it opened on.
    let mut body: Option<(usize, usize)> = None;
    let mut descr: Option<(String, usize)> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        if let Some((text, _)) = descr.as_mut() {
            match raw.find('}') {
                Some(end) => {
                    text.push_str(&raw[..end]);
                    p.d.acc_descr = Some(text.trim().to_string());
                    descr = None;
                }
                None => {
                    text.push_str(raw);
                    text.push('\n');
                }
            }
            continue;
        }
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some((class, _)) = body {
            match line.find('}') {
                Some(end) => {
                    p.member(class, &line[..end]);
                    body = None;
                    if !line[end + 1..].trim().is_empty() {
                        return Err(Error::new(number, "unexpected text after `}`"));
                    }
                }
                None => p.member(class, line),
            }
            continue;
        }
        if !header {
            if line != "classDiagram" && line != "classDiagram-v2" {
                return Err(Error::new(number, "expected `classDiagram`"));
            }
            header = true;
            continue;
        }
        if line == "}" {
            if p.namespaces.pop().is_none() {
                return Err(Error::new(number, "`}` closes no class or namespace"));
            }
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or("");
        let rest = line[keyword.len()..].trim();
        match keyword {
            "direction" => {
                p.d.direction = Direction::from_word(rest).ok_or_else(|| {
                    Error::new(
                        number,
                        format!("unknown direction `{rest}`: expected TB, BT, LR or RL"),
                    )
                })?;
                continue;
            }
            "classDef" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let names = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""), number)?;
                for name in names.split(',') {
                    p.d.styles.push((name.trim().to_string(), colors));
                }
                continue;
            }
            "style" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let id = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""), number)?;
                let name = whole_name(id, number)?;
                let c = p.class(&name);
                p.d.classes[c].style.merge(&colors);
                continue;
            }
            "cssClass" => {
                let (ids, class) = rest
                    .strip_prefix('"')
                    .and_then(|r| r.split_once('"'))
                    .ok_or_else(|| {
                        Error::new(number, "expected `cssClass \"Class1,Class2\" styleClass`")
                    })?;
                let class = class.trim().to_string();
                for id in ids.split(',') {
                    let name = whole_name(id.trim(), number)?;
                    let c = p.class(&name);
                    p.d.classes[c].classes.push(class.clone());
                }
                continue;
            }
            // Interaction: accepted, not drawn.
            "click" | "link" | "callback" => continue,
            "namespace" => {
                let head = rest
                    .strip_suffix('{')
                    .ok_or_else(|| Error::new(number, "expected `{` after the namespace's name"))?
                    .trim();
                let (name, label) = match head.split_once('[') {
                    Some((name, label)) => (name.trim(), Some(bracket_label(label, number)?)),
                    None => (head, None),
                };
                if name.is_empty() || name.contains(char::is_whitespace) {
                    return Err(Error::new(
                        number,
                        format!("`{name}` is not a namespace name"),
                    ));
                }
                let qualified = match p.namespaces.last() {
                    Some(&(n, _)) => format!("{}.{name}", p.d.namespaces[n].id),
                    None => name.to_string(),
                };
                let n = p.namespace(&qualified, label);
                p.namespaces.push((n, number));
                continue;
            }
            "class" => {
                if let Some(open) = class_statement(&mut p, rest, number)? {
                    body = Some((open, number));
                }
                continue;
            }
            "note" => {
                note_statement(&mut p, rest, number)?;
                continue;
            }
            _ => {}
        }
        if let Some(value) = acc(line, "accTitle") {
            p.d.acc_title = Some(value.to_string());
            continue;
        }
        if let Some(value) = acc(line, "accDescr") {
            p.d.acc_descr = Some(value.to_string());
            continue;
        }
        if let Some(open) = line.strip_prefix("accDescr")
            && let Some(body) = open.trim_start().strip_prefix('{')
        {
            match body.find('}') {
                Some(end) => p.d.acc_descr = Some(body[..end].trim().to_string()),
                None => descr = Some((format!("{body}\n"), number)),
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("<<") {
            // `<<interface>> Name`
            let (annotation, name) = rest
                .split_once(">>")
                .ok_or_else(|| Error::new(number, "expected `>>` to close the annotation"))?;
            let name = whole_name(name.trim(), number)?;
            let c = p.class(&name);
            p.d.classes[c]
                .annotations
                .push(annotation.trim().to_string());
            continue;
        }
        statement(&mut p, line, number)?;
    }
    if !header {
        return Err(Error::new(1, "expected `classDiagram`"));
    }
    if let Some((_, line)) = body {
        return Err(Error::new(line, "class body is never closed with `}`"));
    }
    if let Some(&(_, line)) = p.namespaces.last() {
        return Err(Error::new(line, "namespace is never closed with `}`"));
    }
    if let Some((_, line)) = descr {
        return Err(Error::new(line, "`accDescr {` is never closed with `}`"));
    }
    Ok(p.d)
}

/// A relation, a `Name : member` line, or a bare class name.
fn statement(p: &mut Parser, line: &str, number: usize) -> Result<(), Error> {
    let (head, label) = match find_outside(line, ":") {
        Some(at) if !line[at..].starts_with(":::") => {
            (&line[..at], Some(line[at + 1..].trim().to_string()))
        }
        _ => (line, None),
    };
    let Some((at, dashed)) = find_outside(head, "--")
        .map(|at| (at, false))
        .into_iter()
        .chain(find_outside(head, "..").map(|at| (at, true)))
        .min_by_key(|&(at, _)| at)
    else {
        // `Name : member`, or a class on its own.
        let name = whole_name(head.trim(), number)?;
        let c = p.class(&name);
        if let Some(member) = label {
            p.member(c, &member);
        }
        return Ok(());
    };
    let (left, from_head) = left_head(head[..at].trim_end());
    let (right, to_head) = right_head(head[at + 2..].trim_start());
    let (from_name, from_card) = split_card_left(left.trim(), number)?;
    let (to_card, to_name) = split_card_right(right.trim(), number)?;
    let from_name = whole_name(from_name, number)?;
    let to_name = whole_name(to_name, number)?;
    let from = p.class(&from_name);
    let to = p.class(&to_name);
    if from_head == Head::Lollipop {
        p.d.classes[from].interface = true;
    }
    if to_head == Head::Lollipop {
        p.d.classes[to].interface = true;
    }
    p.d.relations.push(Relation {
        from,
        to,
        from_head,
        to_head,
        dashed,
        label: label
            .filter(|l| !l.is_empty())
            .map(|l| l.replace("<br>", "\n")),
        from_card,
        to_card,
    });
    Ok(())
}

fn left_head(text: &str) -> (&str, Head) {
    for (token, head) in [
        ("<|", Head::Inheritance),
        ("()", Head::Lollipop),
        ("*", Head::Composition),
        ("<", Head::Dependency),
    ] {
        if let Some(rest) = text.strip_suffix(token) {
            return (rest, head);
        }
    }
    // `o` only as a token of its own: `A o--`, `A "1" o--`.
    if let Some(rest) = text.strip_suffix('o')
        && rest.ends_with([' ', '\t', '"'])
    {
        return (rest, Head::Aggregation);
    }
    (text, Head::None)
}

fn right_head(text: &str) -> (&str, Head) {
    for (token, head) in [
        ("|>", Head::Inheritance),
        ("()", Head::Lollipop),
        ("*", Head::Composition),
        (">", Head::Dependency),
    ] {
        if let Some(rest) = text.strip_prefix(token) {
            return (rest, head);
        }
    }
    if let Some(rest) = text.strip_prefix('o')
        && (rest.is_empty() || rest.starts_with([' ', '\t', '"']))
    {
        return (rest, Head::Aggregation);
    }
    (text, Head::None)
}

/// `Name "card"` → (`Name`, card).
fn split_card_left(text: &str, number: usize) -> Result<(&str, Option<String>), Error> {
    let Some(inner) = text.strip_suffix('"') else {
        return Ok((text, None));
    };
    let open = inner
        .rfind('"')
        .ok_or_else(|| Error::new(number, "unterminated string"))?;
    Ok((inner[..open].trim(), Some(inner[open + 1..].to_string())))
}

/// `"card" Name` → (card, `Name`).
fn split_card_right(text: &str, number: usize) -> Result<(Option<String>, &str), Error> {
    let Some(inner) = text.strip_prefix('"') else {
        return Ok((None, text));
    };
    let close = inner
        .find('"')
        .ok_or_else(|| Error::new(number, "unterminated string"))?;
    Ok((Some(inner[..close].to_string()), inner[close + 1..].trim()))
}

/// Where `token` first occurs outside quotes and backticks.
fn find_outside(text: &str, token: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '`' => quote = Some(c),
            None if text[i..].starts_with(token) => return Some(i),
            None => {}
        }
    }
    None
}

/// A class name: `Name`, `Name~T~`, `` `Any name` ``, `a.b`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Name {
    id: String,
    generic: Option<String>,
}

/// The name at the start of `text`, and what follows it.
fn name(text: &str, number: usize) -> Result<(Name, &str), Error> {
    let (id, rest) = if let Some(quoted) = text.strip_prefix('`') {
        let close = quoted
            .find('`')
            .ok_or_else(|| Error::new(number, "unterminated `` ` `` name"))?;
        (quoted[..close].to_string(), &quoted[close + 1..])
    } else {
        let end = text
            .char_indices()
            .find(|&(i, c)| {
                !(c.is_alphanumeric() || c == '_' || c == '.' || c == '-')
                    || text[i..].starts_with("--")
            })
            .map_or(text.len(), |(i, _)| i);
        (text[..end].to_string(), &text[end..])
    };
    if id.is_empty() {
        let found = text
            .chars()
            .next()
            .map_or("the end of the line".into(), |c| format!("`{c}`"));
        return Err(Error::new(
            number,
            format!("expected a class name, found {found}"),
        ));
    }
    if let Some(generic) = rest.strip_prefix('~') {
        let close = generic
            .rfind('~')
            .ok_or_else(|| Error::new(number, "a generic `~T~` is never closed with `~`"))?;
        // The generic runs to the last `~` before anything that ends a name.
        let stop = generic
            .find(|c: char| c.is_whitespace() || c == '{' || c == '[' || c == '"')
            .unwrap_or(generic.len());
        let close = generic[..stop].rfind('~').unwrap_or(close);
        return Ok((
            Name {
                id,
                generic: Some(generic[..close].to_string()),
            },
            &generic[close + 1..],
        ));
    }
    Ok((Name { id, generic: None }, rest))
}

/// A name that must be all of `text`.
fn whole_name(text: &str, number: usize) -> Result<Name, Error> {
    let (n, rest) = name(text, number)?;
    if !rest.trim().is_empty() {
        return Err(Error::new(
            number,
            format!("unexpected `{}` after class `{}`", rest.trim(), n.id),
        ));
    }
    Ok(n)
}

/// What follows `class`. Returns the class whose body stays open.
fn class_statement(p: &mut Parser, rest: &str, number: usize) -> Result<Option<usize>, Error> {
    let (n, mut after) = name(rest, number)?;
    let c = p.class(&n);
    after = after.trim_start();
    if let Some(label) = after.strip_prefix('[') {
        let close = label
            .find(']')
            .ok_or_else(|| Error::new(number, "expected `]` after the class label"))?;
        let text = bracket_label(&label[..=close], number)?;
        p.d.classes[c].label = match &n.generic {
            Some(g) => format!("{text}<{}>", generics(g)),
            None => text,
        };
        after = label[close + 1..].trim_start();
    }
    if let Some(css) = after.strip_prefix(":::") {
        let end = css
            .find(|ch: char| !(ch.is_alphanumeric() || ch == '_' || ch == '-'))
            .unwrap_or(css.len());
        p.d.classes[c].classes.push(css[..end].to_string());
        after = css[end..].trim_start();
    }
    if let Some(annotation) = after.strip_prefix("<<") {
        let (annotation, more) = annotation
            .split_once(">>")
            .ok_or_else(|| Error::new(number, "expected `>>` to close the annotation"))?;
        p.d.classes[c]
            .annotations
            .push(annotation.trim().to_string());
        after = more.trim_start();
    }
    if let Some(body) = after.strip_prefix('{') {
        return match body.find('}') {
            Some(end) => {
                p.member(c, &body[..end]);
                if !body[end + 1..].trim().is_empty() {
                    return Err(Error::new(number, "unexpected text after `}`"));
                }
                Ok(None)
            }
            None => {
                p.member(c, body);
                Ok(Some(c))
            }
        };
    }
    if !after.is_empty() {
        return Err(Error::new(
            number,
            format!("unexpected `{after}` after class `{}`", n.id),
        ));
    }
    Ok(None)
}

/// `"label"]` → `label`.
fn bracket_label(text: &str, number: usize) -> Result<String, Error> {
    text.trim()
        .strip_suffix(']')
        .map(str::trim)
        .and_then(|t| t.strip_prefix('"'))
        .and_then(|t| t.strip_suffix('"'))
        .map(str::to_string)
        .ok_or_else(|| Error::new(number, "expected a quoted label: `[\"label\"]`"))
}

/// What follows `note`: `for Name "text"`, or `"text"`.
fn note_statement(p: &mut Parser, rest: &str, number: usize) -> Result<(), Error> {
    let (class, text) = match rest.strip_prefix("for ") {
        Some(after) => {
            let (n, text) = name(after.trim_start(), number)?;
            (Some(p.class(&n)), text.trim())
        }
        None => (None, rest),
    };
    let text = text
        .strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .ok_or_else(|| Error::new(number, "expected the note's text in quotes"))?;
    let text = text
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("\\n", "\n");
    p.d.notes.push(Note {
        text,
        class,
        namespace: p.namespaces.last().map(|&(n, _)| n),
    });
    Ok(())
}

/// `~T~` pairs as `<T>`: `List~List~int~~` → `List<List<int>>`.
pub(crate) fn generics(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c != '~' {
            out.push(c);
            continue;
        }
        let next = chars.get(i + 1).copied();
        let opens = depth == 0
            || next.is_some_and(|n| n.is_alphanumeric() || n == '_')
                && chars[..i].last().is_some_and(|p| p.is_alphanumeric());
        if opens {
            out.push('<');
            depth += 1;
        } else {
            out.push('>');
            depth -= 1;
        }
    }
    out
}

/// An attribute, as mermaid parses it: `+name`, `-int count$`.
fn attribute(text: &str) -> Member {
    let mut body = text;
    let mut classifier = Classifier::None;
    if let Some(rest) = body.strip_suffix('$') {
        classifier = Classifier::Static;
        body = rest;
    } else if let Some(rest) = body.strip_suffix('*') {
        classifier = Classifier::Abstract;
        body = rest;
    }
    Member {
        text: generics(body.trim()),
        classifier,
    }
}

/// A method, as mermaid parses it: visibility, name, `(params)`, then an
/// optional classifier and return type; shown as `+name(params) : type`.
fn method(text: &str) -> Member {
    let close = text.rfind(')').expect("a method has a `)`");
    let open = text[..close].rfind('(');
    let Some(open) = open else {
        return attribute(text);
    };
    let (visibility, name) = match text[..open].chars().next() {
        Some(v @ ('+' | '-' | '#' | '~')) => (v.to_string(), &text[1..open]),
        _ => (String::new(), &text[..open]),
    };
    let params = text[open + 1..close].trim();
    let mut after = &text[close + 1..];
    let mut classifier = Classifier::None;
    for (mark, kind) in [('$', Classifier::Static), ('*', Classifier::Abstract)] {
        if let Some(rest) = after.strip_prefix(mark) {
            classifier = kind;
            after = rest;
        } else if let Some(rest) = after.trim_end().strip_suffix(mark) {
            classifier = kind;
            after = rest;
        }
    }
    let ret = after.trim();
    let mut shown = format!(
        "{visibility}{}({})",
        generics(name.trim()),
        generics(params)
    );
    if !ret.is_empty() {
        shown.push_str(" : ");
        shown.push_str(&generics(ret));
    }
    Member {
        text: shown,
        classifier,
    }
}

fn acc<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?.trim_start();
    Some(rest.strip_prefix(':')?.trim())
}

fn css(text: &str, number: usize) -> Result<Colors, Error> {
    crate::style::colors(&crate::style::declarations(text, number)?, number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class<'a>(d: &'a ClassDiagram, id: &str) -> &'a Class {
        d.classes.iter().find(|c| c.id == id).expect(id)
    }

    fn texts(members: &[Member]) -> Vec<&str> {
        members.iter().map(|m| m.text.as_str()).collect()
    }

    #[test]
    fn classes_members_and_generics() {
        let d = parse(concat!(
            "classDiagram\n",
            "  class Animal {\n",
            "    <<abstract>>\n",
            "    +String name\n",
            "    +int age$\n",
            "    +makeSound()* void\n",
            "    +eat(food: List~Food~) bool\n",
            "  }\n",
            "  class Square~Shape~ {\n",
            "    -List~List~int~~ cells\n",
            "  }\n",
            "  Animal : +isMammal()\n",
            "  Animal : -int legs\n",
            "  class Empty\n",
            "  class Labelled[\"A label\"]\n",
            "  class `Odd name`\n",
            "  <<interface>> Shape\n",
            "  class Bank { +deposit() }\n",
        ))
        .unwrap();
        let animal = class(&d, "Animal");
        assert_eq!(animal.annotations, ["abstract"]);
        assert_eq!(
            texts(&animal.attributes),
            ["+String name", "+int age", "-int legs"]
        );
        assert_eq!(animal.attributes[1].classifier, Classifier::Static);
        assert_eq!(
            texts(&animal.methods),
            [
                "+makeSound() : void",
                "+eat(food: List<Food>) : bool",
                "+isMammal()"
            ]
        );
        assert_eq!(animal.methods[0].classifier, Classifier::Abstract);
        assert_eq!(class(&d, "Square").label, "Square<Shape>");
        assert_eq!(
            texts(&class(&d, "Square").attributes),
            ["-List<List<int>> cells"]
        );
        assert_eq!(class(&d, "Labelled").label, "A label");
        assert_eq!(class(&d, "Odd name").label, "Odd name");
        assert_eq!(class(&d, "Shape").annotations, ["interface"]);
        assert_eq!(texts(&class(&d, "Bank").methods), ["+deposit()"]);
    }

    #[test]
    fn every_relation_kind_with_labels_and_cardinalities() {
        let d = parse(concat!(
            "classDiagram-v2\n",
            "  A <|-- B\n",
            "  C *-- D\n",
            "  E o-- F\n",
            "  G --> H\n",
            "  I -- J\n",
            "  K ..> L\n",
            "  M ..|> N\n",
            "  O .. P\n",
            "  Q \"1\" --> \"*\" R : owns\n",
            "  S <|--|> T\n",
            "  U --o V\n",
            "  W --* X\n",
            "  Y <.. Z\n",
            "  bar ()-- foo\n",
            "  order o-- item\n",
        ))
        .unwrap();
        let heads: Vec<(Head, Head, bool)> = d
            .relations
            .iter()
            .map(|r| (r.from_head, r.to_head, r.dashed))
            .collect();
        use Head::*;
        assert_eq!(
            heads,
            [
                (Inheritance, None, false),
                (Composition, None, false),
                (Aggregation, None, false),
                (None, Dependency, false),
                (None, None, false),
                (None, Dependency, true),
                (None, Inheritance, true),
                (None, None, true),
                (None, Dependency, false),
                (Inheritance, Inheritance, false),
                (None, Aggregation, false),
                (None, Composition, false),
                (Dependency, None, true),
                (Lollipop, None, false),
                (Aggregation, None, false),
            ]
        );
        let owns = &d.relations[8];
        assert_eq!(owns.from_card.as_deref(), Some("1"));
        assert_eq!(owns.to_card.as_deref(), Some("*"));
        assert_eq!(owns.label.as_deref(), Some("owns"));
        assert!(class(&d, "bar").interface);
        assert_eq!(d.classes[d.relations[14].from].id, "order");
    }

    #[test]
    fn namespaces_notes_styles_and_interaction() {
        let d = parse(concat!(
            "classDiagram\n",
            "  direction RL\n",
            "  namespace Shapes {\n",
            "    class Triangle\n",
            "    namespace Round {\n",
            "      class Circle\n",
            "    }\n",
            "  }\n",
            "  namespace a.b {\n",
            "    class Deep\n",
            "  }\n",
            "  note for Triangle \"three\\nsides\"\n",
            "  note \"free\"\n",
            "  classDef hot fill:#f00\n",
            "  class Hot:::hot\n",
            "  cssClass \"Triangle,Circle\" hot\n",
            "  style Deep stroke:#00f\n",
            "  click Triangle href \"https://example.com\"\n",
            "  link Circle \"https://example.com\"\n",
            "  callback Deep \"fn\"\n",
        ))
        .unwrap();
        assert_eq!(d.direction, Direction::RightLeft);
        let shapes = class(&d, "Triangle").namespace.unwrap();
        let round = class(&d, "Circle").namespace.unwrap();
        assert_eq!(d.namespaces[round].parent, Some(shapes));
        assert_eq!(d.namespaces[round].id, "Shapes.Round");
        let deep = class(&d, "Deep").namespace.unwrap();
        assert_eq!(d.namespaces[deep].label, "b");
        assert_eq!(d.namespaces[d.namespaces[deep].parent.unwrap()].label, "a");
        assert_eq!(d.notes[0].text, "three\nsides");
        assert!(d.notes[1].class.is_none());
        assert_eq!(d.colors(class(&d, "Hot")).fill, Some((255, 0, 0)));
        assert_eq!(d.colors(class(&d, "Circle")).fill, Some((255, 0, 0)));
        assert_eq!(d.colors(class(&d, "Deep")).stroke, Some((0, 0, 255)));
    }

    #[test]
    fn generics_nest() {
        assert_eq!(generics("List~List~int~~"), "List<List<int>>");
        assert_eq!(generics("Map~K, V~"), "Map<K, V>");
        assert_eq!(generics("T"), "T");
    }

    #[test]
    fn malformed_input_is_an_error_with_a_line() {
        for (source, line, needle) in [
            ("classDiagram\n  class A {\n  +x\n", 2, "never closed"),
            (
                "classDiagram\n  namespace N {\n  class A\n",
                2,
                "never closed",
            ),
            ("classDiagram\n  }\n", 2, "closes no"),
            ("classDiagram\n  A --> \n", 2, "expected a class name"),
            ("classDiagram\n  A ==> B\n", 2, "unexpected"),
            ("classDiagram\n  note for A text\n", 2, "quotes"),
            ("classDiagram\n  direction UP\n", 2, "unknown direction"),
            ("classDiagram\n  style A fill:blurple\n", 2, "colour"),
            ("classDiagram\n  class A[label]\n", 2, "quoted label"),
            ("graph\n", 1, "classDiagram"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
