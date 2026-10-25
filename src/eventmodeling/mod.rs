//! Event models (`eventmodeling`): commands, events and views in
//! swimlanes, left to right in time.

mod layout;
mod parse;

pub use parse::{EntityType, EventModel, Frame, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
