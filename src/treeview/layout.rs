//! One row per entry, indented by depth, with the listing's connector
//! lines: a stub into each entry and a rail down from each folder.

use diagram_scene::{
    Anchor, Edge, Font, Marker, Paint, Pen, Role, Scene, Seg, Text, TextLine, Weight, geom::Point,
    line_width, shape::line_height,
};

use super::parse::TreeView;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Horizontal step per level, and the stub's length into an entry.
const INDENT: f32 = 22.0;
const STUB: f32 = 14.0;
/// Text inset from its row's left end.
const PAD_X: f32 = 5.0;
const PAD_Y: f32 = 5.0;
const TITLE_GAP: f32 = 10.0;

pub fn layout(tree: &TreeView, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let row = line_height(FONT_SIZE) + 2.0 * PAD_Y;
    let mut top = PADDING;
    let mut width: f32 = 0.0;
    if let Some(title) = tree.title.as_ref().filter(|t| !t.is_empty()) {
        let line = TextLine::new(title.clone(), Font::Title);
        let size = Font::Title.size(FONT_SIZE);
        width = line_width(measure, &line, FONT_SIZE);
        scene.title = Some(Text {
            pos: Point::new(PADDING, top + size * 0.8),
            anchor: Anchor::Start,
            line,
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
        top += line_height(size) + TITLE_GAP;
    }

    // Rows in outline order: each entry's top and left.
    let mut order = Vec::new();
    walk(tree, 0, &mut order);
    let mut at = vec![(0.0_f32, 0.0_f32); tree.entries.len()];
    for (i, &entry) in order.iter().enumerate() {
        let depth = tree.entries[entry].depth as f32;
        // Leave room for the root's own stub at depth 0.
        let x = PADDING + STUB + depth * INDENT;
        let y = top + i as f32 * row;
        at[entry] = (x, y);
        let line = TextLine::new(tree.entries[entry].name.clone(), Font::Body);
        width = width.max(x + PAD_X + line_width(measure, &line, FONT_SIZE) - PADDING);
        scene.texts.push(Text {
            pos: Point::new(x + PAD_X, y + row / 2.0 + FONT_SIZE * 0.35),
            anchor: Anchor::Start,
            line,
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
        scene.edges.push(line_edge(
            Point::new(x - STUB, y + row / 2.0),
            Point::new(x, y + row / 2.0),
        ));
    }
    for (entry, e) in tree.entries.iter().enumerate() {
        if let Some(&last) = e.children.last() {
            let (x, y) = at[entry];
            let end = at[last].1 + row / 2.0;
            // Down from under the entry's text to its last child's stub.
            let rail = x + INDENT - STUB;
            scene.edges.push(line_edge(
                Point::new(rail, y + row / 2.0 + FONT_SIZE * 0.6),
                Point::new(rail, end),
            ));
        }
    }
    scene.width = width + 2.0 * PADDING;
    scene.height = top + order.len() as f32 * row + PADDING;
    scene
}

fn walk(tree: &TreeView, entry: usize, out: &mut Vec<usize>) {
    out.push(entry);
    for &child in &tree.entries[entry].children {
        walk(tree, child, out);
    }
}

fn line_edge(a: Point, b: Point) -> Edge {
    let mut edge = Edge::new(vec![Seg::Move(a), Seg::Line(b)]);
    edge.end = Marker::None;
    edge.pen = Pen {
        weight: Weight::Thin,
        ..Pen::default()
    };
    edge
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    #[test]
    fn every_entry_is_drawn_inside_the_bounds() {
        let tree = super::super::parse(
            "treeView-beta\ntitle Repo\n\"src\"\n  \"main.rs\"\n  \"lib\"\n    \"a.rs\"\n\"Cargo.toml\"\n",
        )
        .unwrap();
        let s = layout(&tree, &Estimate);
        let texts = s.texts();
        for t in ["Repo", "/", "src", "main.rs", "lib", "a.rs", "Cargo.toml"] {
            assert!(texts.iter().any(|x| x == t), "missing {t}");
        }
        // Rows never share a baseline.
        let mut ys: Vec<f32> = s.texts.iter().map(|t| t.pos.y).collect();
        ys.dedup();
        assert_eq!(ys.len(), 6);
        for t in &s.texts {
            assert!(t.pos.x >= 0.0 && t.pos.y <= s.height && t.pos.x < s.width);
        }
    }
}
