//! Text measurement.
//!
//! Node sizes come from real shaping rather than emulated browser metrics, so
//! a label never overflows the box drawn around it. Hosts that already own a
//! font stack (md2pdf does) can implement [`Measure`] and drop the bundled
//! fonts with `--no-default-features`.

/// Measures text the way the eventual renderer will draw it.
pub trait Measure {
    /// Advance width of `text` at `size` px.
    fn width(&self, text: &str, size: f32, bold: bool) -> f32;

    /// Baseline-to-baseline distance at `size` px.
    fn line_height(&self, size: f32) -> f32 {
        size * 1.35
    }

    /// Distance from the top of a line box down to its baseline.
    fn ascent(&self, size: f32) -> f32 {
        size * 0.8
    }
}

/// Wrap `text` to at most `max_width`, breaking on spaces.
///
/// Words longer than the measure are left whole: a diagram label is better
/// slightly wide than silently chopped.
pub fn wrap(
    measure: &dyn Measure,
    text: &str,
    size: f32,
    bold: bool,
    max_width: f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if !current.is_empty() && measure.width(&candidate, size, bold) > max_width {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            } else {
                current = candidate;
            }
        }
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[cfg(feature = "bundled-fonts")]
mod bundled {
    use super::Measure;

    /// Measurement against fonts compiled into the binary.
    ///
    /// The same faces md2pdf sets body text in, so diagrams look native in a
    /// document rather than pasted into one.
    pub struct BundledFonts {
        regular: rustybuzz::Face<'static>,
        bold: rustybuzz::Face<'static>,
    }

    impl BundledFonts {
        /// Load the bundled faces. Fails only if the assets are unusable.
        pub fn new() -> Option<Self> {
            Some(Self {
                regular: face("Libertinus Serif", "Regular")?,
                bold: face("Libertinus Serif", "Bold")?,
            })
        }

        /// The font-family attribute that pairs with this measurement.
        pub const FAMILY: &'static str = "Libertinus Serif, Georgia, serif";
    }

    fn face(family: &str, subfamily: &str) -> Option<rustybuzz::Face<'static>> {
        let data = typst_assets::fonts().find(|data| {
            let Ok(parsed) = rustybuzz::ttf_parser::Face::parse(data, 0) else {
                return false;
            };
            name(&parsed, 1).as_deref() == Some(family)
                && name(&parsed, 2).as_deref() == Some(subfamily)
        })?;
        rustybuzz::Face::from_slice(data, 0)
    }

    fn name(face: &rustybuzz::ttf_parser::Face<'_>, id: u16) -> Option<String> {
        face.names()
            .into_iter()
            .find(|entry| entry.name_id == id && entry.is_unicode())
            .and_then(|entry| entry.to_string())
    }

    impl Measure for BundledFonts {
        fn width(&self, text: &str, size: f32, bold: bool) -> f32 {
            if text.is_empty() {
                return 0.0;
            }
            let face = if bold { &self.bold } else { &self.regular };
            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(text);
            buffer.guess_segment_properties();
            let shaped = rustybuzz::shape(face, &[], buffer);
            let units: i32 = shaped
                .glyph_positions()
                .iter()
                .map(|position| position.x_advance)
                .sum();
            let per_em = (face.units_per_em() as f32).max(1.0);
            units as f32 / per_em * size
        }
    }
}

#[cfg(feature = "bundled-fonts")]
pub use bundled::BundledFonts;

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    #[test]
    fn wrap_respects_the_measure() {
        let lines = wrap(&Fixed, "one two three four", 10.0, false, 40.0);
        assert!(lines.len() > 1, "{lines:?}");
        for line in &lines {
            assert!(Fixed.width(line, 10.0, false) <= 40.0 || !line.contains(' '));
        }
    }

    #[test]
    fn wrap_keeps_every_word() {
        let source = "alpha beta gamma delta epsilon";
        let lines = wrap(&Fixed, source, 10.0, false, 30.0);
        assert_eq!(lines.join(" "), source);
    }

    #[test]
    fn a_word_longer_than_the_measure_is_kept_whole() {
        let lines = wrap(&Fixed, "supercalifragilistic", 10.0, false, 20.0);
        assert_eq!(lines, vec!["supercalifragilistic"]);
    }

    #[cfg(feature = "bundled-fonts")]
    #[test]
    fn bundled_fonts_measure_sensibly() {
        let fonts = BundledFonts::new().expect("bundled fonts load");
        assert!(fonts.width("", 12.0, false) == 0.0);
        let narrow = fonts.width("ii", 12.0, false);
        let wide = fonts.width("MM", 12.0, false);
        assert!(wide > narrow, "{wide} should exceed {narrow}");
        assert!(fonts.width("hello", 24.0, false) > fonts.width("hello", 12.0, false));
    }
}
