//! kanban layout: columns side by side, each a box of stacked cards.

use diagram_scene::{
    Anchor, Container, ContainerKind, Font, Node, Paint, Point, Rect, Role, Scene, Shape, Style,
    Text, TextLine,
};

use super::parse::{Kanban, Priority};
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

const COLUMN_WIDTH: f32 = 200.0;
const GAP: f32 = 16.0;
const INSET: f32 = 8.0;
const HEADER: f32 = 32.0;
const PAD: f32 = 8.0;
const STRIPE: f32 = 4.0;

/// Lay a parsed board out.
pub fn layout(kanban: &Kanban, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let small = Font::Small.size(FONT_SIZE);
    let line_height = FONT_SIZE * 1.35;
    let card_width = COLUMN_WIDTH - 2.0 * INSET;
    let text_left = PAD + STRIPE + 2.0;
    let text_width = card_width - text_left - PAD;

    struct Placed {
        lines: Vec<String>,
        height: f32,
        meta: bool,
    }
    let columns: Vec<Vec<Placed>> = kanban
        .columns
        .iter()
        .map(|column| {
            column
                .cards
                .iter()
                .map(|card| {
                    let lines = wrap(measure, &card.label, FONT_SIZE, false, text_width);
                    let meta = card.ticket.is_some() || card.assigned.is_some();
                    let height = 2.0 * PAD
                        + lines.len() as f32 * line_height
                        + if meta { small * 1.35 + 4.0 } else { 0.0 };
                    Placed {
                        lines,
                        height,
                        meta,
                    }
                })
                .collect()
        })
        .collect();
    let content = columns
        .iter()
        .map(|cards| cards.iter().map(|c| c.height + INSET).sum::<f32>())
        .fold(0.0_f32, f32::max);
    let column_height = HEADER + content + INSET.max(if content == 0.0 { 24.0 } else { 0.0 });

    for (index, (column, cards)) in kanban.columns.iter().zip(&columns).enumerate() {
        let x = PADDING + index as f32 * (COLUMN_WIDTH + GAP);
        let header = wrap(measure, &column.label, FONT_SIZE, true, COLUMN_WIDTH - 20.0);
        scene.containers.push(Container {
            rect: Rect::new(x, PADDING, COLUMN_WIDTH, column_height),
            kind: ContainerKind::Group,
            // The header wraps to one line in the space a header has; longer
            // names continue as further header lines.
            label: header
                .iter()
                .take(1)
                .map(|l| TextLine::new(l.clone(), Font::Bold))
                .collect(),
            style: Style {
                fill: Paint::Role(Role::Surface),
                stroke: Paint::Role(Role::Line),
                text: Paint::Role(Role::Text),
                ..Style::for_container(&ContainerKind::Group)
            },
        });
        let mut header_extra = 0.0;
        for line in header.iter().skip(1) {
            header_extra += line_height;
            scene.texts.push(Text {
                pos: Point::new(x + 10.0, PADDING + FONT_SIZE * 1.25 + header_extra),
                anchor: Anchor::Start,
                line: TextLine::new(line.clone(), Font::Bold),
                paint: Paint::Role(Role::Text),
                backed: false,
            });
        }

        let mut y = PADDING + HEADER + header_extra;
        for (card, placed) in column.cards.iter().zip(cards) {
            let left = x + INSET;
            let mut node = Node::new(
                format!("card:{}", card.id),
                Shape::Rounded,
                Rect::new(left, y, card_width, placed.height),
            );
            node.style.fill = Paint::Role(Role::Background);
            scene.nodes.push(node);
            if let Some(priority) = card.priority {
                let paint = Paint::Role(match priority {
                    Priority::VeryHigh => Role::Series(3),
                    Priority::High => Role::Series(4),
                    Priority::Low => Role::Series(0),
                    Priority::VeryLow => Role::Muted,
                });
                let mut stripe = Node::new(
                    format!("card:{}:priority", card.id),
                    Shape::Rect,
                    Rect::new(left + 3.0, y + 4.0, STRIPE, placed.height - 8.0),
                );
                stripe.style.fill = paint;
                stripe.style.stroke = Paint::None;
                scene.nodes.push(stripe);
            }
            let mut baseline = y + PAD + FONT_SIZE;
            for line in &placed.lines {
                scene.texts.push(Text {
                    pos: Point::new(left + text_left, baseline),
                    anchor: Anchor::Start,
                    line: TextLine::new(line.clone(), Font::Body),
                    paint: Paint::Role(Role::Text),
                    backed: false,
                });
                baseline += line_height;
            }
            if placed.meta {
                let meta_y = y + placed.height - PAD - small * 0.25;
                if let Some(ticket) = &card.ticket {
                    scene.texts.push(Text {
                        pos: Point::new(left + text_left, meta_y),
                        anchor: Anchor::Start,
                        line: TextLine::new(ticket.clone(), Font::Small),
                        paint: Paint::Role(Role::Muted),
                        backed: false,
                    });
                }
                if let Some(assigned) = &card.assigned {
                    scene.texts.push(Text {
                        pos: Point::new(left + card_width - PAD, meta_y),
                        anchor: Anchor::End,
                        line: TextLine::new(assigned.clone(), Font::Small),
                        paint: Paint::Role(Role::Accent),
                        backed: false,
                    });
                }
            }
            y += placed.height + INSET;
        }
    }

    let count = kanban.columns.len().max(1) as f32;
    scene.width = PADDING * 2.0 + count * COLUMN_WIDTH + (count - 1.0) * GAP;
    // Wrapped column headers push their cards down.
    let lowest = scene
        .nodes
        .iter()
        .map(|n| n.rect.bottom() + INSET)
        .fold(PADDING + column_height, f32::max);
    for container in &mut scene.containers {
        container.rect.height = lowest - PADDING;
    }
    scene.height = lowest + PADDING;
    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kanban::parse::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const SOURCE: &str = "kanban
  todo[To do]
    a[Write the parser for every bracket form mermaid accepts]@{ ticket: MMD-1, assigned: graham, priority: 'High' }
    b[Draw cards]
  doing[In progress]
    c[Layout]@{ priority: 'Very High' }
  done[Done]
";

    #[test]
    fn every_column_card_and_detail_is_drawn() {
        let scene = layout(&parse(SOURCE).unwrap(), &Fixed);
        let texts = scene.texts().join(" ");
        for expected in [
            "To do",
            "In progress",
            "Done",
            "Draw cards",
            "Layout",
            "MMD-1",
            "graham",
            "mermaid accepts",
        ] {
            assert!(texts.contains(expected), "missing {expected}: {texts}");
        }
    }

    #[test]
    fn cards_sit_inside_their_column_without_overlapping() {
        let scene = layout(&parse(SOURCE).unwrap(), &Fixed);
        let cards: Vec<_> = scene
            .nodes
            .iter()
            .filter(|n| !n.id.ends_with(":priority"))
            .collect();
        for (i, a) in cards.iter().enumerate() {
            for b in &cards[i + 1..] {
                assert!(!a.rect.intersects(&b.rect), "{} overlaps {}", a.id, b.id);
            }
        }
        let todo = scene.containers[0].rect;
        assert!(todo.encloses(&scene.node("card:a").unwrap().rect, 0.01));
        assert!(scene.bounds().encloses(&todo, 0.01));
    }
}
