//! Rows of 32 equal bit cells; a field is a box across its bits, split
//! where it crosses a row's end, with its first and last bit numbers above.

use diagram_scene::{
    Anchor, Font, Node, Paint, Role, Scene, Shape, Text, TextLine, geom::Point, geom::Rect,
    line_width, shape::line_height,
};

use super::parse::{BITS_PER_ROW, Packet};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Bounds for a bit's width: wide enough for short labels, narrow enough
/// that a 32-bit row stays page-sized.
const MIN_BIT: f32 = 18.0;
const MAX_BIT: f32 = 30.0;
/// The band above each row for bit numbers.
const NUMBERS: f32 = 15.0;
const ROW_GAP: f32 = 4.0;
const TITLE_GAP: f32 = 10.0;

/// A field's share of one row.
struct Piece {
    field: usize,
    row: u32,
    start: u32,
    end: u32,
}

impl Piece {
    fn bits(&self) -> u32 {
        self.end - self.start + 1
    }
}

pub fn layout(packet: &Packet, measure: &dyn Measure) -> Scene {
    let mut pieces = Vec::new();
    for (index, field) in packet.fields.iter().enumerate() {
        let mut start = field.start;
        while start <= field.end {
            let row = start / BITS_PER_ROW;
            let end = field.end.min((row + 1) * BITS_PER_ROW - 1);
            pieces.push(Piece {
                field: index,
                row,
                start,
                end,
            });
            start = end + 1;
        }
    }

    let body = |text: &str| line_width(measure, &TextLine::new(text, Font::Body), FONT_SIZE);
    // A bit as wide as the most demanding label needs, within bounds.
    let bit = pieces
        .iter()
        .map(|p| (body(&packet.fields[p.field].label) + 10.0) / p.bits() as f32)
        .fold(MIN_BIT, f32::max)
        .min(MAX_BIT);

    // Each piece's label: body text if it fits, else small, else wrapped.
    let labels: Vec<Vec<TextLine>> = pieces
        .iter()
        .map(|p| {
            let text = &packet.fields[p.field].label;
            let room = p.bits() as f32 * bit - 6.0;
            if body(text) <= room {
                return vec![TextLine::new(text.clone(), Font::Body)];
            }
            let small = Font::Small.size(FONT_SIZE);
            crate::text::wrap(measure, text, small, false, room)
                .into_iter()
                .map(|line| TextLine::new(line, Font::Small))
                .collect()
        })
        .collect();
    // One size for every narrow field: if a field of N bits needs small
    // text, so does every field of N bits or fewer.
    let threshold = pieces
        .iter()
        .zip(&labels)
        .filter(|(_, l)| l.iter().any(|l| l.font() == Font::Small))
        .map(|(p, _)| p.bits())
        .max()
        .unwrap_or(0);
    let labels: Vec<Vec<TextLine>> = pieces
        .iter()
        .zip(labels)
        .map(|(p, lines)| {
            if p.bits() <= threshold && lines.iter().all(|l| l.font() == Font::Body) {
                lines
                    .into_iter()
                    .map(|l| TextLine::new(l.plain(), Font::Small))
                    .collect()
            } else {
                lines
            }
        })
        .collect();
    let rows = pieces.last().map_or(0, |p| p.row + 1);
    let row_height = labels
        .iter()
        .map(|lines| {
            lines
                .iter()
                .map(|l| line_height(l.font().size(FONT_SIZE)))
                .sum::<f32>()
                + 16.0
        })
        .fold(34.0, f32::max);

    let width = BITS_PER_ROW as f32 * bit;
    let mut top = PADDING;
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    if let Some(title) = packet.title.as_ref().filter(|t| !t.is_empty()) {
        let size = Font::Title.size(FONT_SIZE);
        let title_width = line_width(measure, &TextLine::new(title, Font::Title), FONT_SIZE);
        scene.title = Some(Text {
            pos: Point::new(PADDING + width.max(title_width) / 2.0, top + size * 0.8),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
        top += line_height(size) + TITLE_GAP;
    }
    let pitch = NUMBERS + row_height + ROW_GAP;
    let small = Font::Small.size(FONT_SIZE);
    let number = |text: String, x: f32, y: f32, anchor: Anchor| Text {
        pos: Point::new(x, y),
        anchor,
        line: TextLine::new(text, Font::Small),
        paint: Paint::Role(Role::Muted),
        backed: false,
        rotate: 0.0,
    };
    for (piece, label) in pieces.iter().zip(labels) {
        let band = top + piece.row as f32 * pitch;
        let x = PADDING + (piece.start % BITS_PER_ROW) as f32 * bit;
        let w = piece.bits() as f32 * bit;
        let id = if packet.fields[piece.field].start == piece.start {
            format!("field{}", piece.field)
        } else {
            format!("field{}.{}", piece.field, piece.row)
        };
        let mut node = Node::new(id, Shape::Rect, Rect::new(x, band + NUMBERS, w, row_height));
        node.label = label;
        scene.nodes.push(node);
        let baseline = band + small * 0.95;
        if piece.bits() == 1 {
            scene.texts.push(number(
                piece.start.to_string(),
                x + w / 2.0,
                baseline,
                Anchor::Middle,
            ));
        } else {
            scene.texts.push(number(
                piece.start.to_string(),
                x + 2.0,
                baseline,
                Anchor::Start,
            ));
            scene.texts.push(number(
                piece.end.to_string(),
                x + w - 2.0,
                baseline,
                Anchor::End,
            ));
        }
    }
    let title_width = scene
        .title
        .as_ref()
        .map_or(0.0, |t| line_width(measure, &t.line, FONT_SIZE));
    scene.width = width.max(title_width) + 2.0 * PADDING;
    scene.height = top + rows as f32 * pitch - ROW_GAP + PADDING;
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    fn scene(source: &str) -> Scene {
        layout(&super::super::parse(source).unwrap(), &Estimate)
    }

    #[test]
    fn every_label_and_bit_number_is_drawn() {
        let s = scene(
            "packet\ntitle TCP\n0-15: \"Source Port\"\n16-31: \"Destination Port\"\n32: \"URG\"\n33-63: \"Rest\"\n",
        );
        let texts = s.texts();
        for t in [
            "TCP",
            "Source Port",
            "Destination Port",
            "URG",
            "Rest",
            "0",
            "15",
            "31",
            "32",
            "63",
        ] {
            assert!(texts.iter().any(|x| x == t), "missing {t}: {texts:?}");
        }
    }

    #[test]
    fn a_field_crossing_a_row_is_split_and_nothing_overlaps() {
        let s = scene("packet\n0-23: \"A\"\n24-47: \"B\"\n48-95: \"C\"\n");
        // B splits across rows 0 and 1; C across 1 and 2.
        assert_eq!(s.nodes.len(), 5);
        for (i, a) in s.nodes.iter().enumerate() {
            assert!(s.bounds().encloses(&a.rect, 0.01));
            for b in &s.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{a:?} overlaps {b:?}");
            }
        }
        // Rows are full width.
        let row0: f32 = s.nodes[..2].iter().map(|n| n.rect.width).sum();
        let row1: f32 = s.nodes[2..4].iter().map(|n| n.rect.width).sum();
        assert!((row0 - row1).abs() < 0.01);
    }
}
