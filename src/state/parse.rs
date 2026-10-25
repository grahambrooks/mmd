//! State diagram syntax: `stateDiagram-v2` (and `stateDiagram`) with
//! states, descriptions, transitions, `[*]` start and end states per
//! scope, composite states, concurrency regions, `<<choice>>`/`<<fork>>`/
//! `<<join>>`, notes, `direction` and `classDef`/`class`/`:::`/`style`.
//!
//! Grammar checked against mermaid's
//! `packages/mermaid/src/diagrams/state/parser/stateDiagram.jison`.

pub use crate::graph::Direction;
pub use crate::style::Colors;
use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    /// `[*]` as a transition's source.
    Start,
    /// `[*]` as a transition's target.
    End,
    Choice,
    Fork,
    Join,
    /// A floating `note "…" as id`.
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub side: Side,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct State {
    pub id: String,
    pub kind: Kind,
    /// `state "…" as id` and `id : …`, in order.
    pub descriptions: Vec<String>,
    /// The scope it is in: a composite state's or a region's group.
    pub group: Option<usize>,
    /// The group it encloses, when it is a composite state.
    pub composite: Option<usize>,
    pub notes: Vec<Note>,
    pub classes: Vec<String>,
    pub style: Colors,
}

impl State {
    /// What its box says: its one description, or else its id.
    pub fn label(&self) -> &str {
        match self.descriptions.as_slice() {
            [one] => one,
            _ => &self.id,
        }
    }
}

/// A composite state's inside, or one concurrency region of it.
#[derive(Debug, Clone)]
pub struct Group {
    /// The composite state, or `None` for a region.
    pub state: Option<usize>,
    pub parent: Option<usize>,
    pub direction: Option<Direction>,
}

#[derive(Debug, Clone)]
pub struct Transition {
    pub from: usize,
    pub to: usize,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StateDiagram {
    pub direction: Direction,
    pub states: Vec<State>,
    pub groups: Vec<Group>,
    pub transitions: Vec<Transition>,
    pub classes: Vec<(String, Colors)>,
    /// Stored, not drawn.
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

impl StateDiagram {
    /// A state's colours: the `default` class, its classes, then `style`.
    pub fn colors(&self, state: &State) -> Colors {
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
        for name in &state.classes {
            if let Some(c) = class(name) {
                out.merge(&c);
            }
        }
        out.merge(&state.style);
        out
    }

    /// The group a state encloses (made if new), so it becomes composite.
    fn composite(&mut self, state: usize) -> usize {
        if let Some(g) = self.states[state].composite {
            return g;
        }
        self.groups.push(Group {
            state: Some(state),
            parent: self.states[state].group,
            direction: None,
        });
        let g = self.groups.len() - 1;
        self.states[state].composite = Some(g);
        g
    }
}

struct Parser {
    d: StateDiagram,
    /// Open composite states: (the scope new states go in, the line it
    /// opened on).
    scopes: Vec<(usize, usize)>,
    /// The start and end state of each scope.
    terminals: Vec<(Option<usize>, Kind, usize)>,
}

impl Parser {
    fn scope(&self) -> Option<usize> {
        self.scopes.last().map(|&(g, _)| g)
    }

    /// The state called `id`, made in the current scope if new.
    fn state(&mut self, id: &str) -> usize {
        if let Some(i) = self.d.states.iter().position(|s| s.id == id) {
            return i;
        }
        self.push(id.to_string(), Kind::Normal)
    }

    fn push(&mut self, id: String, kind: Kind) -> usize {
        self.d.states.push(State {
            id,
            kind,
            descriptions: Vec::new(),
            group: self.scope(),
            composite: None,
            notes: Vec::new(),
            classes: Vec::new(),
            style: Colors::default(),
        });
        self.d.states.len() - 1
    }

    /// The current scope's start (or end) state.
    fn terminal(&mut self, kind: Kind) -> usize {
        let scope = self.scope();
        if let Some(&(_, _, i)) = self
            .terminals
            .iter()
            .find(|(s, k, _)| *s == scope && *k == kind)
        {
            return i;
        }
        let name = if kind == Kind::Start { "start" } else { "end" };
        let id = match scope {
            Some(g) => format!("[*] {name} {g}"),
            None => format!("[*] {name}"),
        };
        let i = self.push(id, kind);
        self.terminals.push((scope, kind, i));
        i
    }

    /// One side of a transition, or a lone state: `id`, `[*]`, with an
    /// optional `:::class`.
    fn end(&mut self, text: &str, kind: Kind, number: usize) -> Result<usize, Error> {
        let (name, class) = match text.split_once(":::") {
            Some((name, class)) => (name.trim(), Some(class.trim())),
            None => (text.trim(), None),
        };
        let i = if name == "[*]" {
            self.terminal(kind)
        } else {
            check_id(name, number)?;
            self.state(name)
        };
        if let Some(class) = class {
            check_id(class, number)?;
            self.d.states[i].classes.push(class.to_string());
        }
        Ok(i)
    }

    /// `--` inside a composite: the states so far become its first region,
    /// and a new one opens.
    fn divide(&mut self, number: usize) -> Result<(), Error> {
        let Some(&(scope, opened)) = self.scopes.last() else {
            return Err(Error::new(
                number,
                "`--` divides a composite state into regions; it is only valid inside \
                 `state X { … }`",
            ));
        };
        // The composite: the scope itself, or the region's parent.
        let composite = match self.d.groups[scope].state {
            Some(_) => scope,
            None => self.d.groups[scope].parent.expect("a region has a parent"),
        };
        let direction = self.d.groups[composite].direction;
        if scope == composite {
            self.d.groups.push(Group {
                state: None,
                parent: Some(composite),
                direction,
            });
            let first = self.d.groups.len() - 1;
            for s in &mut self.d.states {
                if s.group == Some(composite) {
                    s.group = Some(first);
                }
            }
            for (g, group) in self.d.groups.iter_mut().enumerate() {
                if g != first && group.parent == Some(composite) {
                    group.parent = Some(first);
                }
            }
            for t in &mut self.terminals {
                if t.0 == Some(composite) {
                    t.0 = Some(first);
                }
            }
        }
        self.d.groups.push(Group {
            state: None,
            parent: Some(composite),
            direction,
        });
        let region = self.d.groups.len() - 1;
        self.scopes.pop();
        self.scopes.push((region, opened));
        Ok(())
    }
}

pub fn parse(source: &str) -> Result<StateDiagram, Error> {
    let mut p = Parser {
        d: StateDiagram {
            direction: Direction::TopDown,
            states: Vec::new(),
            groups: Vec::new(),
            transitions: Vec::new(),
            classes: Vec::new(),
            acc_title: None,
            acc_descr: None,
        },
        scopes: Vec::new(),
        terminals: Vec::new(),
    };
    let mut header = false;
    // An open multi-line note: (state, side, text so far, line opened).
    let mut note: Option<(usize, Side, Vec<String>, usize)> = None;
    // An open multi-line `accDescr { … }`.
    let mut descr: Option<(String, usize)> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        if let Some((state, side, text, _)) = note.as_mut() {
            if raw.trim() == "end note" {
                let text = text.join("\n");
                p.d.states[*state].notes.push(Note { side: *side, text });
                note = None;
            } else {
                text.push(raw.trim().to_string());
            }
            continue;
        }
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
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !header {
            if line != "stateDiagram" && line != "stateDiagram-v2" {
                return Err(Error::new(number, "expected `stateDiagram-v2`"));
            }
            header = true;
            continue;
        }
        if line == "}" {
            if p.scopes.pop().is_none() {
                return Err(Error::new(number, "`}` closes no composite state"));
            }
            continue;
        }
        if line == "--" {
            p.divide(number)?;
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or("");
        let rest = line[keyword.len()..].trim();
        match keyword {
            "direction" => {
                let direction = Direction::from_word(rest).ok_or_else(|| {
                    Error::new(
                        number,
                        format!("unknown direction `{rest}`: expected TB, BT, LR or RL"),
                    )
                })?;
                match p.scope() {
                    Some(g) => {
                        // A region follows its composite.
                        let g = match p.d.groups[g].state {
                            Some(_) => g,
                            None => p.d.groups[g].parent.unwrap_or(g),
                        };
                        p.d.groups[g].direction = Some(direction);
                        for group in &mut p.d.groups {
                            if group.state.is_none() && group.parent == Some(g) {
                                group.direction = Some(direction);
                            }
                        }
                    }
                    None => p.d.direction = direction,
                }
                continue;
            }
            "classDef" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let names = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""), number)?;
                for name in names.split(',') {
                    p.d.classes.push((name.trim().to_string(), colors));
                }
                continue;
            }
            "class" => {
                let mut words = rest.rsplitn(2, char::is_whitespace);
                let class = words.next().unwrap_or("").to_string();
                let ids = words.next().unwrap_or("").trim();
                if ids.is_empty() {
                    return Err(Error::new(number, "`class` needs states and a class name"));
                }
                for id in ids.split(',') {
                    let id = id.trim();
                    check_id(id, number)?;
                    let s = p.state(id);
                    p.d.states[s].classes.push(class.clone());
                }
                continue;
            }
            "style" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let ids = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""), number)?;
                for id in ids.split(',') {
                    let id = id.trim();
                    check_id(id, number)?;
                    let s = p.state(id);
                    p.d.states[s].style.merge(&colors);
                }
                continue;
            }
            // Interaction and sizing: accepted, not drawn.
            "click" | "scale" => continue,
            "hide" if rest == "empty description" => continue,
            "note" => {
                if let Some(open) = note_statement(&mut p, rest, number)? {
                    note = Some((open.0, open.1, Vec::new(), number));
                }
                continue;
            }
            "state" => {
                state_statement(&mut p, rest, number)?;
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
        if let Some(at) = line.find("-->") {
            let left = &line[..at];
            let right = &line[at + 3..];
            let (right, label) = split_description(right);
            let from = p.end(left, Kind::Start, number)?;
            let to = p.end(right, Kind::End, number)?;
            p.d.transitions.push(Transition {
                from,
                to,
                label: label.filter(|l| !l.is_empty()),
            });
            continue;
        }
        // `id`, `id:::class` or `id : description`.
        let (id, description) = split_description(line);
        if id.trim() == "[*]" {
            return Err(Error::new(
                number,
                "`[*]` on its own is not a statement: it starts or ends a transition",
            ));
        }
        let s = p.end(id, Kind::Normal, number)?;
        if let Some(text) = description {
            p.d.states[s].descriptions.push(text);
        }
    }
    if !header {
        return Err(Error::new(1, "expected `stateDiagram-v2`"));
    }
    if let Some((_, _, _, line)) = note {
        return Err(Error::new(line, "note is never closed with `end note`"));
    }
    if let Some((_, line)) = descr {
        return Err(Error::new(line, "`accDescr {` is never closed with `}`"));
    }
    if let Some(&(_, line)) = p.scopes.last() {
        return Err(Error::new(line, "composite state is never closed with `}`"));
    }
    Ok(p.d)
}

/// `left : description` → (`left`, description). A `:::` is a class, not
/// a description.
fn split_description(text: &str) -> (&str, Option<String>) {
    let mut search = 0;
    while let Some(at) = text[search..].find(':') {
        let at = search + at;
        if text[at..].starts_with(":::") {
            search = at + 3;
            continue;
        }
        let description = text[at + 1..]
            .trim()
            .replace("<br>", "\n")
            .replace("<br/>", "\n");
        return (&text[..at], Some(description));
    }
    (text, None)
}

/// A state id: no spaces, `-`, `:`, `{` or quotes.
fn check_id(id: &str, number: usize) -> Result<(), Error> {
    if id.is_empty() {
        return Err(Error::new(number, "expected a state id"));
    }
    if let Some(c) = id
        .chars()
        .find(|c| c.is_whitespace() || matches!(c, '-' | ':' | '{' | '}' | '"'))
    {
        return Err(Error::new(
            number,
            format!("`{id}` is not a state id: `{c}` is not allowed in one"),
        ));
    }
    Ok(())
}

/// What follows `state`.
fn state_statement(p: &mut Parser, rest: &str, number: usize) -> Result<(), Error> {
    for (marker, kind) in [
        ("<<fork>>", Kind::Fork),
        ("<<join>>", Kind::Join),
        ("<<choice>>", Kind::Choice),
        ("[[fork]]", Kind::Fork),
        ("[[join]]", Kind::Join),
        ("[[choice]]", Kind::Choice),
    ] {
        if let Some(id) = rest.strip_suffix(marker) {
            let id = id.trim();
            check_id(id, number)?;
            let s = p.state(id);
            p.d.states[s].kind = kind;
            return Ok(());
        }
    }
    if let Some(open) = ["<<", "[["].iter().find_map(|m| rest.find(m)) {
        return Err(Error::new(
            number,
            format!(
                "`{}` is not supported: expected `<<choice>>`, `<<fork>>` or `<<join>>` (see DESIGN.md)",
                &rest[open..]
            ),
        ));
    }
    // A body opening: `{`, or `{}` for an empty one.
    let (head, opens, closes) = if let Some(head) = rest.strip_suffix("{}") {
        (head.trim(), true, true)
    } else if let Some(head) = rest.strip_suffix('{') {
        (head.trim(), true, false)
    } else {
        (rest, false, false)
    };
    if head.contains(['{', '}']) {
        return Err(Error::new(
            number,
            "a composite state's body on the same line as its `{` is not supported: start \
             it on the next line (see DESIGN.md)",
        ));
    }
    let (id, description) = if let Some(quoted) = head.strip_prefix('"') {
        let close = quoted
            .find('"')
            .ok_or_else(|| Error::new(number, "unterminated string"))?;
        let description = quoted[..close].to_string();
        let after = quoted[close + 1..].trim();
        let id = after
            .strip_prefix("as")
            .filter(|r| r.starts_with(char::is_whitespace));
        let Some(id) = id else {
            return Err(Error::new(
                number,
                "expected `as <id>` after a state's quoted description",
            ));
        };
        let (id, more) = split_description(id.trim());
        let mut descriptions = vec![description];
        descriptions.extend(more);
        (id.trim(), descriptions)
    } else {
        let (id, more) = split_description(head);
        (id.trim(), more.into_iter().collect())
    };
    let (id, class) = match id.split_once(":::") {
        Some((id, class)) => (id.trim(), Some(class.trim())),
        None => (id, None),
    };
    check_id(id, number)?;
    let s = p.state(id);
    p.d.states[s].descriptions.extend(description);
    if let Some(class) = class {
        p.d.states[s].classes.push(class.to_string());
    }
    if opens {
        let g = p.d.composite(s);
        if !closes {
            p.scopes.push((g, number));
        }
    }
    Ok(())
}

/// What follows `note`. A note whose text follows on later lines returns
/// the state it is for and its side.
fn note_statement(
    p: &mut Parser,
    rest: &str,
    number: usize,
) -> Result<Option<(usize, Side)>, Error> {
    if let Some(quoted) = rest.strip_prefix('"') {
        // A floating note: `note "text" as id`.
        let close = quoted
            .find('"')
            .ok_or_else(|| Error::new(number, "unterminated string"))?;
        let text = quoted[..close].replace("<br>", "\n").replace("<br/>", "\n");
        let id = quoted[close + 1..]
            .trim()
            .strip_prefix("as")
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::new(number, "expected `as <id>` after a floating note"))?;
        check_id(id, number)?;
        let s = p.state(id);
        p.d.states[s].kind = Kind::Note;
        p.d.states[s].descriptions = vec![text];
        return Ok(None);
    }
    let (side, after) = if let Some(after) = rest.strip_prefix("left of") {
        (Side::Left, after)
    } else if let Some(after) = rest.strip_prefix("right of") {
        (Side::Right, after)
    } else {
        return Err(Error::new(
            number,
            format!("expected `left of` or `right of` after `note`, found `{rest}`"),
        ));
    };
    let (id, text) = split_description(after.trim());
    let id = id.trim();
    check_id(id, number)?;
    let s = p.state(id);
    match text {
        Some(text) => {
            p.d.states[s].notes.push(Note { side, text });
            Ok(None)
        }
        None => Ok(Some((s, side))),
    }
}

/// `accTitle: text` → `text`.
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

    fn state<'a>(d: &'a StateDiagram, id: &str) -> &'a State {
        d.states.iter().find(|s| s.id == id).expect(id)
    }

    #[test]
    fn states_descriptions_and_transitions() {
        let d = parse(concat!(
            "stateDiagram-v2\n",
            "  [*] --> Still\n",
            "  Still --> [*]\n",
            "  Still --> Moving : push\n",
            "  Moving: going\n",
            "  state \"Very long\" as Crash\n",
            "  Crash : boom\n",
            "  Moving --> Crash: hit<br>wall\n",
            "  Crash --> [*]\n",
        ))
        .unwrap();
        assert_eq!(state(&d, "Moving").label(), "going");
        assert_eq!(state(&d, "Crash").descriptions, ["Very long", "boom"]);
        assert_eq!(state(&d, "Crash").label(), "Crash");
        assert_eq!(d.transitions.len(), 5);
        assert_eq!(d.transitions[2].label.as_deref(), Some("push"));
        assert_eq!(d.transitions[3].label.as_deref(), Some("hit\nwall"));
        // One start and one end at the root.
        let starts = d.states.iter().filter(|s| s.kind == Kind::Start).count();
        let ends = d.states.iter().filter(|s| s.kind == Kind::End).count();
        assert_eq!((starts, ends), (1, 1));
    }

    #[test]
    fn composites_regions_and_their_own_terminals() {
        let d = parse(concat!(
            "stateDiagram-v2\n",
            "  [*] --> Active\n",
            "  state Active {\n",
            "    direction LR\n",
            "    [*] --> NumLockOff\n",
            "    NumLockOff --> NumLockOn : press\n",
            "    --\n",
            "    [*] --> CapsOff\n",
            "    state Inner {\n",
            "      [*] --> Deep\n",
            "    }\n",
            "  }\n",
        ))
        .unwrap();
        let active = state(&d, "Active");
        let g = active.composite.unwrap();
        assert_eq!(d.groups[g].direction, Some(Direction::LeftRight));
        let regions: Vec<usize> = (0..d.groups.len())
            .filter(|&r| d.groups[r].state.is_none() && d.groups[r].parent == Some(g))
            .collect();
        assert_eq!(regions.len(), 2);
        assert_eq!(state(&d, "NumLockOff").group, Some(regions[0]));
        assert_eq!(state(&d, "CapsOff").group, Some(regions[1]));
        assert_eq!(state(&d, "Inner").group, Some(regions[1]));
        let inner = state(&d, "Inner").composite.unwrap();
        assert_eq!(state(&d, "Deep").group, Some(inner));
        // Root, two regions and Inner each have their own start.
        let starts = d.states.iter().filter(|s| s.kind == Kind::Start).count();
        assert_eq!(starts, 4);
    }

    #[test]
    fn pseudo_states_notes_and_styles() {
        let d = parse(concat!(
            "stateDiagram\n",
            "  state fork_state <<fork>>\n",
            "  state join_state <<join>>\n",
            "  state if_state <<choice>>\n",
            "  note left of fork_state : one line\n",
            "  note right of if_state\n",
            "    first\n",
            "    second\n",
            "  end note\n",
            "  note \"floating\" as N1\n",
            "  classDef hot fill:#f00,color:white\n",
            "  class fork_state, join_state hot\n",
            "  if_state:::hot --> join_state\n",
            "  style if_state stroke:#00f\n",
            "  hide empty description\n",
            "  scale 300 width\n",
            "  click if_state \"https://example.com\" \"tip\"\n",
        ))
        .unwrap();
        assert_eq!(state(&d, "fork_state").kind, Kind::Fork);
        assert_eq!(state(&d, "join_state").kind, Kind::Join);
        assert_eq!(state(&d, "if_state").kind, Kind::Choice);
        assert_eq!(state(&d, "N1").kind, Kind::Note);
        assert_eq!(state(&d, "fork_state").notes[0].text, "one line");
        assert_eq!(state(&d, "if_state").notes[0].text, "first\nsecond");
        assert_eq!(state(&d, "if_state").notes[0].side, Side::Right);
        let colors = d.colors(state(&d, "if_state"));
        assert_eq!(colors.fill, Some((255, 0, 0)));
        assert_eq!(colors.stroke, Some((0, 0, 255)));
        assert_eq!(
            d.colors(state(&d, "join_state")).text,
            Some((255, 255, 255))
        );
    }

    #[test]
    fn malformed_input_is_an_error_with_a_line() {
        for (source, line, needle) in [
            ("stateDiagram-v2\n  A --> B\n  --\n", 3, "only valid inside"),
            (
                "stateDiagram-v2\n  state A {\n  A --> B\n",
                2,
                "never closed",
            ),
            ("stateDiagram-v2\n  }\n", 2, "closes no"),
            ("stateDiagram-v2\n  note over A : x\n", 2, "left of"),
            ("stateDiagram-v2\n  note left of A\n  x\n", 2, "end note"),
            ("stateDiagram-v2\n  A-B --> C\n", 2, "not a state id"),
            ("stateDiagram-v2\n  state A <<history>>\n", 2, "<<history>>"),
            ("stateDiagram-v2\n  direction XY\n", 2, "unknown direction"),
            ("stateDiagram-v2\n  style A fill:blurple\n", 2, "colour"),
            ("flowchart\n", 1, "stateDiagram"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }
}
