//! `info` layout: one box naming the renderer and its version, where
//! mermaid prints `v<mermaid version>`.

use diagram_scene::{Anchor, Font, Node, Paint, Point, Rect, Role, Scene, Shape, Text, TextLine};

use super::parse::Info;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// What the box says.
pub fn banner() -> String {
    format!("mmd {}", env!("CARGO_PKG_VERSION"))
}

pub fn layout(info: &Info, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let title_size = Font::Title.size(FONT_SIZE);
    let mut top = PADDING;
    let mut width: f32 = 0.0;
    if let Some(title) = &info.title {
        width = measure.width(title, title_size, true);
        top += title_size + PADDING;
    }
    let text = banner();
    let box_width = measure.width(&text, title_size, true) + 40.0;
    let box_height = title_size * 2.4;
    width = width.max(box_width) + PADDING * 2.0;
    let mut node = Node::new(
        "info",
        Shape::Rounded,
        Rect::new((width - box_width) / 2.0, top, box_width, box_height),
    );
    node.label = vec![TextLine::new(text, Font::Title)];
    scene.nodes.push(node);
    if let Some(title) = &info.title {
        scene.title = Some(Text {
            pos: Point::new(width / 2.0, PADDING + title_size),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
        });
    }
    scene.width = width;
    scene.height = top + box_height + PADDING;
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    #[test]
    fn the_version_and_title_reach_the_scene_inside_the_bounds() {
        let info = Info {
            title: Some("About".into()),
            ..Info::default()
        };
        let scene = layout(&info, &Estimate);
        let texts = scene.texts();
        assert!(texts.contains(&"About".to_string()));
        assert!(texts.contains(&banner()));
        assert!(scene.bounds().encloses(&scene.nodes[0].rect, 0.01));
    }
}
