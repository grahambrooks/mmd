//! Block diagrams (`block`, `block-beta`): blocks placed on a column grid
//! the author controls, with nested composites and links between blocks.

mod layout;
mod parse;

pub use parse::{Block, BlockDiagram, BlockKind, BlockShape, Dir, End, Link, Stroke, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
