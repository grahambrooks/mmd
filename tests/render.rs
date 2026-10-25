//! End-to-end checks over the public API.
//!
//! Layout properties are asserted on the scene rather than on SVG strings, so
//! these survive cosmetic changes to the output.

use std::path::Path;

use mmd::Scene;
use mmd::geom::Rect;

const GITGRAPH: &str = include_str!("../examples/gitgraph.mmd");
const GITGRAPH_TB: &str = include_str!("../examples/gitgraph-tb.mmd");
const C4: &str = include_str!("../examples/c4.mmd");
const FLOWCHART: &str = include_str!("../examples/flowchart.mmd");
const ER: &str = include_str!("../examples/er.mmd");
const C4_CONTAINER: &str = include_str!("../examples/c4-container.mmd");

fn fonts() -> mmd::text::BundledFonts {
    mmd::text::BundledFonts::new().expect("bundled fonts load")
}

/// Element boxes, which must never sit on top of one another.
fn rects(scene: &Scene) -> Vec<Rect> {
    scene.nodes.iter().map(|node| node.rect).collect()
}

/// Everything the scene draws: nodes, containers and the points of every
/// edge's path.
fn drawn(scene: &Scene) -> Option<Rect> {
    let mut all: Vec<Rect> = scene.nodes.iter().map(|n| n.rect).collect();
    all.extend(scene.containers.iter().map(|c| c.rect));
    for edge in &scene.edges {
        for poly in mmd::scene::flatten::flatten(&edge.path, 0.5) {
            all.extend(poly.points.iter().map(|p| Rect::new(p.x, p.y, 0.0, 0.0)));
        }
    }
    all.into_iter().reduce(|a, b| a.union(&b))
}

/// Every `examples/*.mmd`, by path, so a new sample is checked without
/// being listed here.
fn examples() -> Vec<(String, String)> {
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples"))
}

/// Every `examples/gaps/*.mmd`: real mermaid input mmd does not handle yet.
fn gaps() -> Vec<(String, String)> {
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/gaps"))
}

fn sources(dir: &Path) -> Vec<(String, String)> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let source = std::fs::read_to_string(&p).expect("sample is readable");
            (p.display().to_string(), source)
        })
        .collect()
}

#[test]
fn every_example_renders_to_svg() {
    let examples = examples();
    assert!(examples.len() >= 6, "examples/ went missing");
    for (path, source) in examples {
        let svg = mmd::render(&source).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert!(svg.starts_with("<svg"), "{path}");
        assert!(svg.trim_end().ends_with("</svg>"), "{path}");
    }
}

#[test]
fn output_is_never_html_in_disguise() {
    for (path, source) in examples() {
        let svg = mmd::render(&source).unwrap();
        // The whole point of the crate: SVG-only consumers must keep the text.
        assert!(
            !svg.contains("foreignObject"),
            "{path}: labels must be SVG text"
        );
        assert!(
            !svg.contains("<style"),
            "{path}: styling must be presentation attributes"
        );
        assert!(svg.contains("<text"), "{path}: no text");
    }
}

#[test]
fn everything_drawn_stays_inside_the_reported_bounds() {
    for (path, source) in examples() {
        let scene = mmd::scene(&source, &fonts()).unwrap();
        let bounds = scene.bounds();
        if let Some(drawn) = drawn(&scene) {
            assert!(
                bounds.encloses(&drawn, 0.01),
                "{path}: {drawn:?} escapes {bounds:?}"
            );
        }
    }
}

#[test]
fn rendering_is_deterministic() {
    for (path, source) in examples() {
        assert_eq!(
            mmd::render(&source).unwrap(),
            mmd::render(&source).unwrap(),
            "{path}"
        );
    }
}

#[test]
fn every_known_gap_fails_loudly_with_a_line_number() {
    for (path, source) in gaps() {
        match mmd::render(&source) {
            Ok(_) => panic!("{path} renders now: move it out of examples/gaps/"),
            Err(error) => {
                assert!(error.line >= 1, "{path}: {error}");
                assert!(!error.message.is_empty(), "{path}");
            }
        }
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
fn unsupported_syntax_fails_loudly_with_a_line_number() {
    let error = mmd::render("gitGraph\n  commit\n  cherry-pick id: \"x\"\n").unwrap_err();
    assert_eq!(error.line, 3);
    assert!(error.message.contains("unknown commit `x`"));
    assert!(error.to_string().starts_with("line 3:"));

    let error =
        mmd::render("C4Context\n  Deployment_Node(a, \"A\") {\n}\nRel(a, a, \"x\")\n").unwrap_err();
    assert_eq!(error.line, 4);
    assert!(error.message.contains("boundary"), "{error}");

    let error = mmd::render("C4Dynamic\n  Rel(a, b, \"x\")\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("unknown element `a`"), "{error}");
}

#[test]
fn c4_dynamic_numbers_relations_and_deployment_nodes_hold_their_containers() {
    let source = include_str!("../examples/c4-dynamic.mmd");
    let texts = mmd::c4::scene(source, &fonts()).unwrap().texts();
    for expected in [
        "1: Saves the draft",
        "2: POST /documents",
        "4: Queues a render",
        "5: 202 Accepted",
    ] {
        assert!(texts.iter().any(|t| t == expected), "missing {expected}");
    }

    let source = include_str!("../examples/c4-deployment.mmd");
    let scene = mmd::c4::scene(source, &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "AWS",
        "[Cloud]",
        "eu-west-1",
        "Scaled on queue depth",
        "RDS",
    ] {
        assert!(texts.iter().any(|t| t == expected), "missing {expected}");
    }
    let node = |label: &str| {
        scene
            .containers
            .iter()
            .find(|c| c.label[0].plain() == label)
            .unwrap()
    };
    let api = scene.node("api").unwrap().rect;
    for holder in ["api pod", "Cluster", "AWS"] {
        assert!(node(holder).rect.encloses(&api, 0.01), "{holder}");
        assert_eq!(node(holder).kind, mmd::scene::ContainerKind::DeploymentNode);
    }
    assert!(!node("Reader's device").rect.intersects(&api));
}

#[test]
fn every_c4_container_element_boundary_and_relation_reaches_the_scene() {
    let scene = mmd::c4::scene(C4_CONTAINER, &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "Containers of a document publishing service",
        "Author",
        "Git host",
        "Publisher",
        "[SYSTEM]",
        "API",
        "[Rust, axum]",
        "Render jobs",
        "[SQS]",
        "Render worker",
        "Document store",
        "[Postgres]",
        "CDN cache",
        "[S3]",
        "Publishes",
        "[HTTPS]",
        "Notifies",
        "[webhook]",
        "Enqueues",
        "Polls",
        "Reads and writes",
        "[SQL]",
        "Uploads",
        "[S3 API]",
    ] {
        assert!(
            texts.iter().any(|t| t == expected),
            "missing {expected}: {texts:?}"
        );
    }
}

#[test]
fn c4_container_boxes_never_overlap_and_the_boundary_holds_its_members() {
    let scene = mmd::c4::scene(C4_CONTAINER, &fonts()).unwrap();
    let boxes = rects(&scene);
    assert_eq!(boxes.len(), 7);
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
        }
    }
    let boundary = scene.containers[0].rect;
    for id in ["api", "jobs", "worker", "store"] {
        let node = scene.node(id).unwrap();
        assert!(boundary.encloses(&node.rect, 0.01), "{id} outside");
    }
    for id in ["author", "git", "cdn"] {
        let node = scene.node(id).unwrap();
        assert!(!boundary.intersects(&node.rect), "{id} inside");
    }
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

#[test]
fn every_er_entity_attribute_and_relationship_reaches_the_scene() {
    let model = mmd::er::parse(ER).unwrap();
    let scene = mmd::er::scene(ER, &fonts()).unwrap();
    let texts = scene.texts();
    let has = |expected: &str| {
        assert!(
            texts.iter().any(|t| t == expected),
            "missing {expected}: {texts:?}"
        )
    };
    assert_eq!(scene.nodes.len(), model.entities.len());
    for entity in &model.entities {
        has(entity.label());
        for attribute in &entity.attributes {
            has(&attribute.kind);
            has(&attribute.name);
            if !attribute.keys.is_empty() {
                has(&attribute.keys.join(", "));
            }
            if let Some(comment) = &attribute.comment {
                has(comment);
            }
        }
    }
    assert_eq!(scene.edges.len(), model.relationships.len());
    for relationship in &model.relationships {
        has(&relationship.label);
    }
}

#[test]
fn er_entities_never_overlap() {
    let scene = mmd::er::scene(ER, &fonts()).unwrap();
    let boxes = rects(&scene);
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.intersects(b), "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn flowchart_v11_shapes_link_styles_and_styled_subgraphs_reach_the_scene() {
    let scene = mmd::flowchart::scene(FLOWCHART, &fonts()).unwrap();
    let pdf = scene.node("pdf").unwrap();
    assert_eq!(pdf.shape, mmd::scene::Shape::Document);
    assert_eq!(pdf.label[0].plain(), "PDF via md2pdf");
    assert!(
        pdf.label[0]
            .runs
            .iter()
            .any(|r| r.font == mmd::scene::Font::Bold)
    );
    assert_eq!(
        scene.edges[2].pen.paint,
        mmd::scene::Paint::Rgb(0xb8, 0x86, 0x0b)
    );
    assert_eq!(
        scene.containers[0].style.fill,
        mmd::scene::Paint::Rgb(0xee, 0xf4, 0xfb)
    );
}

#[test]
fn flowchart_out_of_subset_fails_with_a_line_number() {
    for (source, line, names) in [
        ("flowchart TD\n  A@{ shape: wobbly }\n", 2, "wobbly"),
        (
            "flowchart TD\n  A --> B\n  linkStyle 5 stroke:#f00\n",
            3,
            "link 5",
        ),
        ("flowchart TD\n  A\n  click Z cb\n", 3, "`Z`"),
        (
            "flowchart TD\n  subgraph s\n    A\n  end\n  B --> s[Box]\n",
            5,
            "subgraph",
        ),
    ] {
        let error = mmd::render(source).unwrap_err();
        assert_eq!(error.line, line, "{source}");
        assert!(error.message.contains(names), "{error}");
    }
}
