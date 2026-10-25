//! C4 context, container, component, dynamic and deployment diagrams.

mod layout;
mod parse;

pub use parse::{
    Arrows, Boundary, BoundaryKind, C4, Colors, Direction, Element, ElementKind, ElementStyle,
    Form, Level, LineStyle, RelStyle, Relation, Rgb, Tag, View, parse,
};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}
