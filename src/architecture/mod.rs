//! Architecture diagrams (`architecture-beta`): services and junctions in
//! nested groups, joined by edges that leave and enter named sides.

mod layout;
mod parse;

pub use parse::{Architecture, Edge, Group, Item, ItemKind, Side, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
