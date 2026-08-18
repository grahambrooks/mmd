//! C4 context diagrams.

mod layout;
mod parse;

pub use parse::{C4, Element, ElementKind, Relation, parse};

use crate::Error;
use crate::scene::to_svg;
use crate::text::Measure;
use crate::theme::Theme;

/// Parse and render a C4 source to SVG.
pub fn render(source: &str, theme: &Theme, measure: &dyn Measure) -> Result<String, Error> {
    let model = parse(source)?;
    let (scene, bounds) = layout::layout(&model, theme, measure);
    Ok(to_svg(&scene, theme, bounds))
}

/// Lay out a source without rendering it, for tests.
pub fn layout(
    source: &str,
    theme: &Theme,
    measure: &dyn Measure,
) -> Result<(crate::scene::Scene, crate::geom::Rect), Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, theme, measure))
}
