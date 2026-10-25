//! sequenceDiagram parsing.
//!
//! Grammar per `DESIGN.md`, checked against mermaid's
//! `packages/mermaid/src/diagrams/sequence/parser/sequenceDiagram.jison`;
//! the semantic checks (create and destroy need a message, only an active
//! participant can be deactivated) follow `sequenceDb.ts`.
//!
//! Keywords are case-insensitive, as in mermaid. A statement ends at a
//! newline or `;`. `#name;` and `#123;` are entity codes, decoded before
//! the line is split, so `#59;` is a literal semicolon.

use crate::style::{self, Rgb};
use crate::{Error, strip_comment};

/// How a participant is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParticipantKind {
    /// A box (`participant`).
    #[default]
    Participant,
    /// A stick figure (`actor`).
    Actor,
    // The `@{ "type": … }` kinds.
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

#[derive(Debug, Clone, Default)]
pub struct Participant {
    /// What messages name it by.
    pub id: String,
    /// What is drawn: the `as` alias, or the id. Lines split on `<br>`.
    pub label: String,
    pub kind: ParticipantKind,
    /// Index into [`Sequence::boxes`].
    pub group: Option<usize>,
    /// `create`: its head is drawn where the creating message arrives.
    pub created: bool,
    /// `destroy`: its lifeline ends in a cross; no footer.
    pub destroyed: bool,
    /// `link`, `links`, `properties` and `details`, as written. Mermaid
    /// shows these in a pop-up menu, which a static drawing has no place
    /// for; stored, not drawn.
    pub menus: Vec<String>,
}

/// A `box … end` group of participants.
#[derive(Debug, Clone, Default)]
pub struct ParticipantBox {
    pub label: Option<String>,
    pub fill: Option<Rgb>,
}

/// What is drawn at one end of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Head {
    #[default]
    None,
    /// A filled arrowhead (`->>`).
    Arrow,
    /// An open, stroked arrowhead (`-)`, async).
    Open,
    /// A cross (`-x`).
    Cross,
}

#[derive(Debug, Clone, Default)]
pub struct Message {
    pub from: usize,
    pub to: usize,
    pub text: String,
    /// `wrap:` before the text.
    pub wrap: bool,
    pub dashed: bool,
    pub start: Head,
    pub end: Head,
    /// A half arrowhead (`-|\`, `-//` …), drawn as a whole one.
    pub half: bool,
    /// `()` at either end: a dot on the lifeline.
    pub dot_from: bool,
    pub dot_to: bool,
    /// The `autonumber` number, when numbering is on.
    pub number: Option<Number>,
    /// `create`: this message brings `to` into being.
    pub creates: bool,
    /// `destroy`: participants whose lifelines end at this message.
    pub destroys: Vec<usize>,
    /// `+` before the target: `to` is activated after the message.
    pub activate: bool,
    /// `-` before the target: `from` is deactivated after the message.
    pub deactivate: bool,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSide {
    Left,
    Right,
    Over,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub side: NoteSide,
    /// The participant, or the first of the two a note spans.
    pub first: usize,
    pub last: usize,
    pub text: String,
    pub wrap: bool,
}

/// A combined fragment, or a `rect` background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    Loop,
    Alt,
    Opt,
    Par,
    Critical,
    Break,
    /// ZenUML's `try`/`catch`/`finally`; sequenceDiagram has no keyword
    /// for it.
    Try,
    /// `rect`: a coloured background, no border or label.
    Rect(Rgb),
}

impl Frame {
    /// The operator drawn in the frame's tab.
    pub fn operator(self) -> &'static str {
        match self {
            Frame::Loop => "loop",
            Frame::Alt => "alt",
            Frame::Opt => "opt",
            Frame::Par => "par",
            Frame::Critical => "critical",
            Frame::Break => "break",
            Frame::Try => "try",
            Frame::Rect(_) => "rect",
        }
    }

    /// The keyword that opens another section of this frame.
    fn section(self) -> Option<&'static str> {
        match self {
            Frame::Alt => Some("else"),
            Frame::Par => Some("and"),
            Frame::Critical => Some("option"),
            _ => None,
        }
    }
}

/// One statement, in source order.
#[derive(Debug, Clone)]
pub enum Event {
    Message(Message),
    Note(Note),
    Activate(usize),
    Deactivate(usize),
    /// A frame opens, with its first section's label.
    Start(Frame, String),
    /// `else`, `and` or `option`: a dashed divider and its label.
    Section(String),
    End,
    /// ZenUML's `ref(…)`: an interaction use, a `ref` frame of its own
    /// row; sequenceDiagram has no statement for it.
    Ref(Ref),
    /// ZenUML's `==label==`: a rule across every lifeline, its label on
    /// a pill; sequenceDiagram has no statement for it.
    Divider(String),
}

/// A reference to another interaction, drawn as a `ref` frame.
#[derive(Debug, Clone, Default)]
pub struct Ref {
    /// What the frame names, centred in it.
    pub label: String,
    /// The participants it spans, leftmost to rightmost column; empty
    /// spans them all.
    pub participants: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Sequence {
    pub title: Option<String>,
    /// `accTitle:`; stored, not drawn.
    pub acc_title: Option<String>,
    /// `accDescr`; stored, not drawn.
    pub acc_descr: Option<String>,
    /// In column order: declaration or first mention.
    pub participants: Vec<Participant>,
    pub boxes: Vec<ParticipantBox>,
    pub events: Vec<Event>,
}

impl Sequence {
    pub fn messages(&self) -> impl Iterator<Item = &Message> {
        self.events.iter().filter_map(|e| match e {
            Event::Message(m) => Some(m),
            _ => None,
        })
    }
}

/// Parse a sequenceDiagram source.
pub fn parse(source: &str) -> Result<Sequence, Error> {
    let mut p = Parser::default();
    let mut description: Option<(usize, Vec<String>)> = None;

    for (index, raw) in source.lines().enumerate() {
        let line = index + 1;
        if let Some((_, lines)) = &mut description {
            match raw.find('}') {
                Some(end) => {
                    lines.push(raw[..end].trim().to_string());
                    p.seq.acc_descr = Some(lines.join("\n").trim().to_string());
                    description = None;
                    let tail = strip_comment(&raw[end + 1..]).trim();
                    if !tail.is_empty() {
                        return Err(Error::new(
                            line,
                            format!("unexpected `{tail}` after `accDescr {{ … }}`"),
                        ));
                    }
                }
                None => lines.push(raw.trim().to_string()),
            }
            continue;
        }
        let encoded = encode_entities(strip_comment(raw));
        for statement in encoded.split(';') {
            let statement = statement.trim();
            if statement.is_empty() || statement.starts_with('#') {
                continue;
            }
            if let Some(start) = p.statement(statement, line)? {
                description = Some((line, vec![start]));
            }
        }
    }

    if let Some((line, _)) = description {
        return Err(Error::new(line, "`accDescr {` is never closed with `}`"));
    }
    if !p.seen_header {
        return Err(Error::new(1, "expected `sequenceDiagram`"));
    }
    if let Some(line) = p.in_box {
        return Err(Error::new(line, "`box` is never closed with `end`"));
    }
    if let Some((frame, line)) = p.frames.last() {
        return Err(Error::new(
            *line,
            format!("`{}` is never closed with `end`", frame.operator()),
        ));
    }
    if let Some((who, line)) = p.pending_create {
        return Err(Error::new(
            line,
            format!(
                "`create` of `{}` needs a message to it after it",
                p.seq.participants[who].id
            ),
        ));
    }
    if let Some((who, line)) = p.pending_destroy.first() {
        return Err(Error::new(
            *line,
            format!(
                "`destroy` of `{}` needs a message to or from it after it",
                p.seq.participants[*who].id
            ),
        ));
    }
    Ok(p.seq)
}

/// An `autonumber` value. Mermaid's lexer takes at most two decimals
/// (`1.1`, `.25`) and rounds each sum to hundredths, so counting in
/// whole hundredths reproduces it without `1.2000000000000002`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Number(pub u32);

impl Number {
    pub const ONE: Number = Number(100);

    /// `12`, `1.5`, `0.25` or `.5`; anything else is `None`.
    fn parse(s: &str) -> Option<Number> {
        let (int, frac) = match s.split_once('.') {
            Some((_, "")) => return None,
            Some(parts) => parts,
            None => (s, ""),
        };
        let digits = |d: &str| d.bytes().all(|b| b.is_ascii_digit());
        if !digits(int) || !digits(frac) || frac.len() > 2 || (int.is_empty() && frac.is_empty()) {
            return None;
        }
        let int: u32 = if int.is_empty() { 0 } else { int.parse().ok()? };
        let frac: u32 = match frac.len() {
            0 => 0,
            1 => frac.parse::<u32>().ok()? * 10,
            _ => frac.parse().ok()?,
        };
        int.checked_mul(100)?.checked_add(frac).map(Number)
    }

    /// The value when it is whole.
    pub fn whole(self) -> Option<u32> {
        self.0.is_multiple_of(100).then_some(self.0 / 100)
    }
}

impl std::fmt::Display for Number {
    /// As JavaScript prints it: `2`, `1.1`, `1.25`, never `1.10`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (int, frac) = (self.0 / 100, self.0 % 100);
        match frac {
            0 => write!(f, "{int}"),
            _ if frac.is_multiple_of(10) => write!(f, "{int}.{}", frac / 10),
            _ => write!(f, "{int}.{frac:02}"),
        }
    }
}

#[derive(Default)]
struct Parser {
    seq: Sequence,
    seen_header: bool,
    /// The open `box`: where it started.
    in_box: Option<usize>,
    frames: Vec<(Frame, usize)>,
    /// Activation depth per participant.
    active: Vec<u32>,
    /// `autonumber`: the next number and the step, while on.
    numbering: Option<(Number, Number)>,
    next_number: Number,
    step: Number,
    pending_create: Option<(usize, usize)>,
    pending_destroy: Vec<(usize, usize)>,
}

impl Parser {
    /// Handle one statement. Returns the first line of an `accDescr {`
    /// block that continues on later lines.
    fn statement(&mut self, s: &str, line: usize) -> Result<Option<String>, Error> {
        let (word, rest) = split_word(s);
        let keyword = word.to_ascii_lowercase();
        if !self.seen_header {
            if keyword == "sequencediagram" && rest.trim().is_empty() {
                self.seen_header = true;
                return Ok(None);
            }
            return Err(Error::new(line, "expected `sequenceDiagram`"));
        }
        if self.in_box.is_some() {
            return match keyword.as_str() {
                "participant" => self.declare(rest, ParticipantKind::Participant, false, line),
                "actor" => self.declare(rest, ParticipantKind::Actor, false, line),
                "end" if rest.trim().is_empty() => {
                    let g = self.seq.boxes.len() - 1;
                    if !self.seq.participants.iter().any(|p| p.group == Some(g)) {
                        return Err(Error::new(
                            line,
                            "a `box` must declare at least one participant",
                        ));
                    }
                    self.in_box = None;
                    Ok(())
                }
                _ => Err(Error::new(
                    line,
                    format!(
                        "only `participant` and `actor` may appear inside a `box`, found `{word}`"
                    ),
                )),
            }
            .map(|()| None);
        }
        match keyword.as_str() {
            "participant" => self.declare(rest, ParticipantKind::Participant, false, line)?,
            "actor" => self.declare(rest, ParticipantKind::Actor, false, line)?,
            "create" => {
                let (word, rest) = split_word(rest.trim());
                let kind = match word.to_ascii_lowercase().as_str() {
                    "participant" => ParticipantKind::Participant,
                    "actor" => ParticipantKind::Actor,
                    _ => {
                        return Err(Error::new(
                            line,
                            "`create` must be followed by `participant` or `actor`",
                        ));
                    }
                };
                self.declare(rest, kind, true, line)?;
            }
            "destroy" => {
                let id = participant_name(rest, line)?;
                let who = self.ensure(&id);
                self.seq.participants[who].destroyed = true;
                self.pending_destroy.push((who, line));
            }
            "box" => {
                let group = parse_box(rest.trim(), line)?;
                self.seq.boxes.push(group);
                self.in_box = Some(line);
            }
            "end" if rest.trim().is_empty() => {
                if self.frames.pop().is_none() {
                    return Err(Error::new(line, "`end` without an open block"));
                }
                self.seq.events.push(Event::End);
            }
            "loop" | "opt" | "alt" | "par" | "par_over" | "critical" | "break" => {
                let frame = match keyword.as_str() {
                    "loop" => Frame::Loop,
                    "opt" => Frame::Opt,
                    "alt" => Frame::Alt,
                    "critical" => Frame::Critical,
                    "break" => Frame::Break,
                    _ => Frame::Par,
                };
                self.frames.push((frame, line));
                self.seq
                    .events
                    .push(Event::Start(frame, text(rest).0.replace('\n', " ")));
            }
            "rect" => {
                let fill = rect_color(rest.trim(), line)?;
                self.frames.push((Frame::Rect(fill), line));
                self.seq
                    .events
                    .push(Event::Start(Frame::Rect(fill), String::new()));
            }
            "else" | "and" | "option" => {
                let open = self.frames.last().map(|(f, _)| *f);
                if open.and_then(Frame::section) != Some(keyword.as_str()) {
                    let parent = match keyword.as_str() {
                        "else" => "alt",
                        "and" => "par",
                        _ => "critical",
                    };
                    return Err(Error::new(
                        line,
                        format!("`{keyword}` outside an `{parent}` block"),
                    ));
                }
                self.seq
                    .events
                    .push(Event::Section(text(rest).0.replace('\n', " ")));
            }
            "note" => self.note(rest, line)?,
            "activate" | "deactivate" => {
                let id = participant_name(rest, line)?;
                let who = self.ensure(&id);
                if keyword == "activate" {
                    self.activate(who);
                } else {
                    self.deactivate(who, line)?;
                }
            }
            "autonumber" => self.autonumber(rest.trim(), line)?,
            "title" => {
                let title = rest.trim().trim_start_matches(':').trim();
                self.seq.title = Some(text(title).0);
            }
            "acctitle" => {
                self.seq.acc_title = Some(colon_value(rest, "accTitle", line)?);
            }
            "accdescr" => {
                let rest = rest.trim();
                if let Some(body) = rest.strip_prefix('{') {
                    match body.find('}') {
                        Some(end) => self.seq.acc_descr = Some(body[..end].trim().to_string()),
                        None => return Ok(Some(body.trim().to_string())),
                    }
                } else {
                    self.seq.acc_descr = Some(colon_value(rest, "accDescr", line)?);
                }
            }
            "link" | "links" | "properties" | "details" => {
                let (id, value) = rest
                    .split_once(':')
                    .ok_or_else(|| Error::new(line, format!("expected `{word} Participant: …`")))?;
                let id = participant_name(id, line)?;
                let who = self.ensure(&id);
                self.seq.participants[who]
                    .menus
                    .push(format!("{keyword}: {}", restore(value.trim())));
            }
            _ => self.message(s, line)?,
        }
        Ok(None)
    }

    /// The index of `id`, adding it as a plain participant if it is new.
    fn ensure(&mut self, id: &str) -> usize {
        if let Some(i) = self.seq.participants.iter().position(|p| p.id == id) {
            return i;
        }
        self.seq.participants.push(Participant {
            id: id.to_string(),
            label: id.to_string(),
            ..Participant::default()
        });
        self.active.push(0);
        self.seq.participants.len() - 1
    }

    fn declare(
        &mut self,
        rest: &str,
        mut kind: ParticipantKind,
        created: bool,
        line: usize,
    ) -> Result<(), Error> {
        let rest = rest.trim();
        let (id, alias) = if let Some(at) = rest.find("@{") {
            let id = &rest[..at];
            let after = &rest[at + 2..];
            let close = after.find('}').ok_or_else(|| {
                Error::new(line, "participant metadata `@{` is never closed with `}`")
            })?;
            let (meta_kind, meta_alias) = metadata(&after[..close], line)?;
            if let Some(k) = meta_kind {
                kind = k;
            }
            let tail = after[close + 1..].trim();
            let alias = match split_word(tail) {
                ("", _) => meta_alias,
                (w, alias) if w.eq_ignore_ascii_case("as") => Some(alias.trim().to_string()),
                _ => {
                    return Err(Error::new(
                        line,
                        format!("unexpected `{tail}` after participant metadata"),
                    ));
                }
            };
            (id.to_string(), alias)
        } else {
            match split_word(rest) {
                (id, tail) if split_word(tail.trim()).0.eq_ignore_ascii_case("as") => (
                    id.to_string(),
                    Some(split_word(tail.trim()).1.trim().to_string()),
                ),
                _ => (rest.to_string(), None),
            }
        };
        let id = participant_name(&id, line)?;
        let existed = self.seq.participants.iter().any(|p| p.id == id);
        let who = self.ensure(&id);
        let group = self.in_box.map(|_| self.seq.boxes.len() - 1);
        let participant = &mut self.seq.participants[who];
        if let (Some(old), Some(new)) = (participant.group, group)
            && old != new
        {
            return Err(Error::new(
                line,
                format!("`{id}` is already in another `box`; a participant belongs to one box"),
            ));
        }
        participant.group = participant.group.or(group);
        participant.kind = kind;
        if let Some(alias) = alias.filter(|a| !a.is_empty()) {
            participant.label = text(&alias).0;
        }
        if created {
            if existed {
                return Err(Error::new(
                    line,
                    format!("`create` of `{id}`, which already exists"),
                ));
            }
            participant.created = true;
            self.pending_create = Some((who, line));
        }
        Ok(())
    }

    fn note(&mut self, rest: &str, line: usize) -> Result<(), Error> {
        let (head, body) = rest.split_once(':').unwrap_or((rest, ""));
        let words: Vec<&str> = head.split_whitespace().collect();
        let lower: Vec<String> = words.iter().map(|w| w.to_ascii_lowercase()).collect();
        let (side, names) = match lower.as_slice() {
            [a, b, ..] if a == "left" && b == "of" => (NoteSide::Left, words[2..].join(" ")),
            [a, b, ..] if a == "right" && b == "of" => (NoteSide::Right, words[2..].join(" ")),
            [a, ..] if a == "over" => (NoteSide::Over, words[1..].join(" ")),
            _ => {
                return Err(Error::new(
                    line,
                    "expected `Note left of`, `Note right of` or `Note over`",
                ));
            }
        };
        let names: Vec<&str> = names.split(',').collect();
        if names.len() > 2 || (side != NoteSide::Over && names.len() > 1) {
            return Err(Error::new(
                line,
                "a note spans at most two participants, and only `over` takes two",
            ));
        }
        let first = participant_name(names[0], line)?;
        let first = self.ensure(&first);
        let last = match names.get(1) {
            Some(name) => {
                let name = participant_name(name, line)?;
                self.ensure(&name)
            }
            None => first,
        };
        let (text, wrap) = text(body);
        self.seq.events.push(Event::Note(Note {
            side,
            first,
            last,
            text,
            wrap,
        }));
        Ok(())
    }

    fn activate(&mut self, who: usize) {
        self.active[who] += 1;
        self.seq.events.push(Event::Activate(who));
    }

    fn deactivate(&mut self, who: usize, line: usize) -> Result<(), Error> {
        if self.active[who] == 0 {
            return Err(Error::new(
                line,
                format!(
                    "`{}` is deactivated but is not active",
                    self.seq.participants[who].id
                ),
            ));
        }
        self.active[who] -= 1;
        self.seq.events.push(Event::Deactivate(who));
        Ok(())
    }

    fn autonumber(&mut self, rest: &str, line: usize) -> Result<(), Error> {
        if self.step == Number(0) {
            self.next_number = Number::ONE;
            self.step = Number::ONE;
        }
        let words: Vec<&str> = rest.split_whitespace().collect();
        let number = |w: &str| {
            Number::parse(w).ok_or_else(|| {
                Error::new(
                    line,
                    format!("`autonumber {rest}`: `{w}` is not a number with at most two decimals"),
                )
            })
        };
        match words.as_slice() {
            [] => self.numbering = Some((self.next_number, self.step)),
            [off] if off.eq_ignore_ascii_case("off") => self.numbering = None,
            [start] => {
                self.next_number = number(start)?;
                self.step = Number::ONE;
                self.numbering = Some((self.next_number, self.step));
            }
            [start, step] => {
                self.next_number = number(start)?;
                self.step = number(step)?;
                self.numbering = Some((self.next_number, self.step));
            }
            _ => {
                return Err(Error::new(
                    line,
                    format!("`autonumber {rest}`: expected `autonumber [start [step]]` or `off`"),
                ));
            }
        }
        Ok(())
    }

    fn message(&mut self, s: &str, line: usize) -> Result<(), Error> {
        let (left, right) = s.split_once(':').unwrap_or((s, ""));
        let Some((at, arrow)) = find_arrow(left) else {
            let word = split_word(s).0;
            return Err(Error::new(
                line,
                format!(
                    "`{word}` is not a sequenceDiagram statement; expected a message such as \
                     `A->>B: text` (see DESIGN.md)"
                ),
            ));
        };
        let mut from = left[..at].trim();
        let mut to = left[at + arrow.token.len()..].trim();
        let mut message = Message {
            dashed: arrow.dashed,
            start: arrow.start,
            end: arrow.end,
            half: arrow.half,
            line,
            ..Message::default()
        };
        if let Some(f) = from.strip_suffix("()") {
            from = f.trim();
            message.dot_from = true;
        }
        if let Some(t) = to.strip_prefix("()") {
            to = t.trim();
            message.dot_to = true;
        } else if let Some(t) = to.strip_prefix('+') {
            to = t.trim();
            message.activate = true;
        } else if let Some(t) = to.strip_prefix('-') {
            to = t.trim();
            message.deactivate = true;
        }
        let from = participant_name(from, line)?;
        let to = participant_name(to, line)?;
        message.from = self.ensure(&from);
        message.to = self.ensure(&to);
        (message.text, message.wrap) = text(right);

        if let Some((who, created_at)) = self.pending_create.take() {
            if message.to != who {
                return Err(Error::new(
                    created_at,
                    format!(
                        "`create` of `{}` must be followed by a message to it",
                        self.seq.participants[who].id
                    ),
                ));
            }
            message.creates = true;
        }
        let (ends, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_destroy)
            .into_iter()
            .partition(|(who, _)| *who == message.from || *who == message.to);
        if let Some((who, destroyed_at)) = waiting.first() {
            return Err(Error::new(
                *destroyed_at,
                format!(
                    "`destroy` of `{}` must be followed by a message to or from it",
                    self.seq.participants[*who].id
                ),
            ));
        }
        message.destroys = ends.into_iter().map(|(who, _)| who).collect();

        if let Some((number, step)) = self.numbering {
            message.number = Some(number);
            self.next_number = Number(number.0 + step.0);
            self.numbering = Some((self.next_number, step));
        }
        let (activate, deactivate) = (message.activate, message.deactivate);
        let (from, to) = (message.from, message.to);
        self.seq.events.push(Event::Message(message));
        if activate {
            self.activate(to);
        }
        if deactivate {
            self.deactivate(from, line)?;
        }
        Ok(())
    }
}

struct Arrow {
    token: &'static str,
    dashed: bool,
    start: Head,
    end: Head,
    half: bool,
}

/// Every arrow mermaid's lexer knows, longest first so `-->>` is not read
/// as `-->` followed by `>`.
const ARROWS: &[(&str, bool, Head, Head, bool)] = &[
    ("<<-->>", true, Head::Arrow, Head::Arrow, false),
    ("<<->>", false, Head::Arrow, Head::Arrow, false),
    ("--|\\", true, Head::None, Head::Arrow, true),
    ("--|/", true, Head::None, Head::Arrow, true),
    ("--\\\\", true, Head::None, Head::Open, true),
    ("--//", true, Head::None, Head::Open, true),
    ("/|--", true, Head::Arrow, Head::None, true),
    ("\\|--", true, Head::Arrow, Head::None, true),
    ("//--", true, Head::Open, Head::None, true),
    ("\\\\--", true, Head::Open, Head::None, true),
    ("-->>", true, Head::None, Head::Arrow, false),
    ("-|\\", false, Head::None, Head::Arrow, true),
    ("-|/", false, Head::None, Head::Arrow, true),
    ("-\\\\", false, Head::None, Head::Open, true),
    ("-//", false, Head::None, Head::Open, true),
    ("/|-", false, Head::Arrow, Head::None, true),
    ("\\|-", false, Head::Arrow, Head::None, true),
    ("//-", false, Head::Open, Head::None, true),
    ("\\\\-", false, Head::Open, Head::None, true),
    ("->>", false, Head::None, Head::Arrow, false),
    ("-->", true, Head::None, Head::None, false),
    ("--x", true, Head::None, Head::Cross, false),
    ("--)", true, Head::None, Head::Open, false),
    ("->", false, Head::None, Head::None, false),
    ("-x", false, Head::None, Head::Cross, false),
    ("-)", false, Head::None, Head::Open, false),
];

/// The first arrow in `s`, and where it starts.
fn find_arrow(s: &str) -> Option<(usize, Arrow)> {
    let lower = s.to_ascii_lowercase();
    for (at, c) in lower.char_indices() {
        if !matches!(c, '-' | '<' | '/' | '\\') {
            continue;
        }
        for &(token, dashed, start, end, half) in ARROWS {
            if lower[at..].starts_with(token) {
                return Some((
                    at,
                    Arrow {
                        token,
                        dashed,
                        start,
                        end,
                        half,
                    },
                ));
            }
        }
    }
    None
}

/// A participant's id, checked against what mermaid's lexer accepts.
fn participant_name(raw: &str, line: usize) -> Result<String, Error> {
    let name = restore(raw.trim());
    if name.is_empty() {
        return Err(Error::new(line, "expected a participant name"));
    }
    if let Some(c) = name
        .chars()
        .find(|c| matches!(c, '<' | '>' | ':' | ',' | ';' | '+' | '(' | ')' | '@'))
    {
        return Err(Error::new(
            line,
            format!("participant name `{name}` may not contain `{c}`"),
        ));
    }
    Ok(name)
}

/// The first whitespace- or colon-delimited word, and the rest.
fn split_word(s: &str) -> (&str, &str) {
    let end = s
        .find(|c: char| c.is_whitespace() || c == ':')
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

/// The text after `accTitle:` or `accDescr:`.
fn colon_value(rest: &str, keyword: &str, line: usize) -> Result<String, Error> {
    rest.trim()
        .strip_prefix(':')
        .map(|v| restore(v.trim()))
        .ok_or_else(|| Error::new(line, format!("expected `{keyword}: …`")))
}

/// `@{ "type": "database", "alias": "DB" }`: the kind and alias it sets.
fn metadata(body: &str, line: usize) -> Result<(Option<ParticipantKind>, Option<String>), Error> {
    let (mut kind, mut alias) = (None, None);
    for pair in body.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (key, value) = pair.split_once(':').ok_or_else(|| {
            Error::new(
                line,
                format!("expected `key: value` in participant metadata, found `{pair}`"),
            )
        })?;
        let unquote = |s: &str| s.trim().trim_matches(['"', '\'']).trim().to_string();
        let (key, value) = (unquote(key), unquote(value));
        match key.as_str() {
            "type" => {
                kind = Some(match value.to_ascii_lowercase().as_str() {
                    "participant" => ParticipantKind::Participant,
                    "actor" => ParticipantKind::Actor,
                    "boundary" => ParticipantKind::Boundary,
                    "control" => ParticipantKind::Control,
                    "entity" => ParticipantKind::Entity,
                    "database" => ParticipantKind::Database,
                    "collections" => ParticipantKind::Collections,
                    "queue" => ParticipantKind::Queue,
                    _ => {
                        return Err(Error::new(
                            line,
                            format!("participant type `{value}` is not supported (see DESIGN.md)"),
                        ));
                    }
                })
            }
            "alias" => alias = Some(restore(&value)),
            _ => {
                return Err(Error::new(
                    line,
                    format!("participant metadata `{key}` is not supported (see DESIGN.md)"),
                ));
            }
        }
    }
    Ok((kind, alias))
}

/// `box [colour] [label]`, as mermaid's `parseBoxData` reads it: a leading
/// word that is a colour is the fill, the rest is the label.
fn parse_box(rest: &str, line: usize) -> Result<ParticipantBox, Error> {
    let lower = rest.to_ascii_lowercase();
    let (color, label) = if lower.starts_with("rgb") || lower.starts_with("hsl") {
        match rest.find(')') {
            Some(end) => (Some(&rest[..=end]), rest[end + 1..].trim()),
            None => (None, rest),
        }
    } else {
        let (word, tail) = split_word(rest);
        if word.eq_ignore_ascii_case("transparent") {
            (None, tail.trim())
        } else if !word.is_empty() && style::color(word, line).is_ok() {
            (Some(word), tail.trim())
        } else {
            (None, rest)
        }
    };
    let fill = match color {
        Some(c) => Some(blended(c, line)?),
        None => None,
    };
    let label = text(label).0;
    Ok(ParticipantBox {
        label: (!label.is_empty()).then_some(label),
        fill,
    })
}

/// `rect rgb(…)`: the background colour.
fn rect_color(rest: &str, line: usize) -> Result<Rgb, Error> {
    if rest.is_empty() {
        return Err(Error::new(
            line,
            "`rect` needs a colour, such as `rect rgb(220, 235, 255)`",
        ));
    }
    blended(rest, line)
}

/// A colour, with any alpha blended over white: `rect rgba(0, 0, 255, .1)`
/// is mermaid's usual way to tint a region, and drawn opaque it would hide
/// everything behind it.
fn blended(value: &str, line: usize) -> Result<Rgb, Error> {
    let (r, g, b) = style::color(value, line)?;
    let alpha = alpha(value).clamp(0.0, 1.0);
    let mix = |c: u8| (f32::from(c) * alpha + 255.0 * (1.0 - alpha)).round() as u8;
    Ok((mix(r), mix(g), mix(b)))
}

/// The alpha of `rgba(…)`, `#rrggbbaa` or `#rgba`; 1 otherwise.
fn alpha(value: &str) -> f32 {
    let v = value.trim();
    if let Some(args) = v
        .to_ascii_lowercase()
        .strip_prefix("rgba")
        .and_then(|s| s.trim().strip_prefix('('))
        .and_then(|s| s.strip_suffix(')'))
    {
        let parts: Vec<&str> = args.split([',', '/']).collect();
        if parts.len() == 4 {
            let a = parts[3].trim();
            return match a.strip_suffix('%') {
                Some(pct) => pct.trim().parse::<f32>().map_or(1.0, |p| p / 100.0),
                None => a.parse().unwrap_or(1.0),
            };
        }
    }
    if let Some(h) = v.strip_prefix('#') {
        let digit = |s: &str| u8::from_str_radix(s, 16).ok();
        return match h.len() {
            4 => digit(&h[3..4]).map_or(1.0, |d| f32::from(d * 17) / 255.0),
            8 => digit(&h[6..8]).map_or(1.0, |d| f32::from(d) / 255.0),
            _ => 1.0,
        };
    }
    1.0
}

/// Message, note or label text: `wrap:`/`nowrap:` stripped (and whether it
/// asked to wrap), `<br>` as a newline, entities restored.
fn text(raw: &str) -> (String, bool) {
    let mut t = raw.trim();
    let mut wrap = false;
    for (prefix, wraps) in [("wrap:", true), ("nowrap:", false)] {
        if t.len() >= prefix.len() && t[..prefix.len()].eq_ignore_ascii_case(prefix) {
            t = t[prefix.len()..].trim();
            wrap = wraps;
        }
    }
    let lines: Vec<String> = split_breaks(t).iter().map(|l| restore(l.trim())).collect();
    (lines.join("\n"), wrap)
}

/// Split on `<br>`, `<br/>` and `<br />`, in any case.
fn split_breaks(t: &str) -> Vec<&str> {
    let lower = t.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while let Some(off) = lower[i..].find("<br") {
        let at = i + off;
        let tail = &lower[at + 3..];
        let spaces = tail.len() - tail.trim_start().len();
        let after = &tail[spaces..];
        let close = if after.starts_with("/>") {
            Some(2)
        } else if after.starts_with('>') {
            Some(1)
        } else {
            None
        };
        match close {
            Some(n) => {
                out.push(&t[start..at]);
                start = at + 3 + spaces + n;
                i = start;
            }
            None => i = at + 3,
        }
    }
    out.push(&t[start..]);
    out
}

// Decoded entities that would otherwise end or split a statement stand in
// as private-use characters until the text is read.
const SEMI: char = '\u{E000}';
const HASH: char = '\u{E001}';
const COLON: char = '\u{E002}';
const COMMA: char = '\u{E003}';

/// Decode `#name;` and `#123;` entity codes, as mermaid does before it
/// parses.
fn encode_entities(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let name_len = after
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(after.len());
        if name_len > 0 && after[name_len..].starts_with(';') {
            let name = &after[..name_len];
            match entity(name) {
                Some(c) => out.push(match c {
                    ';' => SEMI,
                    '#' => HASH,
                    ':' => COLON,
                    ',' => COMMA,
                    c => c,
                }),
                None => {
                    out.push('&');
                    out.push_str(name);
                    out.push(SEMI);
                }
            }
            rest = &after[name_len + 1..];
        } else {
            out.push('#');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn entity(name: &str) -> Option<char> {
    if let Ok(code) = name.parse::<u32>() {
        return char::from_u32(code);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "semi" => ';',
        "num" => '#',
        "colon" => ':',
        "comma" => ',',
        "hearts" => '♥',
        "copy" => '©',
        "reg" => '®',
        "deg" => '°',
        "larr" => '←',
        "rarr" => '→',
        "uarr" => '↑',
        "darr" => '↓',
        "harr" => '↔',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "times" => '×',
        "check" => '✓',
        _ => return None,
    })
}

/// Put decoded punctuation back.
fn restore(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            SEMI => ';',
            HASH => '#',
            COLON => ':',
            COMMA => ',',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(seq: &Sequence) -> Vec<&Message> {
        seq.messages().collect()
    }

    #[test]
    fn reads_participants_aliases_and_implicit_ones() {
        let seq =
            parse("sequenceDiagram\n  participant A as Alice<br>Smith\n  actor B\n  A->>C: hi\n")
                .unwrap();
        let names: Vec<_> = seq.participants.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(names, ["A", "B", "C"]);
        assert_eq!(seq.participants[0].label, "Alice\nSmith");
        assert_eq!(seq.participants[1].kind, ParticipantKind::Actor);
    }

    #[test]
    fn reads_every_arrow() {
        let cases = [
            ("->", false, Head::None, Head::None),
            ("-->", true, Head::None, Head::None),
            ("->>", false, Head::None, Head::Arrow),
            ("-->>", true, Head::None, Head::Arrow),
            ("-x", false, Head::None, Head::Cross),
            ("--x", true, Head::None, Head::Cross),
            ("-)", false, Head::None, Head::Open),
            ("--)", true, Head::None, Head::Open),
            ("<<->>", false, Head::Arrow, Head::Arrow),
            ("<<-->>", true, Head::Arrow, Head::Arrow),
            ("-|\\", false, Head::None, Head::Arrow),
            ("//--", true, Head::Open, Head::None),
        ];
        for (arrow, dashed, start, end) in cases {
            let seq = parse(&format!("sequenceDiagram\nmy-api{arrow}B: x\n")).unwrap();
            let m = messages(&seq)[0];
            assert_eq!(seq.participants[m.from].id, "my-api", "{arrow}");
            assert_eq!(seq.participants[m.to].id, "B", "{arrow}");
            assert_eq!((m.dashed, m.start, m.end), (dashed, start, end), "{arrow}");
        }
    }

    #[test]
    fn activation_shorthand_and_errors() {
        let seq = parse("sequenceDiagram\nA->>+B: a\nB-->>-A: b\n").unwrap();
        assert!(matches!(seq.events[1], Event::Activate(1)));
        assert!(matches!(seq.events[3], Event::Deactivate(1)));
        let error = parse("sequenceDiagram\nA->>B: a\ndeactivate B\n").unwrap_err();
        assert_eq!(error.line, 3);
    }

    #[test]
    fn frames_must_nest_and_close() {
        let error = parse("sequenceDiagram\nloop x\nA->>B: a\n").unwrap_err();
        assert_eq!(error.line, 2);
        let error = parse("sequenceDiagram\nloop x\nelse y\nend\n").unwrap_err();
        assert!(error.message.contains("else"), "{error}");
        assert!(parse("sequenceDiagram\nalt a\nA->>B: a\nelse b\nA->>B: b\nend\n").is_ok());
    }

    #[test]
    fn autonumber_counts_messages() {
        let seq = parse(
            "sequenceDiagram\nautonumber 10 5\nA->>B: a\nB->>A: b\nautonumber off\nA->>B: c\n",
        )
        .unwrap();
        let numbers: Vec<_> = messages(&seq).iter().map(|m| m.number).collect();
        assert_eq!(numbers, [Some(Number(1000)), Some(Number(1500)), None]);
    }

    #[test]
    fn autonumber_takes_decimals_without_float_drift() {
        let mut src = String::from("sequenceDiagram\nautonumber 1.1 0.1\n");
        for _ in 0..12 {
            src.push_str("A->>B: m\n");
        }
        let seq = parse(&src).unwrap();
        let shown: Vec<String> = messages(&seq)
            .iter()
            .map(|m| m.number.unwrap().to_string())
            .collect();
        assert_eq!(
            shown,
            [
                "1.1", "1.2", "1.3", "1.4", "1.5", "1.6", "1.7", "1.8", "1.9", "2", "2.1", "2.2"
            ]
        );
        let seq = parse("sequenceDiagram\nautonumber .25 .25\nA->>B: a\nA->>B: b\n").unwrap();
        let shown: Vec<String> = messages(&seq)
            .iter()
            .map(|m| m.number.unwrap().to_string())
            .collect();
        assert_eq!(shown, ["0.25", "0.5"]);
    }

    #[test]
    fn autonumber_rejects_three_decimals() {
        let error = parse("sequenceDiagram\nautonumber 1.125\nA->>B: a\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("1.125"), "{error}");
        assert!(parse("sequenceDiagram\nautonumber 1.\n").is_err());
        assert!(parse("sequenceDiagram\nautonumber x\n").is_err());
    }

    #[test]
    fn create_and_destroy_need_their_message() {
        let seq = parse(
            "sequenceDiagram\nA->>B: a\ncreate participant C\nA->>C: new\ndestroy C\nC->>A: bye\n",
        )
        .unwrap();
        let m = messages(&seq);
        assert!(m[1].creates);
        assert_eq!(m[2].destroys, [2]);
        let error = parse("sequenceDiagram\ncreate participant C\nA->>B: a\n").unwrap_err();
        assert_eq!(error.line, 2);
    }

    #[test]
    fn entities_semicolons_and_comments() {
        let seq = parse("sequenceDiagram\n# a comment\nA->>B: I #9829; you#59; ok; B->>A: #35;1\n")
            .unwrap();
        let texts: Vec<_> = messages(&seq).iter().map(|m| m.text.clone()).collect();
        assert_eq!(texts, ["I ♥ you; ok", "#1"]);
    }

    #[test]
    fn boxes_take_a_colour_and_label() {
        let seq = parse(
            "sequenceDiagram\nbox Aqua Front end\nparticipant A\nend\nbox rgba(0,0,255,0.5) Back\nparticipant B\nend\nbox Just a label\nparticipant C\nend\n",
        )
        .unwrap();
        assert_eq!(seq.boxes[0].fill, Some((0, 255, 255)));
        assert_eq!(seq.boxes[0].label.as_deref(), Some("Front end"));
        assert_eq!(seq.boxes[1].fill, Some((128, 128, 255)));
        assert_eq!(seq.boxes[2].fill, None);
        assert_eq!(seq.boxes[2].label.as_deref(), Some("Just a label"));
        assert_eq!(seq.participants[2].group, Some(2));
    }

    #[test]
    fn unknown_statements_name_themselves() {
        let error = parse("sequenceDiagram\n  frobnicate A\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("frobnicate"), "{error}");
    }
}
