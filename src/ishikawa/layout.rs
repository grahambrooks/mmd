//! A fishbone: the effect in a box at the right, a spine running into it,
//! and one slanted bone per category, alternately above and below. Each
//! cause is text on a short horizontal line off its bone; a cause's own
//! causes hang off a rail dropped from the start of its line.

use diagram_scene::{
    Anchor, Edge, Font, Marker, Node, Paint, Pen, Role, Scene, Seg, Shape, Text, TextLine, Weight,
    geom::Point, geom::Rect, line_width, shape::line_height, shape::natural_size,
};

use super::parse::{Cause, Ishikawa};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

/// Vertical pitch of cause rows.
const ROW: f32 = 24.0;
/// How far a bone leans left per unit of height.
const LEAN: f32 = 0.3;
/// Between a line's end and its text.
const TEXT_PAD: f32 = 6.0;
/// Where a sub-cause rail leaves its parent's line, from the line's start.
const RAIL: f32 = 10.0;
/// Between neighbouring bone pairs, and before the head.
const GAP: f32 = 28.0;
const LABEL_WIDTH: f32 = 160.0;

/// One cause, as a row on its bone.
struct Row {
    text: String,
    width: f32,
    depth: usize,
    parent: Option<usize>,
}

/// A bone laid out with its spine attachment at x = 0, y = 0.
#[derive(Default)]
struct Bone {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    texts: Vec<Text>,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

pub fn layout(diagram: &Ishikawa, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;

    // The head, its left edge at x = 0 on the spine.
    let mut head = Node::new("effect", Shape::Rounded, Rect::default());
    head.label = wrapped(measure, &diagram.effect, Font::Bold);
    head.style.emphasis = true;
    let (w, h) = natural_size(&head, FONT_SIZE, measure);
    head.rect = Rect::new(0.0, -h / 2.0, w, h);

    let bones: Vec<Bone> = diagram
        .causes
        .iter()
        .enumerate()
        .map(|(i, c)| bone(c, i, i % 2 == 0, measure))
        .collect();
    let mut cursor = -GAP;
    let mut placed: Vec<Bone> = Vec::new();
    for pair in bones.chunks(2).map(|c| c.iter().collect::<Vec<_>>()) {
        let right = pair.iter().map(|b| b.right).fold(0.0, f32::max);
        let left = pair.iter().map(|b| b.left).fold(0.0, f32::min);
        let x = cursor - right;
        for b in pair {
            placed.push(shifted(b, x, 0.0));
        }
        cursor = x + left - GAP;
    }
    let spine_start = cursor.min(-2.0 * GAP);
    let mut spine = Edge::new(vec![
        Seg::Move(Point::new(spine_start, 0.0)),
        Seg::Line(Point::new(0.0, 0.0)),
    ]);
    spine.pen.weight = Weight::Thick;

    let mut all = Bone {
        left: spine_start,
        right: head.rect.right(),
        top: head.rect.y,
        bottom: head.rect.bottom(),
        ..Bone::default()
    };
    all.edges.push(spine);
    all.nodes.push(head);
    for b in placed {
        all.left = all.left.min(b.left);
        all.right = all.right.max(b.right);
        all.top = all.top.min(b.top);
        all.bottom = all.bottom.max(b.bottom);
        all.nodes.extend(b.nodes);
        all.edges.extend(b.edges);
        all.texts.extend(b.texts);
    }
    let all = shifted(&all, PADDING - all.left, PADDING - all.top);
    scene.width = all.right - all.left + 2.0 * PADDING;
    scene.height = all.bottom - all.top + 2.0 * PADDING;
    scene.nodes = all.nodes;
    scene.edges = all.edges;
    scene.texts = all.texts;
    scene
}

fn wrapped(measure: &dyn Measure, text: &str, font: Font) -> Vec<TextLine> {
    crate::text::wrap(
        measure,
        text,
        font.size(FONT_SIZE),
        font.bold(),
        LABEL_WIDTH,
    )
    .into_iter()
    .map(|l| TextLine::new(l, font))
    .collect()
}

/// Category `index`'s bone, above the spine when `upper`.
fn bone(category: &Cause, index: usize, upper: bool, measure: &dyn Measure) -> Bone {
    let mut rows = Vec::new();
    flatten(&category.causes, 1, None, measure, &mut rows);
    let n = rows.len() as f32;
    // Rows top to bottom; each line's y.
    let ys: Vec<f32> = (0..rows.len())
        .map(|i| {
            if upper {
                -(n - i as f32) * ROW
            } else {
                (i as f32 + 1.0) * ROW + 4.0
            }
        })
        .collect();
    let lean = |y: f32| -y.abs() * LEAN;

    let mut out = Bone::default();
    let mut label = Node::new(format!("category{index}"), Shape::Rect, Rect::default());
    label.label = wrapped(measure, &category.text, Font::Bold);
    let (w, h) = natural_size(&label, FONT_SIZE, measure);
    let cat_y = if upper {
        ys.first().copied().unwrap_or(0.0) - ROW * 1.1 - h / 2.0
    } else {
        ys.last().copied().unwrap_or(0.0) + ROW * 0.5 + h / 2.0
    };
    let cat_x = lean(cat_y);
    label.rect = Rect::centered(Point::new(cat_x, cat_y), w, h);
    label.style.stroke = Paint::Role(Role::Series(index as u8));
    out.left = label.rect.x;
    out.right = label.rect.right().max(0.0);
    out.top = label.rect.y.min(0.0);
    out.bottom = label.rect.bottom().max(0.0);
    out.edges.push(plain(
        vec![Point::new(0.0, 0.0), Point::new(cat_x, cat_y)],
        Weight::Normal,
    ));
    out.nodes.push(label);

    // Each row's line runs from `start` to `end`.
    let mut starts = vec![0.0_f32; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        let y = ys[i];
        let end = match row.parent {
            None => lean(y),
            Some(p) => starts[p] + RAIL,
        };
        let start = end - row.width - 2.0 * TEXT_PAD;
        starts[i] = start;
        out.edges.push(plain(
            vec![Point::new(start, y), Point::new(end, y)],
            Weight::Thin,
        ));
        if let Some(p) = row.parent {
            out.edges.push(plain(
                vec![Point::new(end, ys[p]), Point::new(end, y)],
                Weight::Thin,
            ));
        }
        let font = if row.depth == 1 {
            Font::Body
        } else {
            Font::Small
        };
        out.texts.push(Text {
            pos: Point::new(end - TEXT_PAD, y - 5.0),
            anchor: Anchor::End,
            line: TextLine::new(row.text.clone(), font),
            paint: Paint::Role(Role::Text),
            backed: false,
        });
        out.left = out.left.min(start);
        out.top = out.top.min(y - 5.0 - line_height(FONT_SIZE));
        out.bottom = out.bottom.max(y + 2.0);
    }
    out
}

fn flatten(
    causes: &[Cause],
    depth: usize,
    parent: Option<usize>,
    measure: &dyn Measure,
    out: &mut Vec<Row>,
) {
    for cause in causes {
        let font = if depth == 1 { Font::Body } else { Font::Small };
        let index = out.len();
        out.push(Row {
            text: cause.text.clone(),
            width: line_width(measure, &TextLine::new(&cause.text, font), FONT_SIZE),
            depth,
            parent,
        });
        flatten(&cause.causes, depth + 1, Some(index), measure, out);
    }
}

fn plain(points: Vec<Point>, weight: Weight) -> Edge {
    let mut segs = vec![Seg::Move(points[0])];
    segs.extend(points[1..].iter().map(|&p| Seg::Line(p)));
    let mut edge = Edge::new(segs);
    edge.end = Marker::None;
    edge.pen = Pen {
        weight,
        ..Pen::default()
    };
    edge
}

/// `bone` moved by (dx, dy).
fn shifted(bone: &Bone, dx: f32, dy: f32) -> Bone {
    let p = |p: Point| p.offset(dx, dy);
    Bone {
        nodes: bone
            .nodes
            .iter()
            .map(|n| {
                let mut n = n.clone();
                n.rect.x += dx;
                n.rect.y += dy;
                n
            })
            .collect(),
        edges: bone
            .edges
            .iter()
            .map(|e| {
                let mut e = e.clone();
                for seg in &mut e.path {
                    *seg = match *seg {
                        Seg::Move(a) => Seg::Move(p(a)),
                        Seg::Line(a) => Seg::Line(p(a)),
                        Seg::Cubic(a, b, c) => Seg::Cubic(p(a), p(b), p(c)),
                        Seg::Close => Seg::Close,
                    };
                }
                e
            })
            .collect(),
        texts: bone
            .texts
            .iter()
            .map(|t| Text {
                pos: p(t.pos),
                ..t.clone()
            })
            .collect(),
        left: bone.left + dx,
        right: bone.right + dx,
        top: bone.top + dy,
        bottom: bone.bottom + dy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    const SOURCE: &str = "ishikawa-beta\n    Blurry Photo\n    Process\n        Out of focus\n        Shutter speed too slow\n    User\n        Shaky hands\n    Equipment\n        LENS\n            Inappropriate lens\n            Dirty lens\n        SENSOR\n            Damaged sensor\n    Environment\n        Too dark\n    Software\n";

    fn scene() -> Scene {
        layout(&super::super::parse(SOURCE).unwrap(), &Estimate)
    }

    #[test]
    fn every_cause_is_drawn() {
        let texts = scene().texts();
        for t in [
            "Blurry Photo",
            "Process",
            "Out of focus",
            "Shutter speed too slow",
            "User",
            "Shaky hands",
            "Equipment",
            "LENS",
            "Inappropriate lens",
            "Dirty lens",
            "SENSOR",
            "Damaged sensor",
            "Environment",
            "Too dark",
            "Software",
        ] {
            assert!(texts.iter().any(|x| x == t), "missing {t}");
        }
    }

    #[test]
    fn boxes_do_not_overlap_and_everything_is_in_bounds() {
        let s = scene();
        let bounds = s.bounds();
        for (i, a) in s.nodes.iter().enumerate() {
            assert!(bounds.encloses(&a.rect, 0.01), "{a:?}");
            for b in &s.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
        for t in &s.texts {
            let w = line_width(&Estimate, &t.line, FONT_SIZE);
            assert!(t.pos.x - w >= 0.0 && t.pos.x <= s.width, "{t:?}");
            assert!(t.pos.y - FONT_SIZE >= 0.0 && t.pos.y <= s.height, "{t:?}");
        }
    }

    #[test]
    fn categories_alternate_above_and_below_the_spine() {
        let s = scene();
        let spine = s.node("effect").unwrap().rect.center().y;
        let above = |id: &str| s.node(id).unwrap().rect.center().y < spine;
        assert!(above("category0") && !above("category1") && above("category2"));
    }
}
