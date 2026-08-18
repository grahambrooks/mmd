//! End-to-end checks over the public API.
//!
//! Layout properties are asserted on the scene rather than on SVG strings, so
//! these survive cosmetic changes to the output.

use mmd::geom::Rect;
use mmd::scene::{Item, Scene};
use mmd::theme::Theme;

const GITGRAPH: &str = include_str!("../examples/gitgraph.mmd");
const C4: &str = include_str!("../examples/c4.mmd");

fn theme() -> Theme {
    Theme::default()
}

fn fonts() -> mmd::text::BundledFonts {
    mmd::text::BundledFonts::new().expect("bundled fonts load")
}

/// Boxes that must never sit on top of one another.
fn rects(scene: &Scene) -> Vec<Rect> {
    scene
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

#[test]
fn both_samples_render_to_svg() {
    for source in [GITGRAPH, C4] {
        let svg = mmd::render(source).expect("sample renders");
        assert!(svg.starts_with("<svg"), "{}", &svg[..40.min(svg.len())]);
        assert!(svg.trim_end().ends_with("</svg>"));
    }
}

#[test]
fn output_is_never_html_in_disguise() {
    for source in [GITGRAPH, C4] {
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
    let (scene, _) = mmd::gitgraph::layout(GITGRAPH, &theme(), &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "scaffold", "svg base", "gitGraph", "C4", "docs", "release", "revert", "v0.1", "v0.2",
        "main", "diagrams", "hotfix",
    ] {
        assert!(texts.contains(&expected), "missing {expected}: {texts:?}");
    }
}

#[test]
fn every_c4_element_and_relation_reaches_the_scene() {
    let (scene, _) = mmd::c4::layout(C4, &theme(), &fonts()).unwrap();
    let texts = scene.texts();
    for expected in [
        "Author",
        "md2pdf",
        "mmd",
        "Embedded fonts",
        "PDF reader",
        "System context for md2pdf",
        "runs",
        "renders diagrams [SVG]",
        "produces PDF",
    ] {
        assert!(texts.contains(&expected), "missing {expected}: {texts:?}");
    }
}

#[test]
fn c4_element_boxes_never_overlap_each_other() {
    let (scene, _) = mmd::c4::layout(C4, &theme(), &fonts()).unwrap();
    // Element boxes are the full-width ones; label knockouts are narrower.
    let boxes: Vec<Rect> = rects(&scene)
        .into_iter()
        .filter(|rect| rect.width > 150.0)
        .collect();
    assert!(
        boxes.len() >= 5,
        "expected the five elements, got {boxes:?}"
    );
    for (i, a) in boxes.iter().enumerate() {
        for b in &boxes[i + 1..] {
            assert!(!a.intersects(*b), "{a:?} overlaps {b:?}");
        }
    }
}

#[test]
fn everything_drawn_stays_inside_the_reported_bounds() {
    for (scene, bounds) in [
        mmd::gitgraph::layout(GITGRAPH, &theme(), &fonts()).unwrap(),
        mmd::c4::layout(C4, &theme(), &fonts()).unwrap(),
    ] {
        let drawn = scene.shape_bounds().expect("scene draws something");
        assert!(drawn.x >= bounds.x - 0.01, "{drawn:?} escapes {bounds:?}");
        assert!(drawn.y >= bounds.y - 0.01, "{drawn:?} escapes {bounds:?}");
        assert!(
            drawn.right() <= bounds.right() + 0.01,
            "{drawn:?} escapes {bounds:?}"
        );
        assert!(
            drawn.bottom() <= bounds.bottom() + 0.01,
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
    assert_eq!(mmd::render(C4).unwrap(), mmd::render(C4).unwrap());
}

#[test]
fn unsupported_syntax_fails_loudly_with_a_line_number() {
    let error = mmd::render("gitGraph\n  commit\n  cherry-pick id: \"x\"\n").unwrap_err();
    assert_eq!(error.line, 3);
    assert!(error.to_string().starts_with("line 3:"));

    let error = mmd::render("C4Context\n  Container(a, \"A\")\n").unwrap_err();
    assert_eq!(error.line, 2);
    assert!(error.message.contains("Container"));
}
