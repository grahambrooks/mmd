//! Palette and font settings, as data.

/// An sRGB colour, written as `#rrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub u8, pub u8, pub u8);

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// Colours and sizes for a rendered diagram.
#[derive(Debug, Clone)]
pub struct Theme {
    pub font_family: String,
    pub font_size: f32,
    pub title_size: f32,
    pub small_size: f32,

    pub text: Color,
    pub muted: Color,
    pub background: Color,
    pub line: Color,

    /// Fills used for series: branches, actors, and so on. Cycled by index.
    pub series: Vec<Color>,
    pub series_text: Color,

    /// C4 element fills.
    pub person: Color,
    pub system: Color,
    pub external: Color,

    pub padding: f32,
    pub corner_radius: f32,
    pub stroke_width: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            #[cfg(feature = "bundled-fonts")]
            font_family: crate::text::BundledFonts::FAMILY.to_string(),
            #[cfg(not(feature = "bundled-fonts"))]
            font_family: "serif".to_string(),
            font_size: 14.0,
            title_size: 18.0,
            small_size: 11.0,

            text: Color(0x1f, 0x23, 0x28),
            muted: Color(0x59, 0x63, 0x6e),
            background: Color(0xff, 0xff, 0xff),
            line: Color(0x8c, 0x8c, 0x8c),

            series: vec![
                Color(0x15, 0x65, 0xc0),
                Color(0xc6, 0x28, 0x28),
                Color(0x2e, 0x7d, 0x32),
                Color(0x6a, 0x1b, 0x9a),
                Color(0xef, 0x6c, 0x00),
                Color(0x00, 0x83, 0x8f),
            ],
            series_text: Color(0xff, 0xff, 0xff),

            person: Color(0x08, 0x54, 0x8b),
            system: Color(0x11, 0x68, 0xbd),
            external: Color(0x99, 0x99, 0x99),

            padding: 16.0,
            corner_radius: 4.0,
            stroke_width: 1.5,
        }
    }
}

impl Theme {
    /// The fill for series `index`, cycling through the palette.
    pub fn series_color(&self, index: usize) -> Color {
        self.series[index % self.series.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_format_as_css_hex() {
        assert_eq!(Color(0x1f, 0x23, 0x28).to_string(), "#1f2328");
        assert_eq!(Color(0, 0, 0).to_string(), "#000000");
    }

    #[test]
    fn series_colors_cycle() {
        let theme = Theme::default();
        let len = theme.series.len();
        assert_eq!(theme.series_color(0), theme.series_color(len));
    }
}
