//! Entity-relationship syntax: `erDiagram` with entities (optionally
//! aliased, optionally with attribute blocks), relationships with a
//! cardinality at each end in symbol or word form, `direction`, and
//! `classDef`/`class`/`:::`/`style` colours.
//!
//! Grammar checked against mermaid's
//! `packages/mermaid/src/diagrams/er/parser/erDiagram.jison`.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

/// How many of an entity take part in a relationship, at one end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    ZeroOrOne,
    ExactlyOne,
    ZeroOrMore,
    OneOrMore,
    /// `u`: the parent of a master-detail relationship.
    MdParent,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub kind: String,
    pub name: String,
    /// `PK`, `FK`, `UK`, in source order.
    pub keys: Vec<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub name: String,
    /// Shown instead of the name: `p[Person]`.
    pub alias: Option<String>,
    pub attributes: Vec<Attribute>,
    pub classes: Vec<String>,
    pub style: Colors,
}

impl Entity {
    /// The text in the entity's header.
    pub fn label(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone)]
pub struct Relationship {
    /// Indexes into [`Er::entities`].
    pub from: usize,
    pub to: usize,
    /// At `from`'s end and at `to`'s.
    pub from_card: Cardinality,
    pub to_card: Cardinality,
    /// `--` (solid) rather than `..` (dashed).
    pub identifying: bool,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct Er {
    pub direction: Direction,
    pub entities: Vec<Entity>,
    pub relationships: Vec<Relationship>,
    pub classes: Vec<(String, Colors)>,
    /// Stored, not drawn.
    pub acc_title: Option<String>,
    pub acc_descr: Option<String>,
}

impl Er {
    /// An entity's colours: the `default` class, its classes, then `style`.
    pub fn colors(&self, entity: &Entity) -> Colors {
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
        for name in &entity.classes {
            if let Some(c) = class(name) {
                out.merge(&c);
            }
        }
        out.merge(&entity.style);
        out
    }

    /// The entity called `name`, made if new.
    fn entity(&mut self, name: &str) -> usize {
        if let Some(i) = self.entities.iter().position(|e| e.name == name) {
            return i;
        }
        self.entities.push(Entity {
            name: name.to_string(),
            alias: None,
            attributes: Vec::new(),
            classes: Vec::new(),
            style: Colors::default(),
        });
        self.entities.len() - 1
    }
}

pub fn parse(source: &str) -> Result<Er, Error> {
    let mut er = Er {
        direction: Direction::TopDown,
        entities: Vec::new(),
        relationships: Vec::new(),
        classes: Vec::new(),
        acc_title: None,
        acc_descr: None,
    };
    let mut header = false;
    // The entity whose `{ … }` block is open, and the line it opened on.
    let mut block: Option<(usize, usize)> = None;
    let mut pending = Vec::new();
    // An open multi-line `accDescr { … }`.
    let mut descr: Option<String> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        if let Some(text) = descr.as_mut() {
            match raw.find('}') {
                Some(end) => {
                    text.push_str(&raw[..end]);
                    er.acc_descr = Some(text.trim().to_string());
                    descr = None;
                    if !raw[end + 1..].trim().is_empty() {
                        return Err(Error::new(number, "unexpected text after `}`"));
                    }
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
        if !header {
            if line != "erDiagram" {
                return Err(Error::new(number, "expected `erDiagram`"));
            }
            header = true;
            continue;
        }
        if let Some((entity, _)) = block {
            let mut p = Cursor::new(line, number);
            let closed = attribute_tokens(&mut p, &mut pending)?;
            if closed {
                let attributes = attributes(&pending, number)?;
                er.entities[entity].attributes.extend(attributes);
                pending.clear();
                block = None;
                p.skip_space();
                if !p.done() {
                    return Err(p.unexpected("after `}`"));
                }
            }
            continue;
        }
        let keyword = line.split_whitespace().next().unwrap_or("");
        let rest = line[keyword.len()..].trim();
        match keyword {
            "direction" => {
                er.direction = match rest {
                    "TB" | "TD" => Direction::TopDown,
                    "BT" => Direction::BottomUp,
                    "LR" => Direction::LeftRight,
                    "RL" => Direction::RightLeft,
                    _ => {
                        return Err(Error::new(
                            number,
                            format!("unknown direction `{rest}`: expected TB, BT, LR or RL"),
                        ));
                    }
                };
                continue;
            }
            "classDef" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let names = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""));
                for name in names.split(',') {
                    er.classes.push((name.trim().to_string(), colors));
                }
                continue;
            }
            "class" => {
                let mut words = rest.rsplitn(2, char::is_whitespace);
                let class = words.next().unwrap_or("").to_string();
                let ids = words.next().unwrap_or("").trim();
                if ids.is_empty() {
                    return Err(Error::new(
                        number,
                        "`class` needs entities and a class name",
                    ));
                }
                for id in ids.split(',') {
                    let entity = er.entity(id.trim());
                    er.entities[entity].classes.push(class.clone());
                }
                continue;
            }
            "style" => {
                let mut words = rest.splitn(2, char::is_whitespace);
                let ids = words.next().unwrap_or("");
                let colors = css(words.next().unwrap_or(""));
                for id in ids.split(',') {
                    let entity = er.entity(id.trim());
                    er.entities[entity].style.merge(&colors);
                }
                continue;
            }
            "title" => {
                return Err(Error::new(
                    number,
                    "`title` is not supported in erDiagram (see DESIGN.md)",
                ));
            }
            _ => {}
        }
        if let Some(value) = acc(line, "accTitle") {
            er.acc_title = Some(value.to_string());
            continue;
        }
        if let Some(value) = acc(line, "accDescr") {
            er.acc_descr = Some(value.to_string());
            continue;
        }
        if let Some(open) = line.strip_prefix("accDescr")
            && let Some(body) = open.trim_start().strip_prefix('{')
        {
            match body.find('}') {
                Some(end) => er.acc_descr = Some(body[..end].trim().to_string()),
                None => descr = Some(format!("{body}\n")),
            }
            continue;
        }
        if let Some((entity, open)) = statement(&mut er, line, number)? {
            block = Some((entity, number));
            // Attributes may follow the `{` on the same line.
            let mut p = Cursor::new(&line[open..], number);
            if attribute_tokens(&mut p, &mut pending)? {
                let attributes = attributes(&pending, number)?;
                er.entities[entity].attributes.extend(attributes);
                pending.clear();
                block = None;
                p.skip_space();
                if !p.done() {
                    return Err(p.unexpected("after `}`"));
                }
            }
        }
    }
    if !header {
        return Err(Error::new(1, "expected `erDiagram`"));
    }
    if let Some((_, line)) = block {
        return Err(Error::new(line, "attribute block is never closed with `}`"));
    }
    if descr.is_some() {
        return Err(Error::new(0, "`accDescr {` is never closed with `}`"));
    }
    Ok(er)
}

/// `accTitle: text` → `text`.
fn acc<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(keyword)?.trim_start();
    Some(rest.strip_prefix(':')?.trim())
}

/// An entity or relationship statement. Returns the entity whose attribute
/// block it opens, if any, and where in `line` the block starts.
fn statement(er: &mut Er, line: &str, number: usize) -> Result<Option<(usize, usize)>, Error> {
    let mut p = Cursor::new(line, number);
    let name = p.entity_name()?;
    p.skip_space();
    if p.done() {
        er.entity(&name);
        return Ok(None);
    }
    let mut alias = None;
    if p.eat("[") {
        p.skip_space();
        alias = Some(p.entity_name()?);
        p.skip_space();
        if !p.eat("]") {
            return Err(p.unexpected("in an alias: expected `]`"));
        }
        p.skip_space();
    }
    let classes = p.classes()?;
    p.skip_space();
    if p.done() || p.peek() == Some('{') {
        let entity = er.entity(&name);
        if alias.is_some() {
            er.entities[entity].alias = alias;
        }
        er.entities[entity].classes.extend(classes);
        return Ok(p.eat("{").then_some((entity, p.i)));
    }
    if alias.is_some() {
        return Err(p.unexpected("after an alias: expected `{` or the end of the line"));
    }
    let (from_card, identifying, to_card) = p.rel_spec()?;
    p.skip_space();
    let other = p.entity_name()?;
    p.skip_space();
    let other_classes = p.classes()?;
    p.skip_space();
    if !p.eat(":") {
        return Err(Error::new(
            number,
            "a relationship needs a label: `A ||--o{ B : label`",
        ));
    }
    p.skip_space();
    let label = if p.peek() == Some('"') {
        p.quoted()?
    } else {
        let word = p.word();
        if word.is_empty() {
            return Err(Error::new(number, "a relationship needs a label after `:`"));
        }
        word
    };
    p.skip_space();
    if !p.done() {
        return Err(p.unexpected("after the label: quote a label of several words"));
    }
    let from = er.entity(&name);
    let to = er.entity(&other);
    er.entities[from].classes.extend(classes);
    er.entities[to].classes.extend(other_classes);
    er.relationships.push(Relationship {
        from,
        to,
        from_card,
        to_card,
        identifying,
        label,
    });
    Ok(None)
}

/// A token inside an attribute block, with its line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Key(String),
    Comma,
    Comment(String),
}

/// Reads the block's tokens on this line into `out`; true once `}` closes it.
fn attribute_tokens(p: &mut Cursor, out: &mut Vec<(Token, usize)>) -> Result<bool, Error> {
    loop {
        p.skip_space();
        let Some(c) = p.peek() else { return Ok(false) };
        match c {
            '}' => {
                p.i += 1;
                return Ok(true);
            }
            ',' => {
                p.i += 1;
                out.push((Token::Comma, p.number));
            }
            '"' => {
                let text = p.quoted()?;
                out.push((Token::Comment(text), p.number));
            }
            _ => {
                let start = p.i;
                while let Some(c) = p.peek() {
                    if c.is_whitespace() || matches!(c, ',' | '"' | '}') {
                        break;
                    }
                    p.i += c.len_utf8();
                }
                let word = &p.s[start..p.i];
                let valid = |c: char| {
                    c.is_alphanumeric() || matches!(c, '-' | '_' | '[' | ']' | '(' | ')' | '*')
                };
                if word.is_empty() || !(word.contains('~') || word.chars().all(valid)) {
                    return Err(Error::new(
                        p.number,
                        format!("`{word}` is not a valid attribute type or name"),
                    ));
                }
                let token = if matches!(word.to_ascii_uppercase().as_str(), "PK" | "FK" | "UK") {
                    Token::Key(word.to_ascii_uppercase())
                } else {
                    Token::Word(generics(word))
                };
                out.push((token, p.number));
            }
        }
    }
}

/// `list~int~` reads as `list<int>`, as mermaid draws it.
fn generics(word: &str) -> String {
    let mut out = String::new();
    let mut open = true;
    for c in word.chars() {
        if c == '~' {
            out.push(if open { '<' } else { '>' });
            open = !open;
        } else {
            out.push(c);
        }
    }
    out
}

/// `type name [KEY[, KEY…]] ["comment"]`, repeated.
fn attributes(tokens: &[(Token, usize)], close: usize) -> Result<Vec<Attribute>, Error> {
    let mut out = Vec::new();
    let mut i = 0;
    let line_at = |i: usize| tokens.get(i).map(|t| t.1).unwrap_or(close);
    while i < tokens.len() {
        let Token::Word(kind) = &tokens[i].0 else {
            return Err(Error::new(
                line_at(i),
                "an attribute must start with its type: `string name`",
            ));
        };
        let Some((Token::Word(name), _)) = tokens.get(i + 1) else {
            return Err(Error::new(
                line_at(i),
                format!("attribute of type `{kind}` has no name"),
            ));
        };
        i += 2;
        let mut keys = Vec::new();
        while let Some((Token::Key(key), _)) = tokens.get(i) {
            keys.push(key.clone());
            i += 1;
            if let Some((Token::Comma, _)) = tokens.get(i) {
                if !matches!(tokens.get(i + 1), Some((Token::Key(_), _))) {
                    return Err(Error::new(
                        line_at(i),
                        "expected a key (`PK`, `FK`, `UK`) after `,`",
                    ));
                }
                i += 1;
            } else {
                break;
            }
        }
        let comment = match tokens.get(i) {
            Some((Token::Comment(text), _)) => {
                i += 1;
                Some(text.clone())
            }
            _ => None,
        };
        if let Some((Token::Comma, line)) = tokens.get(i) {
            return Err(Error::new(*line, "unexpected `,` in an attribute"));
        }
        if let Some((Token::Key(key), line)) = tokens.get(i) {
            return Err(Error::new(*line, format!("unexpected key `{key}`")));
        }
        if let Some((Token::Comment(_), line)) = tokens.get(i) {
            return Err(Error::new(*line, "an attribute takes one comment"));
        }
        out.push(Attribute {
            kind: kind.clone(),
            name: name.clone(),
            keys,
            comment,
        });
    }
    Ok(out)
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

/// Cardinalities, longest spellings first; matched case-insensitively.
const CARDINALITIES: &[(&str, Cardinality)] = &[
    ("one or zero", Cardinality::ZeroOrOne),
    ("one or more", Cardinality::OneOrMore),
    ("one or many", Cardinality::OneOrMore),
    ("zero or one", Cardinality::ZeroOrOne),
    ("zero or more", Cardinality::ZeroOrMore),
    ("zero or many", Cardinality::ZeroOrMore),
    ("many(0)", Cardinality::ZeroOrMore),
    ("many(1)", Cardinality::OneOrMore),
    ("only one", Cardinality::ExactlyOne),
    ("many", Cardinality::ZeroOrMore),
    ("one", Cardinality::ExactlyOne),
    ("1+", Cardinality::OneOrMore),
    ("0+", Cardinality::ZeroOrMore),
    ("|o", Cardinality::ZeroOrOne),
    ("o|", Cardinality::ZeroOrOne),
    ("}o", Cardinality::ZeroOrMore),
    ("o{", Cardinality::ZeroOrMore),
    ("}|", Cardinality::OneOrMore),
    ("|{", Cardinality::OneOrMore),
    ("||", Cardinality::ExactlyOne),
    ("1", Cardinality::ExactlyOne),
    ("u", Cardinality::MdParent),
];

/// Relationship lines: true for identifying.
const LINES: &[(&str, bool)] = &[
    ("optionally to", false),
    ("to", true),
    ("--", true),
    ("..", false),
    (".-", false),
    ("-.", false),
];

impl<'a> Cursor<'a> {
    fn new(s: &'a str, number: usize) -> Self {
        Cursor { s, i: 0, number }
    }

    fn peek(&self) -> Option<char> {
        self.s[self.i..].chars().next()
    }

    fn done(&self) -> bool {
        self.i >= self.s.len()
    }

    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }

    fn eat(&mut self, token: &str) -> bool {
        if self.rest().starts_with(token) {
            self.i += token.len();
            true
        } else {
            false
        }
    }

    /// Case-insensitive `eat`.
    fn eat_word(&mut self, token: &str) -> bool {
        let rest = self.rest();
        if rest.len() >= token.len()
            && rest.is_char_boundary(token.len())
            && rest[..token.len()].eq_ignore_ascii_case(token)
        {
            self.i += token.len();
            true
        } else {
            false
        }
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            if !c.is_whitespace() {
                break;
            }
            self.i += c.len_utf8();
        }
    }

    fn unexpected(&self, context: &str) -> Error {
        let found: String = self.rest().chars().take(20).collect();
        Error::new(self.number, format!("unexpected `{found}` {context}"))
    }

    /// Word characters, as mermaid's `UNICODE_TEXT`.
    fn word(&mut self) -> String {
        let start = self.i;
        while let Some(c) = self.peek() {
            if !(c.is_alphanumeric() || matches!(c, '_' | '-' | '*' | '.') || !c.is_ascii()) {
                break;
            }
            self.i += c.len_utf8();
        }
        self.s[start..self.i].to_string()
    }

    fn quoted(&mut self) -> Result<String, Error> {
        let start = self.i;
        self.i += 1;
        match self.rest().find('"') {
            Some(end) => {
                let text = self.rest()[..end].to_string();
                self.i += end + 1;
                Ok(text)
            }
            None => {
                self.i = start;
                Err(Error::new(self.number, "unterminated `\"`"))
            }
        }
    }

    /// A bare or quoted entity name.
    fn entity_name(&mut self) -> Result<String, Error> {
        if self.peek() == Some('"') {
            let name = self.quoted()?;
            if name.is_empty() {
                return Err(Error::new(self.number, "an entity name cannot be empty"));
            }
            return Ok(name);
        }
        let word = self.word();
        if word.is_empty() {
            return Err(self.unexpected("where an entity name was expected"));
        }
        Ok(word)
    }

    /// `:::a,b` (or nothing).
    fn classes(&mut self) -> Result<Vec<String>, Error> {
        if !self.eat(":::") {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        loop {
            let name = self.word();
            if name.is_empty() {
                return Err(self.unexpected("after `:::`: expected a class name"));
            }
            out.push(name);
            if !self.eat(",") {
                return Ok(out);
            }
        }
    }

    fn cardinality(&mut self) -> Option<Cardinality> {
        for &(text, card) in CARDINALITIES {
            let at = self.i;
            if self.eat_word(text) {
                // Words must end where the word ends: `one` not `onerous`.
                let alphabetic = text.ends_with(|c: char| c.is_ascii_alphanumeric());
                if alphabetic && self.peek().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    self.i = at;
                    continue;
                }
                return Some(card);
            }
        }
        None
    }

    /// `||--o{`, `one to zero or more`, …
    fn rel_spec(&mut self) -> Result<(Cardinality, bool, Cardinality), Error> {
        let expected = "where a relationship (`||--o{`) was expected";
        let left = self
            .cardinality()
            .ok_or_else(|| self.unexpected(expected))?;
        self.skip_space();
        let mut identifying = None;
        for &(text, id) in LINES {
            let at = self.i;
            if self.eat_word(text) {
                if text.ends_with(|c: char| c.is_ascii_alphabetic())
                    && !self.peek().is_some_and(char::is_whitespace)
                {
                    self.i = at;
                    continue;
                }
                identifying = Some(id);
                break;
            }
        }
        let identifying = identifying.ok_or_else(|| {
            self.unexpected("in a relationship: expected `--`, `..`, `to` or `optionally to`")
        })?;
        self.skip_space();
        let right = self
            .cardinality()
            .ok_or_else(|| self.unexpected("in a relationship: expected a cardinality"))?;
        Ok((left, identifying, right))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn er(body: &str) -> Er {
        parse(&format!("erDiagram\n{body}")).unwrap()
    }

    fn error(body: &str) -> Error {
        parse(&format!("erDiagram\n{body}")).unwrap_err()
    }

    #[test]
    fn entities_bare_aliased_and_with_attributes() {
        let er = er(concat!(
            "  CUSTOMER\n",
            "  p[Person]\n",
            "  q[\"Quoted name\"] {\n",
            "    string name\n",
            "  }\n",
            "  ORDER {\n",
            "    int id PK \"the key\"\n",
            "    string customer_id FK\n",
            "    varchar(255) email UK, FK \"unique\"\n",
            "    list~int~ items\n",
            "  }\n",
            "  EMPTY { }\n",
            "  ONE_LINE { string a string b PK }\n",
        ));
        let names: Vec<_> = er.entities.iter().map(|e| e.label()).collect();
        assert_eq!(
            names,
            [
                "CUSTOMER",
                "Person",
                "Quoted name",
                "ORDER",
                "EMPTY",
                "ONE_LINE"
            ]
        );
        let order = &er.entities[3].attributes;
        assert_eq!(order.len(), 4);
        assert_eq!(
            order[0],
            Attribute {
                kind: "int".into(),
                name: "id".into(),
                keys: vec!["PK".into()],
                comment: Some("the key".into()),
            }
        );
        assert_eq!(order[2].kind, "varchar(255)");
        assert_eq!(order[2].keys, ["UK", "FK"]);
        assert_eq!(order[2].comment.as_deref(), Some("unique"));
        assert_eq!(order[3].kind, "list<int>");
        assert_eq!(er.entities[5].attributes.len(), 2);
        assert_eq!(er.entities[5].attributes[1].keys, ["PK"]);
    }

    #[test]
    fn every_cardinality_symbol_on_both_sides() {
        use Cardinality::*;
        for (left, card) in [
            ("|o", ZeroOrOne),
            ("||", ExactlyOne),
            ("}o", ZeroOrMore),
            ("}|", OneOrMore),
        ] {
            let er = er(&format!("  A {left}--|| B : x\n"));
            assert_eq!(er.relationships[0].from_card, card, "{left}");
        }
        for (right, card) in [
            ("o|", ZeroOrOne),
            ("||", ExactlyOne),
            ("o{", ZeroOrMore),
            ("|{", OneOrMore),
        ] {
            let er = er(&format!("  A ||..{right} B : x\n"));
            let r = &er.relationships[0];
            assert_eq!(r.to_card, card, "{right}");
            assert!(!r.identifying);
        }
    }

    #[test]
    fn word_cardinalities_and_lines() {
        use Cardinality::*;
        let er = er(concat!(
            "  CAR 1 to zero or more NAMED-DRIVER : allows\n",
            "  A one or zero optionally to 1+ B : x\n",
            "  A many(0) .- many(1) B : x\n",
            "  A only one -. one or many B : x\n",
            "  A 0+ -- many B : x\n",
            "  A zero or one to one B : x\n",
            "  A u--o{ B : x\n",
        ));
        let r = &er.relationships;
        assert_eq!(er.entities[1].name, "NAMED-DRIVER");
        assert_eq!(
            (r[0].from_card, r[0].identifying, r[0].to_card),
            (ExactlyOne, true, ZeroOrMore)
        );
        assert_eq!(
            (r[1].from_card, r[1].identifying, r[1].to_card),
            (ZeroOrOne, false, OneOrMore)
        );
        assert_eq!(
            (r[2].from_card, r[2].identifying, r[2].to_card),
            (ZeroOrMore, false, OneOrMore)
        );
        assert_eq!(
            (r[3].from_card, r[3].identifying, r[3].to_card),
            (ExactlyOne, false, OneOrMore)
        );
        assert_eq!((r[4].from_card, r[4].to_card), (ZeroOrMore, ZeroOrMore));
        assert_eq!((r[5].from_card, r[5].to_card), (ZeroOrOne, ExactlyOne));
        assert_eq!(r[6].from_card, MdParent);
    }

    #[test]
    fn relationship_labels_and_entity_creation() {
        let er = er(concat!(
            "  CUSTOMER ||--o{ ORDER : places\n",
            "  ORDER ||--|{ \"LINE ITEM\" : \"contains many\"\n",
            "  A||--o{B : \"\"\n",
            "  A ||--o{ A : refers\n",
        ));
        let names: Vec<_> = er.entities.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["CUSTOMER", "ORDER", "LINE ITEM", "A", "B"]);
        let labels: Vec<_> = er.relationships.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["places", "contains many", "", "refers"]);
        assert_eq!(er.relationships[3].from, er.relationships[3].to);
    }

    #[test]
    fn direction_comments_styles_and_accessibility() {
        let er = er(concat!(
            "  %% a comment\n",
            "  direction LR\n",
            "  accTitle: Orders\n",
            "  accDescr {\n    multi\n    line\n  }\n",
            "  A:::hot ||--o{ B:::cold : x\n",
            "  C:::hot { string a }\n",
            "  classDef hot fill:#f96,stroke:#333\n",
            "  classDef cold fill:#00f\n",
            "  class C cold\n",
            "  style B color:#fff\n",
        ));
        assert_eq!(er.direction, Direction::LeftRight);
        assert_eq!(er.acc_title.as_deref(), Some("Orders"));
        assert_eq!(er.acc_descr.as_deref(), Some("multi\n    line"));
        let colors = |name: &str| er.colors(er.entities.iter().find(|e| e.name == name).unwrap());
        assert_eq!(colors("A").fill, Some((255, 153, 102)));
        assert_eq!(colors("B").fill, Some((0, 0, 255)));
        assert_eq!(colors("B").text, Some((255, 255, 255)));
        assert_eq!(colors("C").fill, Some((0, 0, 255)));
    }

    #[test]
    fn unsupported_and_malformed_input_is_an_error_with_a_line() {
        let e = error("  title Orders\n");
        assert_eq!(e.line, 2);
        assert!(e.message.contains("`title`"), "{e}");

        let e = error("  A ||--o{ B\n");
        assert!(e.message.contains("needs a label"), "{e}");

        let e = error("  A ||--o{ B : two words\n");
        assert!(e.message.contains("quote"), "{e}");

        let e = error("  A <-- B : x\n");
        assert_eq!(e.line, 2);

        let e = error("  A ||~~o{ B : x\n");
        assert!(e.message.contains("relationship"), "{e}");

        let e = error("  A {\n    string\n  }\n");
        assert!(e.message.contains("no name"), "{e}");

        let e = error("  A {\n    string a\n");
        assert!(e.message.contains("never closed"), "{e}");
        assert_eq!(e.line, 2);

        let e = error("  A {\n    PK a\n  }\n");
        assert_eq!(e.line, 3);

        let e = error("  direction XY\n");
        assert!(e.message.contains("direction"), "{e}");

        assert!(parse("flowchart\n").is_err());
    }
}
