//! ZenUML parsing, into the `sequence` model.
//!
//! The real grammar is the ANTLR grammar inside `@zenuml/core`, which the
//! mermaid checkout does not contain. This follows the syntax page
//! (`packages/mermaid/src/docs/syntax/zenuml.md`), the plugin's demo
//! (`demos/zenuml.html`) and its rendering test.
//!
//! Statements are code-like and nest with `{ … }`, so this is a small
//! recursive-descent parser over the whole text rather than a line loop.
//! A statement ends at a newline, `;` or a closing `}`.
//!
//! The mapping:
//! - `A.m(args)` is a sync call from the current context (the participant
//!   whose block encloses it, or an unnamed starter at the top level); the
//!   callee is activated for the call and its `{ … }` block.
//! - `x = A.m()` adds a dashed return labelled `x`; so does `return x`.
//! - `A->B: text` is async (open arrowhead); after `@return` it is dashed.
//! - `new A(args)` creates `A`.
//! - `if`/`else` is `alt`, the loops are `loop`, `try` is its own frame.
//! - `// comment` lines before a message become a note over it.
//! - `ref(Name, A, B)` is a `ref` frame labelled `Name` across `A`..`B`
//!   (every participant when none is named); `==text==` is a divider.

use crate::sequence::{
    Event, Frame, Head, Message, Note, NoteSide, Participant, ParticipantBox, ParticipantKind, Ref,
    Sequence,
};
use crate::{Error, strip_comment};

/// The id of the unnamed participant top-level calls come from.
const STARTER: &str = "_STARTER_";

/// Parse a zenuml source.
pub fn parse(source: &str) -> Result<Sequence, Error> {
    // Blank `%%` comments and the header, keeping one text line per
    // source line so offsets map back to line numbers.
    let mut text = String::with_capacity(source.len() + 1);
    let mut header = false;
    for (index, raw) in source.lines().enumerate() {
        let line = strip_comment(raw);
        if !header {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                let rest = trimmed
                    .strip_prefix("zenuml")
                    .filter(|r| r.is_empty() || r.starts_with(char::is_whitespace))
                    .ok_or_else(|| Error::new(index + 1, "expected `zenuml`"))?;
                header = true;
                text.push_str(rest);
            }
            text.push('\n');
            continue;
        }
        text.push_str(line);
        text.push('\n');
    }
    if !header {
        return Err(Error::new(1, "expected `zenuml`"));
    }
    let mut p = Parser {
        s: &text,
        pos: 0,
        seq: Sequence::default(),
        stack: vec![Context::default()],
        comments: Vec::new(),
        reply: None,
        group: None,
        starter: None,
        messaged: Vec::new(),
    };
    p.block(None)?;
    p.finish()
}

/// Whose block encloses the current statement.
#[derive(Default)]
struct Context {
    /// `None` at the top level until a statement needs a sender.
    who: Option<usize>,
    /// Where a `return` goes.
    caller: Option<usize>,
    /// A `return` has been drawn for this call.
    returned: bool,
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
    seq: Sequence,
    stack: Vec<Context>,
    /// `//` comments waiting for the message they annotate.
    comments: Vec<String>,
    /// `@return`/`@reply`: the line, until the async message it marks.
    reply: Option<usize>,
    /// The open `group`.
    group: Option<usize>,
    /// The implicit starter, once something needs it.
    starter: Option<usize>,
    /// Per participant: a message has touched it.
    messaged: Vec<bool>,
}

impl<'a> Parser<'a> {
    // ---- the cursor ---------------------------------------------------

    fn line(&self) -> usize {
        self.s[..self.pos].matches('\n').count() + 1
    }

    fn err(&self, message: impl Into<String>) -> Error {
        Error::new(self.line(), message)
    }

    fn rest(&self) -> &'a str {
        &self.s[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.pos += token.len();
            true
        } else {
            false
        }
    }

    /// Spaces within the line.
    fn skip_spaces(&mut self) {
        let rest = self.rest();
        self.pos += rest.len() - rest.trim_start_matches([' ', '\t', '\r']).len();
    }

    /// Spaces, newlines and `;`.
    fn skip_ws(&mut self) {
        let rest = self.rest();
        self.pos += rest.len()
            - rest
                .trim_start_matches(|c: char| c.is_whitespace() || c == ';')
                .len();
    }

    /// The identifier at the cursor, without consuming it.
    fn word(&self) -> &str {
        let rest = self.rest();
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        &rest[..end]
    }

    fn ident(&mut self) -> Option<String> {
        let word = self.word().to_string();
        if word.is_empty() {
            return None;
        }
        self.pos += word.len();
        Some(word)
    }

    /// Consume `keyword` if it is the whole word at the cursor.
    fn eat_word(&mut self, keyword: &str) -> bool {
        if self.word() == keyword {
            self.pos += keyword.len();
            true
        } else {
            false
        }
    }

    /// An identifier or a `"quoted name"`.
    fn name(&mut self) -> Result<Option<String>, Error> {
        if self.peek() != Some('"') {
            return Ok(self.ident());
        }
        let start = self.pos + 1;
        match self.s[start..].find(['"', '\n']) {
            Some(end) if self.s[start + end..].starts_with('"') => {
                self.pos = start + end + 1;
                Ok(Some(self.s[start..start + end].to_string()))
            }
            _ => Err(self.err("a quoted name is never closed with `\"`")),
        }
    }

    /// The rest of the line, consumed up to (not including) the newline.
    fn rest_of_line(&mut self) -> String {
        let rest = self.rest();
        let end = rest.find('\n').unwrap_or(rest.len());
        self.pos += end;
        rest[..end].trim().to_string()
    }

    /// `( … )` at the cursor: its inside, whitespace collapsed.
    fn parens(&mut self) -> Result<String, Error> {
        let open = self.line();
        let start = self.pos + 1;
        let mut depth = 0;
        let mut quoted = false;
        for (i, c) in self.rest().char_indices() {
            match c {
                '"' => quoted = !quoted,
                '(' if !quoted => depth += 1,
                ')' if !quoted => {
                    depth -= 1;
                    if depth == 0 {
                        let inner = &self.s[start..self.pos + i];
                        self.pos += i + 1;
                        return Ok(inner.split_whitespace().collect::<Vec<_>>().join(" "));
                    }
                }
                _ => {}
            }
        }
        Err(Error::new(open, "`(` is never closed with `)`"))
    }

    /// An optional `(condition)` after a fragment keyword.
    fn condition(&mut self) -> Result<String, Error> {
        self.skip_spaces();
        if self.peek() == Some('(') {
            self.parens()
        } else {
            Ok(String::new())
        }
    }

    // ---- statements ---------------------------------------------------

    /// Statements up to the `}` closing a block opened on line `open`, or
    /// to the end of the text.
    fn block(&mut self, open: Option<usize>) -> Result<(), Error> {
        loop {
            self.skip_ws();
            match self.peek() {
                None => {
                    return match open {
                        Some(line) => Err(Error::new(line, "`{` is never closed with `}`")),
                        None => Ok(()),
                    };
                }
                Some('}') => {
                    if open.is_none() {
                        return Err(self.err("`}` without an open `{`"));
                    }
                    self.pos += 1;
                    // ZenUML shows a comment only above a message or
                    // fragment; one closing a block is not drawn there
                    // either.
                    self.comments.clear();
                    return Ok(());
                }
                _ if self.rest().starts_with("//") => {
                    self.pos += 2;
                    let comment = self.rest_of_line();
                    self.comments.push(comment);
                }
                _ => {
                    self.statement(open.is_none())?;
                    self.end_statement()?;
                }
            }
        }
    }

    fn end_statement(&mut self) -> Result<(), Error> {
        self.skip_spaces();
        match self.peek() {
            None | Some('\n' | ';' | '}') => Ok(()),
            _ if self.rest().starts_with("//") => Ok(()),
            _ => {
                let rest = self.rest();
                let snippet = rest[..rest.find('\n').unwrap_or(rest.len())].trim();
                Err(self.err(format!("unexpected `{snippet}`")))
            }
        }
    }

    fn statement(&mut self, top: bool) -> Result<(), Error> {
        let line = self.line();
        if self.eat("@") {
            let annotator = self
                .ident()
                .ok_or_else(|| self.err("expected an annotator name after `@`"))?;
            return match annotator.to_ascii_lowercase().as_str() {
                "return" | "reply" => {
                    self.reply = Some(line);
                    Ok(())
                }
                "starter" => self.starter_annotation(),
                _ => self.declare(Some(annotator), top),
            };
        }
        if self.rest().starts_with("<<") {
            return self.declare(None, top);
        }
        if self.rest().starts_with("==") {
            return self.divider();
        }
        match self.word() {
            "title" if top => {
                self.pos += "title".len();
                self.seq.title = Some(self.rest_of_line());
                self.comments.clear();
                Ok(())
            }
            "group" => self.group(top),
            "if" => self.alt(),
            "while" | "for" | "forEach" | "foreach" | "loop" => {
                self.ident();
                let label = self.condition()?;
                self.fragment(Frame::Loop, label)
            }
            "opt" | "par" | "critical" => {
                let frame = match self.ident().as_deref() {
                    Some("opt") => Frame::Opt,
                    Some("par") => Frame::Par,
                    _ => Frame::Critical,
                };
                let label = self.condition()?;
                self.fragment(frame, label)
            }
            "try" => self.try_catch(),
            "return" => self.ret(),
            "ref" => self.reference(),
            "section" => Err(self.err("`section` is not supported in zenuml (see DESIGN.md)")),
            word @ ("else" | "catch" | "finally") => {
                let opener = if word == "else" { "if" } else { "try" };
                Err(self.err(format!("`{word}` without a preceding `{opener} {{ … }}`")))
            }
            _ => self.message(top),
        }
    }

    /// `@Starter(A)`: top-level calls come from `A`.
    fn starter_annotation(&mut self) -> Result<(), Error> {
        self.skip_spaces();
        if self.peek() != Some('(') {
            return Err(self.err("expected `@Starter(Participant)`"));
        }
        let inner = self.parens()?;
        let name = inner.trim().trim_matches('"');
        if name.is_empty() {
            return Err(self.err("expected `@Starter(Participant)`"));
        }
        let who = self.ensure(name);
        self.stack[0].who = Some(who);
        self.comments.clear();
        Ok(())
    }

    /// A participant declaration: `@Annotator <<stereotype>> Name as Label`.
    fn declare(&mut self, annotator: Option<String>, top: bool) -> Result<(), Error> {
        if !top {
            return Err(self.err(
                "participants are declared at the top level or in a `group`, not inside a block",
            ));
        }
        self.skip_spaces();
        let stereotype = if self.eat("<<") {
            let rest = self.rest();
            let end = rest
                .find(">>")
                .filter(|&end| !rest[..end].contains('\n'))
                .ok_or_else(|| self.err("`<<` is never closed with `>>`"))?;
            let stereotype = rest[..end].trim().to_string();
            self.pos += end + 2;
            self.skip_spaces();
            Some(stereotype)
        } else {
            None
        };
        let id = self
            .name()?
            .ok_or_else(|| self.err("expected a participant name"))?;
        self.skip_spaces();
        let label = if self.eat_word("as") {
            self.skip_spaces();
            Some(
                self.name()?
                    .ok_or_else(|| self.err(format!("expected a label after `{id} as`")))?,
            )
        } else {
            None
        };
        self.skip_spaces();
        match self.peek() {
            Some('#') => {
                return Err(self.err(format!(
                    "participant colours (`{id} #…`) are not supported in zenuml (see DESIGN.md)"
                )));
            }
            Some(c) if c.is_ascii_digit() => {
                return Err(self.err(format!(
                    "participant widths (`{id} {}`) are not supported in zenuml (see DESIGN.md)",
                    self.word()
                )));
            }
            _ => {}
        }

        let kind = match annotator.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None => ParticipantKind::Participant,
            Some("actor") => ParticipantKind::Actor,
            Some("boundary") => ParticipantKind::Boundary,
            Some("control") => ParticipantKind::Control,
            Some("entity") => ParticipantKind::Entity,
            Some("database") => ParticipantKind::Database,
            // The cloud icons (`@EC2`, `@Lambda` …): a plain box, with
            // the annotator kept as a stereotype line.
            Some(_) => ParticipantKind::Participant,
        };
        let mut lines = Vec::new();
        if let Some(s) = stereotype {
            lines.push(format!("«{s}»"));
        }
        if let (Some(a), ParticipantKind::Participant) = (&annotator, kind) {
            lines.push(format!("«{a}»"));
        }
        lines.push(label.unwrap_or_else(|| id.clone()));

        let who = self.ensure(&id);
        let group = self.group;
        let participant = &mut self.seq.participants[who];
        if let (Some(old), Some(new)) = (participant.group, group)
            && old != new
        {
            return Err(self.err(format!(
                "`{id}` is already in another `group`; a participant belongs to one group"
            )));
        }
        participant.group = participant.group.or(group);
        participant.kind = kind;
        participant.label = lines.join("\n");
        self.comments.clear();
        Ok(())
    }

    /// `group Name { A B … }`: a box around its participants.
    fn group(&mut self, top: bool) -> Result<(), Error> {
        if !top {
            return Err(self.err("a `group` is declared at the top level, not inside a block"));
        }
        self.pos += "group".len();
        self.skip_spaces();
        let label = if self.peek() == Some('{') {
            None
        } else {
            self.name()?
        };
        self.skip_ws();
        let open = self.line();
        if !self.eat("{") {
            return Err(self.err("expected `{` after `group`"));
        }
        let index = self.seq.boxes.len();
        self.seq.boxes.push(ParticipantBox { label, fill: None });
        self.group = Some(index);
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Err(Error::new(open, "`group {` is never closed with `}`")),
                Some('}') => {
                    self.pos += 1;
                    break;
                }
                _ if self.rest().starts_with("//") => {
                    self.rest_of_line();
                }
                Some('@') => {
                    self.pos += 1;
                    let annotator = self
                        .ident()
                        .ok_or_else(|| self.err("expected an annotator name after `@`"))?;
                    self.declare(Some(annotator), true)?;
                }
                _ => self.declare(None, true)?,
            }
        }
        self.group = None;
        self.comments.clear();
        if !self.seq.participants.iter().any(|p| p.group == Some(index)) {
            return Err(Error::new(
                open,
                "a `group` must declare at least one participant",
            ));
        }
        Ok(())
    }

    /// `{ … }` after a fragment header.
    fn body(&mut self) -> Result<(), Error> {
        self.skip_ws();
        let open = self.line();
        if !self.eat("{") {
            return Err(self.err("expected `{`"));
        }
        self.block(Some(open))
    }

    fn fragment(&mut self, frame: Frame, label: String) -> Result<(), Error> {
        self.seq.events.push(Event::Start(frame, label));
        self.body()?;
        self.seq.events.push(Event::End);
        Ok(())
    }

    /// Consume `keyword` if it is the next word, across line breaks.
    fn next_word(&mut self, keyword: &str) -> bool {
        let save = self.pos;
        self.skip_ws();
        if self.eat_word(keyword) {
            true
        } else {
            self.pos = save;
            false
        }
    }

    /// `if (c) { } else if (c) { } else { }`: an `alt`.
    fn alt(&mut self) -> Result<(), Error> {
        self.pos += "if".len();
        let label = self.condition()?;
        self.seq.events.push(Event::Start(Frame::Alt, label));
        self.body()?;
        while self.next_word("else") {
            self.skip_ws();
            if self.eat_word("if") {
                let label = self.condition()?;
                self.seq.events.push(Event::Section(label));
                self.body()?;
            } else {
                self.seq.events.push(Event::Section("else".into()));
                self.body()?;
                break;
            }
        }
        self.seq.events.push(Event::End);
        Ok(())
    }

    /// `try { } catch (e) { } finally { }`.
    fn try_catch(&mut self) -> Result<(), Error> {
        self.pos += "try".len();
        self.seq
            .events
            .push(Event::Start(Frame::Try, String::new()));
        self.body()?;
        while self.next_word("catch") {
            let caught = self.condition()?;
            let label = if caught.is_empty() {
                "catch".to_string()
            } else {
                format!("catch({caught})")
            };
            self.seq.events.push(Event::Section(label));
            self.body()?;
        }
        if self.next_word("finally") {
            self.seq.events.push(Event::Section("finally".into()));
            self.body()?;
        }
        self.seq.events.push(Event::End);
        Ok(())
    }

    /// `==text==`: a divider across every lifeline. As in ZenUML, the
    /// label is the line with its `=` fences trimmed.
    fn divider(&mut self) -> Result<(), Error> {
        let line = self.rest_of_line();
        let label = line.trim_matches('=').trim().to_string();
        self.seq.events.push(Event::Divider(label));
        Ok(())
    }

    /// `ref(Name, A, B)`: an interaction use labelled `Name`, across the
    /// participants named after it. A quoted argument is the label
    /// wherever it stands, so `ref(A, B, "Name")` reads the same.
    fn reference(&mut self) -> Result<(), Error> {
        let line = self.line();
        self.pos += "ref".len();
        self.skip_spaces();
        if self.peek() != Some('(') {
            return Err(self.err("expected `ref(Name)` or `ref(Name, A, B)`"));
        }
        let inner = self.parens()?;
        let mut args: Vec<(String, bool)> = Vec::new();
        let mut current = String::new();
        let mut quoted = false;
        let mut was_quoted = false;
        for c in inner.chars().chain([',']) {
            match c {
                '"' => {
                    quoted = !quoted;
                    was_quoted = true;
                }
                ',' if !quoted => {
                    let arg = current.trim().to_string();
                    if arg.is_empty() {
                        return Err(Error::new(
                            line,
                            "`ref(…)` has an empty argument; expected `ref(Name, A, B)`",
                        ));
                    }
                    args.push((arg, was_quoted));
                    current.clear();
                    was_quoted = false;
                }
                c => current.push(c),
            }
        }
        let at = args.iter().position(|(_, q)| *q).unwrap_or(0);
        let (label, _) = args.remove(at);
        let participants = args.iter().map(|(name, _)| self.ensure(name)).collect();
        self.seq.events.push(Event::Ref(Ref {
            label,
            participants,
        }));
        Ok(())
    }

    /// `return x`: a dashed reply to the caller.
    fn ret(&mut self) -> Result<(), Error> {
        self.pos += "return".len();
        self.skip_spaces();
        let rest = self.rest();
        let end = rest.find(['\n', ';', '}']).unwrap_or(rest.len());
        let end = rest[..end].find("//").unwrap_or(end);
        let value = rest[..end].trim().to_string();
        self.pos += end;
        let context = self
            .stack
            .last()
            .expect("the top level is always on the stack");
        let (Some(who), Some(caller)) = (context.who, context.caller) else {
            return Err(self.err("`return` outside a method's `{ … }` block"));
        };
        self.reply_message(who, caller, value);
        self.stack.last_mut().expect("non-empty").returned = true;
        Ok(())
    }

    /// A message statement, possibly assigned: `x = A.m()`,
    /// `A->B: text`, `new A()`, `m()` — or a bare participant declaration.
    fn message(&mut self, top: bool) -> Result<(), Error> {
        let line = self.line();
        let start = self.pos;
        let assignee = self.assignment();
        self.skip_spaces();
        if self.eat_word("new") {
            return self.creation(assignee);
        }
        let Some(first) = self.name()? else {
            let c = self.peek().unwrap_or(' ');
            return Err(self.err(format!("unexpected `{c}`: expected a zenuml statement")));
        };
        self.skip_spaces();
        if self.eat("->") {
            self.skip_spaces();
            let to = self
                .name()?
                .ok_or_else(|| self.err(format!("expected a participant after `{first}->`")))?;
            self.skip_spaces();
            let from = self.ensure(&first);
            if self.eat(".") {
                self.sync(Some(from), &to, assignee)
            } else if self.eat(":") {
                self.asynchronous(Some(from), &to, assignee)
            } else {
                Err(self.err(format!(
                    "expected `.method()` or `: message` after `{first}->{to}`"
                )))
            }
        } else if self.eat(".") {
            self.sync(None, &first, assignee)
        } else if self.eat(":") {
            self.asynchronous(None, &first, assignee)
        } else if self.peek() == Some('(') {
            // A call on the current participant itself.
            let args = self.parens()?;
            let from = self.current();
            self.call(
                from,
                from,
                format!("{first}({args})"),
                assignee,
                false,
                line,
            )
        } else if assignee.is_some() {
            Err(self.err(format!(
                "expected a call such as `A.method()` after the assignment to `{first}`"
            )))
        } else {
            self.pos = start;
            self.declare(None, top)
        }
    }

    /// `x =` or `Type x =` before a call: the return label.
    fn assignment(&mut self) -> Option<String> {
        let save = self.pos;
        let mut words = Vec::new();
        while words.len() < 2 {
            let Some(word) = self.ident() else { break };
            words.push(word);
            self.skip_spaces();
            if self.rest().starts_with('=') && !self.rest().starts_with("==") {
                self.pos += 1;
                return Some(words.join(" "));
            }
        }
        self.pos = save;
        None
    }

    fn sync(
        &mut self,
        from: Option<usize>,
        to: &str,
        assignee: Option<String>,
    ) -> Result<(), Error> {
        let line = self.line();
        let mut method = self
            .ident()
            .ok_or_else(|| self.err(format!("expected a method name after `{to}.`")))?;
        while self.eat(".") {
            let part = self
                .ident()
                .ok_or_else(|| self.err(format!("expected a name after `{method}.`")))?;
            method = format!("{method}.{part}");
        }
        self.skip_spaces();
        if self.peek() == Some('(') {
            let args = self.parens()?;
            method = format!("{method}({args})");
        }
        let from = from.unwrap_or_else(|| self.current());
        let to = self.ensure(to);
        self.call(from, to, method, assignee, false, line)
    }

    fn asynchronous(
        &mut self,
        from: Option<usize>,
        to: &str,
        assignee: Option<String>,
    ) -> Result<(), Error> {
        if let Some(a) = assignee {
            return Err(self.err(format!(
                "`{a} = …`: an async message (`A->B: text`) has no return value to assign"
            )));
        }
        let line = self.line();
        let rest = self.rest();
        let mut end = rest.find('\n').unwrap_or(rest.len());
        // The text runs to the end of the line, except a `}` it does not
        // open itself: `loop { A->B: tick }` closes the loop.
        let opens = rest[..end].matches('{').count();
        let closes = rest[..end].matches('}').count();
        if closes > opens {
            let mut excess = closes - opens;
            while excess > 0 {
                end = rest[..end].rfind('}').expect("counted above");
                excess -= 1;
            }
        }
        let text = rest[..end].trim().to_string();
        self.pos += end;
        let from = from.unwrap_or_else(|| self.current());
        let to = self.ensure(to);
        let dashed = self.reply.take().is_some();
        self.push_message(Message {
            from,
            to,
            text,
            dashed,
            end: Head::Open,
            line,
            ..Message::default()
        });
        Ok(())
    }

    /// `new A(args) { … }`.
    fn creation(&mut self, assignee: Option<String>) -> Result<(), Error> {
        let line = self.line();
        self.skip_spaces();
        let name = self
            .name()?
            .ok_or_else(|| self.err("expected a participant name after `new`"))?;
        self.skip_spaces();
        let text = if self.peek() == Some('(') {
            let args = self.parens()?;
            if args.is_empty() {
                "«create»".to_string()
            } else {
                format!("«create»({args})")
            }
        } else {
            "«create»".to_string()
        };
        let from = self.current();
        let to = self.ensure(&name);
        if self.messaged[to] || self.seq.participants[to].created {
            return Err(self.err(format!(
                "`new {name}`: `{name}` already exists by this point; a participant is created once"
            )));
        }
        self.seq.participants[to].created = true;
        self.call(from, to, text, assignee, true, line)
    }

    /// A sync call or creation: the message, the callee's activation over
    /// its block, and a return if the result is assigned.
    fn call(
        &mut self,
        from: usize,
        to: usize,
        text: String,
        assignee: Option<String>,
        creates: bool,
        line: usize,
    ) -> Result<(), Error> {
        if let Some(at) = self.reply {
            return Err(Error::new(
                at,
                "`@return` must be followed by an async message such as `A->B: result`",
            ));
        }
        self.push_message(Message {
            from,
            to,
            text,
            end: Head::Arrow,
            creates,
            line,
            ..Message::default()
        });
        self.seq.events.push(Event::Activate(to));
        let save = self.pos;
        self.skip_ws();
        let mut returned = false;
        if self.peek() == Some('{') {
            let open = self.line();
            self.pos += 1;
            self.stack.push(Context {
                who: Some(to),
                caller: Some(from),
                returned: false,
            });
            self.block(Some(open))?;
            returned = self.stack.pop().is_some_and(|c| c.returned);
        } else {
            self.pos = save;
        }
        if let (Some(value), false) = (assignee, returned) {
            self.reply_message(to, from, value);
        }
        self.seq.events.push(Event::Deactivate(to));
        Ok(())
    }

    fn reply_message(&mut self, from: usize, to: usize, text: String) {
        let line = self.line();
        self.push_message(Message {
            from,
            to,
            text,
            dashed: true,
            end: Head::Open,
            line,
            ..Message::default()
        });
    }

    /// Push a message, with any comments before it as a note over it.
    fn push_message(&mut self, message: Message) {
        if !self.comments.is_empty() {
            let text = std::mem::take(&mut self.comments).join("\n");
            self.seq.events.push(Event::Note(Note {
                side: NoteSide::Over,
                first: message.from.min(message.to),
                last: message.from.max(message.to),
                text,
                wrap: false,
            }));
        }
        self.messaged[message.from] = true;
        self.messaged[message.to] = true;
        self.seq.events.push(Event::Message(message));
    }

    // ---- participants -------------------------------------------------

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
        self.messaged.push(false);
        self.seq.participants.len() - 1
    }

    /// Who sends a message with no explicit sender.
    fn current(&mut self) -> usize {
        let context = self
            .stack
            .last()
            .expect("the top level is always on the stack");
        if let Some(who) = context.who {
            return who;
        }
        let who = self.ensure(STARTER);
        self.seq.participants[who].kind = ParticipantKind::Actor;
        self.seq.participants[who].label = String::new();
        self.starter = Some(who);
        self.stack[0].who = Some(who);
        who
    }

    fn finish(mut self) -> Result<Sequence, Error> {
        if let Some(line) = self.reply {
            return Err(Error::new(
                line,
                "`@return` must be followed by an async message such as `A->B: result`",
            ));
        }
        // The starter stands leftmost, as the caller of everything.
        if let Some(s) = self.starter.filter(|&s| s != 0) {
            let map = |i: usize| match i {
                i if i == s => 0,
                i if i < s => i + 1,
                i => i,
            };
            let starter = self.seq.participants.remove(s);
            self.seq.participants.insert(0, starter);
            for event in &mut self.seq.events {
                match event {
                    Event::Message(m) => {
                        m.from = map(m.from);
                        m.to = map(m.to);
                        for d in &mut m.destroys {
                            *d = map(*d);
                        }
                    }
                    Event::Note(n) => {
                        n.first = map(n.first);
                        n.last = map(n.last);
                    }
                    Event::Activate(w) | Event::Deactivate(w) => *w = map(*w),
                    Event::Ref(r) => {
                        for p in &mut r.participants {
                            *p = map(*p);
                        }
                    }
                    Event::Start(..) | Event::Section(_) | Event::End | Event::Divider(_) => {}
                }
            }
        }
        Ok(self.seq)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(seq: &Sequence) -> Vec<(String, String, String, bool)> {
        let id = |i: usize| seq.participants[i].id.clone();
        seq.messages()
            .map(|m| (id(m.from), id(m.to), m.text.clone(), m.dashed))
            .collect()
    }

    fn m(from: &str, to: &str, text: &str, dashed: bool) -> (String, String, String, bool) {
        (from.into(), to.into(), text.into(), dashed)
    }

    #[test]
    fn async_messages_and_title() {
        let seq = parse(
            "zenuml\n  title Demo\n  Alice->John: Hello John, how are you?\n  John->Alice: Great!\n",
        )
        .unwrap();
        assert_eq!(seq.title.as_deref(), Some("Demo"));
        assert_eq!(
            messages(&seq),
            [
                m("Alice", "John", "Hello John, how are you?", false),
                m("John", "Alice", "Great!", false)
            ]
        );
        assert!(seq.messages().all(|m| m.end == Head::Open));
    }

    #[test]
    fn declarations_annotators_and_aliases() {
        let seq = parse(
            "zenuml\n Bob\n @Actor Alice\n @Database DB as \"Book store\"\n @EC2 Host\n <<Interface>> Api\n A as Alice2\n Alice->Bob: Hi\n",
        )
        .unwrap();
        let p = &seq.participants;
        assert_eq!(p[0].id, "Bob");
        assert_eq!(p[1].kind, ParticipantKind::Actor);
        assert_eq!(
            (p[2].kind, p[2].label.as_str()),
            (ParticipantKind::Database, "Book store")
        );
        assert_eq!(
            (p[3].kind, p[3].label.as_str()),
            (ParticipantKind::Participant, "«EC2»\nHost")
        );
        assert_eq!(p[4].label, "«Interface»\nApi");
        assert_eq!(p[5].label, "Alice2");
    }

    #[test]
    fn groups_become_boxes() {
        let seq = parse("zenuml\n group Back end { @Database DB\n Cache }\n A->DB: q\n");
        assert!(seq.is_err(), "a group name is one word or quoted");
        let seq =
            parse("zenuml\n group \"Back end\" { @Database DB\n Cache }\n A->DB: q\n").unwrap();
        assert_eq!(seq.boxes[0].label.as_deref(), Some("Back end"));
        assert_eq!(seq.participants[0].group, Some(0));
        assert_eq!(seq.participants[1].group, Some(0));
        assert_eq!(seq.participants[2].group, None);
    }

    #[test]
    fn sync_calls_nest_and_activate() {
        let seq = parse(
            "zenuml\n A.SyncMessage\n A.SyncMessage(with, parameters) {\n   B.nested()\n }\n",
        )
        .unwrap();
        assert_eq!(
            messages(&seq),
            [
                m(STARTER, "A", "SyncMessage", false),
                m(STARTER, "A", "SyncMessage(with, parameters)", false),
                m("A", "B", "nested()", false),
            ]
        );
        assert_eq!(
            seq.participants[0].id, STARTER,
            "the starter stands leftmost"
        );
        let activations = seq
            .events
            .iter()
            .filter(|e| matches!(e, Event::Activate(_)))
            .count();
        let deactivations = seq
            .events
            .iter()
            .filter(|e| matches!(e, Event::Deactivate(_)))
            .count();
        assert_eq!((activations, deactivations), (3, 3));
    }

    #[test]
    fn explicit_senders_self_calls_and_returns() {
        let seq = parse(
            "zenuml\n Client->A.method() {\n   B.method() {\n     selfCall() { return X }\n   }\n   return x2\n }\n x = A.get()\n SomeType y = A.get()\n",
        )
        .unwrap();
        assert_eq!(
            messages(&seq),
            [
                m("Client", "A", "method()", false),
                m("A", "B", "method()", false),
                m("B", "B", "selfCall()", false),
                m("B", "B", "X", true),
                m("A", "Client", "x2", true),
                m(STARTER, "A", "get()", false),
                m("A", STARTER, "x", true),
                m(STARTER, "A", "get()", false),
                m("A", STARTER, "SomeType y", true),
            ]
        );
    }

    #[test]
    fn an_explicit_return_replaces_the_assignment_label() {
        let seq = parse("zenuml\n x = A.m() {\n return y\n }\n").unwrap();
        let texts: Vec<_> = seq.messages().map(|m| m.text.as_str()).collect();
        assert_eq!(texts, ["m()", "y"]);
    }

    #[test]
    fn at_return_dashes_the_next_async_message() {
        let seq = parse("zenuml\n @return\n A->B: result\n A->B: plain\n").unwrap();
        assert_eq!(
            messages(&seq),
            [m("A", "B", "result", true), m("A", "B", "plain", false)]
        );
        let error = parse("zenuml\n @reply\n A.m()\n").unwrap_err();
        assert_eq!(error.line, 2);
    }

    #[test]
    fn creation() {
        let seq =
            parse("zenuml\n new A1\n receipt = new Receipt(id, dueDate) {\n   DB.save()\n }\n")
                .unwrap();
        assert_eq!(
            messages(&seq),
            [
                m(STARTER, "A1", "«create»", false),
                m(STARTER, "Receipt", "«create»(id, dueDate)", false),
                m("Receipt", "DB", "save()", false),
                m("Receipt", STARTER, "receipt", true),
            ]
        );
        let created: Vec<_> = seq.messages().map(|m| m.creates).collect();
        assert_eq!(created, [true, true, false, false]);
        assert!(
            seq.participants
                .iter()
                .find(|p| p.id == "A1")
                .unwrap()
                .created
        );
        let error = parse("zenuml\n A->B: hi\n new B()\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("created once"), "{error}");
    }

    #[test]
    fn comments_become_notes_over_the_next_message() {
        let seq = parse(
            "zenuml\n // on a participant: not drawn\n BookService\n // a comment\n // **two** lines\n BookService.getBook()\n",
        )
        .unwrap();
        let notes: Vec<_> = seq
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Note(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].text, "a comment\n**two** lines");
        assert!(
            matches!(seq.events[1], Event::Message(_)),
            "the note comes first"
        );
    }

    fn frames(seq: &Sequence) -> Vec<String> {
        seq.events
            .iter()
            .filter_map(|e| match e {
                Event::Start(f, label) => Some(format!("{}[{label}]", f.operator())),
                Event::Section(label) => Some(format!("|{label}")),
                Event::End => Some("end".into()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn control_flow_maps_to_frames() {
        let seq = parse(
            "zenuml\n if(a) {\n A->B: 1\n } else if (b) {\n A->B: 2\n }\n else {\n A->B: 3\n }\n while(true) { A->B: w }\n forEach(x in xs) { A->B: f }\n loop { A->B: l }\n opt { A->B: o }\n par {\n A->B: p\n A->C: q\n }\n critical(lock) { A->B: c }\n try {\n A->B: t\n } catch (E) {\n A->B: c\n } finally {\n A->B: f\n }\n",
        )
        .unwrap();
        assert_eq!(
            frames(&seq),
            [
                "alt[a]",
                "|b",
                "|else",
                "end",
                "loop[true]",
                "end",
                "loop[x in xs]",
                "end",
                "loop[]",
                "end",
                "opt[]",
                "end",
                "par[]",
                "end",
                "critical[lock]",
                "end",
                "try[]",
                "|catch(E)",
                "|finally",
                "end",
            ]
        );
        assert_eq!(seq.messages().count(), 13);
    }

    #[test]
    fn the_borrowing_example_maps_completely() {
        let seq = parse(
            "zenuml\nBookLibService.Borrow(id) {\n  User = Session.GetUser()\n  if(User.isActive) {\n    try {\n      BookRepository.Update(id, onLoan, User)\n      receipt = new Receipt(id, dueDate)\n    } catch (BookNotFoundException) {\n      ErrorService.onException(BookNotFoundException)\n    } finally {\n      Connection.close()\n    }\n  }\n  return receipt\n}\n",
        )
        .unwrap();
        assert_eq!(
            messages(&seq),
            [
                m(STARTER, "BookLibService", "Borrow(id)", false),
                m("BookLibService", "Session", "GetUser()", false),
                m("Session", "BookLibService", "User", true),
                m(
                    "BookLibService",
                    "BookRepository",
                    "Update(id, onLoan, User)",
                    false
                ),
                m("BookLibService", "Receipt", "«create»(id, dueDate)", false),
                m("Receipt", "BookLibService", "receipt", true),
                m(
                    "BookLibService",
                    "ErrorService",
                    "onException(BookNotFoundException)",
                    false
                ),
                m("BookLibService", "Connection", "close()", false),
                m("BookLibService", STARTER, "receipt", true),
            ]
        );
    }

    #[test]
    fn quoted_participants() {
        let seq = parse("zenuml\n Auth0->\"Identity Provider\": 2. OAuth2 / SAML, etc\n").unwrap();
        assert_eq!(
            messages(&seq),
            [m(
                "Auth0",
                "Identity Provider",
                "2. OAuth2 / SAML, etc",
                false
            )]
        );
    }

    #[test]
    fn starter_annotation_names_the_caller() {
        let seq = parse("zenuml\n @Starter(Client)\n A.m()\n").unwrap();
        assert_eq!(messages(&seq), [m("Client", "A", "m()", false)]);
    }

    #[test]
    fn unsupported_constructs_are_errors_with_lines() {
        for (source, line, needle) in [
            ("zenuml\n A->B: x\n ref(Other,)\n", 3, "empty argument"),
            ("zenuml\n ref Other\n", 2, "expected `ref(Name)`"),
            ("zenuml\n section(S) {\n }\n", 2, "`section`"),
            ("zenuml\n A #FF0000\n", 2, "colours"),
            ("zenuml\n A.m() {\n", 2, "never closed"),
            ("zenuml\n }\n", 2, "without an open"),
            ("zenuml\n return x\n", 2, "`return` outside"),
            ("zenuml\n else {\n }\n", 2, "`else` without"),
            ("zenuml\n A.m() {\n   B\n }\n", 3, "top level"),
            ("zenuml\n A.m() B.n()\n", 2, "unexpected"),
            ("zenuml\n x = A->B: hi\n", 2, "async"),
            ("graph\n", 1, "expected `zenuml`"),
        ] {
            let error = parse(source).unwrap_err();
            assert_eq!(error.line, line, "{source}: {error}");
            assert!(error.message.contains(needle), "{source}: {error}");
        }
    }

    #[test]
    fn refs_and_dividers_parse_into_the_model() {
        let seq = parse(
            "zenuml\n A->B: hi\n ref(Checkout)\n ref(Pay, B, C)\n ref(A, C, \"Audit trail\")\n == Later ==\n ====\n B->A: bye\n",
        )
        .unwrap();
        let id = |i: usize| seq.participants[i].id.as_str();
        let items: Vec<String> = seq
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Ref(r) => Some(format!(
                    "ref {}: {}",
                    r.label,
                    r.participants
                        .iter()
                        .map(|&p| id(p))
                        .collect::<Vec<_>>()
                        .join(",")
                )),
                Event::Divider(label) => Some(format!("== {label}")),
                _ => None,
            })
            .collect();
        assert_eq!(
            items,
            [
                "ref Checkout: ",
                "ref Pay: B,C",
                "ref Audit trail: A,C",
                "== Later",
                "== ",
            ]
        );
        // `C` is a participant once a ref names it.
        assert_eq!(seq.participants.len(), 3);
    }

    #[test]
    fn a_ref_inside_a_call_maps_past_the_starter() {
        let seq = parse("zenuml\n A.m() {\n   ref(Sub, A)\n }\n").unwrap();
        let r = seq
            .events
            .iter()
            .find_map(|e| match e {
                Event::Ref(r) => Some(r),
                _ => None,
            })
            .unwrap();
        assert_eq!(seq.participants[r.participants[0]].id, "A");
    }

    #[test]
    fn refs_and_dividers_are_drawn() {
        use diagram_scene::ContainerKind;
        struct Fixed;
        impl crate::text::Measure for Fixed {
            fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
                text.chars().count() as f32 * size * 0.5
            }
        }
        let source = "zenuml\n A->B: hi\n B->C: on\n C->D: on\n ref(Pay, B, C)\n ==Later==\n ref(Everything)\n D->A: bye\n";
        let scene = super::super::scene(source, &Fixed).unwrap();
        let texts = scene.texts();
        for want in ["Pay", "Later", "Everything", "hi", "bye"] {
            assert!(texts.iter().any(|t| t == want), "{want} missing: {texts:?}");
        }
        let refs: Vec<_> = scene
            .containers
            .iter()
            .filter(|c| matches!(&c.kind, ContainerKind::Fragment { operator, .. } if operator == "ref"))
            .map(|c| c.rect)
            .collect();
        assert_eq!(refs.len(), 2);
        let head = |id: &str| scene.node(id).unwrap().rect;
        // Exactly the named participants' heads wide.
        assert!((refs[0].x - head("B").x).abs() < 0.01, "{:?}", refs[0]);
        assert!((refs[0].right() - head("C").right()).abs() < 0.01);
        // None named: every participant.
        assert!((refs[1].x - head("A").x).abs() < 0.01);
        assert!((refs[1].right() - head("D").right()).abs() < 0.01);
        // Each label is centred in its frame, and the frames keep to
        // their own rows, either side of the divider.
        for (rect, label) in refs.iter().zip(["Pay", "Everything"]) {
            let t = scene
                .texts
                .iter()
                .find(|t| t.line.plain() == label)
                .unwrap();
            assert!((t.pos.x - rect.center().x).abs() < 0.01);
            assert!(t.pos.y > rect.y && t.pos.y < rect.bottom());
        }
        let pill = scene
            .nodes
            .iter()
            .find(|n| n.id.starts_with("divider#"))
            .unwrap();
        assert_eq!(pill.label[0].plain(), "Later");
        assert!(pill.rect.y > refs[0].bottom() && pill.rect.bottom() < refs[1].y);
        // A double rule from the first head to the last.
        let rule: Vec<_> = scene
            .edges
            .iter()
            .filter(|e| e.from.is_none() && e.path.len() == 2)
            .collect();
        assert_eq!(rule.len(), 2, "a double rule");
        for e in rule {
            let (diagram_scene::Seg::Move(a), diagram_scene::Seg::Line(b)) = (e.path[0], e.path[1])
            else {
                panic!("a straight rule")
            };
            assert!((a.x - head("A").x).abs() < 0.01 && (b.x - head("D").right()).abs() < 0.01);
        }
        let bounds = scene.bounds();
        let mut all: Vec<_> = scene.nodes.iter().map(|n| n.rect).collect();
        all.extend(scene.containers.iter().map(|c| c.rect));
        for r in all {
            assert!(bounds.encloses(&r, 0.01), "{r:?} escapes {bounds:?}");
        }
    }

    #[test]
    fn nothing_is_dropped_in_the_scene() {
        struct Fixed;
        impl crate::text::Measure for Fixed {
            fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
                text.chars().count() as f32 * size * 0.5
            }
        }
        let source = "zenuml\n title T\n @Actor Alice\n group G { @Database DB }\n // note\n Alice->Svc.load(id) {\n   rows = DB.query()\n   if(empty) { return none }\n }\n";
        let scene = super::super::scene(source, &Fixed).unwrap();
        let texts = scene.texts();
        for want in [
            "T", "Alice", "DB", "G", "note", "load(id)", "query()", "rows", "none", "[empty]",
        ] {
            assert!(
                texts.iter().any(|t| t.contains(want)),
                "{want} missing: {texts:?}"
            );
        }
    }
}
