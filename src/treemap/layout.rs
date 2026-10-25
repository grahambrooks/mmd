//! Squarified rectangles (Bruls, Huizing and van Wijk), largest first as
//! upstream's d3 layout sorts them. A section is a box with its name in a
//! header band and its children inside; a leaf is a box with its name and
//! value.

use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Font, Node, Paint, Role, Scene, Shape, Style, Text,
    TextLine, geom::Point, geom::Rect, line_width, shape::line_height,
};

use super::parse::Treemap;
use crate::style::Colors;
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const WIDTH: f32 = 720.0;
/// Space under a section's name, and around its children.
const HEADER: f32 = 24.0;
const INSET: f32 = 4.0;
/// Between outermost items.
const GAP: f32 = 4.0;
const TITLE_GAP: f32 = 10.0;

pub fn layout(map: &Treemap, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let mut top = PADDING;
    if let Some(title) = map.title.as_ref().filter(|t| !t.is_empty()) {
        let size = Font::Title.size(FONT_SIZE);
        scene.title = Some(Text {
            pos: Point::new(PADDING + WIDTH / 2.0, top + size * 0.8),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
        top += line_height(size) + TITLE_GAP;
    }
    let leaves = map.items.iter().filter(|i| i.value.is_some()).count();
    let height = (leaves as f32 * 36.0).clamp(300.0, 560.0);
    let area = Rect::new(PADDING, top, WIDTH, height);
    let mut ctx = Ctx {
        map,
        measure,
        scene: &mut scene,
    };
    ctx.fill(&map.roots, area, None, GAP);
    scene.width = WIDTH + 2.0 * PADDING;
    scene.height = top + height + PADDING;
    scene
}

struct Ctx<'a> {
    map: &'a Treemap,
    measure: &'a dyn Measure,
    scene: &'a mut Scene,
}

impl Ctx<'_> {
    /// Lay `items` out in `rect`. `series` is the colour of the outermost
    /// section they belong to, or `None` at the top, where each outermost
    /// item takes its own.
    fn fill(&mut self, items: &[usize], rect: Rect, series: Option<u8>, gap: f32) {
        let mut sorted: Vec<usize> = items.to_vec();
        sorted.sort_by(|&a, &b| self.map.total(b).total_cmp(&self.map.total(a)));
        let values: Vec<f64> = sorted.iter().map(|&i| self.map.total(i)).collect();
        let rects = squarify(&values, rect);
        // Order of `items` keeps colours stable whatever the sizes.
        for (&item, cell) in sorted.iter().zip(rects) {
            let series =
                series.unwrap_or_else(|| items.iter().position(|&i| i == item).unwrap_or(0) as u8);
            let cell = shrink(cell, gap / 2.0);
            self.item(item, cell, series);
        }
    }

    fn item(&mut self, index: usize, rect: Rect, series: u8) {
        let it = &self.map.items[index];
        if it.value.is_some() {
            let mut node = Node::new(format!("item{index}"), Shape::Rect, rect);
            node.style.stroke = Paint::Role(Role::Series(series));
            apply(&mut node.style, &it.colors);
            let room = rect.width - 8.0;
            let fits = |font: Font| {
                line_width(self.measure, &TextLine::new(&it.name, font), FONT_SIZE) <= room
                    && rect.height >= 2.0 * line_height(font.size(FONT_SIZE)) + 4.0
            };
            let (name_font, value_font) = if fits(Font::Bold) {
                (Font::Bold, Font::Body)
            } else {
                (Font::Small, Font::Small)
            };
            let size = name_font.size(FONT_SIZE);
            node.label = crate::text::wrap(self.measure, &it.name, size, name_font.bold(), room)
                .into_iter()
                .map(|l| TextLine::new(l, name_font))
                .collect();
            node.sublabel = vec![TextLine::new(
                display(it.value.unwrap_or(0.0), &it.shown),
                value_font,
            )];
            self.scene.nodes.push(node);
            return;
        }
        let kind = ContainerKind::Group;
        let mut style = Style::for_container(&kind);
        style.stroke = Paint::Role(Role::Series(series));
        style.text = Paint::Role(Role::Series(series));
        apply(&mut style, &it.colors);
        let total = self.map.total(index);
        self.scene.containers.push(Container {
            rect,
            kind,
            label: vec![TextLine {
                runs: vec![
                    diagram_scene::Run {
                        text: it.name.clone(),
                        font: Font::Bold,
                    },
                    diagram_scene::Run {
                        text: format!("  {}", display(total, "")),
                        font: Font::Small,
                    },
                ],
            }],
            style,
        });
        let inner = Rect::new(
            rect.x + INSET,
            rect.y + HEADER,
            (rect.width - 2.0 * INSET).max(1.0),
            (rect.height - HEADER - INSET).max(1.0),
        );
        let children = it.children.clone();
        self.fill(&children, inner, Some(series), INSET);
    }
}

fn apply(style: &mut Style, colors: &Colors) {
    if let Some((r, g, b)) = colors.fill {
        style.fill = Paint::Rgb(r, g, b);
    }
    if let Some((r, g, b)) = colors.stroke {
        style.stroke = Paint::Rgb(r, g, b);
    }
    if let Some((r, g, b)) = colors.text {
        style.text = Paint::Rgb(r, g, b);
    }
    if colors.dashed {
        style.dash = Dash::Dashed;
    }
}

/// A value as written, or a sum with thousands separated.
fn display(value: f64, shown: &str) -> String {
    if !shown.is_empty() {
        return shown.to_string();
    }
    if value.fract() != 0.0 {
        return format!("{value:.2}");
    }
    let digits = format!("{}", value as i64);
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn shrink(rect: Rect, by: f32) -> Rect {
    let bx = by.min(rect.width / 4.0);
    let byy = by.min(rect.height / 4.0);
    Rect::new(
        rect.x + bx,
        rect.y + byy,
        rect.width - 2.0 * bx,
        rect.height - 2.0 * byy,
    )
}

/// Rectangles for `values` (largest first) tiling `rect`, each with area
/// in proportion to its value.
pub(crate) fn squarify(values: &[f64], rect: Rect) -> Vec<Rect> {
    let total: f64 = values.iter().sum();
    if values.is_empty() || total <= 0.0 {
        return vec![Rect::new(rect.x, rect.y, 0.0, 0.0); values.len()];
    }
    let scale = (rect.width as f64 * rect.height as f64) / total;
    let areas: Vec<f64> = values.iter().map(|v| v * scale).collect();
    let mut out = Vec::with_capacity(values.len());
    let (mut x, mut y, mut w, mut h) = (
        rect.x as f64,
        rect.y as f64,
        rect.width as f64,
        rect.height as f64,
    );
    let mut start = 0;
    while start < areas.len() {
        let side = w.min(h);
        // Grow the row while it makes the worst aspect ratio better.
        let mut end = start + 1;
        while end < areas.len()
            && worst(&areas[start..=end], side) <= worst(&areas[start..end], side)
        {
            end += 1;
        }
        let row = &areas[start..end];
        let sum: f64 = row.iter().sum();
        if w >= h {
            // A column along the left.
            let thick = if h > 0.0 { sum / h } else { 0.0 };
            let mut cy = y;
            for a in row {
                let len = if thick > 0.0 { a / thick } else { 0.0 };
                out.push(Rect::new(x as f32, cy as f32, thick as f32, len as f32));
                cy += len;
            }
            x += thick;
            w -= thick;
        } else {
            // A row along the top.
            let thick = if w > 0.0 { sum / w } else { 0.0 };
            let mut cx = x;
            for a in row {
                let len = if thick > 0.0 { a / thick } else { 0.0 };
                out.push(Rect::new(cx as f32, y as f32, len as f32, thick as f32));
                cx += len;
            }
            y += thick;
            h -= thick;
        }
        start = end;
    }
    out
}

/// The worst aspect ratio of `row` laid along a side of length `side`.
fn worst(row: &[f64], side: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    let (max, min) = row
        .iter()
        .fold((f64::MIN, f64::MAX), |(hi, lo), &a| (hi.max(a), lo.min(a)));
    if sum <= 0.0 || min <= 0.0 {
        return f64::MAX;
    }
    let s2 = side * side;
    let sum2 = sum * sum;
    (s2 * max / sum2).max(sum2 / (s2 * min))
}

#[cfg(test)]
mod tests {
    use super::*;
    use diagram_scene::Estimate;

    #[test]
    fn squarify_tiles_the_rectangle_in_proportion() {
        let values = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let rect = Rect::new(0.0, 0.0, 600.0, 400.0);
        let rects = squarify(&values, rect);
        let area: f32 = rects.iter().map(|r| r.width * r.height).sum();
        assert!((area - 240_000.0).abs() < 1.0);
        for (v, r) in values.iter().zip(&rects) {
            let expect = *v as f32 / 24.0 * 240_000.0;
            assert!((r.width * r.height - expect).abs() < 1.0);
            assert!(rect.encloses(r, 0.01));
        }
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(
                    !a.inflate(-0.01).intersects(&b.inflate(-0.01)),
                    "{a:?} {b:?}"
                );
            }
        }
    }

    #[test]
    fn every_item_is_drawn_and_leaves_sit_in_their_section() {
        let map = super::super::parse(
            "treemap-beta\ntitle Budget\n\"Ops\"\n  \"Salaries\": 700\n  \"Kit\": 200\n\"Marketing\"\n  \"Ads\": 400\n  \"Events\": 100\n\"Misc\": 50\n",
        )
        .unwrap();
        let s = layout(&map, &Estimate);
        let texts = s.texts();
        for t in [
            "Budget",
            "Ops  900",
            "Salaries",
            "700",
            "Kit",
            "Marketing  500",
            "Ads",
            "Events",
            "Misc",
            "50",
        ] {
            assert!(texts.iter().any(|x| x == t), "missing {t}: {texts:?}");
        }
        let ops = s
            .containers
            .iter()
            .find(|c| c.label[0].plain().starts_with("Ops"))
            .unwrap();
        for id in ["item1", "item2"] {
            assert!(ops.rect.encloses(&s.node(id).unwrap().rect, 0.01));
        }
        for (i, a) in s.nodes.iter().enumerate() {
            for b in &s.nodes[i + 1..] {
                assert!(!a.rect.intersects(&b.rect));
            }
        }
        assert_eq!(s.containers[0].style.stroke, Paint::Role(Role::Series(0)));
    }
}
