//! End-to-end checks over the public API.
//!
//! Layout properties are asserted on the scene rather than on SVG strings, so
//! these survive cosmetic changes to the output.

use mmd::Scene;
use mmd::geom::Rect;

const GITGRAPH: &str = include_str!("../examples/gitgraph.mmd");
const GITGRAPH_TB: &str = include_str!("../examples/gitgraph-tb.mmd");
const C4: &str = include_str!("../examples/c4.mmd");
const FLOWCHART: &str = include_str!("../examples/flowchart.mmd");

fn fonts() -> mmd::text::BundledFonts {
    mmd::text::BundledFonts::new().expect("bundled fonts load")
}

/// Element boxes, which must never sit on top of one another.
fn rects(scene: &Scene) -> Vec<Rect> {
    scene.nodes.iter().map(|node| node.rect).collect()
}

/// Everything the scene draws: nodes, containers and the points of every
/// edge's path.
fn drawn(scene: &Scene) -> Rect {
    let mut all: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
    all.extend(scene.containers.iter().map(|c| c.rect));
    for edge in &scene.edges {
        for poly in mmd::scene::flatten::flatten(&edge.path, 0.5) {
            all.extend(poly.points.iter().map(|p| Rect::new(p.x, p.y, 0.0, 0.0)));
        }
    }
    all.into_iter()
        .reduce(|a, b| a.union(&b))
        .expect("scene draws something")
}

#[test]
fn both_samples_render_to_svg() {
    for source in [GITGRAPH, GITGRAPH_TB, C4, FLOWCHART] {
        let svg = mmd::render(source).expect("sample renders");
        assert!(svg.starts_with("<svg"), "{}", &svg[..40.min(svg.len())]);
        assert!(svg.trim_end().ends_with("</svg>"));
    }
}

#[test]
fn output_is_never_html_in_disguise() {
    for source in [GITGRAPH, GITGRAPH_TB, C4, FLOWCHART] {
        let svg = mmd::render(source).unwrap();
        // The whole point of the crate: SVG-only consumers must keep the text.
        assert!(!svg.contains("foreignObject"), "labels must be SVG text");
        assert!(
            !svg.contains("<style"),
            "styling must be presentation attributes"
        );
        assert!(svg.contains("<text"));
    }
}

#[test]
fn every_commit_and_branch_reaches_the_scene() {
    let scene = mmd::gitgraph::scene(GITGRAPH, &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "scaffold", "svg base", "gitGraph", "C4", "docs", "release", "revert", "v0.1", "v0.2",
        "main", "diagrams", "hotfix",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "missing {expected}: {texts:?}"
        );
    }
}

#[test]
fn every_vertical_commit_tag_and_branch_reaches_the_scene() {
    let scene = mmd::gitgraph::scene(GITGRAPH_TB, &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "init",
        "parser",
        "fix crash",
        "notes",
        "layout",
        "ship",
        "v1.0.1",
        "v1.1",
        "cherry-pick:fix crash",
        "main",
        "develop",
        "release",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "missing {expected}: {texts:?}"
        );
    }
    // Seven commits, each linked to its parents: the cherry-pick and the
    // merge have two.
    let commits = scene
        .nodes
        .iter()
        .filter(|n| n.id.starts_with("commit:") && n.id.matches(':').count() == 1)
        .count();
    assert_eq!(commits, 7);
    let links = scene.edges.iter().filter(|e| e.from.is_some()).count();
    assert_eq!(links, 8);
    // Lanes in `order:`: main, develop, release, left to right.
    let x = |id: &str| scene.node(id).unwrap().rect.center().x;
    assert!(x("branch:0") < x("branch:2") && x("branch:2") < x("branch:1"));
}

#[test]
fn every_c4_element_and_relation_reaches_the_scene() {
    let scene = mmd::c4::scene(C4, &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "Author",
        "md2pdf",
        "mmd",
        "Embedded fonts",
        "PDF reader",
        "System context for md2pdf",
        "runs",
        "renders diagrams",
        "[SVG]",
        "produces PDF",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "missing {expected}: {texts:?}"
        );
    }
}

#[test]
fn c4_element_boxes_never_overlap_each_other() {
    let scene = mmd::c4::scene(C4, &fonts()).unwrap();
    let boxes = rects(&scene);
    assert!(
        boxes.len() >= 5,
        "expected the five elements, got {boxes:?}"
    );
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn everything_drawn_stays_inside_the_reported_bounds() {
    for scene in [
        mmd::gitgraph::scene(GITGRAPH, &fonts()).unwrap(),
        mmd::gitgraph::scene(GITGRAPH_TB, &fonts()).unwrap(),
        mmd::c4::scene(C4, &fonts()).unwrap(),
        mmd::flowchart::scene(FLOWCHART, &fonts()).unwrap(),
    ] {
        let bounds = scene.bounds();
        let drawn = drawn(&scene);
        assert!(
            bounds.encloses(&drawn, 0.01),
            "{drawn:?} escapes {bounds:?}"
        );
    }
}

#[test]
fn rendering_is_deterministic() {
    assert_eq!(
        mmd::render(GITGRAPH).unwrap(),
        mmd::render(GITGRAPH).unwrap()
    );
    assert_eq!(
        mmd::render(GITGRAPH_TB).unwrap(),
        mmd::render(GITGRAPH_TB).unwrap()
    );
    assert_eq!(mmd::render(C4).unwrap(), mmd::render(C4).unwrap());
}

#[test]
fn unsupported_syntax_fails_loudly_with_a_line_number() {
    let error = mmd::render("gitGraph\n  commit\n  cherry-pick id: \"x\"\n").unwrap_err();
    assert_eq!(error.line, 3);
    assert!(error.message.contains("unknown commit `x`"));
    assert!(error.to_string().starts_with("line 3:"));

    let error = mmd::render("C4Context\n  Container(a, \"A\")\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("Container"));
}

#[test]
fn every_flowchart_node_and_label_reaches_the_scene() {
    let scene = mmd::flowchart::scene(FLOWCHART, &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "Keystroke",
        "Paused 150 ms?",
        "Render on a worker",
        "The user's crates",
        "Scene",
        "yes",
        "no",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "missing {expected}: {texts:?}"
        );
    }
    assert_eq!(scene.containers.len(), 1);
}

#[test]
fn flowchart_nodes_never_overlap_and_subgraphs_hold_their_members() {
    let scene = mmd::flowchart::scene(FLOWCHART, &fonts()).unwrap();
    let boxes = rects(&scene);
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
        }
    }
    let group = scene.containers[0].rect;
    for id in ["mmd", "puml", "forge"] {
        let node = scene.node(id).unwrap();
        assert!(
            group.encloses(&node.rect, 0.01),
            "{id} outside its subgraph"
        );
    }
    for id in ["edit", "scene", "done"] {
        let node = scene.node(id).unwrap();
        assert!(!group.intersects(&node.rect), "{id} inside the subgraph");
    }
}

#[test]
fn flowchart_edges_end_on_their_nodes() {
    let scene = mmd::flowchart::scene(FLOWCHART, &fonts()).unwrap();
    for edge in &scene.edges {
        let (Some(from), Some(to)) = (&edge.from, &edge.to) else {
            continue;
        };
        if from == to {
            continue;
        }
        let (start, _) = mmd::scene::flatten::start(&edge.path).unwrap();
        let (end, _) = mmd::scene::flatten::end(&edge.path).unwrap();
        let near =
            |id: &str, p: mmd::geom::Point| scene.node(id).unwrap().rect.inflate(1.0).contains(p);
        assert!(near(from, start), "{from}->{to} starts at {start:?}");
        assert!(near(to, end), "{from}->{to} ends at {end:?}");
    }
}

#[test]
fn flowcharts_flow_in_their_direction() {
    for (dir, check) in [
        (
            "TD",
            (|a: Rect, b: Rect| a.y < b.y) as fn(Rect, Rect) -> bool,
        ),
        ("BT", |a, b| a.y > b.y),
        ("LR", |a, b| a.x < b.x),
        ("RL", |a, b| a.x > b.x),
    ] {
        let source = format!("flowchart {dir}\n  A --> B --> C\n");
        let scene = mmd::flowchart::scene(&source, &fonts()).unwrap();
        let r = |id: &str| scene.node(id).unwrap().rect;
        assert!(check(r("A"), r("B")) && check(r("B"), r("C")), "{dir}");
    }
}
