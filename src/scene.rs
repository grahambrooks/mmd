//! The intermediate every diagram type lays out into.
//!
//! A scene is positioned shapes and text in diagram coordinates. Layout stops
//! here, which is what lets layout be tested without matching SVG strings, and
//! what would let a future consumer draw a scene directly.

use crate::geom::{Point, Rect};
use crate::svg::{Number, Svg, Value};
use crate::theme::{Color, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

impl Anchor {
    fn as_str(self) -> &'static str {
        match self {
            Anchor::Start => "start",
            Anchor::Middle => "middle",
            Anchor::End => "end",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
    pub dashed: bool,
}

#[derive(Debug, Clone)]
pub enum Item {
    Rect {
        rect: Rect,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<Stroke>,
    },
    Circle {
        center: Point,
        radius: f32,
        fill: Option<Color>,
        stroke: Option<Stroke>,
    },
    /// A path given as SVG path data, built through [`PathData`].
    Path {
        data: String,
        fill: Option<Color>,
        stroke: Option<Stroke>,
        arrow: bool,
    },
    Text {
        at: Point,
        text: String,
        size: f32,
        bold: bool,
        anchor: Anchor,
        fill: Color,
        /// Degrees, clockwise, about `at`.
        rotate: f32,
    },
}

/// A laid-out diagram.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub items: Vec<Item>,
}

impl Scene {
    pub fn push(&mut self, item: Item) {
        self.items.push(item);
    }

    /// Every text string in the scene, in order. Tests use this to assert that
    /// nothing was dropped between parsing and layout.
    pub fn texts(&self) -> Vec<&str> {
        self.items
            .iter()
            .filter_map(|item| match item {
                Item::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// The drawn extent, ignoring text (which has no measured box here).
    pub fn shape_bounds(&self) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        let mut add = |rect: Rect| {
            bounds = Some(match bounds {
                Some(current) => current.union(rect),
                None => rect,
            });
        };
        for item in &self.items {
            match item {
                Item::Rect { rect, .. } => add(*rect),
                Item::Circle { center, radius, .. } => add(Rect::new(
                    center.x - radius,
                    center.y - radius,
                    radius * 2.0,
                    radius * 2.0,
                )),
                _ => {}
            }
        }
        bounds
    }
}

/// Builds SVG path data without hand-writing strings.
#[derive(Debug, Default)]
pub struct PathData(String);

impl PathData {
    pub fn new() -> Self {
        Self(String::new())
    }

    pub fn move_to(mut self, point: Point) -> Self {
        self.push('M', &[point.x, point.y]);
        self
    }

    pub fn line_to(mut self, point: Point) -> Self {
        self.push('L', &[point.x, point.y]);
        self
    }

    /// Cubic Bézier.
    pub fn curve_to(mut self, c1: Point, c2: Point, end: Point) -> Self {
        self.push('C', &[c1.x, c1.y, c2.x, c2.y, end.x, end.y]);
        self
    }

    pub fn close(mut self) -> Self {
        self.0.push('Z');
        self
    }

    pub fn finish(self) -> String {
        self.0
    }

    fn push(&mut self, command: char, values: &[f32]) {
        if !self.0.is_empty() {
            self.0.push(' ');
        }
        self.0.push(command);
        for value in values {
            self.0.push(' ');
            self.0.push_str(&Number(*value).to_string());
        }
    }
}

const ARROW_ID: &str = "mmd-arrow";

/// Write a scene as an SVG document.
pub fn to_svg(scene: &Scene, theme: &Theme, bounds: Rect) -> String {
    let mut svg = Svg::new();

    svg.element(
        "rect",
        &[
            ("x", bounds.x.into()),
            ("y", bounds.y.into()),
            ("width", bounds.width.into()),
            ("height", bounds.height.into()),
            ("fill", theme.background.to_string().as_str().into()),
        ],
    );

    if scene
        .items
        .iter()
        .any(|item| matches!(item, Item::Path { arrow: true, .. }))
    {
        write_arrow_marker(&mut svg, theme);
    }

    for item in &scene.items {
        write_item(&mut svg, item, theme);
    }
    svg.finish(bounds)
}

fn write_arrow_marker(svg: &mut Svg, theme: &Theme) {
    svg.begin("defs", &[]);
    svg.begin(
        "marker",
        &[
            ("id", ARROW_ID.into()),
            ("viewBox", "0 0 10 10".into()),
            ("refX", 9.0.into()),
            ("refY", 5.0.into()),
            ("markerWidth", 7.0.into()),
            ("markerHeight", 7.0.into()),
            ("orient", "auto-start-reverse".into()),
            ("markerUnits", "strokeWidth".into()),
        ],
    );
    svg.element(
        "path",
        &[
            ("d", "M 0 1 L 10 5 L 0 9 z".into()),
            ("fill", theme.line.to_string().as_str().into()),
        ],
    );
    svg.end();
    svg.end();
}

fn write_item(svg: &mut Svg, item: &Item, theme: &Theme) {
    match item {
        Item::Rect {
            rect,
            radius,
            fill,
            stroke,
        } => {
            let mut attrs = vec![
                ("x", Value::from(rect.x)),
                ("y", rect.y.into()),
                ("width", rect.width.into()),
                ("height", rect.height.into()),
            ];
            if *radius > 0.0 {
                attrs.push(("rx", (*radius).into()));
            }
            let fill_text = fill_value(*fill);
            attrs.push(("fill", fill_text.as_str().into()));
            let stroke_attrs = stroke_values(stroke.as_ref());
            let borrowed: Vec<(&str, Value<'_>)> = stroke_attrs
                .iter()
                .map(|(name, value)| (*name, Value::from(value.as_str())))
                .collect();
            attrs.extend(borrowed);
            svg.element("rect", &attrs);
        }
        Item::Circle {
            center,
            radius,
            fill,
            stroke,
        } => {
            let fill_text = fill_value(*fill);
            let mut attrs = vec![
                ("cx", Value::from(center.x)),
                ("cy", center.y.into()),
                ("r", (*radius).into()),
                ("fill", fill_text.as_str().into()),
            ];
            let stroke_attrs = stroke_values(stroke.as_ref());
            let borrowed: Vec<(&str, Value<'_>)> = stroke_attrs
                .iter()
                .map(|(name, value)| (*name, Value::from(value.as_str())))
                .collect();
            attrs.extend(borrowed);
            svg.element("circle", &attrs);
        }
        Item::Path {
            data,
            fill,
            stroke,
            arrow,
        } => {
            let fill_text = fill_value(*fill);
            let marker = format!("url(#{ARROW_ID})");
            let mut attrs = vec![
                ("d", Value::from(data.as_str())),
                ("fill", fill_text.as_str().into()),
            ];
            let stroke_attrs = stroke_values(stroke.as_ref());
            let borrowed: Vec<(&str, Value<'_>)> = stroke_attrs
                .iter()
                .map(|(name, value)| (*name, Value::from(value.as_str())))
                .collect();
            attrs.extend(borrowed);
            if *arrow {
                attrs.push(("marker-end", marker.as_str().into()));
            }
            svg.element("path", &attrs);
        }
        Item::Text {
            at,
            text,
            size,
            bold,
            anchor,
            fill,
            rotate,
        } => {
            let fill_text = fill.to_string();
            let transform = format!(
                "rotate({} {} {})",
                Number(*rotate),
                Number(at.x),
                Number(at.y)
            );
            let mut attrs = vec![
                ("x", Value::from(at.x)),
                ("y", at.y.into()),
                ("font-family", theme.font_family.as_str().into()),
                ("font-size", (*size).into()),
                ("fill", fill_text.as_str().into()),
                ("text-anchor", anchor.as_str().into()),
            ];
            if *bold {
                attrs.push(("font-weight", "bold".into()));
            }
            if *rotate != 0.0 {
                attrs.push(("transform", transform.as_str().into()));
            }
            svg.text_element("text", &attrs, text);
        }
    }
}

fn fill_value(fill: Option<Color>) -> String {
    match fill {
        Some(color) => color.to_string(),
        None => "none".to_string(),
    }
}

fn stroke_values(stroke: Option<&Stroke>) -> Vec<(&'static str, String)> {
    match stroke {
        None => Vec::new(),
        Some(stroke) => {
            let mut out = vec![
                ("stroke", stroke.color.to_string()),
                ("stroke-width", Number(stroke.width).to_string()),
            ];
            if stroke.dashed {
                out.push(("stroke-dasharray", "4 3".to_string()));
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_data_is_compact() {
        let data = PathData::new()
            .move_to(Point::new(0.0, 0.0))
            .line_to(Point::new(10.5, 2.0))
            .close()
            .finish();
        assert_eq!(data, "M 0 0 L 10.5 2Z");
    }

    #[test]
    fn arrow_marker_is_defined_only_when_used() {
        let theme = Theme::default();
        let bounds = Rect::new(0.0, 0.0, 10.0, 10.0);

        let plain = to_svg(&Scene::default(), &theme, bounds);
        assert!(!plain.contains("marker"), "{plain}");

        let mut scene = Scene::default();
        scene.push(Item::Path {
            data: "M 0 0 L 1 1".into(),
            fill: None,
            stroke: None,
            arrow: true,
        });
        let arrowed = to_svg(&scene, &theme, bounds);
        assert!(arrowed.contains("<marker"), "{arrowed}");
        assert!(arrowed.contains("marker-end"), "{arrowed}");
    }

    #[test]
    fn output_never_contains_a_foreign_object_or_stylesheet() {
        let mut scene = Scene::default();
        scene.push(Item::Text {
            at: Point::new(1.0, 2.0),
            text: "label".into(),
            size: 12.0,
            bold: false,
            anchor: Anchor::Middle,
            fill: Color(0, 0, 0),
            rotate: 0.0,
        });
        let out = to_svg(&scene, &Theme::default(), Rect::new(0.0, 0.0, 10.0, 10.0));
        assert!(!out.contains("foreignObject"));
        assert!(!out.contains("<style"));
        assert!(out.contains("<text"));
    }
}
