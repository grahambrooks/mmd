//! Mindmaps (`mindmap`): an indented outline drawn as a tree spreading
//! left and right from its root.

mod layout;
mod parse;

pub use parse::{Mindmap, Node, NodeShape, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
