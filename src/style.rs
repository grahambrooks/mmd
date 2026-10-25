//! `classDef`/`style` declarations shared by the diagram types that take them.
//!
//! Colours become presentation attributes on the scene, never a stylesheet.
//! A value this module cannot read is an error naming it, not a colour that
//! quietly falls back to the theme's.

use crate::Error;

/// A colour, as `#rrggbb` components.
pub type Rgb = (u8, u8, u8);

/// Colours set in the source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Colors {
    pub fill: Option<Rgb>,
    pub stroke: Option<Rgb>,
    pub text: Option<Rgb>,
    pub dashed: bool,
}

impl Colors {
    /// `other`'s colours over these.
    pub(crate) fn merge(&mut self, other: &Colors) {
        self.fill = other.fill.or(self.fill);
        self.stroke = other.stroke.or(self.stroke);
        self.text = other.text.or(self.text);
        self.dashed |= other.dashed;
    }
}

/// CSS-ish `fill:#f9f,stroke:#333,stroke-dasharray:5 5`, as pairs.
pub(crate) fn declarations(text: &str, line: usize) -> Result<Vec<(String, String)>, Error> {
    let mut out = Vec::new();
    for part in split(text) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part.split_once(':').ok_or_else(|| {
            Error::new(
                line,
                format!("expected `property:value` in styles, found `{part}`"),
            )
        })?;
        out.push((key.trim().to_string(), value.trim().to_string()));
    }
    Ok(out)
}

/// Split on `,` and `;`, but not inside `rgb(…)`.
fn split(text: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' | ';' if depth <= 0 => {
                out.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// A node's, entity's or subgraph's colours: fill, stroke, color and
/// stroke-dasharray. Other properties (stroke-width, font-*) are fine
/// styling the scene doesn't carry.
pub(crate) fn colors(declarations: &[(String, String)], line: usize) -> Result<Colors, Error> {
    let mut out = Colors::default();
    for (key, value) in declarations {
        match key.as_str() {
            "fill" | "background" => out.fill = Some(color(value, line)?),
            "stroke" => out.stroke = Some(color(value, line)?),
            "color" => out.text = Some(color(value, line)?),
            "stroke-dasharray" => out.dashed = dash_length(value, line)?.is_some_and(|l| l > 0.0),
            _ => {}
        }
    }
    Ok(out)
}

/// The first dash length of a `stroke-dasharray`; `None` for `none`.
pub(crate) fn dash_length(value: &str, line: usize) -> Result<Option<f32>, Error> {
    if value.eq_ignore_ascii_case("none") {
        return Ok(None);
    }
    let first = value
        .split([' ', ','])
        .find(|s| !s.is_empty())
        .unwrap_or("");
    number(first, line, "stroke-dasharray").map(Some)
}

/// A length such as `2`, `2px` or `1.5`.
pub(crate) fn number(value: &str, line: usize, property: &str) -> Result<f32, Error> {
    value
        .trim()
        .trim_end_matches("px")
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
        .ok_or_else(|| {
            Error::new(
                line,
                format!("`{property}:{value}` is not a length mermaid's styles accept"),
            )
        })
}

/// `#rgb`, `#rrggbb` (with or without alpha), `rgb(r, g, b)`, `rgba(…)`, or a
/// CSS colour name.
pub(crate) fn color(value: &str, line: usize) -> Result<Rgb, Error> {
    parse_color(value).ok_or_else(|| {
        let hint = if matches!(value.to_ascii_lowercase().as_str(), "none" | "transparent") {
            "; leave the property out for the theme's colour"
        } else {
            ""
        };
        Error::new(
            line,
            format!("unrecognised colour `{value}`{hint} (see DESIGN.md)"),
        )
    })
}

fn parse_color(value: &str) -> Option<Rgb> {
    let value = value.trim().trim_end_matches("!important").trim();
    let hex = |s: &str| u8::from_str_radix(s, 16).ok();
    if let Some(h) = value.strip_prefix('#') {
        if !h.is_ascii() {
            return None;
        }
        return match h.len() {
            3 | 4 => {
                let d = |i: usize| hex(&h[i..i + 1]).map(|v| v * 17);
                Some((d(0)?, d(1)?, d(2)?))
            }
            6 | 8 => Some((hex(&h[0..2])?, hex(&h[2..4])?, hex(&h[4..6])?)),
            _ => None,
        };
    }
    let lower = value.to_ascii_lowercase();
    if let Some(args) = lower
        .strip_prefix("rgba(")
        .or_else(|| lower.strip_prefix("rgb("))
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let parts: Vec<&str> = args
            .split([',', ' ', '/'])
            .filter(|s| !s.is_empty())
            .collect();
        if !(3..=4).contains(&parts.len()) {
            return None;
        }
        let channel = |s: &str| -> Option<u8> {
            match s.strip_suffix('%') {
                Some(p) => p
                    .parse::<f32>()
                    .ok()
                    .filter(|p| (0.0..=100.0).contains(p))
                    .map(|p| (p * 2.55).round() as u8),
                None => s.parse::<u8>().ok(),
            }
        };
        return Some((channel(parts[0])?, channel(parts[1])?, channel(parts[2])?));
    }
    NAMED
        .binary_search_by(|(name, _)| name.cmp(&lower.as_str()))
        .ok()
        .map(|i| {
            let v = NAMED[i].1;
            ((v >> 16) as u8, (v >> 8) as u8, v as u8)
        })
}

/// The CSS named colours, sorted for binary search.
const NAMED: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_colours_are_sorted_for_binary_search() {
        assert!(NAMED.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(NAMED.len(), 148);
    }

    #[test]
    fn reads_hex_rgb_and_names() {
        assert_eq!(color("#f96", 1).unwrap(), (0xff, 0x99, 0x66));
        assert_eq!(color("#B8860B", 1).unwrap(), (0xb8, 0x86, 0x0b));
        assert_eq!(color("#11223380", 1).unwrap(), (0x11, 0x22, 0x33));
        assert_eq!(color("rgb(10, 20, 30)", 1).unwrap(), (10, 20, 30));
        assert_eq!(color("rgba(10,20,30,0.5)", 1).unwrap(), (10, 20, 30));
        assert_eq!(color("LightBlue", 1).unwrap(), (0xad, 0xd8, 0xe6));
        assert_eq!(color("red !important", 1).unwrap(), (0xff, 0, 0));
    }

    #[test]
    fn an_unreadable_colour_is_an_error_naming_it() {
        for bad in ["blurple", "#ggg", "hsl(0, 50%, 50%)", "none", "rgb(1,2)"] {
            let error = color(bad, 4).unwrap_err();
            assert_eq!(error.line, 4);
            assert!(error.message.contains(bad), "{error}");
        }
    }

    #[test]
    fn declarations_split_outside_parentheses() {
        let d = declarations("fill:rgb(1, 2, 3),stroke:#000;color:white", 1).unwrap();
        assert_eq!(d.len(), 3);
        let c = colors(&d, 1).unwrap();
        assert_eq!(c.fill, Some((1, 2, 3)));
        assert_eq!(c.text, Some((255, 255, 255)));
    }

    #[test]
    fn dasharray_none_and_zero_are_solid() {
        let solid = |v: &str| {
            !colors(&[("stroke-dasharray".into(), v.into())], 1)
                .unwrap()
                .dashed
        };
        assert!(solid("none"));
        assert!(solid("0"));
        assert!(!solid("5 5"));
        assert!(dash_length("lots", 1).is_err());
    }
}
