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
pub mod class;
pub mod er;
pub mod flowchart;
pub mod gantt;
pub mod gitgraph;
mod graph;
pub mod journey;
pub mod kanban;
mod layered;
pub mod requirement;
pub mod sequence;
pub mod state;
mod style;
pub mod text;
pub mod timeline;

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
    C4Container,
    C4Component,
    C4Dynamic,
    C4Deployment,
    Gantt,
    Timeline,
    Journey,
    Kanban,
    Sequence,
    State,
    Class,
    Requirement,
}

impl Kind {
    /// Every kind, in the order `detect()` tries them. Adding a diagram type
    /// adds its variant here, a `keyword`, and an arm in `scene()`.
    pub const ALL: &[Kind] = &[
        Kind::Flowchart,
        Kind::GitGraph,
        Kind::C4Context,
        Kind::C4Container,
        Kind::C4Component,
        Kind::C4Dynamic,
        Kind::C4Deployment,
        Kind::Er,
        Kind::Gantt,
        Kind::Timeline,
        Kind::Journey,
        Kind::Kanban,
        Kind::Sequence,
        Kind::State,
        Kind::Class,
        Kind::Requirement,
    ];

    /// The keyword that introduces this diagram.
    pub fn keyword(self) -> &'static str {
        match self {
            Kind::GitGraph => "gitGraph",
            Kind::C4Context => "C4Context",
            Kind::Flowchart => "flowchart",
            Kind::Er => "erDiagram",
            Kind::C4Container => "C4Container",
            Kind::C4Component => "C4Component",
            Kind::C4Dynamic => "C4Dynamic",
            Kind::C4Deployment => "C4Deployment",
            Kind::Gantt => "gantt",
            Kind::Timeline => "timeline",
            Kind::Journey => "journey",
            Kind::Kanban => "kanban",
            Kind::Sequence => "sequenceDiagram",
            Kind::State => "stateDiagram-v2",
            Kind::Class => "classDiagram",
            Kind::Requirement => "requirementDiagram",
        }
    }

    /// Other keywords that introduce the same diagram.
    pub fn aliases(self) -> &'static [&'static str] {
        match self {
            // `graph` is the older name for a flowchart. `flowchart-elk`
            // asks for the ELK layout engine; it gets mmd's own layout.
            Kind::Flowchart => &["graph", "flowchart-elk"],
            Kind::State => &["stateDiagram"],
            Kind::Class => &["classDiagram-v2"],
            Kind::Requirement => &["requirement"],
            _ => &[],
        }
    }

    /// Whether this kind renders, rather than being recognised only so its
    /// own parser can name what is missing.
    pub fn is_supported(self) -> bool {
        true
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
    let line = source
        .lines()
        .map(|line| strip_comment(line).trim())
        .find(|line| !line.is_empty())?;
    let introduces = |keyword: &str| {
        line.strip_prefix(keyword)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', ':', ';']))
    };
    Kind::ALL.iter().copied().find(|kind| {
        introduces(kind.keyword()) || kind.aliases().iter().any(|alias| introduces(alias))
    })
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
        Some(
            Kind::C4Context
            | Kind::C4Container
            | Kind::C4Component
            | Kind::C4Dynamic
            | Kind::C4Deployment,
        ) => c4::scene(source, measure),
        Some(Kind::Flowchart) => flowchart::scene(source, measure),
        Some(Kind::Er) => er::scene(source, measure),
        Some(Kind::Gantt) => gantt::scene(source, measure),
        Some(Kind::Timeline) => timeline::scene(source, measure),
        Some(Kind::Journey) => journey::scene(source, measure),
        Some(Kind::Kanban) => kanban::scene(source, measure),
        Some(Kind::Sequence) => sequence::scene(source, measure),
        Some(Kind::State) => state::scene(source, measure),
        Some(Kind::Class) => class::scene(source, measure),
        Some(Kind::Requirement) => requirement::scene(source, measure),
        None => {
            let line = first_content_line(source);
            if source.lines().nth(line - 1).map(str::trim) == Some("---") {
                return Err(Error::new(
                    line,
                    "YAML front matter (`---`) is not supported; use `title` where the \
                     diagram takes one (see DESIGN.md)",
                ));
            }
            let expected: Vec<String> = Kind::ALL
                .iter()
                .filter(|kind| kind.is_supported())
                .map(|kind| format!("`{}`", kind.keyword()))
                .collect();
            Err(Error::new(
                line,
                format!(
                    "unrecognised diagram: expected one of {}",
                    expected.join(", ")
                ),
            ))
        }
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
        assert_eq!(detect("C4Container\n"), Some(Kind::C4Container));
        assert_eq!(detect("C4Component\n"), Some(Kind::C4Component));
    }

    #[test]
    fn every_c4_header_reaches_the_c4_parser() {
        for header in ["C4Dynamic", "C4Deployment"] {
            let svg = render_with(
                &format!("{header}\nSystem(a, \"A\")\n"),
                &Theme::default(),
                &Fixed,
            );
            assert!(svg.unwrap().contains(">A<"), "{header}");
        }
        let svg = render_with(
            "C4Container\nContainer(a, \"A\", \"Rust\")\n",
            &Theme::default(),
            &Fixed,
        );
        assert!(svg.unwrap().contains("[Rust]"));
    }

    #[test]
    fn skips_comments_and_blank_lines_when_detecting() {
        assert_eq!(
            detect("\n%% a comment\n\ngitGraph\n  commit\n"),
            Some(Kind::GitGraph)
        );
    }

    #[test]
    fn front_matter_is_named_in_its_error() {
        let error =
            render_with("\n---\ntitle: x\n---\ngraph\n", &Theme::default(), &Fixed).unwrap_err();
        assert_eq!(error.line, 2);
        assert!(error.message.contains("front matter"), "{error}");
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
