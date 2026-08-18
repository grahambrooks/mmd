//! C4 layout: elements on a grid, relationships as straight arrows.
//!
//! Mermaid's own C4 renderer routes relationship labels through the boxes it
//! just drew. Here labels sit on the line with the background knocked out
//! behind them, which is legible without needing edge routing.

use super::parse::{C4, ElementKind};
use crate::geom::{Point, Rect};
use crate::scene::{Anchor, Item, PathData, Scene, Stroke};
use crate::text::{Measure, wrap};
use crate::theme::Theme;

const BOX_WIDTH: f32 = 190.0;
const COLUMN_GAP: f32 = 80.0;
const ROW_GAP: f32 = 96.0;
const COLUMNS: usize = 3;
const HEAD_RADIUS: f32 = 13.0;

pub fn layout(model: &C4, theme: &Theme, measure: &dyn Measure) -> (Scene, Rect) {
    let mut scene = Scene::default();
    let inner_width = BOX_WIDTH - 20.0;
    let small = theme.small_size;

    // Wrap every label first: box height follows its content.
    let wrapped: Vec<(Vec<String>, Vec<String>)> = model
        .elements
        .iter()
        .map(|element| {
            let label = wrap(measure, &element.label, theme.font_size, true, inner_width);
            let description = element
                .description
                .as_ref()
                .map(|text| wrap(measure, text, small, false, inner_width))
                .unwrap_or_default();
            (label, description)
        })
        .collect();

    let box_height = |index: usize| {
        let (label, description) = &wrapped[index];
        let mut height = 14.0 + small + 4.0;
        height += label.len() as f32 * measure.line_height(theme.font_size);
        if !description.is_empty() {
            height += 4.0 + description.len() as f32 * measure.line_height(small);
        }
        height + 12.0
    };

    let title_height = if model.title.is_some() {
        theme.title_size + theme.padding
    } else {
        0.0
    };
    let top = theme.padding + title_height;

    // Row heights are set by the tallest box in the row.
    let mut rects: Vec<Rect> = Vec::with_capacity(model.elements.len());
    let mut y = top;
    for row_start in (0..model.elements.len()).step_by(COLUMNS) {
        let row_end = (row_start + COLUMNS).min(model.elements.len());
        let row_height = (row_start..row_end).map(box_height).fold(0.0_f32, f32::max);
        for index in row_start..row_end {
            let column = index - row_start;
            let x = theme.padding + column as f32 * (BOX_WIDTH + COLUMN_GAP);
            rects.push(Rect::new(x, y, BOX_WIDTH, row_height));
        }
        y += row_height + ROW_GAP;
    }
    let content_bottom = y - ROW_GAP + theme.padding;

    // Lines first so boxes cover their ends; labels last so boxes cannot
    // cover the labels.
    let mut labels: Vec<(Point, String)> = Vec::new();
    for relation in &model.relations {
        let from = rects[relation.from];
        let to = rects[relation.to];
        let start = from.edge_towards(to.center());
        let end = to.edge_towards(from.center());
        scene.push(Item::Path {
            data: PathData::new().move_to(start).line_to(end).finish(),
            fill: None,
            stroke: Some(Stroke {
                color: theme.line,
                width: theme.stroke_width,
                dashed: false,
            }),
            arrow: true,
        });

        let mut text = relation.label.clone();
        if let Some(technology) = &relation.technology {
            text.push_str(&format!(" [{technology}]"));
        }
        labels.push((
            Point::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0),
            text,
        ));
    }

    // Elements.
    for (index, element) in model.elements.iter().enumerate() {
        let rect = rects[index];
        let fill = match element.kind {
            _ if element.kind.is_external() => theme.external,
            ElementKind::Person => theme.person,
            _ => theme.system,
        };

        if element.kind.is_person() {
            scene.push(Item::Circle {
                center: Point::new(rect.center().x, rect.y - HEAD_RADIUS * 0.55),
                radius: HEAD_RADIUS,
                fill: Some(fill),
                stroke: None,
            });
        }
        // A pill reads as a store without an ellipse's curve cutting into the
        // description text at the corners.
        let radius = if element.kind == ElementKind::SystemDb {
            22.0
        } else {
            theme.corner_radius
        };
        scene.push(Item::Rect {
            rect,
            radius,
            fill: Some(fill),
            stroke: None,
        });

        let mut text_y = rect.y + 14.0 + small;
        scene.push(Item::Text {
            at: Point::new(rect.center().x, text_y),
            text: format!("\u{ab}{}\u{bb}", element.kind.stereotype()),
            size: small,
            bold: false,
            anchor: Anchor::Middle,
            fill: theme.series_text,
            rotate: 0.0,
        });
        text_y += 6.0;

        let (label, description) = &wrapped[index];
        for line in label {
            text_y += measure.line_height(theme.font_size);
            scene.push(Item::Text {
                at: Point::new(rect.center().x, text_y),
                text: line.clone(),
                size: theme.font_size,
                bold: true,
                anchor: Anchor::Middle,
                fill: theme.series_text,
                rotate: 0.0,
            });
        }
        if !description.is_empty() {
            text_y += 4.0;
            for line in description {
                text_y += measure.line_height(small);
                scene.push(Item::Text {
                    at: Point::new(rect.center().x, text_y),
                    text: line.clone(),
                    size: small,
                    bold: false,
                    anchor: Anchor::Middle,
                    fill: theme.series_text,
                    rotate: 0.0,
                });
            }
        }
    }

    for (midpoint, text) in labels {
        let width = measure.width(&text, small, false);
        // Knock the background out behind the label so it stays readable where
        // it crosses its own line or runs close to a box.
        scene.push(Item::Rect {
            rect: Rect::new(
                midpoint.x - width / 2.0 - 4.0,
                midpoint.y - small,
                width + 8.0,
                small + 6.0,
            ),
            radius: 2.0,
            fill: Some(theme.background),
            stroke: None,
        });
        scene.push(Item::Text {
            at: Point::new(midpoint.x, midpoint.y - 1.0),
            text,
            size: small,
            bold: false,
            anchor: Anchor::Middle,
            fill: theme.muted,
            rotate: 0.0,
        });
    }

    let columns_used = model.elements.len().clamp(1, COLUMNS);
    let width = theme.padding * 2.0
        + columns_used as f32 * BOX_WIDTH
        + (columns_used.saturating_sub(1)) as f32 * COLUMN_GAP;

    if let Some(title) = &model.title {
        scene.push(Item::Text {
            at: Point::new(width / 2.0, theme.padding + theme.title_size),
            text: title.clone(),
            size: theme.title_size,
            bold: true,
            anchor: Anchor::Middle,
            fill: theme.text,
            rotate: 0.0,
        });
    }

    let bounds = Rect::new(0.0, 0.0, width, content_bottom.max(80.0));
    (scene, bounds)
}
