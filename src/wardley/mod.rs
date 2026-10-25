//! Wardley maps (`wardley-beta`).

mod layout;
mod parse;

pub use parse::{Component, Flow, Kind, Link, Strategy, Wardley, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
