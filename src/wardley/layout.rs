//! Wardley map layout: a fixed plot, evolution across and visibility up,
//! with every element placed at its own coordinates.

use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge, EdgeLabel, Font, Marker, Node, Paint, Pen, Point,
    Rect, Role, Scene, Seg, Shape, Style, Text, TextLine, Weight,
};

use super::parse::{Flow, Kind, Strategy, Wardley};
use crate::FONT_SIZE;
use crate::text::Measure;

/// Mermaid's defaults: a 900 × 600 canvas with a 48 px margin.
const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 600.0;
const MARGIN: f32 = 48.0;
const RADIUS: f32 = 6.0;
const LABEL_OFFSET: f32 = 8.0;
const SQUARE: f32 = RADIUS * 1.6;

/// mermaid's sourcing overlays: build, buy, outsource.
const BUILD_FILL: Paint = Paint::Rgb(0xee, 0xee, 0xee);
const BUY_FILL: Paint = Paint::Rgb(0xcc, 0xcc, 0xcc);
const OUTSOURCE_FILL: Paint = Paint::Rgb(0x66, 0x66, 0x66);

pub fn layout(map: &Wardley, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;
    let small = Font::Small.size(FONT_SIZE);
    let title_size = Font::Title.size(FONT_SIZE);
    let top = if map.title.is_some() {
        title_size + MARGIN * 0.75
    } else {
        0.0
    };
    let (width, height) = map.size.unwrap_or((WIDTH, HEIGHT));
    let chart_w = (width - MARGIN * 2.0).max(100.0);
    let chart_h = (height - MARGIN * 2.0).max(100.0);
    let left = MARGIN;
    let bottom = top + MARGIN + chart_h;
    let project = |visibility: f32, evolution: f32| {
        Point::new(
            left + evolution / 100.0 * chart_w,
            bottom - visibility / 100.0 * chart_h,
        )
    };
    let line = |a: Point, b: Point| vec![Seg::Move(a), Seg::Line(b)];
    let pen = |dash: Dash, weight: Weight, paint: Paint| Pen {
        dash,
        weight,
        paint,
    };
    let plain = |path: Vec<Seg>, pen: Pen| Edge {
        pen,
        end: Marker::None,
        ..Edge::new(path)
    };
    let mut extent = Rect::new(0.0, 0.0, width, top + height);
    let text = |scene: &mut Scene, pos: Point, anchor: Anchor, line: TextLine, paint: Paint| {
        scene.texts.push(Text {
            pos,
            anchor,
            line,
            paint,
            backed: false,
        });
    };

    // Axes, with the evolution stages along the bottom.
    let axis = pen(Dash::Solid, Weight::Normal, Paint::Role(Role::Line));
    scene.edges.push(plain(
        line(Point::new(left, bottom), Point::new(left + chart_w, bottom)),
        axis,
    ));
    scene.edges.push(plain(
        line(Point::new(left, bottom), Point::new(left, top + MARGIN)),
        axis,
    ));
    let names = map.stage_names();
    for (i, ((start, end), name)) in map.stage_spans().into_iter().zip(&names).enumerate() {
        let (x0, x1) = (left + start * chart_w, left + end * chart_w);
        if i > 0 {
            scene.edges.push(plain(
                line(Point::new(x0, top + MARGIN), Point::new(x0, bottom)),
                pen(Dash::Dashed, Weight::Thin, Paint::Role(Role::Muted)),
            ));
        }
        text(
            &mut scene,
            Point::new((x0 + x1) / 2.0, bottom + small + 6.0),
            Anchor::Middle,
            TextLine::new(name.clone(), Font::Small),
            Paint::Role(Role::Muted),
        );
    }
    text(
        &mut scene,
        Point::new(left + chart_w / 2.0, bottom + small * 2.0 + 18.0),
        Anchor::Middle,
        TextLine::new("Evolution", Font::Bold),
        Paint::Role(Role::Text),
    );
    text(
        &mut scene,
        Point::new(left, top + MARGIN - 10.0),
        Anchor::Middle,
        TextLine::new("Visibility", Font::Bold),
        Paint::Role(Role::Text),
    );

    // Where each component is drawn; a pipeline's parent moves to the top
    // of its box, as mermaid does.
    let mut at: Vec<Point> = map
        .components
        .iter()
        .map(|c| project(c.visibility, c.evolution))
        .collect();
    let mut parents = vec![false; map.components.len()];
    for pipeline in &map.pipelines {
        parents[pipeline.parent] = true;
        if pipeline.members.is_empty() {
            continue;
        }
        let xs = pipeline.members.iter().map(|&m| at[m].x);
        let (min, max) = xs.fold((f32::MAX, f32::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
        let y = at[pipeline.members[0]].y;
        let box_h = RADIUS * 4.0;
        let rect = Rect::new(min - 15.0, y - box_h / 2.0, max - min + 30.0, box_h);
        at[pipeline.parent] = Point::new((min + max) / 2.0, rect.y - SQUARE / 6.0);
        scene.containers.push(Container {
            rect,
            kind: ContainerKind::Group,
            label: Vec::new(),
            style: Style {
                fill: Paint::None,
                ..Style::for_container(&ContainerKind::Group)
            },
        });
        // Dotted steps between the members, in evolution order.
        let mut members = pipeline.members.clone();
        members.sort_by(|a, b| at[*a].x.total_cmp(&at[*b].x));
        for pair in members.windows(2) {
            scene.edges.push(plain(
                line(at[pair[0]], at[pair[1]]),
                pen(Dash::Dashed, Weight::Thin, Paint::Role(Role::Line)),
            ));
        }
    }

    // The outline a link meets: a circle, a pipeline square, or an
    // anchor's label.
    let anchor_rect = |i: usize| {
        let c = &map.components[i];
        let w = measure.width(&c.name, small, true) + 6.0;
        Rect::centered(at[i], w, small + 6.0)
    };
    let outline = |i: usize| -> (Shape, Rect) {
        match map.components[i].kind {
            Kind::Anchor => (Shape::Rect, anchor_rect(i)),
            _ if parents[i] => (Shape::Rect, Rect::centered(at[i], SQUARE, SQUARE)),
            _ => (
                Shape::Circle,
                Rect::centered(at[i], RADIUS * 2.0, RADIUS * 2.0),
            ),
        }
    };

    for link in &map.links {
        // Mermaid leaves out a pipeline member's link to its own parent.
        if map
            .pipelines
            .iter()
            .any(|p| p.parent == link.to && p.members.contains(&link.from))
        {
            continue;
        }
        let (fs, fr) = outline(link.from);
        let (ts, tr) = outline(link.to);
        let a = diagram_scene::route::clip(fs, fr, at[link.to]);
        let b = diagram_scene::route::clip(ts, tr, at[link.from]);
        let mut edge = Edge::new(line(a, b)).between(
            &format!("component:{}", link.from),
            &format!("component:{}", link.to),
        );
        edge.pen = pen(
            if link.dashed {
                Dash::Dashed
            } else {
                Dash::Solid
            },
            Weight::Thin,
            Paint::Role(Role::Line),
        );
        edge.start = match link.flow {
            Some(Flow::Backward | Flow::Both) => Marker::Arrow,
            _ => Marker::None,
        };
        edge.end = match link.flow {
            Some(Flow::Forward | Flow::Both) => Marker::Arrow,
            _ => Marker::None,
        };
        if let Some(label) = &link.label {
            let mid = a.lerp(b, 0.5);
            let label = EdgeLabel::new(
                mid,
                vec![TextLine::new(label.clone(), Font::Small)],
                true,
                measure,
                FONT_SIZE,
            );
            extent = extent.union(&Rect::centered(mid, label.width + 12.0, small * 1.6));
            edge.labels.push(label);
        }
        scene.edges.push(edge);
    }

    // Evolution trends: dashed arrows to where a component is heading.
    for (i, component) in map.components.iter().enumerate() {
        let Some(target) = component.evolve else {
            continue;
        };
        let to = project(component.visibility, target);
        let from = at[i];
        let d = to.x - from.x;
        if d.abs() <= RADIUS + 2.0 {
            continue;
        }
        let end = Point::new(to.x - d.signum() * (RADIUS + 2.0), to.y);
        let start = Point::new(from.x + d.signum() * RADIUS, from.y);
        let mut edge = Edge::new(line(start, end));
        edge.pen = pen(Dash::Dashed, Weight::Thin, Paint::Role(Role::Accent));
        edge.from = Some(format!("component:{i}"));
        scene.edges.push(edge);
        // The evolved position, as a ghost of the component.
        let mut ghost = Node::new(
            format!("evolved:{i}"),
            Shape::Circle,
            Rect::centered(to, RADIUS * 2.0, RADIUS * 2.0),
        );
        ghost.style.fill = Paint::Role(Role::Background);
        ghost.style.stroke = Paint::Role(Role::Accent);
        ghost.style.dash = Dash::Dashed;
        scene.nodes.push(ghost);
    }

    // Accelerators and deaccelerators: broad arrows with a name below.
    for (i, force) in map.forces.iter().enumerate() {
        let p = project(force.visibility, force.evolution);
        let (a, b) = if force.accelerates {
            (p, p.offset(60.0, 0.0))
        } else {
            (p.offset(60.0, 0.0), p)
        };
        let mut edge = Edge::new(line(a, b));
        edge.pen = pen(Dash::Solid, Weight::Heavy, Paint::Role(Role::Muted));
        edge.from = Some(format!("force:{i}"));
        scene.edges.push(edge);
        text(
            &mut scene,
            Point::new(p.x + 30.0, p.y + 15.0 + small),
            Anchor::Middle,
            TextLine::new(force.name.clone(), Font::Small),
            Paint::Role(Role::Text),
        );
    }

    // Components: sourcing overlays under their circles, then the marks.
    for (i, component) in map.components.iter().enumerate() {
        let p = at[i];
        let id = format!("component:{i}");
        let circle = |r: f32| Rect::centered(p, r * 2.0, r * 2.0);
        if component.kind == Kind::Anchor {
            let rect = anchor_rect(i);
            let mut node = Node::new(id, Shape::Rect, rect);
            node.style.fill = Paint::None;
            node.style.stroke = Paint::None;
            node.label = vec![TextLine::new(component.name.clone(), Font::Bold)];
            // Anchors are body-bold in the node; size the box for it.
            let w = measure.width(&component.name, FONT_SIZE, true) + 6.0;
            node.rect = Rect::centered(p, w, FONT_SIZE + 6.0);
            extent = extent.union(&node.rect);
            scene.nodes.push(node);
            continue;
        }
        if let Some(strategy) = component.strategy {
            let mut overlay = Node::new(
                format!("{id}:strategy"),
                Shape::Circle,
                circle(RADIUS * 2.0),
            );
            overlay.style.stroke = Paint::Role(Role::Line);
            overlay.style.fill = match strategy {
                Strategy::Build => BUILD_FILL,
                Strategy::Buy => BUY_FILL,
                Strategy::Outsource => OUTSOURCE_FILL,
                Strategy::Market => Paint::Role(Role::Background),
            };
            scene.nodes.push(overlay);
        }
        if parents[i] {
            let mut node = Node::new(id.clone(), Shape::Rect, Rect::centered(p, SQUARE, SQUARE));
            node.style.fill = Paint::Role(Role::Background);
            node.style.stroke = Paint::Role(Role::Text);
            scene.nodes.push(node);
        } else if component.strategy == Some(Strategy::Market) {
            // Three small circles in a triangle, in place of the dot.
            let k = RADIUS * 1.2;
            let cos30 = 3.0_f32.sqrt() / 2.0;
            for (j, d) in [(0.0, -k), (-k * cos30, k / 2.0), (k * cos30, k / 2.0)]
                .into_iter()
                .enumerate()
            {
                let mut dot = Node::new(
                    if j == 0 {
                        id.clone()
                    } else {
                        format!("{id}:market{j}")
                    },
                    Shape::Circle,
                    Rect::centered(p.offset(d.0, d.1), RADIUS * 1.0, RADIUS * 1.0),
                );
                dot.style.fill = Paint::Role(Role::Background);
                dot.style.stroke = Paint::Role(Role::Text);
                scene.nodes.push(dot);
            }
        } else {
            let mut node = Node::new(id.clone(), Shape::Circle, circle(RADIUS));
            node.style.fill = Paint::Role(Role::Background);
            node.style.stroke = Paint::Role(Role::Text);
            scene.nodes.push(node);
        }
        if component.inertia {
            let mut offset = if parents[i] {
                SQUARE / 2.0 + 15.0
            } else {
                RADIUS + 15.0
            };
            if component.strategy.is_some() {
                offset += RADIUS + 10.0;
            }
            let h = if parents[i] { SQUARE } else { RADIUS * 2.0 };
            scene.edges.push(plain(
                line(
                    Point::new(p.x + offset, p.y - h / 2.0),
                    Point::new(p.x + offset, p.y + h / 2.0),
                ),
                pen(Dash::Solid, Weight::Heavy, Paint::Role(Role::Text)),
            ));
        }
        let extra = if component.strategy.is_some() {
            10.0
        } else {
            0.0
        };
        let (dx, dy) = component
            .label_offset
            .unwrap_or((LABEL_OFFSET + extra, -LABEL_OFFSET - extra));
        text(
            &mut scene,
            Point::new(p.x + dx, p.y + dy),
            Anchor::Start,
            TextLine::new(component.name.clone(), Font::Small),
            Paint::Role(Role::Text),
        );
    }

    for note in &map.notes {
        text(
            &mut scene,
            project(note.visibility, note.evolution),
            Anchor::Start,
            TextLine::new(note.text.clone(), Font::Small),
            Paint::Role(Role::Text),
        );
    }

    // Numbered annotations, and their key when `annotations` places it.
    let mut annotations: Vec<_> = map.annotations.iter().collect();
    annotations.sort_by_key(|a| a.number);
    for (i, annotation) in annotations.iter().enumerate() {
        let p = project(annotation.visibility, annotation.evolution);
        let mut node = Node::new(
            format!("annotation:{i}"),
            Shape::Circle,
            Rect::centered(p, 18.0, 18.0),
        );
        node.style.fill = Paint::Role(Role::Background);
        node.style.stroke = Paint::Role(Role::Text);
        node.label = vec![TextLine::new(annotation.number.to_string(), Font::Small)];
        scene.nodes.push(node);
    }
    if let (Some((visibility, evolution)), false) = (map.annotations_box, annotations.is_empty()) {
        let origin = project(visibility, evolution);
        let row = small * 1.5;
        let lines: Vec<String> = annotations
            .iter()
            .map(|a| format!("{}. {}", a.number, a.text))
            .collect();
        let w = lines
            .iter()
            .map(|l| measure.width(l, small, false))
            .fold(0.0_f32, f32::max)
            + 20.0;
        let rect = Rect::new(origin.x, origin.y, w, row * lines.len() as f32 + 12.0);
        extent = extent.union(&rect);
        scene.containers.push(Container {
            rect,
            kind: ContainerKind::Group,
            label: Vec::new(),
            style: Style::for_container(&ContainerKind::Group),
        });
        for (i, l) in lines.into_iter().enumerate() {
            text(
                &mut scene,
                Point::new(origin.x + 10.0, origin.y + 6.0 + row * i as f32 + small),
                Anchor::Start,
                TextLine::new(l, Font::Small),
                Paint::Role(Role::Text),
            );
        }
    }

    // Grow the canvas for anything that strayed past it, and shift so
    // nothing sits left of or above the origin.
    for edge in &scene.edges {
        for seg in &edge.path {
            if let Seg::Move(p) | Seg::Line(p) = seg {
                extent = extent.union(&Rect::new(p.x - 4.0, p.y - 4.0, 8.0, 8.0));
            }
        }
    }
    for node in &scene.nodes {
        extent = extent.union(&node.rect);
    }
    for t in &scene.texts {
        let w = diagram_scene::line_width(measure, &t.line, FONT_SIZE);
        let x = match t.anchor {
            Anchor::Start => t.pos.x,
            Anchor::Middle => t.pos.x - w / 2.0,
            Anchor::End => t.pos.x - w,
        };
        let size = t.line.font().size(FONT_SIZE);
        extent = extent.union(&Rect::new(x, t.pos.y - size, w, size * 1.3));
    }
    let pad = 8.0;
    let (dx, dy) = ((pad - extent.x).max(0.0), (pad - extent.y).max(0.0));
    if dx > 0.0 || dy > 0.0 {
        shift(&mut scene, dx, dy);
    }
    let full_width = extent.right() + dx + pad;
    if let Some(title) = &map.title {
        scene.title = Some(Text {
            pos: Point::new(full_width / 2.0, dy + MARGIN * 0.5 + title_size),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
        });
    }
    scene.width = full_width.max(width);
    scene.height = extent.bottom() + dy + pad;
    scene
}

fn shift(scene: &mut Scene, dx: f32, dy: f32) {
    let move_point = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for c in &mut scene.containers {
        c.rect.x += dx;
        c.rect.y += dy;
    }
    for n in &mut scene.nodes {
        n.rect.x += dx;
        n.rect.y += dy;
    }
    for t in &mut scene.texts {
        move_point(&mut t.pos);
    }
    for e in &mut scene.edges {
        for seg in &mut e.path {
            match seg {
                Seg::Move(p) | Seg::Line(p) => move_point(p),
                Seg::Cubic(a, b, c) => {
                    move_point(a);
                    move_point(b);
                    move_point(c);
                }
                Seg::Close => {}
            }
        }
        for l in &mut e.labels {
            move_point(&mut l.pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;
    use diagram_scene::Estimate;

    #[test]
    fn nothing_is_dropped_and_everything_is_inside_the_bounds() {
        let source = "wardley-beta\ntitle Tea\nanchor Business [0.95, 0.63]\n\
            component Cup of Tea [0.79, 0.61] (build)\ncomponent Water [0.52, 0.99] (market) inertia\n\
            component Kettle [0.43, 0.02] (outsource)\nBusiness -> Cup of Tea\n\
            Cup of Tea +'heat'> Water\nWater -> Kettle\nevolve Kettle 0.62\n\
            pipeline Kettle {\n component Campfire [0.30]\n component Electric [0.53]\n}\n\
            note \"A very long note that runs past the right edge of the plot\" [0.3, 0.95]\n\
            annotations [0.1, 0.9]\nannotation 1,[0.5, 0.5] \"Key\"\n\
            accelerator Cloud [0.2, 0.98]\ndeaccelerator Legacy [0.4, 0.0]\n";
        let map = parse(source).unwrap();
        let scene = layout(&map, &Estimate);
        let texts = scene.texts();
        for c in &map.components {
            assert!(texts.contains(&c.name), "{}", c.name);
        }
        for expected in [
            "Tea",
            "heat",
            "Genesis",
            "Commodity",
            "Cloud",
            "Legacy",
            "1. Key",
        ] {
            assert!(texts.iter().any(|t| t == expected), "{expected}");
        }
        let bounds = scene.bounds();
        for node in &scene.nodes {
            assert!(bounds.encloses(&node.rect, 0.01), "{node:?}");
        }
        for t in &scene.texts {
            let w = diagram_scene::line_width(&Estimate, &t.line, FONT_SIZE);
            let x = match t.anchor {
                Anchor::Start => t.pos.x,
                Anchor::Middle => t.pos.x - w / 2.0,
                Anchor::End => t.pos.x - w,
            };
            assert!(x >= 0.0 && x + w <= bounds.width + 0.01, "{t:?}");
            assert!(t.pos.y >= 0.0 && t.pos.y <= bounds.height, "{t:?}");
        }
        // Links less the member-to-parent ones, the trend and both forces.
        assert!(scene.edges.iter().filter(|e| e.from.is_some()).count() >= 6);
    }
}
