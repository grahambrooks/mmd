//! sequenceDiagram layout: participants in columns, time running down.
//!
//! No graph layout is needed. Column gaps start from the participants'
//! widths and grow until every message label, self-message and note
//! between two columns fits. Then one pass down the events places each
//! message, note and frame divider below the last, the way mermaid's
//! `sequenceRenderer.ts` does.
//!
//! Paint order: boxes and `rect` backgrounds, lifelines, activation bars,
//! frames (their tabs) as containers; messages as edges; participants and
//! notes as nodes; message labels over those, and frame labels last.

use diagram_scene::shape;
use diagram_scene::{
    Anchor, Container, ContainerKind, Dash, Edge, Font, Marker, Node, Paint, Pen, Point, Rect,
    Role, Scene, Seg, Shape, Style, Text, TextLine, Weight,
};

use super::parse::{Event, Frame, Head, Message, Note, NoteSide, ParticipantKind, Sequence};
use crate::text::{Measure, wrap};
use crate::{FONT_SIZE, PADDING};

/// Space between two participants' boxes.
const COLUMN_GAP: f32 = 40.0;
const MIN_HEAD_WIDTH: f32 = 100.0;
const MIN_HEAD_HEIGHT: f32 = 40.0;
/// Around a `box`'s participants.
const BOX_PAD: f32 = 10.0;
const BAR_WIDTH: f32 = 10.0;
/// Between a lifeline (or its bar) and a note beside it.
const NOTE_GAP: f32 = 10.0;
/// Around a frame's contents, per level of nesting.
const FRAME_PAD: f32 = 14.0;
/// How far a self-message loops out to the right.
const SELF_LOOP: f32 = 45.0;
/// How far a self-message drops before it returns.
const SELF_DROP: f32 = 20.0;
/// Between a message's label and its line.
const LABEL_GAP: f32 = 5.0;
/// Width that `wrap:` text is wrapped to.
const WRAP_WIDTH: f32 = 200.0;
const ICON: f32 = 30.0;

/// Lay a parsed sequence out.
pub fn layout(seq: &Sequence, measure: &dyn Measure) -> Scene {
    let mut l = Layout::new(seq, measure);
    l.columns();
    l.events();
    l.finish()
}

/// A participant's measured head.
struct HeadSize {
    width: f32,
    height: f32,
}

struct OpenFrame {
    frame: Frame,
    /// Its position among frames opened, so outer frames paint first.
    order: usize,
    top: f32,
    label: String,
    /// The y of each `else`/`and`/`option` divider, and its label.
    sections: Vec<(f32, String)>,
    min_x: f32,
    max_x: f32,
}

struct Layout<'a> {
    seq: &'a Sequence,
    measure: &'a dyn Measure,
    heads: Vec<HeadSize>,
    /// Column centres.
    x: Vec<f32>,
    /// The text of each event, split into lines (wrapped where asked).
    lines: Vec<Vec<String>>,
    line_height: f32,

    cursor: f32,
    /// The y of the message just placed, while nothing else has been.
    last_message: Option<f32>,
    /// Where each lifeline starts and (if destroyed) ends.
    alive_from: Vec<f32>,
    ended_at: Vec<Option<f32>>,
    /// Open activations per participant: where each started.
    active: Vec<Vec<f32>>,
    frames: Vec<OpenFrame>,
    frames_opened: usize,
    /// The x extents of coloured boxes.
    tints: Vec<(f32, f32)>,

    boxes: Vec<Container>,
    rects: Vec<(usize, Container)>,
    bars: Vec<Container>,
    fragments: Vec<(usize, Container)>,
    edges: Vec<Edge>,
    nodes: Vec<Node>,
    texts: Vec<Text>,
    bounds: Option<Rect>,
}

impl<'a> Layout<'a> {
    fn new(seq: &'a Sequence, measure: &'a dyn Measure) -> Self {
        let n = seq.participants.len();
        let line_height = shape::line_height(FONT_SIZE);
        let mut l = Layout {
            seq,
            measure,
            heads: Vec::new(),
            x: vec![0.0; n],
            lines: Vec::new(),
            line_height,
            cursor: 0.0,
            last_message: None,
            alive_from: vec![0.0; n],
            ended_at: vec![None; n],
            active: vec![Vec::new(); n],
            frames: Vec::new(),
            frames_opened: 0,
            tints: Vec::new(),
            boxes: Vec::new(),
            rects: Vec::new(),
            bars: Vec::new(),
            fragments: Vec::new(),
            edges: Vec::new(),
            nodes: Vec::new(),
            texts: Vec::new(),
            bounds: None,
        };
        l.heads = (0..n).map(|i| l.measure_head(i)).collect();
        l.lines = seq
            .events
            .iter()
            .map(|event| match event {
                Event::Message(m) => l.split(&m.text, m.wrap, WRAP_WIDTH),
                Event::Note(n) => l.split(&n.text, n.wrap, WRAP_WIDTH * 0.75),
                Event::Start(_, label) | Event::Section(label) => {
                    l.split(&bracket(label), false, 0.0)
                }
                _ => Vec::new(),
            })
            .collect();
        l
    }

    fn split(&self, text: &str, wraps: bool, width: f32) -> Vec<String> {
        if text.is_empty() {
            return Vec::new();
        }
        if wraps {
            wrap(self.measure, text, FONT_SIZE, false, width)
        } else {
            text.split('\n').map(str::to_string).collect()
        }
    }

    fn width(&self, text: &str) -> f32 {
        self.measure.width(text, FONT_SIZE, false)
    }

    /// The width a free label takes, backing included: the SVG writer
    /// sizes the backing by its own estimate, so take the wider.
    fn label_width(&self, text: &str) -> f32 {
        let line = TextLine::new(text, Font::Body);
        self.width(text).max(diagram_scene::line_width(
            &diagram_scene::Estimate,
            &line,
            FONT_SIZE,
        )) + 6.0
    }

    fn label_widest(&self, lines: &[String]) -> f32 {
        lines
            .iter()
            .map(|l| self.label_width(l))
            .fold(0.0, f32::max)
    }

    fn widest(&self, lines: &[String]) -> f32 {
        lines.iter().map(|l| self.width(l)).fold(0.0, f32::max)
    }

    fn text_height(&self, lines: &[String]) -> f32 {
        lines.len() as f32 * self.line_height
    }

    fn grow(&mut self, rect: Rect) {
        self.bounds = Some(match self.bounds {
            Some(b) => b.union(&rect),
            None => rect,
        });
    }

    /// Whether `x0..x1` lies on a tinted panel at the current position: in
    /// an open `rect`, or over a coloured `box`.
    fn tinted(&self, x0: f32, x1: f32) -> bool {
        self.frames
            .iter()
            .any(|f| matches!(f.frame, Frame::Rect(_)))
            || self.tints.iter().any(|&(a, b)| x0 < b && x1 > a)
    }

    /// Widen every open frame to take in `x0..x1`.
    fn touch(&mut self, x0: f32, x1: f32) {
        for frame in &mut self.frames {
            frame.min_x = frame.min_x.min(x0);
            frame.max_x = frame.max_x.max(x1);
        }
    }

    // ---- participants -------------------------------------------------

    fn label_lines(&self, i: usize) -> Vec<TextLine> {
        TextLine::lines(&self.seq.participants[i].label, Font::Body)
    }

    fn measure_head(&self, i: usize) -> HeadSize {
        let p = &self.seq.participants[i];
        let lines: Vec<String> = p.label.split('\n').map(str::to_string).collect();
        let (w, h) = match p.kind {
            ParticipantKind::Boundary | ParticipantKind::Control | ParticipantKind::Entity => {
                let w = self.widest(&lines).max(ICON + 20.0);
                (w + 8.0, ICON + 6.0 + self.text_height(&lines))
            }
            kind => {
                let mut node = Node::new("", node_shape(kind), Rect::default());
                node.label = self.label_lines(i);
                shape::natural_size(&node, FONT_SIZE, self.measure)
            }
        };
        let min_w = match p.kind {
            ParticipantKind::Actor => 60.0,
            _ => MIN_HEAD_WIDTH,
        };
        HeadSize {
            width: w.max(min_w),
            height: h.max(MIN_HEAD_HEIGHT),
        }
    }

    /// The nodes (and decorations) that draw participant `i` in `rect`.
    fn draw_head(&mut self, i: usize, rect: Rect, suffix: &str) {
        let p = &self.seq.participants[i];
        let id = format!("{}{suffix}", p.id);
        match p.kind {
            ParticipantKind::Boundary | ParticipantKind::Control | ParticipantKind::Entity => {
                let c = Point::new(rect.center().x, rect.y + ICON / 2.0);
                let circle = Node::new(id, Shape::Circle, Rect::centered(c, ICON, ICON));
                let r = ICON / 2.0;
                let line = |a: Point, b: Point| {
                    let mut e = Edge::new(vec![Seg::Move(a), Seg::Line(b)]);
                    e.end = Marker::None;
                    e
                };
                match p.kind {
                    ParticipantKind::Boundary => {
                        let bar = c.x - r - 8.0;
                        self.edges.push(line(
                            Point::new(bar, c.y - r * 0.8),
                            Point::new(bar, c.y + r * 0.8),
                        ));
                        self.edges
                            .push(line(Point::new(bar, c.y), Point::new(c.x - r, c.y)));
                        self.grow(Rect::new(bar - 1.0, c.y - r, 1.0, ICON));
                    }
                    ParticipantKind::Entity => {
                        self.edges.push(line(
                            Point::new(c.x - r, c.y + r + 2.0),
                            Point::new(c.x + r, c.y + r + 2.0),
                        ));
                    }
                    _ => {
                        // Control: an arrowhead on the circle's top.
                        let top = c.y - r;
                        self.edges.push(line(
                            Point::new(c.x + 4.0, top - 4.0),
                            Point::new(c.x - 2.0, top),
                        ));
                        self.edges.push(line(
                            Point::new(c.x + 4.0, top + 4.0),
                            Point::new(c.x - 2.0, top),
                        ));
                    }
                }
                self.nodes.push(circle);
                let mut y = rect.y + ICON + 6.0;
                for line in self.label_lines(i) {
                    self.texts.push(Text {
                        pos: Point::new(c.x, y + FONT_SIZE * 0.95),
                        anchor: Anchor::Middle,
                        line,
                        paint: Paint::Role(Role::Text),
                        backed: false,
                    });
                    y += self.line_height;
                }
            }
            kind => {
                if kind == ParticipantKind::Collections {
                    let back = Rect::new(rect.x + 5.0, rect.y - 5.0, rect.width, rect.height);
                    self.nodes
                        .push(Node::new(format!("{id}#stack"), Shape::Rect, back));
                    self.grow(back);
                }
                let mut node = Node::new(id, node_shape(kind), rect);
                node.label = self.label_lines(i);
                self.nodes.push(node);
            }
        }
        self.grow(rect);
    }

    /// Place the columns: wide enough for each participant, then for every
    /// label, self-message and note that sits between two of them.
    fn columns(&mut self) {
        let n = self.seq.participants.len();
        if n == 0 {
            return;
        }
        let group = |i: usize| self.seq.participants[i].group;
        let mut gaps: Vec<f32> = (0..n.saturating_sub(1))
            .map(|i| {
                let mut gap = (self.heads[i].width + self.heads[i + 1].width) / 2.0 + COLUMN_GAP;
                if group(i) != group(i + 1) {
                    gap +=
                        group(i).map_or(0.0, |_| BOX_PAD) + group(i + 1).map_or(0.0, |_| BOX_PAD);
                }
                gap
            })
            .collect();

        // (left column, right column, the distance their centres need).
        let mut needs: Vec<(usize, usize, f32)> = Vec::new();
        for (event, lines) in self.seq.events.iter().zip(&self.lines) {
            let w = self.label_widest(lines);
            match event {
                Event::Message(m) if m.from == m.to => {
                    if m.from + 1 < n {
                        needs.push((m.from, m.from + 1, w.max(SELF_LOOP) + 30.0));
                    }
                }
                Event::Message(m) => {
                    let (lo, hi) = (m.from.min(m.to), m.from.max(m.to));
                    let number = if m.number.is_some() { 24.0 } else { 0.0 };
                    needs.push((lo, hi, w + 24.0 + number));
                }
                Event::Note(note) => {
                    let width = w + 24.0;
                    let beside = width + 2.0 * NOTE_GAP + BAR_WIDTH;
                    match note.side {
                        NoteSide::Right if note.first + 1 < n => {
                            needs.push((note.first, note.first + 1, beside));
                        }
                        NoteSide::Left if note.first > 0 => {
                            needs.push((note.first - 1, note.first, beside));
                        }
                        NoteSide::Over if note.first != note.last => {
                            let (lo, hi) = (note.first.min(note.last), note.first.max(note.last));
                            needs.push((lo, hi, width - 40.0));
                        }
                        NoteSide::Over => {
                            let i = note.first;
                            if i > 0 {
                                needs.push((i - 1, i, width / 2.0 + 10.0));
                            }
                            if i + 1 < n {
                                needs.push((i, i + 1, width / 2.0 + 10.0));
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        needs.sort_by_key(|&(lo, hi, _)| hi - lo);
        for (lo, hi, need) in needs {
            let have: f32 = gaps[lo..hi].iter().sum();
            if have < need {
                let extra = (need - have) / (hi - lo) as f32;
                for gap in &mut gaps[lo..hi] {
                    *gap += extra;
                }
            }
        }
        for i in 1..n {
            self.x[i] = self.x[i - 1] + gaps[i - 1];
        }
    }

    // ---- the pass down the page ---------------------------------------

    fn events(&mut self) {
        let row = self
            .seq
            .participants
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.created)
            .map(|(i, _)| self.heads[i].height)
            .fold(0.0, f32::max);
        for i in 0..self.seq.participants.len() {
            if self.seq.participants[i].created {
                continue;
            }
            let h = &self.heads[i];
            let rect = Rect::new(self.x[i] - h.width / 2.0, row - h.height, h.width, h.height);
            self.draw_head(i, rect, "");
            self.alive_from[i] = row;
        }
        self.cursor = row + 10.0;
        for (g, group) in self.seq.boxes.iter().enumerate() {
            if group.fill.is_none() {
                continue;
            }
            let extent = (0..self.x.len())
                .filter(|&i| self.seq.participants[i].group == Some(g))
                .map(|i| {
                    let half = self.heads[i].width / 2.0 + BOX_PAD;
                    (self.x[i] - half, self.x[i] + half)
                })
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));
            self.tints.extend(extent);
        }

        for (index, event) in self.seq.events.iter().enumerate() {
            let lines = self.lines[index].clone();
            match event {
                Event::Message(m) => self.message(m, &lines, index),
                Event::Note(n) => self.note(n, &lines, index),
                Event::Activate(who) => {
                    let start = self.last_message.unwrap_or(self.cursor);
                    self.active[*who].push(start);
                }
                Event::Deactivate(who) => {
                    let end = self.last_message.unwrap_or(self.cursor);
                    self.close_bar(*who, end);
                }
                Event::Start(frame, label) => self.open_frame(*frame, label),
                Event::Section(label) => self.section(label),
                Event::End => self.close_frame(),
            }
        }

        for who in 0..self.active.len() {
            while !self.active[who].is_empty() {
                let end = self.cursor;
                self.close_bar(who, end);
            }
        }
    }

    /// Where a message meets participant `i`'s lifeline, arriving from
    /// the right if `from_right`. `extra` counts an activation that starts
    /// with this message.
    fn lifeline_x(&self, i: usize, from_right: bool, extra: usize) -> f32 {
        let level = self.active[i].len() + extra;
        if level == 0 {
            return self.x[i];
        }
        let centre = self.x[i] + (level - 1) as f32 * BAR_WIDTH / 2.0;
        if from_right {
            centre + BAR_WIDTH / 2.0
        } else {
            centre - BAR_WIDTH / 2.0
        }
    }

    fn message(&mut self, m: &Message, lines: &[String], index: usize) {
        let label_h = self.text_height(lines);
        let label_w = self.label_widest(lines);
        let lift = if lines.is_empty() {
            14.0
        } else {
            8.0 + label_h + LABEL_GAP
        };
        let created = m.creates.then(|| self.heads[m.to].height);
        let line_y = self.cursor + lift.max(created.map_or(0.0, |h| h / 2.0 + 4.0));
        let label_y = line_y - LABEL_GAP - label_h / 2.0;

        let rightward = self.x[m.to] >= self.x[m.from];
        let from_x = self.lifeline_x(m.from, rightward, 0);
        let (path, label_x, x0, x1) = if m.from == m.to {
            let sx = self.lifeline_x(m.from, true, 0);
            let ex = self.lifeline_x(m.from, true, usize::from(m.activate));
            let y = line_y;
            let path = vec![
                Seg::Move(Point::new(sx, y)),
                Seg::Cubic(
                    Point::new(sx + 60.0, y - 10.0),
                    Point::new(ex + 60.0, y + SELF_DROP + 10.0),
                    Point::new(ex, y + SELF_DROP),
                ),
            ];
            let label_x = sx + 8.0 + label_w / 2.0;
            (path, label_x, sx, (sx + SELF_LOOP).max(sx + 8.0 + label_w))
        } else {
            let to_x = match created {
                Some(_) => {
                    let half = self.heads[m.to].width / 2.0;
                    if rightward {
                        self.x[m.to] - half
                    } else {
                        self.x[m.to] + half
                    }
                }
                None => self.lifeline_x(m.to, !rightward, usize::from(m.activate)),
            };
            // A start arrowhead would hide under the number's circle.
            // Likewise under a `()` dot, at either end.
            let dir = if rightward { 1.0 } else { -1.0 };
            let inset = if m.number.is_some() && m.start != Head::None {
                12.0
            } else if m.dot_from {
                5.0
            } else {
                0.0
            };
            let outset = if m.dot_to { 5.0 } else { 0.0 };
            let path = vec![
                Seg::Move(Point::new(from_x + dir * inset, line_y)),
                Seg::Line(Point::new(to_x - dir * outset, line_y)),
            ];
            let (lo, hi) = (from_x.min(to_x), from_x.max(to_x));
            let mid = (from_x + to_x) / 2.0;
            (
                path,
                mid,
                lo.min(mid - label_w / 2.0),
                hi.max(mid + label_w / 2.0),
            )
        };

        let mut edge = Edge::new(path).between(
            &self.seq.participants[m.from].id,
            &self.seq.participants[m.to].id,
        );
        edge.pen = Pen {
            dash: if m.dashed { Dash::Dashed } else { Dash::Solid },
            weight: Weight::Normal,
            paint: Paint::Role(Role::Line),
        };
        edge.start = marker(m.start);
        edge.end = marker(m.end);
        // Labels are free text on a backing, so a lifeline they cross
        // does not strike them through; on a tinted panel the backing
        // would show as a white patch, so there they go without.
        let backed = !self.tinted(label_x - label_w / 2.0, label_x + label_w / 2.0);
        let mut top = label_y - label_h / 2.0;
        for line in lines {
            self.texts.push(Text {
                pos: Point::new(
                    label_x,
                    top + (self.line_height - FONT_SIZE) / 2.0 + FONT_SIZE * 0.8,
                ),
                anchor: Anchor::Middle,
                line: TextLine::new(line.clone(), Font::Body),
                paint: Paint::Role(Role::Muted),
                backed,
            });
            top += self.line_height;
        }
        let number_r = FONT_SIZE * 0.75;
        if let Some(n) = m.number {
            edge.step = Some((n, Point::new(from_x, line_y)));
            self.grow(Rect::centered(
                Point::new(from_x, line_y),
                number_r * 2.0,
                number_r * 2.0,
            ));
        }
        self.edges.push(edge);
        let drop = if m.from == m.to { SELF_DROP } else { 0.0 };
        let top = line_y - LABEL_GAP - label_h;
        let x0 = x0 - if m.number.is_some() { number_r } else { 0.0 };
        self.grow(Rect::new(x0, top, x1 - x0, line_y + drop - top + 1.0));
        self.touch(x0, x1);

        for (dot, who) in [(m.dot_from, m.from), (m.dot_to, m.to)] {
            if dot {
                let c = Point::new(self.x[who], line_y);
                self.nodes.push(Node::new(
                    format!("dot#{index}#{who}"),
                    Shape::Start,
                    Rect::centered(c, 10.0, 10.0),
                ));
            }
        }

        if let Some(h) = created {
            let to = m.to;
            let w = self.heads[to].width;
            let rect = Rect::new(self.x[to] - w / 2.0, line_y - h / 2.0, w, h);
            self.draw_head(to, rect, "");
            self.alive_from[to] = rect.bottom();
            self.touch(rect.x, rect.right());
        }
        // The cross goes just below the message, clear of its number.
        let cross_y = line_y + 16.0;
        for &who in &m.destroys {
            self.ended_at[who] = Some(cross_y);
            let c = Point::new(self.x[who], cross_y);
            let s = 9.0;
            for (a, b) in [
                (c.offset(-s, -s), c.offset(s, s)),
                (c.offset(-s, s), c.offset(s, -s)),
            ] {
                let mut e = Edge::new(vec![Seg::Move(a), Seg::Line(b)]);
                e.end = Marker::None;
                e.pen.weight = Weight::Thick;
                self.edges.push(e);
            }
            self.grow(Rect::centered(c, 2.0 * s, 2.0 * s));
        }

        let mut after = match created {
            Some(h) => (h / 2.0 + 8.0).max(12.0),
            None => 12.0,
        };
        if !m.destroys.is_empty() {
            after = after.max(cross_y - line_y + 9.0 + 8.0);
        }
        self.cursor = line_y + drop + after;
        self.last_message = Some(line_y);
    }

    fn note(&mut self, n: &Note, lines: &[String], index: usize) {
        let text_w = self.widest(lines);
        let width = (text_w + 24.0).max(60.0);
        let height = self.text_height(lines).max(self.line_height) + 16.0;
        let top = self.cursor + 4.0;
        let (lo, hi) = (n.first.min(n.last), n.first.max(n.last));
        let rect = match n.side {
            NoteSide::Right => Rect::new(
                self.lifeline_x(n.first, true, 0).max(self.x[n.first]) + NOTE_GAP,
                top,
                width,
                height,
            ),
            NoteSide::Left => Rect::new(
                self.lifeline_x(n.first, false, 0).min(self.x[n.first]) - NOTE_GAP - width,
                top,
                width,
                height,
            ),
            NoteSide::Over => {
                let (x0, x1) = (self.x[lo], self.x[hi]);
                let span = if lo == hi { 0.0 } else { x1 - x0 + 40.0 };
                let w = width.max(span);
                Rect::new((x0 + x1) / 2.0 - w / 2.0, top, w, height)
            }
        };
        let mut node = Node::new(format!("note#{index}"), Shape::Rect, rect);
        node.style.fill = Paint::Role(Role::Highlight);
        node.label = lines
            .iter()
            .map(|l| TextLine::new(l.clone(), Font::Body))
            .collect();
        self.nodes.push(node);
        self.grow(rect);
        self.touch(rect.x, rect.right());
        self.cursor = rect.bottom() + 8.0;
        self.last_message = None;
    }

    fn close_bar(&mut self, who: usize, end: f32) {
        let Some(start) = self.active[who].pop() else {
            return;
        };
        let level = self.active[who].len();
        let end = end.max(start + 12.0);
        let x = self.x[who] + level as f32 * BAR_WIDTH / 2.0 - BAR_WIDTH / 2.0;
        let rect = Rect::new(x, start, BAR_WIDTH, end - start);
        let kind = ContainerKind::Swimlane;
        let mut style = Style::for_container(&kind);
        style.fill = Paint::Role(Role::Surface);
        style.stroke = Paint::Role(Role::Line);
        self.bars.push(Container {
            rect,
            kind,
            label: Vec::new(),
            style,
        });
        self.grow(rect);
    }

    fn tab_width(frame: Frame) -> f32 {
        // As the SVG writer sizes the tab.
        14.0 + frame.operator().chars().count() as f32 * FONT_SIZE * 0.6
    }

    fn open_frame(&mut self, frame: Frame, label: &str) {
        let top = self.cursor + 4.0;
        let header = match frame {
            Frame::Rect(_) => 10.0,
            _ => FONT_SIZE * 1.4 + 8.0,
        };
        self.frames.push(OpenFrame {
            frame,
            order: self.frames_opened,
            top,
            label: label.to_string(),
            sections: Vec::new(),
            min_x: f32::INFINITY,
            max_x: f32::NEG_INFINITY,
        });
        self.frames_opened += 1;
        self.cursor = top + header;
        self.last_message = None;
    }

    fn section(&mut self, label: &str) {
        let y = self.cursor + 4.0;
        if let Some(frame) = self.frames.last_mut() {
            frame.sections.push((y, label.to_string()));
        }
        self.cursor = y + if label.is_empty() { 10.0 } else { 26.0 };
        self.last_message = None;
    }

    fn close_frame(&mut self) {
        let Some(open) = self.frames.pop() else {
            return;
        };
        let bottom = self.cursor + 6.0;
        let (mut x0, mut x1) = if open.min_x.is_finite() {
            (open.min_x - FRAME_PAD, open.max_x + FRAME_PAD)
        } else {
            let x = self.x.first().copied().unwrap_or(0.0);
            (x - 50.0, x + 50.0)
        };
        let tab = Self::tab_width(open.frame);
        let guard_w = |s: &str| {
            if s.is_empty() {
                0.0
            } else {
                self.label_width(&bracket(s)) + 6.0
            }
        };
        let mut need = match open.frame {
            Frame::Rect(_) => 0.0,
            _ => 2.0 * (tab + 6.0) + guard_w(&open.label),
        };
        for (_, label) in &open.sections {
            need = need.max(guard_w(label) + 20.0);
        }
        if x1 - x0 < need {
            let grow = (need - (x1 - x0)) / 2.0;
            x0 -= grow;
            x1 += grow;
        }
        let rect = Rect::new(x0, open.top, x1 - x0, bottom - open.top);
        let centre = rect.center().x;
        match open.frame {
            Frame::Rect(fill) => {
                let kind = ContainerKind::Group;
                let mut style = Style::for_container(&kind);
                style.fill = Paint::Rgb(fill.0, fill.1, fill.2);
                style.stroke = Paint::None;
                self.rects.push((
                    open.order,
                    Container {
                        rect,
                        kind,
                        label: Vec::new(),
                        style,
                    },
                ));
            }
            frame => {
                let kind = ContainerKind::Fragment {
                    operator: frame.operator().to_string(),
                    guards: open
                        .sections
                        .iter()
                        .map(|(y, _)| (*y, TextLine::default()))
                        .collect(),
                };
                let style = Style::for_container(&kind);
                self.fragments.push((
                    open.order,
                    Container {
                        rect,
                        kind,
                        label: Vec::new(),
                        style,
                    },
                ));
                let tab_h = FONT_SIZE * 1.4;
                let mut labels = vec![(open.top + tab_h * 0.72 + 1.0, open.label.clone())];
                labels.extend(
                    open.sections
                        .iter()
                        .map(|(y, l)| (*y + FONT_SIZE + 4.0, l.clone())),
                );
                let backed = !self.tinted(rect.x, rect.right());
                for (y, label) in labels {
                    if label.is_empty() {
                        continue;
                    }
                    self.texts.push(Text {
                        pos: Point::new(centre, y),
                        anchor: Anchor::Middle,
                        line: TextLine::new(bracket(&label), Font::Body),
                        paint: Paint::Role(Role::Text),
                        backed,
                    });
                }
            }
        }
        self.grow(rect);
        self.touch(rect.x, rect.right());
        self.cursor = bottom + 8.0;
        self.last_message = None;
    }

    // ---- the bottom, boxes, and the frame of the whole ----------------

    fn finish(mut self) -> Scene {
        let n = self.seq.participants.len();
        let footer_top = self.cursor + 10.0;
        let mut bottom = footer_top;
        for i in 0..n {
            if self.seq.participants[i].destroyed {
                continue;
            }
            let h = &self.heads[i];
            let rect = Rect::new(self.x[i] - h.width / 2.0, footer_top, h.width, h.height);
            bottom = bottom.max(rect.bottom());
            self.draw_head(i, rect, "#footer");
        }

        let mut lifelines = Vec::new();
        for i in 0..n {
            let start = self.alive_from[i];
            let end = self.ended_at[i].unwrap_or(footer_top);
            let kind = ContainerKind::Lifeline;
            lifelines.push(Container {
                rect: Rect::new(self.x[i], start, 0.0, (end - start).max(0.0)),
                style: Style::for_container(&kind),
                kind,
                label: Vec::new(),
            });
        }

        let head_top = self
            .seq
            .participants
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.created)
            .map(|(i, _)| self.alive_from[i] - self.heads[i].height)
            .fold(0.0, f32::min);
        for (g, group) in self.seq.boxes.iter().enumerate() {
            let members: Vec<usize> = (0..n)
                .filter(|&i| self.seq.participants[i].group == Some(g))
                .collect();
            if members.is_empty() {
                continue;
            }
            let x0 = members
                .iter()
                .map(|&i| self.x[i] - self.heads[i].width / 2.0)
                .fold(f32::INFINITY, f32::min)
                - BOX_PAD;
            let x1 = members
                .iter()
                .map(|&i| self.x[i] + self.heads[i].width / 2.0)
                .fold(f32::NEG_INFINITY, f32::max)
                + BOX_PAD;
            let label: Vec<TextLine> = group
                .label
                .as_deref()
                .map(|l| TextLine::lines(l, Font::Bold))
                .unwrap_or_default();
            let label_h = label.len() as f32 * self.line_height;
            let label_w = label
                .iter()
                .map(|l| diagram_scene::line_width(self.measure, l, FONT_SIZE))
                .fold(0.0, f32::max);
            let width = (x1 - x0).max(label_w + 20.0);
            let top = head_top - BOX_PAD - if label.is_empty() { 0.0 } else { label_h + 6.0 };
            let rect = Rect::new(x0, top, width, bottom + BOX_PAD - top);
            let kind = ContainerKind::Group;
            let mut style = Style::for_container(&kind);
            style.fill = group
                .fill
                .map_or(Paint::None, |(r, g, b)| Paint::Rgb(r, g, b));
            self.grow(rect);
            self.boxes.push(Container {
                rect,
                kind,
                label,
                style,
            });
        }

        self.rects.sort_by_key(|(order, _)| *order);
        self.fragments.sort_by_key(|(order, _)| *order);
        for t in &self.texts {
            let w = diagram_scene::line_width(self.measure, &t.line, FONT_SIZE).max(
                diagram_scene::line_width(&diagram_scene::Estimate, &t.line, FONT_SIZE),
            ) + 6.0;
            let x = match t.anchor {
                Anchor::Start => t.pos.x,
                Anchor::Middle => t.pos.x - w / 2.0,
                Anchor::End => t.pos.x - w,
            };
            let r = Rect::new(x, t.pos.y - FONT_SIZE, w, FONT_SIZE * 1.3);
            self.bounds = Some(self.bounds.map_or(r, |b| b.union(&r)));
        }

        let mut scene = Scene::new(0.0, 0.0);
        scene.font_size = FONT_SIZE;
        scene.containers = self.boxes;
        scene
            .containers
            .extend(self.rects.into_iter().map(|(_, c)| c));
        scene.containers.extend(lifelines);
        scene.containers.extend(self.bars);
        scene
            .containers
            .extend(self.fragments.into_iter().map(|(_, c)| c));
        scene.edges = self.edges;
        scene.nodes = self.nodes;
        scene.texts = self.texts;

        let bounds = self.bounds.unwrap_or_default();
        let title_h = if self.seq.title.is_some() {
            Font::Title.size(FONT_SIZE) + PADDING
        } else {
            0.0
        };
        let dx = PADDING - bounds.x;
        let dy = PADDING + title_h - bounds.y;
        shift(&mut scene, dx, dy);
        let mut width = bounds.width + 2.0 * PADDING;
        if let Some(title) = &self.seq.title {
            let title = title.replace('\n', " ");
            let w = self
                .measure
                .width(&title, Font::Title.size(FONT_SIZE), true);
            if w + 2.0 * PADDING > width {
                shift(&mut scene, (w + 2.0 * PADDING - width) / 2.0, 0.0);
                width = w + 2.0 * PADDING;
            }
            scene.title = Some(Text {
                pos: Point::new(width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
                anchor: Anchor::Middle,
                line: TextLine::new(title, Font::Title),
                paint: Paint::Role(Role::Text),
                backed: false,
            });
        }
        scene.width = width;
        scene.height = bounds.height + 2.0 * PADDING + title_h;
        scene
    }
}

fn bracket(label: &str) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!("[{label}]")
    }
}

fn node_shape(kind: ParticipantKind) -> Shape {
    match kind {
        ParticipantKind::Actor => Shape::Actor,
        ParticipantKind::Database => Shape::Cylinder,
        ParticipantKind::Queue => Shape::Queue,
        _ => Shape::Rect,
    }
}

fn marker(head: Head) -> Marker {
    match head {
        Head::None => Marker::None,
        Head::Arrow => Marker::Arrow,
        Head::Open => Marker::OpenArrow,
        Head::Cross => Marker::Cross,
    }
}

/// Move everything in `scene` by `(dx, dy)`.
fn shift(scene: &mut Scene, dx: f32, dy: f32) {
    let rect = |r: &mut Rect| {
        r.x += dx;
        r.y += dy;
    };
    let point = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for c in &mut scene.containers {
        rect(&mut c.rect);
        if let ContainerKind::Fragment { guards, .. } = &mut c.kind {
            for (y, _) in guards {
                *y += dy;
            }
        }
    }
    for e in &mut scene.edges {
        for seg in &mut e.path {
            match seg {
                Seg::Move(p) | Seg::Line(p) => point(p),
                Seg::Cubic(a, b, p) => {
                    point(a);
                    point(b);
                    point(p);
                }
                Seg::Close => {}
            }
        }
        for l in &mut e.labels {
            point(&mut l.pos);
        }
        if let Some((_, p)) = &mut e.step {
            point(p);
        }
    }
    for n in &mut scene.nodes {
        rect(&mut n.rect);
    }
    for t in &mut scene.texts {
        point(&mut t.pos);
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse::parse;
    use super::*;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    const SAMPLE: &str = "sequenceDiagram
    title Checkout
    actor U as Shopper
    participant W as Web<br>shop
    participant P@{ \"type\": \"database\" }
    autonumber
    U->>+W: Place order
    loop Every item
        W->>P: Reserve stock
        alt In stock
            P-->>W: Reserved
        else Sold out
            P--xW: Refused
        end
    end
    Note over W,P: Stock is held for ten minutes
    W->>W: Total up
    create participant Q as Mailer
    W-)Q: Send receipt
    destroy Q
    Q--)W: Sent
    W-->>-U: Confirmation
    Note left of U: Happy
";

    fn scene() -> Scene {
        layout(&parse(SAMPLE).unwrap(), &Fixed)
    }

    #[test]
    fn nothing_is_dropped() {
        let texts = scene().texts();
        for expected in [
            "Checkout",
            "Shopper",
            "Web",
            "shop",
            "P",
            "Mailer",
            "Place order",
            "Reserve stock",
            "Reserved",
            "Refused",
            "Total up",
            "Send receipt",
            "Sent",
            "Confirmation",
            "Stock is held for ten minutes",
            "Happy",
            "[Every item]",
            "[In stock]",
            "[Sold out]",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "missing {expected}: {texts:?}"
            );
        }
        let s = scene();
        let operators: Vec<_> = s
            .containers
            .iter()
            .filter_map(|c| match &c.kind {
                ContainerKind::Fragment { operator, .. } => Some(operator.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(operators, ["loop", "alt"], "outer frame paints first");
    }

    #[test]
    fn messages_run_down_the_page_in_order() {
        let s = scene();
        let ys: Vec<f32> = s
            .edges
            .iter()
            .filter(|e| e.from.is_some())
            .map(|e| match e.path[0] {
                Seg::Move(p) => p.y,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(ys.len(), 8);
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
        let numbers: Vec<u32> = s.edges.iter().filter_map(|e| e.step.map(|s| s.0)).collect();
        assert_eq!(numbers, (1..=8).collect::<Vec<_>>());
    }

    #[test]
    fn everything_is_inside_the_bounds() {
        let s = scene();
        let bounds = s.bounds();
        let mut all: Vec<Rect> = s.nodes.iter().map(|n| n.rect).collect();
        all.extend(s.containers.iter().map(|c| c.rect));
        for e in &s.edges {
            for poly in diagram_scene::flatten::flatten(&e.path, 0.5) {
                all.extend(poly.points.iter().map(|p| Rect::new(p.x, p.y, 0.0, 0.0)));
            }
            for l in &e.labels {
                all.push(Rect::centered(l.pos, l.width, 14.0));
            }
        }
        for r in all {
            assert!(bounds.encloses(&r, 0.01), "{r:?} escapes {bounds:?}");
        }
    }

    #[test]
    fn participants_are_mirrored_except_the_destroyed() {
        let s = scene();
        assert!(s.node("U#footer").is_some());
        assert!(s.node("P#footer").is_some());
        assert!(s.node("Q").is_some());
        assert!(s.node("Q#footer").is_none());
        // The created participant's head sits below the top row.
        assert!(s.node("Q").unwrap().rect.y > s.node("W").unwrap().rect.bottom());
    }

    #[test]
    fn columns_fit_their_labels() {
        let seq =
            parse("sequenceDiagram\nA->>B: a very very very very very very long message label\n")
                .unwrap();
        let s = layout(&seq, &Fixed);
        let (a, b) = (s.node("A").unwrap().rect, s.node("B").unwrap().rect);
        let label = &s
            .texts
            .iter()
            .find(|t| t.line.plain().contains("long"))
            .unwrap();
        let width = Fixed.width(&label.line.plain(), FONT_SIZE, false);
        assert!(b.center().x - a.center().x > width);
    }
}
