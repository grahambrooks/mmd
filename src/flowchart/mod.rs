//! Flowcharts (`flowchart` and `graph`).

mod layout;
mod parse;

/// Labels as lines, with markdown emphasis; shared with the types whose
/// labels take the same markdown strings.
pub(crate) use layout::text_lines;

pub use parse::{
    Colors, Direction, End, Flowchart, LineKind, Link, LinkStyle, Node, NodeShape, Stroke,
    Subgraph, parse,
};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let chart = parse(source)?;
    Ok(layout::layout(&chart, measure))
}
