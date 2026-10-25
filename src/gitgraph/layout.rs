//! gitGraph layout: commits along a shared timeline, one lane per branch.
//!
//! `LR` runs time left to right with horizontal lanes, branch labels in a
//! gutter on the left, tags above a commit and its label below. `TB` and
//! `BT` run time down or up with vertical lanes, branch labels at the start
//! of the timeline, a commit's label to its left and its tags to its right —
//! the placement in mermaid's `gitGraphRenderer.ts`.

use diagram_scene::{
    Anchor, Dash, Edge, Font, Marker, Node, Paint, Pen, Point, Rect, Role, Scene, Seg, Shape, Text,
    TextLine, Weight,
};

use super::parse::{CommitType, Direction, GitGraph, Origin};
use crate::text::Measure;
use crate::{FONT_SIZE, PADDING};

const LANE_HEIGHT: f32 = 52.0;
const DOT_RADIUS: f32 = 9.0;
const MIN_SLOT: f32 = 68.0;
const LABEL_GAP: f32 = 16.0;
const TAG_GAP: f32 = 14.0;
/// Between stacked tags.
const TAG_STACK: f32 = 4.0;
/// Vertical: one commit per step down (or up) the timeline.
const STEP: f32 = 44.0;
/// Vertical: from a dot to its label or first tag.
const SIDE_GAP: f32 = 10.0;
const MIN_LANE_GAP: f32 = 64.0;

/// Lay a parsed graph out.
pub fn layout(graph: &GitGraph, measure: &dyn Measure) -> Scene {
    let mut scene = Scene::new(0.0, 0.0);
    scene.font_size = FONT_SIZE;

    let lanes = graph.lane_order();
    let mut lane_of = vec![0; graph.branches.len()];
    for (lane, &branch) in lanes.iter().enumerate() {
        lane_of[branch] = lane;
    }
    let ctx = Ctx {
        graph,
        measure,
        lanes,
        lane_of,
        label_size: Font::Small.size(FONT_SIZE),
    };

    let title_height = if graph.title.is_some() {
        Font::Title.size(FONT_SIZE) + PADDING
    } else {
        0.0
    };
    let top = PADDING + title_height;

    let (width, height) = match graph.direction {
        Direction::LR => horizontal(&ctx, &mut scene, top),
        Direction::TB => vertical(&ctx, &mut scene, top, false),
        Direction::BT => vertical(&ctx, &mut scene, top, true),
    };

    let mut width = width;
    if let Some(title) = &graph.title {
        let title_width = measure.width(title, Font::Title.size(FONT_SIZE), true);
        width = width.max(title_width + PADDING * 2.0);
        scene.title = Some(Text {
            pos: Point::new(width / 2.0, PADDING + Font::Title.size(FONT_SIZE)),
            anchor: Anchor::Middle,
            line: TextLine::new(title.clone(), Font::Title),
            paint: Paint::Role(Role::Text),
            backed: false,
            rotate: 0.0,
        });
    }
    scene.width = width.max(200.0);
    scene.height = height.max(80.0);
    scene
}

struct Ctx<'a> {
    graph: &'a GitGraph,
    measure: &'a dyn Measure,
    /// Branch indices, first lane first.
    lanes: Vec<usize>,
    /// Each branch's lane.
    lane_of: Vec<usize>,
    label_size: f32,
}

impl Ctx<'_> {
    fn lane(&self, commit: usize) -> usize {
        self.lane_of[self.graph.commits[commit].branch]
    }

    fn label_width(&self, commit: usize) -> f32 {
        self.graph.commits[commit]
            .label
            .as_ref()
            .map(|label| {
                // The SVG writer sizes a label's backing by its own estimate,
                // so make room for whichever is wider.
                let line = TextLine::new(label.clone(), Font::Small);
                let estimate =
                    diagram_scene::line_width(&diagram_scene::Estimate, &line, FONT_SIZE);
                self.measure
                    .width(label, self.label_size, false)
                    .max(estimate)
            })
            .unwrap_or(0.0)
    }

    fn tag_width(&self, tag: &str) -> f32 {
        self.measure.width(tag, self.label_size, true) + 14.0
    }

    fn tag_height(&self) -> f32 {
        self.label_size + 8.0
    }

    fn branch_label_size(&self, branch: usize) -> (f32, f32) {
        let name = &self.graph.branches[branch].name;
        (
            self.measure.width(name, FONT_SIZE, true) + 18.0,
            FONT_SIZE + 10.0,
        )
    }

    /// The branch's colour: by lane, so lanes read in order.
    fn color(&self, branch: usize) -> Paint {
        Paint::Role(Role::Series(self.lane_of[branch] as u8))
    }

    /// Mermaid colours a link by the child's branch, except the link from a
    /// merged branch, which keeps that branch's colour.
    fn edge_color(&self, child: usize, parent: usize) -> Paint {
        let commit = &self.graph.commits[child];
        let merged = commit.origin == Origin::Merge && commit.parents.first() != Some(&parent);
        let branch = if merged {
            self.graph.commits[parent].branch
        } else {
            commit.branch
        };
        self.color(branch)
    }
}

fn line(from: Point, to: Point) -> Vec<Seg> {
    vec![Seg::Move(from), Seg::Line(to)]
}

fn plain(path: Vec<Seg>, pen: Pen) -> Edge {
    Edge {
        pen,
        end: Marker::None,
        ..Edge::new(path)
    }
}

fn lane_rule(from: Point, to: Point) -> Edge {
    plain(
        line(from, to),
        Pen {
            dash: Dash::Dashed,
            weight: Weight::Thin,
            paint: Paint::Role(Role::Line),
        },
    )
}

fn thin(paint: Paint) -> Pen {
    Pen {
        dash: Dash::Solid,
        weight: Weight::Thin,
        paint,
    }
}

/// An S-curve between lanes that leaves and arrives along the timeline.
fn s_curve(from: Point, to: Point, horizontal: bool) -> Vec<Seg> {
    let (c1, c2) = if horizontal {
        let mid = (from.x + to.x) / 2.0;
        (Point::new(mid, from.y), Point::new(mid, to.y))
    } else {
        let mid = (from.y + to.y) / 2.0;
        (Point::new(from.x, mid), Point::new(to.x, mid))
    };
    vec![Seg::Move(from), Seg::Cubic(c1, c2, to)]
}

/// Links from each commit back to its parents.
///
/// Along a lane a link is straight. Between lanes it runs along whichever
/// lane is empty over its span and changes lane within one commit's step,
/// as mermaid does, so it never cuts diagonally across the commits and
/// labels in between. A branch's first commit turns off its parent at once
/// and runs along its own lane; a merged branch runs along its own lane and
/// turns in at the merge.
fn links(ctx: &Ctx, scene: &mut Scene, positions: &[Point], horizontal: bool) {
    let lane_free =
        |lane: usize, from: usize, to: usize| !(from + 1..to).any(|k| ctx.lane(k) == lane);
    // A point on the lane through `on`, at the time of commit `at`.
    let at_time = |on: Point, at: usize| {
        if horizontal {
            Point::new(positions[at].x, on.y)
        } else {
            Point::new(on.x, positions[at].y)
        }
    };
    for (index, commit) in ctx.graph.commits.iter().enumerate() {
        let to = positions[index];
        for &parent in &commit.parents {
            let from = positions[parent];
            let early = lane_free(ctx.lane(index), parent, index);
            let late = lane_free(ctx.lane(parent), parent, index);
            let second = commit.parents.first() != Some(&parent);
            let turn_early = if second { early && !late } else { early };
            let path = if ctx.lane(parent) == ctx.lane(index) {
                line(from, to)
            } else if index == parent + 1 || !(early || late) {
                s_curve(from, to, horizontal)
            } else if turn_early {
                let mut path = s_curve(from, at_time(to, parent + 1), horizontal);
                path.push(Seg::Line(to));
                path
            } else {
                let turn = at_time(from, index - 1);
                let mut path = line(from, turn);
                path.extend(s_curve(turn, to, horizontal).into_iter().skip(1));
                path
            };
            let mut edge = plain(
                path,
                Pen {
                    dash: Dash::Solid,
                    weight: Weight::Heavy,
                    paint: ctx.edge_color(index, parent),
                },
            );
            edge.from = Some(format!("commit:{parent}"));
            edge.to = Some(format!("commit:{index}"));
            scene.edges.push(edge);
        }
    }
}

fn dot(at: Point, r: f32) -> Rect {
    Rect::centered(at, r * 2.0, r * 2.0)
}

/// A commit's mark: its dot and whatever distinguishes its type.
fn mark(ctx: &Ctx, scene: &mut Scene, index: usize, at: Point) {
    let commit = &ctx.graph.commits[index];
    let color = ctx.color(commit.branch);
    let id = format!("commit:{index}");
    let filled = |id: String, shape: Shape, rect: Rect, fill: Paint| {
        let mut node = Node::new(id, shape, rect);
        node.style.fill = fill;
        node.style.stroke = Paint::None;
        node
    };

    match commit.kind {
        CommitType::Highlight => {
            let mut node = Node::new(id, Shape::Rect, dot(at, DOT_RADIUS));
            node.style.fill = color;
            node.style.stroke = Paint::Role(Role::Text);
            node.style.emphasis = true;
            scene.nodes.push(node);
        }
        CommitType::Reverse => {
            let mut node = Node::new(id.clone(), Shape::Circle, dot(at, DOT_RADIUS));
            node.style.fill = Paint::Role(Role::Background);
            node.style.stroke = color;
            node.style.emphasis = true;
            scene.nodes.push(node);
            // The cross is drawn as a small node over the ring, so it
            // paints above it.
            let arm = DOT_RADIUS * 0.55;
            let mut cross = Node::new(format!("{id}:reverse"), Shape::Rect, dot(at, arm));
            cross.style.fill = Paint::None;
            cross.style.stroke = Paint::None;
            cross.label = vec![TextLine::new("\u{00d7}", Font::Bold)];
            cross.style.text = color;
            scene.nodes.push(cross);
        }
        CommitType::Normal | CommitType::Merge | CommitType::CherryPick => {
            let mut node = Node::new(id.clone(), Shape::Circle, dot(at, DOT_RADIUS));
            node.style.fill = color;
            node.style.stroke = color;
            scene.nodes.push(node);
            let background = Paint::Role(Role::Background);
            if commit.kind == CommitType::Merge {
                // A merge reads as a ring, as it does in mermaid.
                scene.nodes.push(filled(
                    format!("{id}:merge"),
                    Shape::Circle,
                    dot(at, DOT_RADIUS * 0.45),
                    background,
                ));
            }
            if commit.kind == CommitType::CherryPick {
                // Mermaid's cherry: two small fruit on a stem.
                scene.nodes.push(filled(
                    format!("{id}:stem"),
                    Shape::Rect,
                    Rect::new(at.x - 0.7, at.y - 5.5, 1.4, 6.5),
                    background,
                ));
                for (side, dx) in [("left", -3.0), ("right", 3.0)] {
                    scene.nodes.push(filled(
                        format!("{id}:cherry-{side}"),
                        Shape::Circle,
                        dot(Point::new(at.x + dx, at.y + 2.0), 2.6),
                        background,
                    ));
                }
            }
        }
    }
}

fn tag_node(ctx: &Ctx, index: usize, k: usize, tag: &str, rect: Rect) -> Node {
    let color = ctx.color(ctx.graph.commits[index].branch);
    let mut node = Node::new(format!("tag:{index}:{k}"), Shape::Rounded, rect);
    node.label = vec![TextLine::new(tag.to_string(), Font::Small)];
    node.style.fill = Paint::Role(Role::Background);
    node.style.stroke = color;
    node.style.text = color;
    node
}

fn branch_label(ctx: &Ctx, branch: usize, rect: Rect) -> Node {
    let color = ctx.color(branch);
    let mut node = Node::new(format!("branch:{branch}"), Shape::Rounded, rect)
        .with_label(&ctx.graph.branches[branch].name);
    node.style.fill = color;
    node.style.stroke = color;
    node.style.text = Paint::Role(Role::Background);
    node
}

fn commit_label(ctx: &Ctx, index: usize, pos: Point, anchor: Anchor) -> Option<Text> {
    let label = ctx.graph.commits[index].label.as_ref()?;
    Some(Text {
        pos,
        anchor,
        line: TextLine::new(label.clone(), Font::Small),
        paint: Paint::Role(Role::Muted),
        // Merge curves can cross a label.
        backed: true,
        rotate: 0.0,
    })
}

/// Left to right. Returns the scene's size.
fn horizontal(ctx: &Ctx, scene: &mut Scene, top: f32) -> (f32, f32) {
    let graph = ctx.graph;
    let tag_height = ctx.tag_height();
    let (_, branch_height) = ctx.branch_label_size(0);

    // Branch labels sit in a gutter to the left of the timeline.
    let gutter = graph
        .branches
        .iter()
        .map(|branch| ctx.measure.width(&branch.name, FONT_SIZE, true) + 20.0)
        .fold(0.0_f32, f32::max);
    let origin_x = gutter + PADDING * 2.0;

    // Tags stack above their commit, so the first lane must sit low enough
    // for the tallest stack — wherever its lane is — to stay in bounds.
    let stack = |tags: usize| {
        if tags == 0 {
            0.0
        } else {
            TAG_GAP + tags as f32 * tag_height + (tags - 1) as f32 * TAG_STACK
        }
    };
    let headroom = graph
        .commits
        .iter()
        .enumerate()
        .map(|(index, commit)| {
            DOT_RADIUS + stack(commit.tags.len()) - ctx.lane(index) as f32 * LANE_HEIGHT
        })
        .fold(DOT_RADIUS.max(branch_height / 2.0), f32::max);
    let origin_y = top + headroom;
    let lane_y = |lane: usize| origin_y + lane as f32 * LANE_HEIGHT;

    // Each commit gets a slot wide enough for its own label and tags.
    let mut positions = Vec::with_capacity(graph.commits.len());
    let mut x = origin_x;
    for (index, commit) in graph.commits.iter().enumerate() {
        let tags = commit
            .tags
            .iter()
            .map(|tag| ctx.tag_width(tag))
            .fold(0.0_f32, f32::max);
        let slot = MIN_SLOT.max(ctx.label_width(index) + 18.0).max(tags + 18.0);
        x += slot / 2.0;
        positions.push(Point::new(x, lane_y(ctx.lane(index))));
        x += slot / 2.0;
    }
    let timeline_end = x + PADDING;

    // Lane rules first, so commits and links paint over them.
    for lane in 0..ctx.lanes.len() {
        let y = lane_y(lane);
        scene.edges.push(lane_rule(
            Point::new(origin_x - PADDING, y),
            Point::new(timeline_end, y),
        ));
    }
    links(ctx, scene, &positions, true);

    for (index, commit) in graph.commits.iter().enumerate() {
        let at = positions[index];
        mark(ctx, scene, index, at);
        let label = Point::new(at.x, at.y + DOT_RADIUS + LABEL_GAP);
        scene
            .texts
            .extend(commit_label(ctx, index, label, Anchor::Middle));

        if commit.tags.is_empty() {
            continue;
        }
        // A stem joining the nearest tag to its commit, under the tags.
        let nearest = at.y - DOT_RADIUS - TAG_GAP;
        scene.edges.push(plain(
            line(
                Point::new(at.x, nearest),
                Point::new(at.x, at.y - DOT_RADIUS),
            ),
            thin(ctx.color(commit.branch)),
        ));
        // As in mermaid, the last tag sits nearest the commit.
        for (k, tag) in commit.tags.iter().rev().enumerate() {
            let width = ctx.tag_width(tag);
            let bottom = nearest - k as f32 * (tag_height + TAG_STACK);
            let rect = Rect::new(at.x - width / 2.0, bottom - tag_height, width, tag_height);
            scene.nodes.push(tag_node(ctx, index, k, tag, rect));
        }
    }

    // Branch labels in the gutter.
    for (lane, &branch) in ctx.lanes.iter().enumerate() {
        let y = lane_y(lane);
        let (width, height) = ctx.branch_label_size(branch);
        let rect = Rect::new(PADDING, y - height / 2.0, width, height);
        scene.nodes.push(branch_label(ctx, branch, rect));
    }

    let height = lane_y(ctx.lanes.len().saturating_sub(1))
        + DOT_RADIUS
        + LABEL_GAP
        + ctx.label_size
        + PADDING;
    (timeline_end, height)
}

/// Top to bottom, or bottom to top when `upward`. Returns the scene's size.
fn vertical(ctx: &Ctx, scene: &mut Scene, top: f32, upward: bool) -> (f32, f32) {
    let graph = ctx.graph;
    let lanes = ctx.lanes.len();
    let tag_height = ctx.tag_height();
    let branch_height = ctx.branch_label_size(0).1;

    // How far each lane's labels reach to its left and its tags to its
    // right, from the lane's centre line.
    let mut left = vec![DOT_RADIUS; lanes];
    let mut right = vec![DOT_RADIUS; lanes];
    for (index, commit) in graph.commits.iter().enumerate() {
        let lane = ctx.lane(index);
        if commit.label.is_some() {
            // The label's backing reaches 3px past its text.
            let reach = DOT_RADIUS + SIDE_GAP + ctx.label_width(index) + 3.0;
            left[lane] = left[lane].max(reach);
        }
        if !commit.tags.is_empty() {
            let tags: f32 = commit.tags.iter().map(|tag| ctx.tag_width(tag)).sum();
            let reach = DOT_RADIUS + SIDE_GAP + tags + (commit.tags.len() - 1) as f32 * TAG_STACK;
            right[lane] = right[lane].max(reach);
        }
    }
    let half_label = |lane: usize| ctx.branch_label_size(ctx.lanes[lane]).0 / 2.0;

    // Commits on neighbouring lanes never share a step, so a lane's labels
    // only need to clear the dots of the lane to their left, and its tags
    // those of the lane to their right.
    let mut lane_x = Vec::with_capacity(lanes);
    let mut x = PADDING + left.first().copied().unwrap_or(0.0).max(half_label(0));
    for lane in 0..lanes {
        if lane > 0 {
            let gap = MIN_LANE_GAP
                .max(right[lane - 1] + DOT_RADIUS + 6.0)
                .max(left[lane] + DOT_RADIUS + 6.0)
                .max(half_label(lane - 1) + half_label(lane) + 10.0);
            x += gap;
        }
        lane_x.push(x);
    }
    let width = lane_x
        .iter()
        .enumerate()
        .map(|(lane, x)| x + right[lane].max(half_label(lane)))
        .fold(0.0_f32, f32::max)
        + PADDING;

    // The timeline: branch labels at its start, commits one step apart.
    let count = graph.commits.len();
    let span = count.saturating_sub(1) as f32 * STEP;
    // Room beside a dot for a label or tag that is taller than the dot.
    let reach = DOT_RADIUS.max(tag_height / 2.0) + 4.0;
    let (labels_y, first, rule_from, rule_to, height);
    if upward {
        let newest = top + reach;
        first = newest + span;
        rule_from = top;
        rule_to = first + DOT_RADIUS + 12.0;
        labels_y = rule_to;
        height = labels_y + branch_height + PADDING;
    } else {
        labels_y = top;
        rule_from = top + branch_height;
        first = rule_from + 12.0 + reach;
        rule_to = first + span + reach;
        height = rule_to + PADDING;
    }
    let step = if upward { -STEP } else { STEP };
    let positions: Vec<Point> = (0..count)
        .map(|index| Point::new(lane_x[ctx.lane(index)], first + index as f32 * step))
        .collect();

    for &x in &lane_x {
        scene
            .edges
            .push(lane_rule(Point::new(x, rule_from), Point::new(x, rule_to)));
    }
    links(ctx, scene, &positions, false);

    let baseline = ctx.label_size * 0.35;
    for (index, commit) in graph.commits.iter().enumerate() {
        let at = positions[index];
        mark(ctx, scene, index, at);
        let label = Point::new(at.x - DOT_RADIUS - SIDE_GAP, at.y + baseline);
        scene
            .texts
            .extend(commit_label(ctx, index, label, Anchor::End));

        if commit.tags.is_empty() {
            continue;
        }
        let mut x = at.x + DOT_RADIUS + SIDE_GAP;
        scene.edges.push(plain(
            line(Point::new(at.x + DOT_RADIUS, at.y), Point::new(x, at.y)),
            thin(ctx.color(commit.branch)),
        ));
        // As in mermaid, the last tag sits nearest the commit.
        for (k, tag) in commit.tags.iter().rev().enumerate() {
            let width = ctx.tag_width(tag);
            let rect = Rect::new(x, at.y - tag_height / 2.0, width, tag_height);
            scene.nodes.push(tag_node(ctx, index, k, tag, rect));
            x += width + TAG_STACK;
        }
    }

    for (lane, &branch) in ctx.lanes.iter().enumerate() {
        let (width, height) = ctx.branch_label_size(branch);
        let rect = Rect::new(lane_x[lane] - width / 2.0, labels_y, width, height);
        scene.nodes.push(branch_label(ctx, branch, rect));
    }

    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gitgraph::parse;

    struct Fixed;
    impl Measure for Fixed {
        fn width(&self, text: &str, size: f32, _bold: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    fn scene(source: &str) -> Scene {
        layout(&parse(source).unwrap(), &Fixed)
    }

    const SAMPLE: &str = "\
    commit id: \"init\" tag: \"v0\"
    branch dev
    commit id: \"feature\" tag: \"rc1\" tag: \"rc2\"
    checkout main
    commit id: \"hotfix\"
    checkout dev
    cherry-pick id: \"hotfix\"
    checkout main
    merge dev id: \"release\" tag: \"v1\"
";

    fn with_header(header: &str) -> String {
        format!("{header}\n{SAMPLE}")
    }

    fn commit(scene: &Scene, index: usize) -> Rect {
        scene.node(&format!("commit:{index}")).unwrap().rect
    }

    fn branch(scene: &Scene, index: usize) -> Rect {
        scene.node(&format!("branch:{index}")).unwrap().rect
    }

    /// Everything drawn, including text, as boxes.
    fn drawn(scene: &Scene) -> Vec<Rect> {
        let mut all: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
        for edge in &scene.edges {
            for poly in diagram_scene::flatten::flatten(&edge.path, 0.5) {
                all.extend(poly.points.iter().map(|p| Rect::new(p.x, p.y, 0.0, 0.0)));
            }
        }
        for text in &scene.texts {
            let size = text.line.font().size(FONT_SIZE);
            let width = Fixed.width(&text.line.plain(), size, false) + 6.0;
            let x = match text.anchor {
                Anchor::Start => text.pos.x - 3.0,
                Anchor::Middle => text.pos.x - width / 2.0,
                Anchor::End => text.pos.x - width + 3.0,
            };
            all.push(Rect::new(x, text.pos.y - size, width, size * 1.3));
        }
        all
    }

    fn assert_in_bounds(scene: &Scene, what: &str) {
        let bounds = scene.bounds();
        for rect in drawn(scene) {
            assert!(
                bounds.encloses(&rect, 0.01),
                "{what}: {rect:?} escapes {bounds:?}"
            );
        }
    }

    #[test]
    fn everything_stays_in_bounds_in_every_direction() {
        for header in ["gitGraph", "gitGraph TB:", "gitGraph BT:"] {
            assert_in_bounds(&scene(&with_header(header)), header);
            // A tall tag stack on a lower lane, with a title.
            let source = format!(
                "{header}\n  title Tags\n  commit\n  branch dev\n  commit tag: \"a\" tag: \"b\" tag: \"c\"\n"
            );
            assert_in_bounds(&scene(&source), &source);
        }
    }

    #[test]
    fn nothing_is_dropped_in_any_direction() {
        for header in ["gitGraph", "gitGraph TB:", "gitGraph BT:"] {
            let scene = scene(&with_header(header));
            let texts = scene.texts();
            for expected in [
                "init",
                "feature",
                "hotfix",
                "release",
                "v0",
                "rc1",
                "rc2",
                "v1",
                "cherry-pick:hotfix",
                "main",
                "dev",
            ] {
                assert!(
                    texts.iter().any(|t| t == expected),
                    "{header}: missing {expected}: {texts:?}"
                );
            }
            for index in 0..5 {
                assert!(scene.node(&format!("commit:{index}")).is_some());
            }
            // Each parent link is drawn: 4 single-parent commits, two
            // two-parent ones.
            let links = scene.edges.iter().filter(|e| e.from.is_some()).count();
            assert_eq!(links, 6, "{header}");
        }
    }

    #[test]
    fn left_to_right_runs_time_along_x() {
        let scene = scene(&with_header("gitGraph"));
        for index in 1..5 {
            assert!(commit(&scene, index).x > commit(&scene, index - 1).x);
        }
        // dev is the second lane, below main.
        assert!(commit(&scene, 1).y > commit(&scene, 0).y);
        // Branch labels in the left gutter.
        assert!(branch(&scene, 0).right() < commit(&scene, 0).x);
    }

    #[test]
    fn top_to_bottom_runs_time_down_with_lanes_left_to_right() {
        let scene = scene(&with_header("gitGraph TB:"));
        for index in 1..5 {
            assert!(
                commit(&scene, index).y > commit(&scene, index - 1).y,
                "commit {index}"
            );
        }
        let (main, dev) = (commit(&scene, 0), commit(&scene, 1));
        assert!(dev.x > main.x, "dev's lane is right of main's");
        assert_eq!(commit(&scene, 2).x, main.x, "hotfix stays in main's lane");
        // Branch labels head their lanes, above every commit.
        for (index, lane) in [(0, main), (1, dev)] {
            let label = branch(&scene, index);
            assert!(
                label.bottom() < main.y,
                "label {index} above the first commit"
            );
            assert!((label.center().x - lane.center().x).abs() < 0.01);
        }
        // Labels to the left of the dot, tags to its right.
        let label = scene
            .texts
            .iter()
            .find(|t| t.line.plain() == "init")
            .unwrap();
        assert!(label.pos.x < main.x);
        assert_eq!(label.anchor, Anchor::End);
        let tag = scene.node("tag:0:0").unwrap().rect;
        assert!(tag.x > main.right());
        assert!(tag.y < main.center().y && tag.bottom() > main.center().y);
    }

    #[test]
    fn bottom_to_top_runs_time_up_with_labels_at_the_bottom() {
        let scene = scene(&with_header("gitGraph BT:"));
        for index in 1..5 {
            assert!(commit(&scene, index).y < commit(&scene, index - 1).y);
        }
        let first = commit(&scene, 0);
        assert!(
            branch(&scene, 0).y > first.bottom(),
            "labels below the oldest commit"
        );
        assert!(commit(&scene, 1).x > first.x);
    }

    #[test]
    fn vertical_labels_and_tags_never_cover_another_lane_dot() {
        for header in ["gitGraph TB:", "gitGraph BT:"] {
            let scene = scene(&with_header(header));
            let dots: Vec<Rect> = (0..5).map(|index| commit(&scene, index)).collect();
            let tags = scene.nodes.iter().filter(|n| n.id.starts_with("tag:"));
            for tag in tags {
                for dot in &dots {
                    assert!(!tag.rect.intersects(dot), "{header}: {} on a dot", tag.id);
                }
            }
            // A dot's label stops short of the lane to its left.
            let dev_label = scene
                .texts
                .iter()
                .find(|t| t.line.plain() == "feature")
                .unwrap();
            let width = Fixed.width("feature", Font::Small.size(FONT_SIZE), false);
            assert!(dev_label.pos.x - width > dots[0].right(), "{header}");
        }
    }

    #[test]
    fn branch_order_sets_the_lanes() {
        let source = "gitGraph TB:\n  commit\n  branch b order: 2\n  commit\n  branch a order: 1\n  commit\n";
        let vertical = scene(source);
        let (main, b, a) = (
            commit(&vertical, 0),
            commit(&vertical, 1),
            commit(&vertical, 2),
        );
        assert!(
            main.x < a.x && a.x < b.x,
            "main, a, b: {main:?} {a:?} {b:?}"
        );
        let horizontal = scene(&source.replace("gitGraph TB:", "gitGraph"));
        assert!(
            commit(&horizontal, 2).y < commit(&horizontal, 1).y,
            "a above b"
        );
    }

    #[test]
    fn cherry_pick_gets_its_own_mark_and_no_label() {
        let scene = scene(&with_header("gitGraph"));
        assert!(scene.node("commit:3:stem").is_some());
        assert!(scene.node("commit:3:cherry-left").is_some());
        // The pick links back to its source, the hotfix commit.
        assert!(scene.edges.iter().any(|e| {
            e.from.as_deref() == Some("commit:2") && e.to.as_deref() == Some("commit:3")
        }));
        assert!(scene.node("commit:4:merge").is_some(), "release is a ring");
    }

    #[test]
    fn merge_with_a_type_draws_that_type() {
        let base = "gitGraph\n  commit\n  branch dev\n  commit\n  checkout main\n";
        let ring = scene(&format!("{base}  merge dev\n"));
        assert!(ring.node("commit:2:merge").is_some());
        let reverse = scene(&format!("{base}  merge dev type: REVERSE\n"));
        assert!(reverse.node("commit:2:merge").is_none());
        assert!(reverse.node("commit:2:reverse").is_some());
    }

    #[test]
    fn a_title_is_drawn_above_the_graph() {
        for header in ["gitGraph", "gitGraph TB:", "gitGraph BT:"] {
            let scene = scene(&format!(
                "{header}\n  title Releases\n  commit\n  commit tag: \"v1\"\n"
            ));
            let title = scene.title.as_ref().unwrap();
            assert_eq!(title.line.plain(), "Releases");
            for node in &scene.nodes {
                assert!(
                    node.rect.y > title.pos.y,
                    "{header}: {} under the title",
                    node.id
                );
            }
        }
    }

    #[test]
    fn an_empty_graph_still_lays_out() {
        for header in ["gitGraph", "gitGraph TB:", "gitGraph BT:"] {
            let scene = scene(header);
            assert!(scene.node("branch:0").is_some());
            assert_in_bounds(&scene, header);
        }
    }
}
