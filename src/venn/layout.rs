//! Venn layout: one, two or three circles in a fixed symmetric
//! arrangement, each region's labels stacked at its centre.
//!
//! Mermaid lays circles out with venn.js so areas follow the sizes; here a
//! size only scales its circle's radius a little, and every pair overlaps
//! (two sets with no `union` between them are drawn apart).

use diagram_scene::{Anchor, Font, Node, Paint, Point, Rect, Role, Scene, Shape, Text, TextLine};

use super::parse::Venn;
use crate::style::Rgb;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const RADIUS: f32 = 110.0;

/// A region's sets and, when fixed in advance, where its labels go.
type Anchored = (Vec<usize>, Option<Point>);

/// A region's labels: its sets, ascending, and the lines stacked in it.
struct Region {
    sets: Vec<usize>,
    lines: Vec<(TextLine, Paint)>,
}

pub fn layout(venn: &Venn, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let n = venn.sets.len();

    // Radii from sizes: area-ish, but kept within 70–100% of the largest.
    let largest = venn
        .sets
        .iter()
        .map(|s| s.size())
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);
    let radii: Vec<f32> = venn
        .sets
        .iter()
        .map(|s| RADIUS * (s.size() / largest).sqrt().clamp(0.7, 1.0))
        .collect();

    // Centres relative to an origin, and each region's anchor point.
    // An anchor of `None` is found once its labels are known.
    let (centers, anchors): (Vec<Point>, Vec<Anchored>) = match n {
        1 => (
            vec![Point::new(0.0, 0.0)],
            vec![(vec![0], Some(Point::new(0.0, 0.0)))],
        ),
        2 => {
            let apart =
                venn.union(&[0, 1]).is_none() && !venn.texts.iter().any(|t| t.sets.len() == 2);
            let (r0, r1) = (radii[0], radii[1]);
            let distance = if apart {
                r0 + r1 + 30.0
            } else {
                (r0 + r1) * 0.62
            };
            let c0 = Point::new(-distance / 2.0, 0.0);
            let c1 = Point::new(distance / 2.0, 0.0);
            let centers = vec![c0, c1];
            let anchors = if apart {
                vec![(vec![0], Some(c0)), (vec![1], Some(c1))]
            } else {
                vec![(vec![0], None), (vec![1], None), (vec![0, 1], None)]
            };
            (centers, anchors)
        }
        _ => {
            let r = radii.iter().copied().fold(0.0_f32, f32::max);
            let spread = r * 0.66;
            let cos30 = 3.0_f32.sqrt() / 2.0;
            let dirs = [
                Point::new(-cos30, -0.5),
                Point::new(cos30, -0.5),
                Point::new(0.0, 1.0),
            ];
            let centers: Vec<Point> = dirs
                .iter()
                .map(|d| Point::new(d.x * spread, d.y * spread))
                .collect();
            let anchors = [
                vec![0],
                vec![1],
                vec![2],
                vec![0, 1],
                vec![0, 2],
                vec![1, 2],
                vec![0, 1, 2],
            ]
            .into_iter()
            .map(|sets| (sets, None))
            .collect();
            (centers, anchors)
        }
    };

    // What each region says: a set's name, a union's label, its texts.
    let rgb = |c: Option<Rgb>| c.map(|(r, g, b)| Paint::Rgb(r, g, b));
    let mut regions: Vec<Region> = anchors
        .iter()
        .map(|(sets, _)| Region {
            sets: sets.clone(),
            lines: Vec::new(),
        })
        .collect();
    for region in &mut regions {
        if let [only] = region.sets[..] {
            let set = &venn.sets[only];
            region.lines.push((
                TextLine::new(set.name(), Font::Bold),
                rgb(set.colors.text).unwrap_or(Paint::Role(Role::Text)),
            ));
        } else if let Some(union) = venn.union(&region.sets) {
            // A union's fill can't be drawn as a filled lens; it tints the
            // label instead.
            let paint = rgb(union.colors.text)
                .or(rgb(union.colors.fill))
                .unwrap_or(Paint::Role(Role::Text));
            if let Some(label) = &union.label {
                region
                    .lines
                    .push((TextLine::new(label.clone(), Font::Bold), paint));
            }
        }
        for text in venn.texts.iter().filter(|t| t.sets == region.sets) {
            region.lines.push((
                TextLine::new(text.name(), Font::Body),
                rgb(text.colors.text).unwrap_or(Paint::Role(Role::Muted)),
            ));
        }
    }

    // Placed text, relative to the origin, to measure the extent.
    let line_height = measure.line_height(FONT_SIZE);
    let ascent = measure.ascent(FONT_SIZE);
    let mut texts = Vec::new();
    let mut extent = Rect::new(0.0, 0.0, 0.0, 0.0);
    for (i, c) in centers.iter().enumerate() {
        let rect = Rect::centered(*c, radii[i] * 2.0, radii[i] * 2.0);
        extent = if i == 0 { rect } else { extent.union(&rect) };
    }
    for (region, (_, at)) in regions.iter().zip(&anchors) {
        let block_w = region
            .lines
            .iter()
            .map(|(l, _)| diagram_scene::line_width(measure, l, FONT_SIZE))
            .fold(0.0_f32, f32::max);
        let total = region.lines.len() as f32 * line_height;
        let at = at.unwrap_or_else(|| pole(&centers, &radii, &region.sets, (block_w, total)));
        let mut top = at.y - total / 2.0;
        for (line, paint) in &region.lines {
            let width = diagram_scene::line_width(measure, line, FONT_SIZE);
            extent = extent.union(&Rect::new(at.x - width / 2.0, top, width, line_height));
            texts.push(Text {
                pos: Point::new(at.x, top + ascent),
                anchor: Anchor::Middle,
                line: line.clone(),
                paint: *paint,
                backed: false,
            });
            top += line_height;
        }
    }

    // Shift everything so the extent sits inside the padding, under a title.
    let title_size = Font::Title.size(FONT_SIZE);
    let title_height = if venn.title.is_some() {
        title_size + PADDING
    } else {
        0.0
    };
    let title_width = venn
        .title
        .as_ref()
        .map(|t| measure.width(t, title_size, true))
        .unwrap_or(0.0);
    let width = extent.width.max(title_width) + PADDING * 2.0;
    let dx = (width - extent.width) / 2.0 - extent.x;
    let dy = PADDING + title_height - extent.y;
    let shift = |p: Point| Point::new(p.x + dx, p.y + dy);

    for (i, set) in venn.sets.iter().enumerate() {
        let mut node = Node::new(
            format!("set:{}", set.id),
            Shape::Circle,
            Rect::centered(shift(centers[i]), radii[i] * 2.0, radii[i] * 2.0),
        );
        let color = rgb(set.colors.stroke)
            .or(rgb(set.colors.fill))
            .unwrap_or(Paint::Role(Role::Series(i as u8)));
        node.style.fill = Paint::None;
        node.style.stroke = color;
        node.style.emphasis = true;
        scene.nodes.push(node);
    }
    for mut text in texts {
        text.pos = shift(text.pos);
        scene.texts.push(text);
    }
    if let Some(title) = &venn.title {
        scene.title = Some(Text {
            pos: Point::new(width / 2.0, PADDING + title_size),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
        });
    }
    scene.width = width;
    scene.height = extent.height + PADDING * 2.0 + title_height;
    scene
}

/// The point of the region inside exactly `sets` farthest from every
/// circle's edge: where its labels have the most room. A point whose whole
/// `block` (width, height) of labels fits in the region wins over one where
/// only its centre does.
fn pole(centers: &[Point], radii: &[f32], sets: &[usize], block: (f32, f32)) -> Point {
    let (hw, hh) = (block.0 / 2.0, block.1 / 2.0);
    let member = |p: Point| {
        (0..centers.len()).all(|i| (p.distance(centers[i]) < radii[i]) == sets.contains(&i))
    };
    let fits = |p: Point| {
        [
            (-hw, -hh),
            (hw, -hh),
            (-hw, hh),
            (hw, hh),
            (0.0, -hh),
            (0.0, hh),
        ]
        .iter()
        .all(|&(dx, dy)| member(p.offset(dx, dy)))
    };
    let (mut lo, mut hi) = (
        Point::new(f32::MAX, f32::MAX),
        Point::new(f32::MIN, f32::MIN),
    );
    for &i in sets {
        lo = Point::new(
            lo.x.min(centers[i].x - radii[i]),
            lo.y.min(centers[i].y - radii[i]),
        );
        hi = Point::new(
            hi.x.max(centers[i].x + radii[i]),
            hi.y.max(centers[i].y + radii[i]),
        );
    }
    let step = 3.0;
    // (fits the whole block, room at the centre, point)
    let mut best = (false, f32::MIN, centers[sets[0]]);
    let mut y = lo.y;
    while y <= hi.y {
        let mut x = lo.x;
        while x <= hi.x {
            let p = Point::new(x, y);
            if member(p) {
                let room = (0..centers.len())
                    .map(|i| (p.distance(centers[i]) - radii[i]).abs())
                    .fold(f32::MAX, f32::min);
                let whole = fits(p);
                if (whole, room) > (best.0, best.1) {
                    best = (whole, room, p);
                }
            }
            x += step;
        }
        y += step;
    }
    best.2
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;
    use diagram_scene::Estimate;

    fn check(source: &str) -> Scene {
        let venn = parse(source).unwrap();
        let scene = layout(&venn, &Estimate);
        let texts = scene.texts();
        for set in &venn.sets {
            assert!(texts.iter().any(|t| t == set.name()), "{}", set.name());
        }
        for text in &venn.texts {
            assert!(texts.iter().any(|t| t == text.name()), "{}", text.name());
        }
        for union in &venn.unions {
            if let Some(label) = &union.label {
                assert!(texts.contains(label), "{label}");
            }
        }
        assert_eq!(scene.nodes.len(), venn.sets.len());
        for node in &scene.nodes {
            assert!(scene.bounds().encloses(&node.rect, 0.01), "{node:?}");
        }
        for text in &scene.texts {
            assert!(scene.bounds().contains(text.pos), "{text:?}");
        }
        scene
    }

    #[test]
    fn three_sets_keep_every_label_inside_the_bounds() {
        check(
            "venn-beta\n title T\n set A\n  text a1\n set B:5\n set C\n union A,B[AB]\n \
             union B,C[BC]\n union A,C[AC]\n union A,B,C[\"All\"]\n  text core\n",
        );
    }

    #[test]
    fn two_sets_without_a_union_are_drawn_apart() {
        let scene = check("venn-beta\n set A\n set B\n");
        assert!(!scene.nodes[0].rect.intersects(&scene.nodes[1].rect));
        let scene = check("venn-beta\n set A\n set B\n union A,B\n");
        assert!(scene.nodes[0].rect.intersects(&scene.nodes[1].rect));
    }

    #[test]
    fn one_set_draws() {
        check("venn-beta\n set Only[\"Just one\"]\n  text item\n");
    }
}
