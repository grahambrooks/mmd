//! Renders every supported diagram, plus prose, into one PDF.
//!
//! This is the visual check the test suite cannot make: tests prove nothing
//! was dropped, but only looking at the page proves it reads correctly.
//!
//! The PDF stack is a dev-dependency on purpose — the library itself emits
//! SVG and nothing else.
//!
//! ```console
//! $ cargo run --example showcase
//! ```

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{Point, Size, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::text::{Font, TextDirection};
use krilla_svg::{SurfaceExt, SvgSettings};

const PAGE_WIDTH: f32 = 595.276;
const PAGE_HEIGHT: f32 = 841.89;
const MARGIN: f32 = 56.0;
const BODY_SIZE: f32 = 10.5;
const LEADING: f32 = 16.0;

struct Sample {
    heading: &'static str,
    prose: &'static str,
    source: &'static str,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples = [
        Sample {
            heading: "gitGraph",
            prose: "Commits run left to right along a shared timeline, one lane per branch. \
                    Branch creation and merges curve between lanes; tags hang above their \
                    commit; HIGHLIGHT and REVERSE commits get their own marks.",
            source: include_str!("gitgraph.mmd"),
        },
        Sample {
            heading: "gitGraph TB",
            prose: "The same diagram top to bottom: lanes run down the page in their \
                    `order:`, commit labels sit left of each commit and tags to its right. \
                    The fix is cherry-picked onto release, marked with mermaid's cherry; \
                    the merge carries its own id and tag.",
            source: include_str!("gitgraph-tb.mmd"),
        },
        Sample {
            heading: "flowchart",
            prose: "Nodes are ranked along the flow by a layered layout that keeps \
                    crossings down and subgraph members together; links are smooth \
                    curves that leave and arrive square to the flow.",
            source: include_str!("flowchart.mmd"),
        },
        Sample {
            heading: "C4Context",
            prose: "Elements are placed on a grid in declaration order, and relationships \
                    curve between the sides that face each other, under a row rather than \
                    through an element. Labels sit on pills so they stay readable where \
                    they cross a line.",
            source: include_str!("c4.mmd"),
        },
        Sample {
            heading: "C4Container",
            prose: "Boundaries are dashed boxes holding a grid of their own, nested as \
                    deep as the source nests them. Databases are cylinders, queues lie \
                    on their side, and external elements are grey; a BiRel has an \
                    arrowhead at each end.",
            source: include_str!("c4-container.mmd"),
        },
        Sample {
            heading: "erDiagram",
            prose: "Entities are tables of typed attributes with their keys and \
                    comments, placed by the same layered layout as flowcharts. \
                    Relationships carry a crow's foot for the cardinality at each \
                    end; non-identifying ones are dashed.",
            source: include_str!("er.mmd"),
        },
    ];

    let regular = load_font("Regular").ok_or("bundled regular face missing")?;
    let bold = load_font("Bold").ok_or("bundled bold face missing")?;

    // usvg needs the same faces in its database, or it silently drops text.
    let mut options = usvg::Options::default();
    {
        let db = options.fontdb_mut();
        for font in typst_assets::fonts() {
            db.load_font_data(font.to_vec());
        }
        db.set_serif_family("Libertinus Serif");
        db.set_sans_serif_family("Libertinus Serif");
        db.set_monospace_family("DejaVu Sans Mono");
    }

    let mut document = Document::new();
    let mut page = document.start_page_with(page_settings()?);
    let mut surface = page.surface();
    let mut y = MARGIN + 24.0;

    surface.set_fill(Some(fill(0x1f, 0x23, 0x28)));
    surface.draw_text(
        Point::from_xy(MARGIN, y),
        bold.clone(),
        22.0,
        "mmd — supported diagrams",
        false,
        TextDirection::Auto,
    );
    y += 30.0;

    for line in wrap(
        "Every diagram below was rendered by mmd to SVG, then placed in this page. \
         The text you are reading is drawn directly, so this one file shows both \
         prose and diagrams rendering correctly in a PDF.",
        78,
    ) {
        surface.set_fill(Some(fill(0x1f, 0x23, 0x28)));
        surface.draw_text(
            Point::from_xy(MARGIN, y),
            regular.clone(),
            BODY_SIZE,
            &line,
            false,
            TextDirection::Auto,
        );
        y += LEADING;
    }
    y += 14.0;

    for sample in samples {
        let svg = mmd::render(sample.source)?;
        let tree = usvg::Tree::from_str(&svg, &options)?;
        let size = tree.size();

        // Scale to the text measure and to what a fresh page can hold
        // below the heading and prose, never up.
        let prose_lines = wrap(sample.prose, 78);
        let available = PAGE_WIDTH - MARGIN * 2.0;
        let page_room =
            PAGE_HEIGHT - MARGIN * 2.0 - 20.0 - (26.0 + prose_lines.len() as f32 * LEADING + 30.0);
        let scale = (available / size.width())
            .min(page_room / size.height())
            .min(1.0);
        let drawn = Size::from_wh(size.width() * scale, size.height() * scale)
            .ok_or("diagram has no size")?;

        let needed = 26.0 + prose_lines.len() as f32 * LEADING + drawn.height() + 30.0;
        if y + needed > PAGE_HEIGHT - MARGIN {
            surface.finish();
            page.finish();
            page = document.start_page_with(page_settings()?);
            surface = page.surface();
            y = MARGIN + 20.0;
        }

        surface.set_fill(Some(fill(0x1f, 0x23, 0x28)));
        surface.draw_text(
            Point::from_xy(MARGIN, y),
            bold.clone(),
            15.0,
            sample.heading,
            false,
            TextDirection::Auto,
        );
        y += 22.0;

        for line in prose_lines {
            surface.set_fill(Some(fill(0x59, 0x63, 0x6e)));
            surface.draw_text(
                Point::from_xy(MARGIN, y),
                regular.clone(),
                BODY_SIZE,
                &line,
                false,
                TextDirection::Auto,
            );
            y += LEADING;
        }
        y += 8.0;

        surface.push_transform(&Transform::from_translate(MARGIN, y));
        surface.push_transform(&Transform::from_scale(scale, scale));
        surface.draw_svg(&tree, drawn_size(&size)?, SvgSettings::default());
        surface.pop();
        surface.pop();
        y += drawn.height() + 26.0;

        println!(
            "{}: {} x {} svg units, {} bytes",
            sample.heading,
            round(size.width()),
            round(size.height()),
            svg.len()
        );
    }

    surface.finish();
    page.finish();

    std::fs::create_dir_all("target")?;
    let pdf = document.finish()?;
    std::fs::write("target/showcase.pdf", &pdf)?;
    println!("wrote target/showcase.pdf ({} bytes)", pdf.len());
    Ok(())
}

fn load_font(subfamily: &str) -> Option<Font> {
    let data = typst_assets::fonts()
        .find(|data| face_names(data) == Some(("Libertinus Serif".into(), subfamily.into())))?;
    Font::new(data.into(), 0)
}

/// Family and subfamily from a font's name table.
fn face_names(data: &[u8]) -> Option<(String, String)> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    let name = |id: u16| {
        face.names()
            .into_iter()
            .find(|entry| entry.name_id == id && entry.is_unicode())
            .and_then(|entry| entry.to_string())
    };
    Some((name(1)?, name(2)?))
}

fn page_settings() -> Result<PageSettings, Box<dyn std::error::Error>> {
    PageSettings::from_wh(PAGE_WIDTH, PAGE_HEIGHT).ok_or_else(|| "invalid page size".into())
}

/// The SVG's own size, in krilla's units. `draw_svg` scales the tree to this,
/// so the surrounding transform supplies the page scaling.
fn drawn_size(size: &usvg::Size) -> Result<Size, Box<dyn std::error::Error>> {
    Size::from_wh(size.width(), size.height()).ok_or_else(|| "diagram has no size".into())
}

fn fill(r: u8, g: u8, b: u8) -> Fill {
    Fill {
        paint: rgb::Color::new(r, g, b).into(),
        opacity: NormalizedF32::ONE,
        rule: Default::default(),
    }
}

/// Crude character-count wrapping; enough for fixed showcase prose.
fn wrap(text: &str, columns: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() && current.chars().count() + 1 + word.chars().count() > columns {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn round(value: f32) -> i32 {
    value.round() as i32
}
