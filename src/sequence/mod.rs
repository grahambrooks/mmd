//! Sequence diagrams.

mod layout;
mod parse;

pub use parse::{
    Event, Frame, Head, Message, Note, NoteSide, Participant, ParticipantBox, ParticipantKind, Ref,
    Sequence, parse,
};

use diagram_scene::Scene;

use crate::Error;
use crate::text::Measure;

/// Parse and lay out a source.
pub fn scene(source: &str, measure: &dyn Measure) -> Result<Scene, Error> {
    let model = parse(source)?;
    Ok(layout::layout(&model, measure))
}

/// Lay out a model built elsewhere: `zenuml` parses into this model.
pub(crate) fn draw(model: &Sequence, measure: &dyn Measure) -> Scene {
    layout::layout(model, measure)
}
