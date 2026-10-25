//! Ishikawa (fishbone) diagrams (`ishikawa`, `ishikawa-beta`): an effect
//! at the head, categories of causes on bones off the spine.

mod layout;
mod parse;

pub use parse::{Cause, Ishikawa, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
