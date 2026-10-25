//! Venn layout.
//!
//! - One to three sets: circles in a fixed symmetric arrangement (two sets
//!   with no `union` or shared `text` between them are drawn apart).
//! - Four sets: the classic four-ellipse Venn diagram, in which all fifteen
//!   regions exist.
//! - Five or more: an Euler diagram of circles, relaxed so that sets sharing a
//!   `union` (or a `text`) overlap and the rest keep apart. When a region the
//!   source names does not come out, that is an error naming it.
//!
//! Every region's labels are stacked at its roomiest point. Mermaid lays
//! circles out with venn.js so areas follow the sizes; here a size only
//! scales its circle's radius a little.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use diagram_scene::{
    Anchor, Edge, Font, Marker, Node, Paint, Point, Rect, Role, Scene, Seg, Shape, Text, TextLine,
    Weight,
};

use super::parse::Venn;
use crate::Error;
use crate::style::Rgb;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const RADIUS: f32 = 110.0;
/// The four-ellipse arrangement's scale: an ellipse's semi-axes are
/// 0.72 and 0.42 of it.
const ELLIPSE_SCALE: f32 = 300.0;
/// The clear gap between two circles that share no region.
const GAP: f32 = 24.0;
/// The label-placement grid's spacing.
const STEP: f32 = 3.0;

/// One set's outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Outline {
    Circle {
        c: Point,
        r: f32,
    },
    /// Semi-axes `a` along `theta` (radians, clockwise on screen) and `b`
    /// across it.
    Ellipse {
        c: Point,
        a: f32,
        b: f32,
        theta: f32,
    },
}

impl Outline {
    pub(crate) fn contains(&self, p: Point) -> bool {
        match *self {
            Outline::Circle { c, r } => p.distance(c) < r,
            Outline::Ellipse { c, a, b, theta } => {
                let (u, v) = local(p, c, theta);
                (u / a).powi(2) + (v / b).powi(2) < 1.0
            }
        }
    }

    fn bbox(&self) -> Rect {
        match *self {
            Outline::Circle { c, r } => Rect::centered(c, r * 2.0, r * 2.0),
            Outline::Ellipse { c, a, b, theta } => {
                let (s, k) = theta.sin_cos();
                let hw = ((a * k).powi(2) + (b * s).powi(2)).sqrt();
                let hh = ((a * s).powi(2) + (b * k).powi(2)).sqrt();
                Rect::centered(c, hw * 2.0, hh * 2.0)
            }
        }
    }

    fn shifted(&self, dx: f32, dy: f32) -> Outline {
        match *self {
            Outline::Circle { c, r } => Outline::Circle {
                c: c.offset(dx, dy),
                r,
            },
            Outline::Ellipse { c, a, b, theta } => Outline::Ellipse {
                c: c.offset(dx, dy),
                a,
                b,
                theta,
            },
        }
    }

    /// The point at parameter `t` and the derivative there.
    fn ellipse_at(c: Point, a: f32, b: f32, theta: f32, t: f32) -> (Point, Point) {
        let (s, k) = theta.sin_cos();
        let (st, ct) = t.sin_cos();
        let p = Point::new(c.x + a * ct * k - b * st * s, c.y + a * ct * s + b * st * k);
        let d = Point::new(-a * st * k - b * ct * s, -a * st * s + b * ct * k);
        (p, d)
    }

    /// The edge as a closed path of four cubic arcs (ellipses only).
    fn path(&self) -> Vec<Seg> {
        let Outline::Ellipse { c, a, b, theta } = *self else {
            return Vec::new();
        };
        // The control-point distance that makes a cubic follow a quarter
        // circle; an ellipse is an affine image of one.
        const K: f32 = 0.552_284_8;
        let at = |t: f32| Outline::ellipse_at(c, a, b, theta, t);
        let mut segs = vec![Seg::Move(at(0.0).0)];
        for q in 0..4 {
            let (p0, d0) = at(q as f32 * FRAC_PI_2);
            let (p1, d1) = at((q + 1) as f32 * FRAC_PI_2);
            segs.push(Seg::Cubic(
                p0.offset(d0.x * K, d0.y * K),
                p1.offset(-d1.x * K, -d1.y * K),
                p1,
            ));
        }
        segs.push(Seg::Close);
        segs
    }

    /// The boundary as a closed polygon, for distances (ellipses only).
    fn polygon(&self) -> Vec<Point> {
        let Outline::Ellipse { c, a, b, theta } = *self else {
            return Vec::new();
        };
        (0..120)
            .map(|i| Outline::ellipse_at(c, a, b, theta, i as f32 * TAU / 120.0).0)
            .collect()
    }
}

/// `p` in the frame of a centre `c` turned by `theta`.
fn local(p: Point, c: Point, theta: f32) -> (f32, f32) {
    let (s, k) = theta.sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    (dx * k + dy * s, -dx * s + dy * k)
}

fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(Point::new(a.x + dx * t, a.y + dy * t))
}

/// The sets a point lies in, as a bit mask.
fn mask_at(outlines: &[Outline], p: Point) -> u64 {
    outlines
        .iter()
        .enumerate()
        .filter(|(_, o)| o.contains(p))
        .fold(0, |m, (i, _)| m | 1 << i)
}

fn mask_of(sets: &[usize]) -> u64 {
    sets.iter().fold(0, |m, &i| m | 1 << i)
}

/// A region the source names, and what it says.
struct Region {
    sets: Vec<usize>,
    lines: Vec<(TextLine, Paint)>,
    /// The first source line that names it, and how, for an error.
    line: usize,
    what: String,
}

/// A laid-out diagram plus what the tests check it against.
pub(crate) struct Arranged {
    pub scene: Scene,
    /// Each set's outline, in scene coordinates.
    #[cfg_attr(not(test), allow(dead_code))]
    pub outlines: Vec<Outline>,
    /// For each of `scene.texts`, the mask of the region it labels.
    #[cfg_attr(not(test), allow(dead_code))]
    pub regions: Vec<u64>,
}

pub fn layout(venn: &Venn, measure: &dyn Measure) -> Result<Scene, Error> {
    arrange(venn, measure).map(|a| a.scene)
}

pub(crate) fn arrange(venn: &Venn, measure: &dyn Measure) -> Result<Arranged, Error> {
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

    let outlines: Vec<Outline> = match n {
        1 => vec![Outline::Circle {
            c: Point::new(0.0, 0.0),
            r: radii[0],
        }],
        2 => {
            let apart =
                venn.union(&[0, 1]).is_none() && !venn.texts.iter().any(|t| t.sets.len() == 2);
            let (r0, r1) = (radii[0], radii[1]);
            let distance = if apart {
                r0 + r1 + 30.0
            } else {
                (r0 + r1) * 0.62
            };
            vec![
                Outline::Circle {
                    c: Point::new(-distance / 2.0, 0.0),
                    r: r0,
                },
                Outline::Circle {
                    c: Point::new(distance / 2.0, 0.0),
                    r: r1,
                },
            ]
        }
        3 => {
            let r = radii.iter().copied().fold(0.0_f32, f32::max);
            let spread = r * 0.66;
            let cos30 = 3.0_f32.sqrt() / 2.0;
            [
                Point::new(-cos30, -0.5),
                Point::new(cos30, -0.5),
                Point::new(0.0, 1.0),
            ]
            .iter()
            .zip(&radii)
            .map(|(d, &r)| Outline::Circle {
                c: Point::new(d.x * spread, d.y * spread),
                r,
            })
            .collect()
        }
        4 => four_ellipses(),
        _ => euler(venn, &radii)
            .into_iter()
            .zip(&radii)
            .map(|(c, &r)| Outline::Circle { c, r })
            .collect(),
    };

    // What each region says: a set's name, a union's label, its texts.
    let rgb = |c: Option<Rgb>| c.map(|(r, g, b)| Paint::Rgb(r, g, b));
    let ids = |sets: &[usize]| {
        sets.iter()
            .map(|&i| venn.sets[i].id.as_str())
            .collect::<Vec<_>>()
            .join(",")
    };
    let mut regions: Vec<Region> = venn
        .sets
        .iter()
        .enumerate()
        .map(|(i, set)| Region {
            sets: vec![i],
            lines: vec![(
                TextLine::new(set.name(), Font::Bold),
                rgb(set.colors.text).unwrap_or(Paint::Role(Role::Text)),
            )],
            line: set.line,
            what: format!("set `{}`", set.id),
        })
        .collect();
    for union in &venn.unions {
        // A union's fill can't be drawn as a filled lens; it tints the label
        // instead.
        let paint = rgb(union.colors.text)
            .or(rgb(union.colors.fill))
            .unwrap_or(Paint::Role(Role::Text));
        let lines = union
            .label
            .iter()
            .map(|label| (TextLine::new(label.clone(), Font::Bold), paint))
            .collect();
        regions.push(Region {
            sets: union.sets.clone(),
            lines,
            line: union.line,
            what: format!("`union {}`", ids(&union.sets)),
        });
    }
    for text in &venn.texts {
        let line = (
            TextLine::new(text.name(), Font::Body),
            rgb(text.colors.text).unwrap_or(Paint::Role(Role::Muted)),
        );
        match regions.iter_mut().find(|r| r.sets == text.sets) {
            Some(region) => region.lines.push(line),
            None => regions.push(Region {
                sets: text.sets.clone(),
                lines: vec![line],
                line: text.line,
                what: format!("text `{}` in {}", text.id, ids(&text.sets)),
            }),
        }
    }

    // Sample the picture once: each point's sets and its room to the
    // nearest edge.
    let mut extent = outlines[0].bbox();
    for o in &outlines[1..] {
        extent = extent.union(&o.bbox());
    }
    let polygons: Vec<Vec<Point>> = outlines.iter().map(Outline::polygon).collect();
    let room = |p: Point| {
        outlines
            .iter()
            .zip(&polygons)
            .map(|(o, poly)| match *o {
                Outline::Circle { c, r } => (p.distance(c) - r).abs(),
                Outline::Ellipse { .. } => (0..poly.len())
                    .map(|i| segment_distance(p, poly[i], poly[(i + 1) % poly.len()]))
                    .fold(f32::MAX, f32::min),
            })
            .fold(f32::MAX, f32::min)
    };
    let mut samples: Vec<(Point, u64)> = Vec::new();
    let mut y = extent.y;
    while y <= extent.y + extent.height {
        let mut x = extent.x;
        while x <= extent.x + extent.width {
            let p = Point::new(x, y);
            let m = mask_at(&outlines, p);
            if m != 0 {
                samples.push((p, m));
            }
            x += STEP;
        }
        y += STEP;
    }

    // A region the source names must come out in the drawing; one that
    // doesn't is an error, not a dropped label.
    let mut missing: Vec<&Region> = regions
        .iter()
        .filter(|r| {
            let m = mask_of(&r.sets);
            !samples.iter().any(|&(_, s)| s == m)
        })
        .collect();
    missing.sort_by_key(|r| r.line);
    if let Some(region) = missing.first() {
        return Err(Error::new(
            region.line,
            format!(
                "{} cannot be drawn: in mmd's approximate layout of {n} sets those sets \
                 share no region of their own (see DESIGN.md)",
                region.what
            ),
        ));
    }

    // Placed text, relative to the origin, to measure the extent.
    let line_height = measure.line_height(FONT_SIZE);
    let ascent = measure.ascent(FONT_SIZE);
    let mut texts = Vec::new();
    let mut text_regions = Vec::new();
    for region in regions.iter().filter(|r| !r.lines.is_empty()) {
        let block_w = region
            .lines
            .iter()
            .map(|(l, _)| diagram_scene::line_width(measure, l, FONT_SIZE))
            .fold(0.0_f32, f32::max);
        let total = region.lines.len() as f32 * line_height;
        let at = pole(&outlines, &samples, &room, &region.sets, (block_w, total));
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
            text_regions.push(mask_of(&region.sets));
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
    let outlines: Vec<Outline> = outlines.iter().map(|o| o.shifted(dx, dy)).collect();

    for (i, (set, outline)) in venn.sets.iter().zip(&outlines).enumerate() {
        let color = rgb(set.colors.stroke)
            .or(rgb(set.colors.fill))
            .unwrap_or(Paint::Role(Role::Series(i as u8)));
        match outline {
            Outline::Circle { .. } => {
                let mut node = Node::new(format!("set:{}", set.id), Shape::Circle, outline.bbox());
                node.style.fill = Paint::None;
                node.style.stroke = color;
                node.style.emphasis = true;
                scene.nodes.push(node);
            }
            Outline::Ellipse { .. } => {
                let mut edge = Edge::new(outline.path());
                edge.start = Marker::None;
                edge.end = Marker::None;
                edge.pen.paint = color;
                edge.pen.weight = Weight::Thick;
                scene.edges.push(edge);
            }
        }
    }
    for mut text in texts {
        text.pos = text.pos.offset(dx, dy);
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
    Ok(Arranged {
        scene,
        outlines,
        regions: text_regions,
    })
}

/// The classic symmetric Venn diagram of four ellipses: two outer ones low
/// and apart, two inner ones sharing a centre, mirrored in pairs.
fn four_ellipses() -> Vec<Outline> {
    let s = ELLIPSE_SCALE;
    let (a, b) = (0.72 * s, 0.42 * s);
    let tilt = 40.0_f32.to_radians();
    [
        (-0.25, 0.18, tilt),
        (0.0, 0.0, tilt),
        (0.0, 0.0, -tilt),
        (0.25, 0.18, -tilt),
    ]
    .iter()
    .map(|&(x, y, theta)| Outline::Ellipse {
        c: Point::new(x * s, y * s),
        a,
        b,
        theta,
    })
    .collect()
}

/// Circle centres for five or more sets: start on a ring, then relax for a
/// fixed number of rounds so that sets which share a named region overlap
/// (and those sharing a region of three or more crowd one point) while the
/// rest keep at least [`GAP`] apart. Deterministic.
fn euler(venn: &Venn, radii: &[f32]) -> Vec<Point> {
    let n = radii.len();
    let named: Vec<&[usize]> = venn
        .unions
        .iter()
        .map(|u| &u.sets[..])
        .chain(venn.texts.iter().map(|t| &t.sets[..]))
        .filter(|sets| sets.len() >= 2)
        .collect();
    let mut overlap = vec![vec![false; n]; n];
    for sets in &named {
        for &i in *sets {
            for &j in *sets {
                overlap[i][j] = true;
            }
        }
    }
    let crowds: Vec<&[usize]> = named.iter().copied().filter(|s| s.len() >= 3).collect();

    let rmax = radii.iter().copied().fold(0.0_f32, f32::max);
    let ring = rmax * n as f32 / PI * 1.2;
    let direction = |i: usize| {
        let t = TAU * i as f32 / n as f32 - FRAC_PI_2;
        Point::new(t.cos(), t.sin())
    };
    let mut centers: Vec<Point> = (0..n)
        .map(|i| {
            let d = direction(i);
            Point::new(d.x * ring, d.y * ring)
        })
        .collect();

    for _ in 0..800 {
        let mut moves = vec![Point::new(0.0, 0.0); n];
        for i in 0..n {
            for j in i + 1..n {
                let (ci, cj) = (centers[i], centers[j]);
                let d = ci.distance(cj);
                let u = if d > 1e-3 {
                    Point::new((cj.x - ci.x) / d, (cj.y - ci.y) / d)
                } else {
                    direction(j)
                };
                let (ri, rj) = (radii[i], radii[j]);
                let pull = if overlap[i][j] {
                    // Overlap by about a third, never swallowing either.
                    let target = ((ri + rj) * 0.62).max((ri - rj).abs() + 0.5 * ri.min(rj));
                    (d - target) * 0.1
                } else if d < ri + rj + GAP {
                    (d - ri - rj - GAP) * 0.25
                } else {
                    0.0
                };
                moves[i] = moves[i].offset(u.x * pull, u.y * pull);
                moves[j] = moves[j].offset(-u.x * pull, -u.y * pull);
            }
        }
        for crowd in &crowds {
            let k = crowd.len() as f32;
            let mid = crowd.iter().fold(Point::new(0.0, 0.0), |m, &i| {
                m.offset(centers[i].x / k, centers[i].y / k)
            });
            let reach = crowd.iter().map(|&i| radii[i]).fold(f32::MAX, f32::min) * 0.45;
            for &i in *crowd {
                let d = centers[i].distance(mid);
                if d > reach {
                    let f = (d - reach) / d * 0.1;
                    moves[i] =
                        moves[i].offset((mid.x - centers[i].x) * f, (mid.y - centers[i].y) * f);
                }
            }
        }
        for (c, m) in centers.iter_mut().zip(&moves) {
            *c = c.offset(m.x, m.y);
        }
    }
    centers
}

/// The point of the region inside exactly `sets` farthest from every edge:
/// where its labels have the most room. A point whose whole `block` (width,
/// height) of labels fits in the region wins over one where only its centre
/// does. The region is known to have samples.
fn pole(
    outlines: &[Outline],
    samples: &[(Point, u64)],
    room: &dyn Fn(Point) -> f32,
    sets: &[usize],
    block: (f32, f32),
) -> Point {
    let (hw, hh) = (block.0 / 2.0, block.1 / 2.0);
    let want = mask_of(sets);
    // The block's outline, a little clear of the lines: corners, and points
    // along each edge so a slanted line can't slip between probes.
    let (hw, hh) = (hw + 3.0, hh + 1.0);
    let probes: Vec<(f32, f32)> = (0..=6)
        .flat_map(|i| {
            let x = -hw + hw * i as f32 / 3.0;
            [(x, -hh), (x, hh)]
        })
        .chain([(-hw, 0.0), (hw, 0.0)])
        .collect();
    let fits = |p: Point| {
        probes
            .iter()
            .all(|&(dx, dy)| mask_at(outlines, p.offset(dx, dy)) == want)
    };
    // (fits the whole block, room at the centre, point)
    let mut best: Option<(bool, f32, Point)> = None;
    for &(p, m) in samples {
        if m != want {
            continue;
        }
        let candidate = (fits(p), room(p), p);
        if best.is_none_or(|b| (candidate.0, candidate.1) > (b.0, b.1)) {
            best = Some(candidate);
        }
    }
    best.map(|b| b.2).unwrap_or(Point::new(0.0, 0.0))
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;
    use diagram_scene::Estimate;

    /// Lays `source` out and checks every set, union and text label is
    /// drawn, inside exactly the sets it names, and inside the bounds.
    fn check(source: &str) -> Arranged {
        let venn = parse(source).unwrap();
        let arranged = arrange(&venn, &Estimate).unwrap();
        let scene = &arranged.scene;
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
        assert_eq!(scene.nodes.len() + scene.edges.len(), venn.sets.len());
        let bounds = scene.bounds();
        for o in &arranged.outlines {
            assert!(bounds.encloses(&o.bbox(), 0.01), "{o:?}");
        }
        // Each line's middle sits in exactly its region.
        let middle = Estimate.line_height(FONT_SIZE) / 2.0 - Estimate.ascent(FONT_SIZE);
        for (text, &want) in scene.texts.iter().zip(&arranged.regions) {
            assert!(bounds.contains(text.pos), "{text:?}");
            let p = text.pos.offset(0.0, middle);
            assert_eq!(
                mask_at(&arranged.outlines, p),
                want,
                "{} at {p:?}",
                text.line.plain()
            );
        }
        arranged
    }

    #[test]
    fn three_sets_keep_every_label_inside_the_bounds() {
        check(
            "venn-beta\n title T\n set A\n  text a1\n set B:5\n set C\n union A,B[AB]\n\
             union B,C[BC]\n union A,C[AC]\n union A,B,C[\"All\"]\n  text core\n",
        );
    }

    #[test]
    fn two_sets_without_a_union_are_drawn_apart() {
        let scene = check("venn-beta\n set A\n set B\n").scene;
        assert!(!scene.nodes[0].rect.intersects(&scene.nodes[1].rect));
        let scene = check("venn-beta\n set A\n set B\n union A,B\n").scene;
        assert!(scene.nodes[0].rect.intersects(&scene.nodes[1].rect));
    }

    #[test]
    fn one_set_draws() {
        check("venn-beta\n set Only[\"Just one\"]\n  text item\n");
    }

    /// Every non-empty subset of `0..n`, as set ids `S0`, `S1`….
    fn every_union(n: usize) -> String {
        let mut out = String::from("venn-beta\n");
        for i in 0..n {
            out.push_str(&format!(" set S{i}\n"));
        }
        for m in 1u32..1 << n {
            if m.count_ones() >= 2 {
                let ids: Vec<String> = (0..n)
                    .filter(|i| m & 1 << i != 0)
                    .map(|i| format!("S{i}"))
                    .collect();
                out.push_str(&format!(" union {}[\"u{m}\"]\n", ids.join(",")));
            }
        }
        out
    }

    #[test]
    fn four_ellipses_have_all_fifteen_regions_labelled() {
        let arranged = check(&every_union(4));
        assert_eq!(arranged.scene.edges.len(), 4);
        let mut regions = arranged.regions.clone();
        regions.sort_unstable();
        regions.dedup();
        assert_eq!(regions.len(), 15);
    }

    #[test]
    fn four_sets_with_texts_place_each_in_its_region() {
        check(
            "venn-beta\n set A\n  text a1\n set B\n set C\n set D\n union A,B[AB]\n  text ab\n\
             union C,D[CD]\n union A,D[\"Outer pair\"]\n union A,B,C,D[All]\n  text core\n\
             text B,C bc\n",
        );
    }

    #[test]
    fn five_sets_overlap_where_named_and_keep_apart_elsewhere() {
        let arranged = check(
            "venn-beta\n set A\n set B\n set C\n set D\n set E\n union A,B[AB]\n\
             union B,C[BC]\n union C,D[CD]\n union A,B,C[ABC]\n  text abc\ntext D,E de\n",
        );
        let circles: Vec<(Point, f32)> = arranged
            .outlines
            .iter()
            .map(|o| match *o {
                Outline::Circle { c, r } => (c, r),
                Outline::Ellipse { .. } => panic!("five sets are circles"),
            })
            .collect();
        let touch =
            |i: usize, j: usize| circles[i].0.distance(circles[j].0) < circles[i].1 + circles[j].1;
        // A–E and B–E share nothing; A–D neither.
        assert!(!touch(0, 4) && !touch(1, 4) && !touch(0, 3));
        assert!(touch(3, 4) && touch(0, 2));
    }

    #[test]
    fn five_unrelated_sets_do_not_touch() {
        let arranged = check("venn-beta\n set A\n set B\n set C\n set D\n set E\n set F\n");
        for (i, a) in arranged.outlines.iter().enumerate() {
            for b in &arranged.outlines[i + 1..] {
                let (Outline::Circle { c: ca, r: ra }, Outline::Circle { c: cb, r: rb }) = (a, b)
                else {
                    panic!("circles");
                };
                assert!(ca.distance(*cb) >= ra + rb, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn layout_is_deterministic() {
        let source = "venn-beta\n set A\n set B\n set C\n set D\n set E\n union A,B\n\
                      union A,C,E[ACE]\n";
        let venn = parse(source).unwrap();
        let a = layout(&venn, &Estimate).unwrap();
        let b = layout(&venn, &Estimate).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn a_union_the_layout_cannot_show_is_an_error_naming_it() {
        // Every one of 31 regions of five sets: circles cannot draw them all.
        let venn = parse(&every_union(5)).unwrap();
        let error = arrange(&venn, &Estimate).err().expect("an error");
        assert!(error.line > 6, "{error:?}");
        assert!(error.message.contains("`union S"), "{}", error.message);
        assert!(
            error.message.contains("cannot be drawn"),
            "{}",
            error.message
        );
    }
}
