//! State diagrams (`stateDiagram-v2`, `stateDiagram`).

mod layout;
mod parse;

pub use parse::{Direction, Group, Kind, Note, Side, State, StateDiagram, Transition, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
