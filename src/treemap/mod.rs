//! Treemaps (`treemap`, `treemap-beta`): nested sections whose leaves are
//! sized by value.

mod layout;
mod parse;

pub use parse::{Item, Treemap, parse};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
