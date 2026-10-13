//! Layered (Sugiyama-style) graph layout, for flowcharts.
//!
//! 1. Cycles are broken by reversing the back edges a depth-first search
//!    finds.
//! 2. Nodes get ranks by longest path, two apart, so every edge crosses at
//!    least one in-between rank where its label (or a bend) can sit; sources
//!    are then pulled down next to what they feed.
//! 3. Edges longer than one rank run through dummy points, one per rank.
//! 4. Each rank is ordered by barycentre sweeps to cut crossings, keeping
//!    the members of a group (a subgraph) together.
//! 5. Positions across the ranks come from isotonic regression: each node
//!    as near the mean of its neighbours as the spacing allows.
//!
//! Coordinates are worked out on two axes: along the flow ("primary": y
//! for top-down, x for left-right) and across it ("secondary").

use diagram_scene::Point;

/// What to lay out.
pub struct Graph {
    /// Each node's (width, height).
    pub sizes: Vec<(f32, f32)>,
    pub edges: Vec<GraphEdge>,
    /// Each node's groups, outermost first. Members of a group stay
    /// together within a rank.
    pub groups: Vec<Vec<usize>>,
    /// Left-right (or right-left) rather than top-down.
    pub horizontal: bool,
    /// Bottom-up or right-left: the flow runs backwards.
    pub reversed: bool,
}

pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    /// The label's (width, height), which gets room on the edge.
    pub label: Option<(f32, f32)>,
}

/// Where everything went.
pub struct Placed {
    /// Node centres.
    pub centers: Vec<Point>,
    /// For each edge: the points between its ends, in the edge's
    /// direction, and where its label goes. Self-loops get no points.
    pub routes: Vec<Route>,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Route {
    pub points: Vec<Point>,
    pub label: Option<Point>,
}

/// Spacing.
pub struct Spacing {
    /// Between neighbours in a rank.
    pub node_gap: f32,
    /// Between a node's rank and the in-between rank.
    pub rank_gap: f32,
    /// Extra space where a group's members meet others.
    pub group_gap: f32,
    pub margin: f32,
}

/// A slot in a rank: a node, or a point an edge passes through.
#[derive(Clone, Debug)]
struct Slot {
    /// Size along the flow and across it.
    along: f32,
    across: f32,
    groups: Vec<usize>,
    dummy: bool,
}

pub fn layout(graph: &Graph, spacing: &Spacing) -> Placed {
    let n = graph.sizes.len();
    let oriented = |(w, h): (f32, f32)| if graph.horizontal { (w, h) } else { (h, w) };
    let mut slots: Vec<Slot> = graph
        .sizes
        .iter()
        .zip(&graph.groups)
        .map(|(&size, groups)| {
            let (along, across) = oriented(size);
            Slot {
                along,
                across,
                groups: groups.clone(),
                dummy: false,
            }
        })
        .collect();

    // 1. Break cycles.
    let reversed = back_edges(n, &graph.edges);
    let dag: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .enumerate()
        .filter(|(_, e)| e.from != e.to)
        .map(|(i, e)| {
            if reversed[i] {
                (e.to, e.from)
            } else {
                (e.from, e.to)
            }
        })
        .collect();

    // 2. Ranks.
    let rank = ranks(n, &dag);
    let max_rank = rank.iter().copied().max().unwrap_or(0);

    // 3. Chains of slots, one rank apart, for each edge.
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_rank + 1];
    for (v, &r) in rank.iter().enumerate() {
        layers[r].push(v);
    }
    let mut chains: Vec<Vec<usize>> = Vec::with_capacity(graph.edges.len());
    let mut label_slot: Vec<Option<usize>> = Vec::with_capacity(graph.edges.len());
    for (i, e) in graph.edges.iter().enumerate() {
        if e.from == e.to {
            chains.push(Vec::new());
            label_slot.push(None);
            continue;
        }
        let (u, v) = if reversed[i] {
            (e.to, e.from)
        } else {
            (e.from, e.to)
        };
        let common = common_prefix(&graph.groups[u], &graph.groups[v]);
        let mut chain = vec![u];
        let mid = (rank[u] + rank[v]) / 2;
        let mut label = None;
        for (r, layer) in layers
            .iter_mut()
            .enumerate()
            .take(rank[v])
            .skip(rank[u] + 1)
        {
            let (along, across) = match e.label {
                Some(size) if r == mid => oriented(size),
                _ => (0.0, 0.0),
            };
            let id = slots.len();
            slots.push(Slot {
                along,
                across,
                groups: common.clone(),
                dummy: true,
            });
            if r == mid && e.label.is_some() {
                label = Some(id);
            }
            layer.push(id);
            chain.push(id);
        }
        chain.push(v);
        chains.push(chain);
        label_slot.push(label);
    }

    // Adjacent-rank links between slots.
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    let mut down: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    for chain in &chains {
        for w in chain.windows(2) {
            down[w[0]].push(w[1]);
            up[w[1]].push(w[0]);
        }
    }

    // 4. Order the ranks, starting with each group's members together.
    let mut order: Vec<Vec<usize>> = layers
        .into_iter()
        .map(|rank| {
            let items = rank
                .iter()
                .enumerate()
                .map(|(i, &s)| (s, i as f32))
                .collect();
            group_sort(items, &slots, 0)
        })
        .collect();
    let mut best = order.clone();
    let mut best_crossings = crossings(&order, &down, slots.len());
    for sweep in 0..16 {
        let downward = sweep % 2 == 0;
        let ranks: Vec<usize> = if downward {
            (1..order.len()).collect()
        } else {
            (0..order.len().saturating_sub(1)).rev().collect()
        };
        for r in ranks {
            let reference = if downward { r - 1 } else { r + 1 };
            let mut index = vec![f32::NAN; slots.len()];
            for (i, &s) in order[reference].iter().enumerate() {
                index[s] = i as f32;
            }
            let links = if downward { &up } else { &down };
            let current: Vec<(usize, f32)> = order[r]
                .iter()
                .enumerate()
                .map(|(i, &s)| {
                    let near: Vec<f32> = links[s]
                        .iter()
                        .map(|&t| index[t])
                        .filter(|x| !x.is_nan())
                        .collect();
                    let bary = if near.is_empty() {
                        i as f32
                    } else {
                        near.iter().sum::<f32>() / near.len() as f32
                    };
                    (s, bary)
                })
                .collect();
            order[r] = group_sort(current, &slots, 0);
        }
        let c = crossings(&order, &down, slots.len());
        if c < best_crossings {
            best_crossings = c;
            best = order.clone();
        }
    }
    let order = best;

    // 5. Positions. Along the flow: ranks stacked with gaps.
    let mut along_pos = vec![0.0_f32; slots.len()];
    let mut cursor = spacing.margin;
    for rank in &order {
        let thickness = rank.iter().map(|&s| slots[s].along).fold(0.0_f32, f32::max);
        for &s in rank {
            along_pos[s] = cursor + thickness / 2.0;
        }
        cursor += thickness + spacing.rank_gap;
    }
    let along_total = cursor - spacing.rank_gap + spacing.margin;

    // Across: packed, then pulled towards neighbours.
    let gap = |a: usize, b: usize| {
        let (sa, sb) = (&slots[a], &slots[b]);
        let base = if sa.dummy && sb.dummy {
            spacing.node_gap * 0.4
        } else if sa.dummy || sb.dummy {
            spacing.node_gap * 0.7
        } else {
            spacing.node_gap
        };
        let differing =
            sa.groups.len().max(sb.groups.len()) - common_prefix(&sa.groups, &sb.groups).len();
        (sa.across + sb.across) / 2.0 + base + spacing.group_gap * differing as f32
    };
    let mut across = vec![0.0_f32; slots.len()];
    for rank in &order {
        let mut x = 0.0;
        for (i, &s) in rank.iter().enumerate() {
            if i > 0 {
                x += gap(rank[i - 1], s);
            }
            across[s] = x;
        }
    }
    for sweep in 0..24 {
        let downward = sweep % 2 == 0;
        let ranks: Vec<usize> = if downward {
            (1..order.len()).collect()
        } else {
            (0..order.len().saturating_sub(1)).rev().collect()
        };
        for r in ranks {
            let links = if downward { &up } else { &down };
            let desired: Vec<f32> = order[r]
                .iter()
                .map(|&s| {
                    let near = &links[s];
                    if near.is_empty() {
                        across[s]
                    } else {
                        near.iter().map(|&t| across[t]).sum::<f32>() / near.len() as f32
                    }
                })
                .collect();
            let gaps: Vec<f32> = order[r].windows(2).map(|w| gap(w[0], w[1])).collect();
            let placed = isotonic(&desired, &gaps);
            for (&s, x) in order[r].iter().zip(placed) {
                across[s] = x;
            }
        }
    }
    // Shift so the leftmost edge sits at the margin.
    let min = (0..slots.len())
        .map(|s| across[s] - slots[s].across / 2.0)
        .fold(f32::INFINITY, f32::min);
    let shift = spacing.margin - if min.is_finite() { min } else { 0.0 };
    for x in &mut across {
        *x += shift;
    }
    let across_total = (0..slots.len())
        .map(|s| across[s] + slots[s].across / 2.0)
        .fold(0.0_f32, f32::max)
        + spacing.margin;

    let point = |s: usize| {
        let a = if graph.reversed {
            along_total - along_pos[s]
        } else {
            along_pos[s]
        };
        if graph.horizontal {
            Point::new(a, across[s])
        } else {
            Point::new(across[s], a)
        }
    };
    let centers = (0..n).map(point).collect();
    let routes = chains
        .iter()
        .enumerate()
        .map(|(i, chain)| {
            let mut points: Vec<Point> = chain
                .iter()
                .skip(1)
                .take(chain.len().saturating_sub(2))
                .map(|&s| point(s))
                .collect();
            if reversed[i] {
                points.reverse();
            }
            Route {
                points,
                label: label_slot[i].map(point),
            }
        })
        .collect();
    let (width, height) = if graph.horizontal {
        (along_total, across_total)
    } else {
        (across_total, along_total)
    };
    Placed {
        centers,
        routes,
        width,
        height,
    }
}

/// Which edges a depth-first search, in input order, finds going back up
/// the tree: reversing them leaves no cycles.
fn back_edges(n: usize, edges: &[GraphEdge]) -> Vec<bool> {
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (i, e) in edges.iter().enumerate() {
        if e.from != e.to {
            adj[e.from].push((e.to, i));
        }
    }
    let mut state = vec![0u8; n];
    let mut back = vec![false; edges.len()];
    for start in 0..n {
        if state[start] != 0 {
            continue;
        }
        let mut stack = vec![(start, 0usize)];
        state[start] = 1;
        while let Some(&mut (u, ref mut k)) = stack.last_mut() {
            if *k < adj[u].len() {
                let (v, e) = adj[u][*k];
                *k += 1;
                match state[v] {
                    0 => {
                        state[v] = 1;
                        stack.push((v, 0));
                    }
                    1 => back[e] = true,
                    _ => {}
                }
            } else {
                state[u] = 2;
                stack.pop();
            }
        }
    }
    back
}

/// Longest-path ranks, two apart, with sources pulled down to just above
/// the nearest thing they feed.
fn ranks(n: usize, dag: &[(usize, usize)]) -> Vec<usize> {
    let mut succ: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut pred: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut indeg = vec![0usize; n];
    for &(u, v) in dag {
        succ[u].push(v);
        pred[v].push(u);
        indeg[v] += 1;
    }
    let mut topo = Vec::with_capacity(n);
    let mut queue: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    let mut head = 0;
    while head < queue.len() {
        let u = queue[head];
        head += 1;
        topo.push(u);
        for &v in &succ[u] {
            indeg[v] -= 1;
            if indeg[v] == 0 {
                queue.push(v);
            }
        }
    }
    let mut rank = vec![0usize; n];
    for &u in &topo {
        for &v in &succ[u] {
            rank[v] = rank[v].max(rank[u] + 2);
        }
    }
    for &u in topo.iter().rev() {
        if pred[u].is_empty()
            && let Some(nearest) = succ[u].iter().map(|&v| rank[v]).min()
        {
            rank[u] = nearest.saturating_sub(2).max(rank[u]);
        }
    }
    rank
}

/// Orders a rank's slots by barycentre, keeping each group's members
/// together (groups ordered by their members' mean).
fn group_sort(items: Vec<(usize, f32)>, slots: &[Slot], depth: usize) -> Vec<usize> {
    // (the group, or none for a lone slot; its members and barycentres)
    type Bucket = (Option<usize>, Vec<(usize, f32)>);
    let mut buckets: Vec<Bucket> = Vec::new();
    for item in items {
        let key = slots[item.0].groups.get(depth).copied();
        match key.and_then(|k| buckets.iter_mut().find(|(b, _)| *b == Some(k))) {
            Some((_, members)) => members.push(item),
            None => buckets.push((key, vec![item])),
        }
    }
    let mean = |m: &[(usize, f32)]| m.iter().map(|(_, b)| b).sum::<f32>() / m.len() as f32;
    buckets.sort_by(|a, b| mean(&a.1).total_cmp(&mean(&b.1)));
    let mut out = Vec::new();
    for (key, members) in buckets {
        if key.is_some() && members.len() > 1 {
            out.extend(group_sort(members, slots, depth + 1));
        } else {
            out.extend(members.into_iter().map(|(s, _)| s));
        }
    }
    out
}

/// Edge crossings between adjacent ranks.
fn crossings(order: &[Vec<usize>], down: &[Vec<usize>], slots: usize) -> usize {
    let mut pos = vec![0usize; slots];
    for rank in order {
        for (i, &s) in rank.iter().enumerate() {
            pos[s] = i;
        }
    }
    let mut total = 0;
    for rank in order {
        let links: Vec<(usize, usize)> = rank
            .iter()
            .flat_map(|&s| down[s].iter().map(move |&t| (s, t)))
            .map(|(s, t)| (pos[s], pos[t]))
            .collect();
        for (i, a) in links.iter().enumerate() {
            for b in &links[i + 1..] {
                if (a.0 < b.0 && a.1 > b.1) || (a.0 > b.0 && a.1 < b.1) {
                    total += 1;
                }
            }
        }
    }
    total
}

/// The positions nearest (least squares) to `desired`, in order, each at
/// least `gaps[i]` after the one before: pool adjacent violators over the
/// gap-adjusted targets.
fn isotonic(desired: &[f32], gaps: &[f32]) -> Vec<f32> {
    let mut offset = vec![0.0_f32; desired.len()];
    for i in 1..desired.len() {
        offset[i] = offset[i - 1] + gaps[i - 1];
    }
    // Blocks of (mean, count) over the adjusted targets.
    let mut blocks: Vec<(f32, usize)> = Vec::new();
    for (i, &d) in desired.iter().enumerate() {
        blocks.push((d - offset[i], 1));
        while blocks.len() > 1 {
            let (m2, c2) = blocks[blocks.len() - 1];
            let (m1, c1) = blocks[blocks.len() - 2];
            if m1 <= m2 {
                break;
            }
            blocks.pop();
            let last = blocks.len() - 1;
            blocks[last] = (
                (m1 * c1 as f32 + m2 * c2 as f32) / (c1 + c2) as f32,
                c1 + c2,
            );
        }
    }
    let mut out = Vec::with_capacity(desired.len());
    for (mean, count) in blocks {
        for _ in 0..count {
            out.push(mean + offset[out.len()]);
        }
    }
    out
}

fn common_prefix(a: &[usize], b: &[usize]) -> Vec<usize> {
    a.iter()
        .zip(b)
        .take_while(|(x, y)| x == y)
        .map(|(x, _)| *x)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
        Graph {
            sizes: vec![(60.0, 30.0); n],
            edges: edges
                .iter()
                .map(|&(from, to)| GraphEdge {
                    from,
                    to,
                    label: None,
                })
                .collect(),
            groups: vec![Vec::new(); n],
            horizontal: false,
            reversed: false,
        }
    }

    fn spacing() -> Spacing {
        Spacing {
            node_gap: 30.0,
            rank_gap: 25.0,
            group_gap: 20.0,
            margin: 10.0,
        }
    }

    #[test]
    fn isotonic_keeps_order_and_gaps() {
        let placed = isotonic(&[50.0, 0.0, 10.0], &[20.0, 20.0]);
        assert!(placed[1] - placed[0] >= 20.0 - 1e-4);
        assert!(placed[2] - placed[1] >= 20.0 - 1e-4);
        // Nearest in least squares: the mean of adjusted targets.
        assert!((placed[0] - 0.0).abs() < 1e-4, "{placed:?}");
    }

    #[test]
    fn edges_flow_downwards_and_nodes_never_overlap() {
        // A diamond with a long edge and a cycle back.
        let g = graph(5, &[(0, 1), (0, 2), (1, 3), (2, 3), (0, 3), (3, 4), (4, 0)]);
        let p = layout(&g, &spacing());
        for &(a, b) in &[(0, 1), (1, 3), (3, 4)] {
            assert!(p.centers[a].y < p.centers[b].y, "{a} above {b}");
        }
        for i in 0..5 {
            for j in i + 1..5 {
                let (a, b) = (p.centers[i], p.centers[j]);
                let apart = (a.x - b.x).abs() >= 60.0 || (a.y - b.y).abs() >= 30.0;
                assert!(apart, "{i} and {j} overlap");
            }
        }
        // The long edge 0→3 bends through points between the ranks.
        assert!(!p.routes[4].points.is_empty());
        // The edge back up runs bottom to top.
        let back = &p.routes[6].points;
        assert!(back.first().unwrap().y > back.last().unwrap().y);
    }

    #[test]
    fn a_simple_chain_lines_up() {
        let p = layout(&graph(3, &[(0, 1), (1, 2)]), &spacing());
        assert!((p.centers[0].x - p.centers[2].x).abs() < 0.5);
    }

    #[test]
    fn groups_stay_together() {
        // 0 feeds 1,2,3; 1 and 3 share a group, so 2 doesn't sit between.
        let mut g = graph(4, &[(0, 1), (0, 2), (0, 3)]);
        g.groups = vec![vec![], vec![7], vec![], vec![7]];
        let p = layout(&g, &spacing());
        let (x1, x2, x3) = (p.centers[1].x, p.centers[2].x, p.centers[3].x);
        assert!(!(x1.min(x3) < x2 && x2 < x1.max(x3)), "{x1} {x2} {x3}");
    }
}
