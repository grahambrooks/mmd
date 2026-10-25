//! `wardley-beta` parsing, per mermaid's
//! `packages/parser/src/language/wardley/wardley.langium` and
//! `packages/mermaid/src/diagrams/wardley/wardleyParser.ts`.
//!
//! Coordinates are `[visibility, evolution]`, each `0`–`1` (or `0`–`100`,
//! as mermaid also accepts), and are stored as percentages.

use crate::{Error, strip_comment};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Component,
    /// A user or customer: a bold label, no circle.
    Anchor,
    /// A component inside a `pipeline` block, on its parent's visibility.
    PipelineMember,
}

/// `(build)`, `(buy)`, `(outsource)`, `(market)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Build,
    Buy,
    Outsource,
    Market,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    pub name: String,
    pub kind: Kind,
    /// Percent, 0 at the bottom.
    pub visibility: f32,
    /// Percent, 0 at the left (genesis).
    pub evolution: f32,
    /// `label [x, y]`, in px from the component.
    pub label_offset: Option<(f32, f32)>,
    pub strategy: Option<Strategy>,
    pub inertia: bool,
    /// `evolve <name> <target>`: where it is heading, in percent.
    pub evolve: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Forward,
    Backward,
    Both,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    /// Indices into [`Wardley::components`].
    pub from: usize,
    pub to: usize,
    pub dashed: bool,
    pub flow: Option<Flow>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    /// The parent component.
    pub parent: usize,
    pub members: Vec<usize>,
}

/// One stage of the evolution axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    pub name: String,
    /// `@0.3`: where the stage ends, 0–1.
    pub boundary: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub text: String,
    pub visibility: f32,
    pub evolution: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub number: u32,
    pub visibility: f32,
    pub evolution: f32,
    pub text: String,
}

/// An `accelerator` (pointing right) or `deaccelerator` (left).
#[derive(Debug, Clone, PartialEq)]
pub struct Force {
    pub name: String,
    pub visibility: f32,
    pub evolution: f32,
    pub accelerates: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Wardley {
    pub title: Option<String>,
    /// `size [width, height]`, in px.
    pub size: Option<(f32, f32)>,
    pub stages: Vec<Stage>,
    pub components: Vec<Component>,
    pub links: Vec<Link>,
    pub pipelines: Vec<Pipeline>,
    pub notes: Vec<Note>,
    /// `annotations [v, e]`: where the numbered list goes.
    pub annotations_box: Option<(f32, f32)>,
    pub annotations: Vec<Annotation>,
    pub forces: Vec<Force>,
}

impl Wardley {
    fn find(&self, name: &str) -> Option<usize> {
        self.components.iter().position(|c| c.name == name)
    }

    /// The stages as drawn: the source's, or mermaid's default four.
    pub fn stage_names(&self) -> Vec<String> {
        if self.stages.is_empty() {
            ["Genesis", "Custom Built", "Product", "Commodity"]
                .iter()
                .map(|s| s.to_string())
                .collect()
        } else {
            self.stages.iter().map(|s| s.name.clone()).collect()
        }
    }

    /// Each stage's (start, end), 0–1: the `@` boundaries when every stage
    /// has one, else equal widths.
    pub fn stage_spans(&self) -> Vec<(f32, f32)> {
        let count = self.stage_names().len();
        let bounds: Vec<f32> = self.stages.iter().filter_map(|s| s.boundary).collect();
        if !self.stages.is_empty() && bounds.len() == count {
            let mut start = 0.0;
            bounds
                .iter()
                .map(|&end| {
                    let span = (start, end);
                    start = end;
                    span
                })
                .collect()
        } else {
            let width = 1.0 / count as f32;
            (0..count)
                .map(|i| (i as f32 * width, (i + 1) as f32 * width))
                .collect()
        }
    }
}

pub fn parse(source: &str) -> Result<Wardley, Error> {
    let mut map = Wardley::default();
    let mut seen_header = false;
    // Links and evolves name components that may be declared later.
    let mut pending_links = Vec::new();
    let mut pending_evolves = Vec::new();
    let mut pipeline: Option<(usize, usize)> = None;
    for (index, raw) in source.lines().enumerate() {
        let number = index + 1;
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if !seen_header {
            if !line.eq_ignore_ascii_case("wardley-beta") {
                return Err(Error::new(number, "expected `wardley-beta`"));
            }
            seen_header = true;
            continue;
        }
        if let Some((parent, _)) = pipeline {
            if line == "}" {
                pipeline = None;
                continue;
            }
            let rest = line
                .strip_prefix("component")
                .filter(|r| r.starts_with([' ', '\t']))
                .ok_or_else(|| {
                    Error::new(
                        number,
                        "a pipeline holds only `component <name> [evolution]`",
                    )
                })?;
            let (name, rest) = name(rest.trim(), number)?;
            let (values, rest) = bracket(rest, number)?;
            let [evolution] = values[..] else {
                return Err(Error::new(
                    number,
                    "a pipeline component takes one coordinate: `[evolution]`",
                ));
            };
            let mut component = Component {
                name: name.clone(),
                kind: Kind::PipelineMember,
                visibility: map.components[parent].visibility,
                evolution: percent(evolution, number)?,
                label_offset: None,
                strategy: None,
                inertia: false,
                evolve: None,
            };
            decorations(&mut component, rest, number)?;
            if map.find(&name).is_some() {
                return Err(Error::new(number, format!("`{name}` is already declared")));
            }
            map.components.push(component);
            let member = map.components.len() - 1;
            if let Some(p) = map.pipelines.iter_mut().find(|p| p.parent == parent) {
                p.members.push(member);
            }
            continue;
        }
        let (word, rest) = match line.find([' ', '\t']) {
            Some(at) => (&line[..at], line[at..].trim()),
            None => (line, ""),
        };
        match word {
            "title" => map.title = Some(rest.to_string()),
            "accTitle" | "accTitle:" | "accDescr" | "accDescr:" => {}
            "size" => {
                let (values, rest) = bracket(rest, number)?;
                match (&values[..], rest.is_empty()) {
                    ([w, h], true) if *w > 0.0 && *h > 0.0 => map.size = Some((*w, *h)),
                    _ => return Err(Error::new(number, "expected `size [width, height]`")),
                }
            }
            "evolution" => {
                for stage in rest.split("->") {
                    let stage = stage.trim();
                    let (first, second) = match stage.split_once('/') {
                        Some((a, b)) => (a.trim(), Some(b.trim())),
                        None => (stage, None),
                    };
                    let (name, boundary) = match first.split_once('@') {
                        Some((n, b)) => (
                            n.trim(),
                            Some(b.trim().parse::<f32>().map_err(|_| {
                                Error::new(number, format!("`{b}` is not a stage boundary"))
                            })?),
                        ),
                        None => (first, None),
                    };
                    let name = unquote(name);
                    if name.is_empty() {
                        return Err(Error::new(number, "expected `evolution A -> B -> …`"));
                    }
                    let name = match second {
                        Some(s) => format!("{name} / {}", unquote(s)),
                        None => name.to_string(),
                    };
                    map.stages.push(Stage { name, boundary });
                }
                if map.stages.len() < 2 {
                    return Err(Error::new(number, "`evolution` needs two or more stages"));
                }
            }
            "anchor" | "component" => {
                let (name, rest) = name(rest, number)?;
                let (values, rest) = bracket(rest, number)?;
                let [visibility, evolution] = values[..] else {
                    return Err(Error::new(
                        number,
                        format!("expected `{word} <name> [visibility, evolution]`"),
                    ));
                };
                let mut component = Component {
                    name: name.clone(),
                    kind: if word == "anchor" {
                        Kind::Anchor
                    } else {
                        Kind::Component
                    },
                    visibility: percent(visibility, number)?,
                    evolution: percent(evolution, number)?,
                    label_offset: None,
                    strategy: None,
                    inertia: false,
                    evolve: None,
                };
                if word == "anchor" {
                    if !rest.is_empty() {
                        return Err(Error::new(number, format!("unexpected `{rest}`")));
                    }
                } else {
                    decorations(&mut component, rest, number)?;
                }
                if map.find(&name).is_some() {
                    return Err(Error::new(number, format!("`{name}` is already declared")));
                }
                map.components.push(component);
            }
            "evolve" => {
                let usage = || Error::new(number, "expected `evolve <name> <evolution>`");
                let (head, target) = rest.rsplit_once([' ', '\t']).ok_or_else(usage)?;
                let (name, after) = name(head, number)?;
                if !after.trim().is_empty() {
                    return Err(usage());
                }
                let target = target.parse::<f32>().map_err(|_| usage())?;
                pending_evolves.push((name, percent(target, number)?, number));
            }
            "pipeline" => {
                let body = rest
                    .strip_suffix('{')
                    .ok_or_else(|| Error::new(number, "expected `pipeline <name> {`"))?;
                let (name, after) = name(body.trim(), number)?;
                if !after.is_empty() {
                    return Err(Error::new(number, "expected `pipeline <name> {`"));
                }
                let parent = map.find(&name).ok_or_else(|| {
                    Error::new(
                        number,
                        format!("pipeline `{name}` must name a component declared before it"),
                    )
                })?;
                map.pipelines.push(Pipeline {
                    parent,
                    members: Vec::new(),
                });
                pipeline = Some((parent, number));
            }
            "note" => {
                let (text, rest) = quoted(rest, number, "note \"text\" [visibility, evolution]")?;
                let (values, rest) = bracket(rest, number)?;
                let (&[visibility, evolution], true) = (&values[..], rest.is_empty()) else {
                    return Err(Error::new(
                        number,
                        "expected `note \"text\" [visibility, evolution]`",
                    ));
                };
                map.notes.push(Note {
                    text,
                    visibility: percent(visibility, number)?,
                    evolution: percent(evolution, number)?,
                });
            }
            "annotations" => {
                let (values, rest) = bracket(rest, number)?;
                let (&[visibility, evolution], true) = (&values[..], rest.is_empty()) else {
                    return Err(Error::new(
                        number,
                        "expected `annotations [visibility, evolution]`",
                    ));
                };
                map.annotations_box =
                    Some((percent(visibility, number)?, percent(evolution, number)?));
            }
            "annotation" => {
                let usage = "annotation <n>,[visibility, evolution] \"text\"";
                let (n, rest) = rest
                    .split_once(',')
                    .ok_or_else(|| Error::new(number, format!("expected `{usage}`")))?;
                let n = n
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| Error::new(number, format!("expected `{usage}`")))?;
                let (values, rest) = bracket(rest.trim(), number)?;
                let [visibility, evolution] = values[..] else {
                    return Err(Error::new(number, format!("expected `{usage}`")));
                };
                let (text, rest) = quoted(rest, number, usage)?;
                if !rest.is_empty() {
                    return Err(Error::new(number, format!("expected `{usage}`")));
                }
                map.annotations.push(Annotation {
                    number: n,
                    visibility: percent(visibility, number)?,
                    evolution: percent(evolution, number)?,
                    text,
                });
            }
            "accelerator" | "deaccelerator" => {
                let (name, rest) = name(rest, number)?;
                let (values, rest) = bracket(rest, number)?;
                let (&[visibility, evolution], true) = (&values[..], rest.is_empty()) else {
                    return Err(Error::new(
                        number,
                        format!("expected `{word} <name> [visibility, evolution]`"),
                    ));
                };
                map.forces.push(Force {
                    name,
                    visibility: percent(visibility, number)?,
                    evolution: percent(evolution, number)?,
                    accelerates: word == "accelerator",
                });
            }
            _ => pending_links.push((link(line, number)?, number)),
        }
    }
    if !seen_header {
        return Err(Error::new(1, "expected `wardley-beta`"));
    }
    if let Some((_, line)) = pipeline {
        return Err(Error::new(
            line,
            "this `pipeline` block is never closed with `}`",
        ));
    }
    let resolve = |map: &Wardley, name: &str, line: usize| {
        map.find(name)
            .ok_or_else(|| Error::new(line, format!("unknown component `{name}`")))
    };
    for ((from, to, dashed, flow, label), line) in pending_links {
        let link = Link {
            from: resolve(&map, &from, line)?,
            to: resolve(&map, &to, line)?,
            dashed,
            flow,
            label,
        };
        map.links.push(link);
    }
    for (name, target, line) in pending_evolves {
        let index = resolve(&map, &name, line)?;
        map.components[index].evolve = Some(target);
    }
    Ok(map)
}

type RawLink = (String, String, bool, Option<Flow>, Option<String>);

/// `A -> B`, `A +> B`, `A +<> B`, `A +'label'> B`, `A -.-> B`, `A -> B; note`.
fn link(line: &str, number: usize) -> Result<RawLink, Error> {
    let unsupported = || {
        let word = line.split_whitespace().next().unwrap_or(line);
        Error::new(
            number,
            format!("`{word}` is not supported in a wardley map (see DESIGN.md)"),
        )
    };
    let (from, rest) = name(line, number).map_err(|_| unsupported())?;
    let mut rest = rest.trim_start();
    let mut flow = None;
    let mut label = None;
    let mut dashed = false;
    let mut arrow = false;
    for (token, f) in [
        ("+<>", Flow::Both),
        ("+<", Flow::Backward),
        ("+>", Flow::Forward),
    ] {
        if let Some(r) = rest.strip_prefix(token) {
            flow = Some(f);
            rest = r.trim_start();
            arrow = true;
            break;
        }
    }
    if !arrow && let Some(r) = rest.strip_prefix("+'") {
        let end = r
            .find('\'')
            .ok_or_else(|| Error::new(number, "unterminated `+'label'` flow"))?;
        label = Some(r[..end].to_string());
        let r = &r[end + 1..];
        let (f, r) = if let Some(r) = r.strip_prefix("<>") {
            (Flow::Both, r)
        } else if let Some(r) = r.strip_prefix('<') {
            (Flow::Backward, r)
        } else if let Some(r) = r.strip_prefix('>') {
            (Flow::Forward, r)
        } else {
            return Err(Error::new(
                number,
                "expected `<`, `>` or `<>` after `+'label'`",
            ));
        };
        flow = Some(f);
        rest = r.trim_start();
        arrow = true;
    }
    for token in ["-.->", "-->", "->", ">"] {
        if let Some(r) = rest.strip_prefix(token) {
            dashed = token == "-.->";
            rest = r.trim_start();
            arrow = true;
            break;
        }
    }
    if !arrow {
        return Err(unsupported());
    }
    let (to, rest) = name(rest, number)?;
    let mut rest = rest.trim_start();
    for (token, f) in [
        ("+<>", Flow::Both),
        ("+<", Flow::Backward),
        ("+>", Flow::Forward),
    ] {
        if let Some(r) = rest.strip_prefix(token) {
            flow = flow.or(Some(f));
            rest = r.trim_start();
            break;
        }
    }
    if let Some(note) = rest.strip_prefix(';') {
        label = label.or(Some(note.trim().to_string()));
        rest = "";
    }
    if !rest.is_empty() {
        return Err(Error::new(number, format!("unexpected `{rest}` in a link")));
    }
    Ok((from, to, dashed, flow, label))
}

/// A name: quoted, or words up to a `[`, `{`, `;`, `+` or arrow.
fn name(text: &str, line: usize) -> Result<(String, &str), Error> {
    let text = text.trim_start();
    if text.starts_with('"') {
        return quoted(text, line, "a quoted name");
    }
    let mut end = text.len();
    for (i, c) in text.char_indices() {
        let rest = &text[i..];
        if matches!(c, '[' | '{' | ';' | '+' | '>' | '"')
            || rest.starts_with("->")
            || rest.starts_with("-->")
            || rest.starts_with("-.->")
        {
            end = i;
            break;
        }
    }
    // A trailing `label`, `(build)` or `inertia` is not part of the name;
    // they only follow the coordinates, so stopping at `[` is enough.
    let name = text[..end].trim();
    if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
        return Err(Error::new(line, format!("expected a name, found `{text}`")));
    }
    Ok((name.to_string(), &text[end..]))
}

fn quoted<'a>(text: &'a str, line: usize, usage: &str) -> Result<(String, &'a str), Error> {
    let inner = text
        .trim_start()
        .strip_prefix('"')
        .ok_or_else(|| Error::new(line, format!("expected `{usage}`")))?;
    let end = inner
        .find('"')
        .ok_or_else(|| Error::new(line, "unterminated string"))?;
    Ok((inner[..end].to_string(), inner[end + 1..].trim_start()))
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// `[a, b, …]` at the start, as numbers, and what follows.
fn bracket(text: &str, line: usize) -> Result<(Vec<f32>, &str), Error> {
    let inner = text
        .trim_start()
        .strip_prefix('[')
        .ok_or_else(|| Error::new(line, "expected `[` coordinates"))?;
    let end = inner
        .find(']')
        .ok_or_else(|| Error::new(line, "expected `]` after the coordinates"))?;
    let values = inner[..end]
        .split(',')
        .map(|v| {
            let v = v.trim();
            v.parse::<f32>()
                .map_err(|_| Error::new(line, format!("`{v}` is not a number")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((values, inner[end + 1..].trim_start()))
}

/// 0–1 (or 0–100) as a percentage, as mermaid's `toPercent` does.
fn percent(value: f32, line: usize) -> Result<f32, Error> {
    let p = if value <= 1.0 { value * 100.0 } else { value };
    if !(0.0..=100.0).contains(&p) {
        return Err(Error::new(
            line,
            format!("coordinate `{value}` must be between 0 and 1 (or 0 and 100)"),
        ));
    }
    Ok(p)
}

/// What may follow a component's coordinates: `label [x, y]`, a strategy
/// in parentheses, and `inertia` (bare or in parentheses).
fn decorations(component: &mut Component, mut rest: &str, line: usize) -> Result<(), Error> {
    while !rest.is_empty() {
        if let Some(r) = rest.strip_prefix("label") {
            let (values, r) = bracket(r, line)?;
            let [x, y] = values[..] else {
                return Err(Error::new(line, "expected `label [x, y]`"));
            };
            component.label_offset = Some((x, y));
            rest = r;
        } else if let Some(r) = rest.strip_prefix("inertia") {
            component.inertia = true;
            rest = r.trim_start();
        } else if let Some(r) = rest.strip_prefix('(') {
            let end = r
                .find(')')
                .ok_or_else(|| Error::new(line, "expected `)`"))?;
            match r[..end].trim() {
                "inertia" => component.inertia = true,
                "build" => component.strategy = Some(Strategy::Build),
                "buy" => component.strategy = Some(Strategy::Buy),
                "outsource" => component.strategy = Some(Strategy::Outsource),
                "market" => component.strategy = Some(Strategy::Market),
                other => {
                    return Err(Error::new(
                        line,
                        format!(
                            "`({other})` is not a decorator: expected `build`, `buy`, \
                             `outsource`, `market` or `inertia`"
                        ),
                    ));
                }
            }
            rest = r[end + 1..].trim_start();
        } else {
            return Err(Error::new(
                line,
                format!("unexpected `{rest}` after a component"),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEA: &str = "wardley-beta\ntitle Tea Shop\nsize [800, 500]\n\
        evolution Genesis@0.25 -> Custom@0.5 / Emerging -> Product@0.75 -> Commodity@1.0\n\
        anchor Business [0.95, 0.63]\n\
        component Cup of Tea [0.79, 0.61] label [10, -5]\n\
        component \"Hot Water\" [0.52, 0.80] (buy) (inertia)\n\
        component Kettle [0.43, 0.35] (build) inertia\n\
        component Power [10, 70]\n\
        Business -> Cup of Tea\n\
        Cup of Tea +'heat'> \"Hot Water\"\n\
        \"Hot Water\" -.-> Kettle; boils\n\
        Kettle +<> Power\n\
        evolve Kettle 0.62\n\
        pipeline Kettle {\n  component Campfire Kettle [0.30]\n  component Electric Kettle [0.53]\n}\n\
        note \"Standardise power\" [0.30, 0.49]\n\
        annotations [0.1, 0.9]\n\
        annotation 1,[0.5, 0.5] \"Key\"\n\
        accelerator \"Cloud\" [0.2, 0.8]\n\
        deaccelerator Legacy [0.4, 0.3]\n";

    #[test]
    fn reads_every_statement() {
        let map = parse(TEA).unwrap();
        assert_eq!(map.title.as_deref(), Some("Tea Shop"));
        assert_eq!(map.size, Some((800.0, 500.0)));
        assert_eq!(map.stages[1].name, "Custom / Emerging");
        assert_eq!(map.stage_spans()[1], (0.25, 0.5));
        let cup = &map.components[1];
        assert_eq!(cup.name, "Cup of Tea");
        assert!((cup.visibility - 79.0).abs() < 1e-3);
        assert_eq!(cup.label_offset, Some((10.0, -5.0)));
        let water = &map.components[2];
        assert_eq!(water.strategy, Some(Strategy::Buy));
        assert!(water.inertia);
        assert!(map.components[3].inertia);
        assert_eq!(map.components[4].evolution, 70.0);
        assert_eq!(map.links.len(), 4);
        assert_eq!(map.links[1].label.as_deref(), Some("heat"));
        assert_eq!(map.links[1].flow, Some(Flow::Forward));
        assert!(map.links[2].dashed);
        assert_eq!(map.links[2].label.as_deref(), Some("boils"));
        assert_eq!(map.links[3].flow, Some(Flow::Both));
        assert_eq!(map.components[3].evolve, Some(62.0));
        assert_eq!(map.pipelines[0].members.len(), 2);
        assert_eq!(map.components[5].visibility, map.components[3].visibility);
        assert_eq!(map.notes.len(), 1);
        assert_eq!(map.annotations[0].text, "Key");
        assert_eq!(map.forces.len(), 2);
        assert!(!map.forces[1].accelerates);
    }

    #[test]
    fn unknown_names_and_statements_are_errors_with_lines() {
        let error = parse("wardley-beta\ncomponent A [0.5, 0.5]\nA -> B\n").unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.message.contains("unknown component `B`"));
        let error = parse("wardley-beta\nstyle A fill:red\n").unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("style"), "{error}");
        let error = parse("wardley-beta\ncomponent A [150, 0.5]\n").unwrap_err();
        assert!(error.message.contains("between 0 and 1"), "{error}");
    }
}
