//! ZenUML: the external `@mermaid-js/mermaid-zenuml` plugin's code-like
//! sequence syntax, parsed into the `sequence` model and drawn with the
//! sequence layout.

mod parse;

pub use parse::parse;

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(crate::sequence::draw(&model, measure))
}
