//! YAML front matter: the `---` block mermaid accepts before a diagram.
//!
//! Only its top-level keys are read. `title` is drawn above the diagram;
//! `config` and `displayMode` tune mermaid's renderer and are ignored, as
//! `%%{init}%%` directives are. Any other key is an error naming it.

use diagram_scene::{Anchor, Font, Paint, Point, Role, Scene, Seg, Text, TextLine};

use crate::text::Measure;
use crate::{Error, FONT_SIZE, PADDING};

/// What the front matter said, and the source with it blanked out so line
/// numbers in later errors still match the file.
#[derive(Debug)]
pub(crate) struct FrontMatter {
    pub title: Option<String>,
    pub body: String,
}

/// Split front matter off `source`, if it starts with any.
pub(crate) fn split(source: &str) -> Result<Option<FrontMatter>, Error> {
    let lines: Vec<&str> = source.lines().collect();
    let Some(open) = lines
        .iter()
        .position(|l| !crate::strip_comment(l).trim().is_empty())
    else {
        return Ok(None);
    };
    if lines[open].trim() != "---" {
        return Ok(None);
    }
    let close = lines[open + 1..]
        .iter()
        .position(|l| l.trim() == "---")
        .map(|i| open + 1 + i)
        .ok_or_else(|| Error::new(open + 1, "front matter `---` is never closed"))?;

    let mut title = None;
    for (index, line) in lines.iter().enumerate().take(close).skip(open + 1) {
        let number = index + 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        // Nested lines belong to the key above them.
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let (key, value) = line.split_once(':').ok_or_else(|| {
            Error::new(
                number,
                format!("expected `key: value` in front matter, got `{line}`"),
            )
        })?;
        match key.trim() {
            "title" => title = Some(unquote(value.trim()).to_string()),
            "config" | "displayMode" => {}
            other => {
                return Err(Error::new(
                    number,
                    format!(
                        "front matter key `{other}` is not supported; expected `title`, `config` or `displayMode` (see DESIGN.md)"
                    ),
                ));
            }
        }
    }

    let body = lines
        .iter()
        .enumerate()
        .map(|(i, l)| if i <= close { "" } else { *l })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Some(FrontMatter {
        title: title.filter(|t| !t.is_empty()),
        body,
    }))
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner;
        }
    }
    value
}

/// Draw `title` centred above everything the scene already holds.
pub(crate) fn add_title(scene: &mut Scene, title: &str, measure: &dyn Measure) {
    let size = Font::Title.size(FONT_SIZE);
    let band = size + PADDING;
    let width = measure.width(title, size, true) + PADDING * 2.0;
    let dx = ((width - scene.width) / 2.0).max(0.0);
    shift(scene, dx, band);
    scene.width = scene.width.max(width);
    scene.height += band;
    let text = Text {
        pos: Point::new(scene.width / 2.0, PADDING + size * 0.8),
        anchor: Anchor::Middle,
        line: TextLine::new(title, Font::Title),
        paint: Paint::Role(Role::Text),
        backed: false,
    };
    match scene.title {
        None => scene.title = Some(text),
        Some(_) => scene.texts.push(text),
    }
}

/// Move everything in the scene, title and legend included.
fn shift(scene: &mut Scene, dx: f32, dy: f32) {
    let mv = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for n in &mut scene.nodes {
        n.rect.x += dx;
        n.rect.y += dy;
    }
    for c in &mut scene.containers {
        c.rect.x += dx;
        c.rect.y += dy;
    }
    for t in &mut scene.texts {
        mv(&mut t.pos);
    }
    if let Some(t) = &mut scene.title {
        mv(&mut t.pos);
    }
    if let Some(l) = &mut scene.legend {
        l.rect.x += dx;
        l.rect.y += dy;
    }
    for e in &mut scene.edges {
        for seg in &mut e.path {
            match seg {
                Seg::Move(p) | Seg::Line(p) => mv(p),
                Seg::Cubic(a, b, p) => {
                    mv(a);
                    mv(b);
                    mv(p);
                }
                Seg::Close => {}
            }
        }
        for l in &mut e.labels {
            mv(&mut l.pos);
        }
        if let Some((_, p)) = &mut e.step {
            mv(p);
        }
    }
    scene.width += dx * 2.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_title_and_blanks_the_block_keeping_line_numbers() {
        let fm = split("---\ntitle: \"Sales\"\nconfig:\n  theme: dark\n---\npie\n")
            .unwrap()
            .unwrap();
        assert_eq!(fm.title.as_deref(), Some("Sales"));
        assert_eq!(fm.body.lines().nth(5), Some("pie"));
        assert!(fm.body.lines().take(5).all(str::is_empty));
    }

    #[test]
    fn no_front_matter_is_none() {
        assert!(split("pie\n  \"a\": 1\n").unwrap().is_none());
    }

    #[test]
    fn unknown_keys_and_unclosed_blocks_are_errors() {
        let e = split("---\nlayout: elk\n---\ngraph\n").unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.message.contains("layout"), "{e}");
        let e = split("\n---\ntitle: x\n").unwrap_err();
        assert_eq!(e.line, 2);
    }
}
