//! `eventmodeling` parsing, per mermaid's
//! `packages/parser/src/language/eventmodeling/event-modeling.langium` and
//! its validator (which sources each kind of frame may take).
//!
//! ```text
//! eventmodeling
//!     title Cart
//!     tf 01 ui CartScreen
//!     tf 02 cmd AddItem { productId: 7 }
//!     tf 03 evt Cart.ItemAdded ->> 02 [[ItemData]]
//!     rf 04 rmo CartItems
//!     data ItemData {
//!         productId: 7
//!     }
//!     note 02 {
//!         Only signed-in users
//!     }
//!     gwt 03 given evt ItemAdded when cmd AddItem then evt ItemAdded
//! ```

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    Ui,
    Command,
    Event,
    ReadModel,
    Processor,
}

impl EntityType {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "ui" => EntityType::Ui,
            "cmd" | "command" => EntityType::Command,
            "evt" | "event" => EntityType::Event,
            "rmo" | "readmodel" => EntityType::ReadModel,
            "pcr" | "processor" => EntityType::Processor,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            EntityType::Ui => "ui",
            EntityType::Command => "command",
            EntityType::Event => "event",
            EntityType::ReadModel => "read model",
            EntityType::Processor => "processor",
        }
    }

    /// The kinds a frame of this kind may take input from.
    fn sources(self) -> &'static [EntityType] {
        match self {
            EntityType::Command => &[EntityType::Ui, EntityType::Processor],
            EntityType::Event => &[EntityType::Command],
            EntityType::ReadModel => &[EntityType::Event],
            EntityType::Processor | EntityType::Ui => &[EntityType::ReadModel],
        }
    }
}

/// A `tf` (time frame) or `rf` (reset frame).
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The frame number as written (`01`).
    pub name: String,
    pub reset: bool,
    pub kind: EntityType,
    /// `Namespace.Name` or `Name`.
    pub entity: String,
    /// `->> 02`: indices of earlier-or-later frames feeding this one.
    pub sources: Vec<usize>,
    /// Inline data or the referenced `data` block, line by line.
    pub data: Vec<String>,
}

impl Frame {
    pub fn namespace(&self) -> Option<&str> {
        self.entity.split_once('.').map(|(ns, _)| ns)
    }

    pub fn short_name(&self) -> &str {
        self.entity
            .split_once('.')
            .map(|(_, n)| n)
            .unwrap_or(&self.entity)
    }
}

/// A `note` on a frame, or a `gwt` (given/when/then) scenario.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    /// Index into [`EventModel::frames`].
    pub frame: usize,
    pub lines: Vec<String>,
    pub scenario: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventModel {
    pub title: Option<String>,
    pub frames: Vec<Frame>,
    /// `entity` declarations; mermaid draws nothing for them.
    pub entities: Vec<String>,
    pub notes: Vec<Note>,
}

const KEYWORDS: &[&str] = &[
    "tf",
    "timeframe",
    "rf",
    "resetframe",
    "entity",
    "data",
    "note",
    "gwt",
    "title",
    "accTitle",
    "accDescr",
];

struct RawFrame {
    frame: Frame,
    sources: Vec<String>,
    data_ref: Option<String>,
    line: usize,
}

pub fn parse(source: &str) -> Result<EventModel, Error> {
    let lines = clean(source)?;
    let mut model = EventModel::default();
    let mut frames: Vec<RawFrame> = Vec::new();
    let mut data: Vec<(String, Vec<String>)> = Vec::new();
    let mut notes: Vec<(String, Vec<String>, bool, usize)> = Vec::new();
    let mut seen_header = false;
    let mut i = 0;
    while i < lines.len() {
        let (number, ref text) = lines[i];
        i += 1;
        let line = text.trim();
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            let rest = line
                .strip_prefix("eventmodeling")
                .filter(|r| r.is_empty() || r.starts_with([' ', '\t']));
            let Some(rest) = rest else {
                return Err(Error::new(number, "expected `eventmodeling`"));
            };
            seen_header = true;
            if !rest.trim().is_empty() {
                return Err(Error::new(
                    number,
                    "put each statement on its own line after `eventmodeling`",
                ));
            }
            continue;
        }
        let (word, rest) = match line.find([' ', '\t']) {
            Some(at) => (&line[..at], line[at..].trim()),
            None => (line, ""),
        };
        match word {
            "title" => model.title = Some(rest.to_string()),
            "accTitle" | "accTitle:" => {}
            "accDescr" | "accDescr:" => {
                if rest.starts_with('{') && !rest.ends_with('}') {
                    // Multi-line description: skip to its closing brace.
                    while i < lines.len() && !lines[i].1.contains('}') {
                        i += 1;
                    }
                    i += 1;
                }
            }
            "tf" | "timeframe" | "rf" | "resetframe" => {
                frames.push(frame(rest, word.starts_with('r'), number)?);
            }
            "entity" => {
                if !is_qualified(rest) {
                    return Err(Error::new(number, "expected `entity <Name>`"));
                }
                model.entities.push(rest.to_string());
            }
            "data" | "note" => {
                let (head, block_on_line) = match rest.strip_suffix('{') {
                    Some(head) => (head.trim(), true),
                    None => (rest, false),
                };
                let head = strip_data_type(head, number)?;
                let target = head.trim();
                let valid = if word == "data" {
                    is_identifier(target)
                } else {
                    is_frame_number(target)
                };
                if !valid {
                    return Err(Error::new(
                        number,
                        if word == "data" {
                            "expected `data <Name> {` and its lines, closed by `}`"
                        } else {
                            "expected `note <frame> {` and its lines, closed by `}`"
                        },
                    ));
                }
                if !block_on_line {
                    while i < lines.len() && lines[i].1.trim().is_empty() {
                        i += 1;
                    }
                    if i >= lines.len() || lines[i].1.trim() != "{" {
                        return Err(Error::new(
                            number,
                            format!("expected a `{{` block after `{word} {target}`"),
                        ));
                    }
                    i += 1;
                }
                let mut body = Vec::new();
                loop {
                    let Some((_, text)) = lines.get(i) else {
                        return Err(Error::new(number, "this block is never closed with `}`"));
                    };
                    i += 1;
                    if text.trim() == "}" {
                        break;
                    }
                    body.push(text.clone());
                }
                let body = dedent(&body);
                if word == "data" {
                    data.push((target.to_string(), body));
                } else {
                    notes.push((target.to_string(), body, false, number));
                }
            }
            "gwt" => {
                // A scenario may run over several lines.
                let mut words: Vec<String> = rest.split_whitespace().map(String::from).collect();
                while let Some((_, next)) = lines.get(i) {
                    let first = next.split_whitespace().next();
                    match first {
                        None => {}
                        Some(w) if KEYWORDS.contains(&w) => break,
                        Some(_) => words.extend(next.split_whitespace().map(String::from)),
                    }
                    i += 1;
                }
                let (target, body) = scenario(&words, number)?;
                notes.push((target, body, true, number));
            }
            _ => {
                return Err(Error::new(
                    number,
                    format!("`{word}` is not supported in an event model (see DESIGN.md)"),
                ));
            }
        }
    }
    if !seen_header {
        return Err(Error::new(1, "expected `eventmodeling`"));
    }

    let find = |name: &str| frames.iter().position(|f| f.frame.name == name);
    for (index, raw) in frames.iter().enumerate() {
        if frames[..index]
            .iter()
            .any(|f| f.frame.name == raw.frame.name)
        {
            return Err(Error::new(
                raw.line,
                format!("frame `{}` is already declared", raw.frame.name),
            ));
        }
    }
    let mut resolved = Vec::with_capacity(frames.len());
    for raw in &frames {
        let mut frame = raw.frame.clone();
        for source in &raw.sources {
            let index = find(source)
                .ok_or_else(|| Error::new(raw.line, format!("`->> {source}` names no frame")))?;
            let from = frames[index].frame.kind;
            if !frame.kind.sources().contains(&from) {
                let allowed: Vec<&str> = frame.kind.sources().iter().map(|k| k.name()).collect();
                return Err(Error::new(
                    raw.line,
                    format!(
                        "a {} can only receive input from a {}, not from a {}",
                        frame.kind.name(),
                        allowed.join(" or "),
                        from.name()
                    ),
                ));
            }
            frame.sources.push(index);
        }
        if let Some(name) = &raw.data_ref {
            let (_, lines) = data.iter().find(|(n, _)| n == name).ok_or_else(|| {
                Error::new(raw.line, format!("`[[{name}]]` names no `data` block"))
            })?;
            frame.data = lines.clone();
        }
        resolved.push(frame);
    }
    model.frames = resolved;
    for (target, lines, scenario, line) in notes {
        let frame = find(&target)
            .ok_or_else(|| Error::new(line, format!("frame `{target}` is not declared")))?;
        model.notes.push(Note {
            frame,
            lines,
            scenario,
        });
    }
    Ok(model)
}

/// `01 cmd AddItem ->> 02 [[Ref]] `json` { … }` after `tf`.
fn frame(rest: &str, reset: bool, line: usize) -> Result<RawFrame, Error> {
    let usage = || Error::new(line, "expected `tf <number> <kind> <Name>`");
    let mut words = rest.splitn(3, [' ', '\t']).filter(|w| !w.is_empty());
    let name = words.next().ok_or_else(usage)?;
    if !is_frame_number(name) {
        return Err(Error::new(
            line,
            format!("`{name}` is not a frame number (one to three digits)"),
        ));
    }
    let kind_word = words.next().ok_or_else(usage)?;
    let kind = EntityType::parse(kind_word).ok_or_else(|| {
        Error::new(
            line,
            format!(
                "`{kind_word}` is not a frame kind: expected ui, cmd, evt, rmo or pcr \
                 (or command, event, readmodel, processor)"
            ),
        )
    })?;
    let mut rest = words.next().ok_or_else(usage)?.trim();
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
        .unwrap_or(rest.len());
    let entity = &rest[..end];
    if !is_qualified(entity) {
        return Err(usage());
    }
    rest = rest[end..].trim_start();
    let mut sources = Vec::new();
    while let Some(r) = rest.strip_prefix("->>") {
        let r = r.trim_start();
        let end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
        if end == 0 {
            return Err(Error::new(line, "expected a frame number after `->>`"));
        }
        sources.push(r[..end].to_string());
        rest = r[end..].trim_start();
    }
    let mut data_ref = None;
    if let Some(r) = rest.strip_prefix("[[") {
        let end = r
            .find("]]")
            .ok_or_else(|| Error::new(line, "expected `]]`"))?;
        data_ref = Some(r[..end].trim().to_string());
        rest = r[end + 2..].trim_start();
    }
    let rest = strip_data_type(rest, line)?.trim();
    let data = if rest.is_empty() {
        Vec::new()
    } else {
        let wrapped = |open: char, close: char| {
            rest.len() >= 2 && rest.starts_with(open) && rest.ends_with(close)
        };
        let inner = if wrapped('{', '}') || wrapped('"', '"') || wrapped('\'', '\'') {
            &rest[1..rest.len() - 1]
        } else {
            return Err(Error::new(
                line,
                format!("unexpected `{rest}` after a frame"),
            ));
        };
        let inner = inner.trim();
        if inner.is_empty() {
            Vec::new()
        } else {
            vec![inner.to_string()]
        }
    };
    Ok(RawFrame {
        frame: Frame {
            name: name.to_string(),
            reset,
            kind,
            entity: entity.to_string(),
            sources: Vec::new(),
            data,
        },
        sources,
        data_ref,
        line,
    })
}

/// `` `json` `` (or another data type) before a value, which is dropped:
/// the value is drawn as text whatever its type.
fn strip_data_type(text: &str, line: usize) -> Result<&str, Error> {
    let Some(start) = text.find('`') else {
        return Ok(text);
    };
    let after = &text[start + 1..];
    let end = after
        .find('`')
        .ok_or_else(|| Error::new(line, "unterminated `` `type` ``"))?;
    let kind = &after[..end];
    const TYPES: &[&str] = &[
        "json", "jsobj", "figma", "salt", "uri", "md", "html", "text",
    ];
    if !TYPES.contains(&kind) {
        return Err(Error::new(line, format!("`{kind}` is not a data type")));
    }
    if !text[..start].trim().is_empty() && !after[end + 1..].trim().is_empty() {
        return Err(Error::new(
            line,
            format!("unexpected `{}`", after[end + 1..].trim()),
        ));
    }
    if text[..start].trim().is_empty() {
        Ok(&after[end + 1..])
    } else {
        Ok(&text[..start])
    }
}

/// `02 given evt A evt B when cmd C then evt D` after `gwt`.
fn scenario(words: &[String], line: usize) -> Result<(String, Vec<String>), Error> {
    let usage = || {
        Error::new(
            line,
            "expected `gwt <frame> given <kind> <Name>… [when …] then <kind> <Name>…`",
        )
    };
    let (target, rest) = words.split_first().ok_or_else(usage)?;
    if !is_frame_number(target) {
        return Err(usage());
    }
    let mut out = Vec::new();
    let mut section: Option<&str> = None;
    let mut seen = Vec::new();
    let mut k = 0;
    while k < rest.len() {
        let w = rest[k].as_str();
        if matches!(w, "given" | "when" | "then") {
            section = Some(w);
            seen.push(w);
            k += 1;
            continue;
        }
        let heading = section.ok_or_else(usage)?;
        let kind = EntityType::parse(w).ok_or_else(usage)?;
        let entity = rest
            .get(k + 1)
            .filter(|e| is_identifier(e))
            .ok_or_else(usage)?;
        let label = match heading {
            "given" => "Given",
            "when" => "When",
            _ => "Then",
        };
        out.push(format!("{label} {} {entity}", kind.name()));
        k += 2;
    }
    let order_ok = matches!(seen[..], ["given", "then"] | ["given", "when", "then"]);
    if !order_ok {
        return Err(usage());
    }
    Ok((target.clone(), out))
}

fn is_frame_number(s: &str) -> bool {
    (1..=3).contains(&s.len()) && s.chars().all(|c| c.is_ascii_digit())
}

fn is_identifier(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn is_qualified(s: &str) -> bool {
    !s.is_empty() && s.split('.').all(is_identifier)
}

/// Lines with their numbers, comments removed: `%%`, `//` and `/* … */`
/// outside blocks.
fn clean(source: &str) -> Result<Vec<(usize, String)>, Error> {
    let mut out = Vec::new();
    let mut in_comment: Option<usize> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let mut line = String::new();
        let mut rest = raw;
        loop {
            if in_comment.is_some() {
                match rest.find("*/") {
                    Some(end) => {
                        rest = &rest[end + 2..];
                        in_comment = None;
                    }
                    None => break,
                }
            } else if let Some(start) = rest.find("/*") {
                line.push_str(&rest[..start]);
                rest = &rest[start + 2..];
                in_comment = Some(number);
            } else {
                line.push_str(rest);
                break;
            }
        }
        let line = match line.find("%%") {
            Some(at) => line[..at].to_string(),
            None => line,
        };
        // `//` starts a comment, except inside a URI such as `http://`.
        let line = match line.find("//") {
            Some(at) if !line[..at].ends_with(':') => line[..at].to_string(),
            _ => line,
        };
        out.push((number, line));
    }
    if let Some(line) = in_comment {
        return Err(Error::new(line, "this `/*` comment is never closed"));
    }
    Ok(out)
}

/// Lines with their common indentation removed, blank edges dropped.
fn dedent(lines: &[String]) -> Vec<String> {
    let indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let mut out: Vec<String> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").trim_end().to_string())
        .collect();
    while out.first().is_some_and(|l| l.is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_frames_sources_data_notes_and_scenarios() {
        let model = parse(
            "eventmodeling\n  title Cart\n  tf 01 ui CartUI\n  tf 02 cmd Inventory.AddItem { productId: 7 }\n\
             tf 03 cmd RemoveItem\n  tf 04 evt ItemChanged ->> 02 ->> 03 [[Changed]]\n\
             rf 05 rmo Cart // comment\n  entity ItemChanged\n\
             data Changed\n{\n  productId: 7\n  qty: 1\n}\n\
             note 02 `md` {\n  Signed-in users only\n}\n\
             gwt 04 given evt ItemChanged\n  when cmd AddItem then evt ItemChanged\n",
        )
        .unwrap();
        assert_eq!(model.title.as_deref(), Some("Cart"));
        assert_eq!(model.frames.len(), 5);
        assert_eq!(model.frames[1].namespace(), Some("Inventory"));
        assert_eq!(model.frames[1].short_name(), "AddItem");
        assert_eq!(model.frames[1].data, vec!["productId: 7"]);
        assert_eq!(model.frames[3].sources, vec![1, 2]);
        assert_eq!(model.frames[3].data, vec!["productId: 7", "qty: 1"]);
        assert!(model.frames[4].reset);
        assert_eq!(model.notes[0].lines, vec!["Signed-in users only"]);
        assert_eq!(
            model.notes[1].lines,
            vec![
                "Given event ItemChanged",
                "When command AddItem",
                "Then event ItemChanged"
            ]
        );
    }

    #[test]
    fn source_kinds_follow_the_validator() {
        let error = parse("eventmodeling\ntf 01 evt A\ntf 02 ui B ->> 01\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("can only receive input"), "{error}");
    }

    #[test]
    fn unknown_statements_and_frames_are_errors_with_lines() {
        let error = parse("eventmodeling\ntf 01 ui A\nswimlane X\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("swimlane"));
        let error = parse("eventmodeling\ntf 01 ui A\nnote 07 {\n x\n}\n").unwrap_err();
        assert_eq!(error.line, 3);
        let error = parse("eventmodeling\ntf 01 ux A\n").unwrap_err();
        assert!(error.message.contains("`ux`"));
    }
}
