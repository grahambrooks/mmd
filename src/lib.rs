//! Lean Mermaid diagram rendering to SVG, in pure Rust.
//!
//! No Node, no browser, no C toolchain. Labels are real SVG `<text>`, never
//! `<foreignObject>`, so output survives conversion by SVG-only consumers such
//! as `usvg` on the way into a PDF.
//!
//! ```
//! let svg = mmd::render("gitGraph\n  commit\n  branch dev\n  commit\n").unwrap();
//! assert!(svg.starts_with("<svg"));
//! ```
//!
//! See `DESIGN.md` for the supported syntax; anything outside it is a parse
//! error carrying a line number, never a silent omission.

pub mod c4;
pub mod er;
pub mod flowchart;
pub mod gitgraph;
mod layered;
pub mod text;

use std::fmt;

pub use diagram_scene::{self as scene, Scene, Theme, geom};
use text::Measure;

/// Body text size, in px.
pub(crate) const FONT_SIZE: f32 = 14.0;
/// Space around a diagram and its gutters.
pub(crate) const PADDING: f32 = 16.0;

/// Which diagram a source describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Kind {
    GitGraph,
    C4Context,
    Flowchart,
    Er,
}

impl Kind {
    /// The keyword that introduces this diagram.
    pub fn keyword(self) -> &'static str {
        match self {
            Kind::GitGraph => "gitGraph",
            Kind::C4Context => "C4Context",
            Kind::Flowchart => "flowchart",
            Kind::Er => "erDiagram",
        }
    }
}

/// A parse failure, located in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl Error {
    pub(crate) fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}", self.message)
        } else {
            write!(f, "line {}: {}", self.line, self.message)
        }
    }
}

impl std::error::Error for Error {}

/// Identify the diagram type from its first meaningful line.
pub fn detect(source: &str) -> Option<Kind> {
    for line in source.lines() {
        let line = strip_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        // `graph` is the older name for a flowchart.
        if line == "graph" || line.starts_with("graph ") {
            return Some(Kind::Flowchart);
        }
        for kind in [Kind::GitGraph, Kind::C4Context, Kind::Flowchart, Kind::Er] {
            let keyword = kind.keyword();
            if line == keyword
                || line.starts_with(&format!("{keyword} "))
                || line.starts_with(&format!("{keyword}:"))
            {
                return Some(kind);
            }
        }
        return None;
    }
    None
}

/// Strip a `%%` comment from a line.
pub(crate) fn strip_comment(line: &str) -> &str {
    match line.find("%%") {
        Some(index) => &line[..index],
        None => line,
    }
}

/// Render a diagram to SVG with the default (light) theme and bundled
/// fonts.
#[cfg(feature = "bundled-fonts")]
pub fn render(source: &str) -> Result<String, Error> {
    let fonts = text::BundledFonts::new()
        .ok_or_else(|| Error::new(0, "the bundled fonts could not be loaded"))?;
    let theme = Theme {
        font_family: text::BundledFonts::FAMILY.into(),
        ..Theme::light()
    };
    render_with(source, &theme, &fonts)
}

/// Render a diagram to SVG with an explicit theme and text measurer.
pub fn render_with(source: &str, theme: &Theme, measure: &dyn Measure) -> Result<String, Error> {
    Ok(diagram_scene::svg::to_svg(&scene(source, measure)?, theme))
}

/// Lay a diagram out as a [`Scene`], for hosts that draw it themselves.
/// Colours in it are theme roles; `measure` should measure the font the
/// host will draw text in.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    match detect(source) {
        Some(Kind::GitGraph) => gitgraph::scene(source, measure),
        Some(Kind::C4Context) => c4::scene(source, measure),
        Some(Kind::Flowchart) => flowchart::scene(source, measure),
        Some(Kind::Er) => er::scene(source, measure),
        None => Err(Error::new(
            first_content_line(source),
            "unrecognised diagram: expected one of `flowchart`, `erDiagram`, `gitGraph`, `C4Context`",
        )),
    }
}

fn first_content_line(source: &str) -> usize {
    source
        .lines()
        .enumerate()
        .find(|(_, line)| !strip_comment(line).trim().is_empty())
        .map(|(index, _)| index + 1)
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_diagrams() {
        assert_eq!(detect("gitGraph\n  commit\n"), Some(Kind::GitGraph));
        assert_eq!(detect("gitGraph:\n"), Some(Kind::GitGraph));
        assert_eq!(detect("C4Context\n  title x\n"), Some(Kind::C4Context));
        assert_eq!(detect("erDiagram\n  A ||--o{ B : x\n"), Some(Kind::Er));
    }

    #[test]
    fn skips_comments_and_blank_lines_when_detecting() {
        assert_eq!(
            detect("\n%% a comment\n\ngitGraph\n  commit\n"),
            Some(Kind::GitGraph)
        );
    }

    #[test]
    fn unsupported_diagrams_are_an_error_with_a_line() {
        assert_eq!(detect("pie title Pets\n  \"Dogs\" : 3\n"), None);
        let error = render_with("\n\npie\n", &Theme::default(), &Fixed).unwrap_err();
        assert_eq!(error.line, 3);
        assert!(error.to_string().contains("unrecognised diagram"));
    }

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }
}
