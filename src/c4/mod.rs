//! C4 context, container and component diagrams.

mod layout;
mod parse;

pub use parse::{
    Arrows, Boundary, BoundaryKind, C4, Direction, Element, ElementKind, Form, Level, Relation,
    View, parse,
};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
