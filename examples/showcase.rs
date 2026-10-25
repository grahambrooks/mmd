//! Renders every sample and every known gap, plus a coverage matrix, into one
//! PDF for human review.
//!
//! This is the visual check the test suite cannot make: tests prove nothing
//! was dropped, but only looking at the page proves it reads correctly.
//!
//! Samples are discovered, not listed:
//!
//! - `examples/*.mmd` render as diagrams.
//! - `examples/gaps/*.mmd` are real mermaid input mmd does not handle yet.
//!   Each is drawn as the error mmd reports, so the gaps can be reviewed
//!   alongside what works. One that starts rendering is flagged as closed.
//!
//! Each file carries its own caption in leading `%%` comments, which mmd
//! ignores:
//!
//! ```text
//! %% showcase: C4Container with a boundary
//! %% prose: What the reader should look for.
//! %% approx: Where the drawing knowingly differs from mermaid's.
//! %% type: pie           (gaps only, when mmd can't detect the type)
//! ```
//!
//! The PDF stack is a dev-dependency on purpose — the library itself emits
//! SVG and nothing else.
//!
//! ```console
//! $ cargo run --example showcase
//! ```

use std::path::{Path, PathBuf};

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{Point, Size, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::Fill;
use krilla::surface::Surface;
use krilla::text::{Font, TextDirection};
use krilla_svg::{SurfaceExt, SvgSettings};

const PAGE_WIDTH: f32 = 595.276;
const PAGE_HEIGHT: f32 = 841.89;
const MARGIN: f32 = 56.0;
const BODY_SIZE: f32 = 10.5;
const LEADING: f32 = 16.0;
const SMALL: f32 = 8.5;
/// Space taken by a diagram type's heading.
const TYPE_HEADING: f32 = 30.0;

const INK: (u8, u8, u8) = (0x1f, 0x23, 0x28);
const MUTED: (u8, u8, u8) = (0x59, 0x63, 0x6e);
const APPROX: (u8, u8, u8) = (0x9a, 0x5b, 0x00);
const GAP: (u8, u8, u8) = (0xb4, 0x23, 0x18);
const OK: (u8, u8, u8) = (0x1a, 0x7f, 0x37);

/// Every diagram type upstream mermaid knows, with the keywords that
/// introduce it. The first keyword is the one shown.
const UPSTREAM: &[&[&str]] = &[
    &["flowchart", "graph"],
    &["sequenceDiagram"],
    &["classDiagram", "classDiagram-v2"],
    &["stateDiagram-v2", "stateDiagram"],
    &["erDiagram"],
    &["journey"],
    &["gantt"],
    &["pie"],
    &["quadrantChart"],
    &["requirementDiagram", "requirement"],
    &["gitGraph"],
    &["C4Context"],
    &["C4Container"],
    &["C4Component"],
    &["C4Dynamic"],
    &["C4Deployment"],
    &["mindmap"],
    &["timeline"],
    &["sankey", "sankey-beta"],
    &["xychart", "xychart-beta"],
    &["block", "block-beta"],
    &["packet", "packet-beta"],
    &["kanban"],
    &["architecture-beta", "architecture"],
    &["radar-beta"],
    &["treemap", "treemap-beta"],
    &["venn-beta"],
    &["wardley-beta"],
    &["treeView-beta"],
    &["ishikawa", "ishikawa-beta"],
    &["eventmodeling"],
    &["info"],
    &["zenuml"],
];

struct Sample {
    path: PathBuf,
    heading: String,
    prose: String,
    approx: Vec<String>,
    source: String,
    /// The row of [`UPSTREAM`] this belongs to, if any.
    row: Option<usize>,
    gap: bool,
}

impl Sample {
    fn load(path: &Path, gap: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let source = std::fs::read_to_string(path)?;
        let (mut heading, mut prose, mut approx, mut kind) = (None, Vec::new(), Vec::new(), None);
        for line in source.lines() {
            let Some(comment) = line.trim().strip_prefix("%%") else {
                continue;
            };
            let Some((key, value)) = comment.split_once(':') else {
                continue;
            };
            let value = value.trim().to_string();
            match key.trim() {
                "showcase" => heading = Some(value),
                "prose" => prose.push(value),
                "approx" => approx.push(value),
                "type" => kind = Some(value),
                _ => {}
            }
        }
        let keyword = kind
            .or_else(|| mmd::detect(&source).map(|k| k.keyword().to_string()))
            .or_else(|| first_word(&source));
        let row = keyword.and_then(|k| UPSTREAM.iter().position(|row| row.contains(&k.as_str())));
        let name = path.file_stem().unwrap_or_default().to_string_lossy();
        Ok(Self {
            heading: heading.unwrap_or_else(|| name.to_string()),
            prose: prose.join(" "),
            approx,
            source,
            row,
            gap,
            path: path.to_path_buf(),
        })
    }
}

fn first_word(source: &str) -> Option<String> {
    source
        .lines()
        .map(|l| l.split("%%").next().unwrap_or("").trim())
        .find(|l| !l.is_empty())
        .and_then(|l| l.split([' ', ':', ';']).next())
        .map(str::to_string)
}

fn discover(dir: &Path, gap: bool) -> Result<Vec<Sample>, Box<dyn std::error::Error>> {
    let mut paths: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "mmd"))
            .collect(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    paths.iter().map(|p| Sample::load(p, gap)).collect()
}

/// How mmd handles a row of [`UPSTREAM`].
fn status(row: &[&str]) -> (&'static str, (u8, u8, u8)) {
    match mmd::detect(&format!("{}\n", row[0])) {
        Some(kind) if kind.is_supported() => ("renders", OK),
        Some(_) => ("recognised, rejected", GAP),
        None => ("not supported", GAP),
    }
}

struct Pen {
    regular: Font,
    bold: Font,
    options: usvg::Options<'static>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut samples = discover(&root, false)?;
    samples.extend(discover(&root.join("gaps"), true)?);
    // Grouped by diagram type in upstream's order, each type's samples
    // before its gaps.
    samples.sort_by_key(|s| (s.row.unwrap_or(usize::MAX), s.gap, s.path.clone()));

    let mut options = usvg::Options::default();
    {
        // usvg needs the same faces in its database, or it silently drops text.
        let db = options.fontdb_mut();
        for font in typst_assets::fonts() {
            db.load_font_data(font.to_vec());
        }
        db.set_serif_family("Libertinus Serif");
        db.set_sans_serif_family("Libertinus Serif");
        db.set_monospace_family("DejaVu Sans Mono");
    }
    let pen = Pen {
        regular: load_font("Regular").ok_or("bundled regular face missing")?,
        bold: load_font("Bold").ok_or("bundled bold face missing")?,
        options,
    };

    let mut document = Document::new();
    matrix(&mut document, &pen, &samples)?;

    let mut failures = Vec::new();
    let mut closed = Vec::new();
    let mut page = document.start_page_with(page_settings()?);
    let mut surface = page.surface();
    let mut y = MARGIN + 20.0;
    let mut current_row = None;

    for sample in &samples {
        let result = mmd::render(&sample.source);
        let (svg, kind) = match (&result, sample.gap) {
            (Ok(svg), false) => (svg.clone(), Kind::Diagram),
            (Ok(svg), true) => {
                closed.push(sample.path.display().to_string());
                (svg.clone(), Kind::Closed)
            }
            (Err(error), true) => (error_card(&sample.source, error, GAP), Kind::Gap),
            (Err(error), false) => {
                failures.push(format!("{}: {error}", sample.path.display()));
                (error_card(&sample.source, error, GAP), Kind::Broken)
            }
        };
        let tree = usvg::Tree::from_str(&svg, &pen.options)?;
        let size = tree.size();

        let label = match kind {
            Kind::Diagram => sample.heading.clone(),
            Kind::Gap => format!("Gap · {}", sample.heading),
            Kind::Closed => format!(
                "Gap closed? · {} — now renders; move it out of gaps/",
                sample.heading
            ),
            Kind::Broken => format!("BROKEN · {}", sample.heading),
        };
        let file = sample
            .path
            .strip_prefix(&root)
            .unwrap_or(&sample.path)
            .display()
            .to_string();
        let prose_lines = wrap(&sample.prose, 84);
        let approx_lines: Vec<String> = sample
            .approx
            .iter()
            .flat_map(|a| wrap(&format!("≈ {a}"), 96))
            .collect();
        let text_height = 22.0
            + 13.0
            + prose_lines.len() as f32 * LEADING
            + approx_lines.len() as f32 * 12.0
            + 10.0;

        let available = PAGE_WIDTH - MARGIN * 2.0;
        let page_room = PAGE_HEIGHT - MARGIN * 2.0 - 20.0 - TYPE_HEADING - 26.0 - text_height;
        let scale = (available / size.width())
            .min(page_room / size.height())
            .min(1.0);
        let drawn = Size::from_wh(size.width() * scale, size.height() * scale)
            .ok_or("diagram has no size")?;

        // A new diagram type always starts a page, so each type can be
        // reviewed on its own.
        let new_type = current_row != Some(sample.row);
        // The first page is already fresh.
        let fresh = current_row.is_none();
        let heading = if new_type { TYPE_HEADING } else { 0.0 };
        if (new_type && !fresh)
            || y + heading + text_height + drawn.height() + 26.0 > PAGE_HEIGHT - MARGIN
        {
            surface.finish();
            page.finish();
            page = document.start_page_with(page_settings()?);
            surface = page.surface();
            y = MARGIN + 20.0;
        }
        if new_type {
            current_row = Some(sample.row);
            let name = sample.row.map(|r| UPSTREAM[r][0]).unwrap_or("unknown type");
            text(&mut surface, &pen.bold, 20.0, name, MARGIN, y, INK);
            y += TYPE_HEADING;
        }

        let color = match kind {
            Kind::Diagram => INK,
            Kind::Gap | Kind::Broken => GAP,
            Kind::Closed => APPROX,
        };
        text(&mut surface, &pen.bold, 14.0, &label, MARGIN, y, color);
        y += 13.0;
        text(&mut surface, &pen.regular, SMALL, &file, MARGIN, y, MUTED);
        y += LEADING;
        for line in prose_lines {
            text(
                &mut surface,
                &pen.regular,
                BODY_SIZE,
                &line,
                MARGIN,
                y,
                MUTED,
            );
            y += LEADING;
        }
        for line in approx_lines {
            text(&mut surface, &pen.regular, 9.0, &line, MARGIN, y, APPROX);
            y += 12.0;
        }
        y += 6.0;

        surface.push_transform(&Transform::from_translate(MARGIN, y));
        surface.push_transform(&Transform::from_scale(scale, scale));
        surface.draw_svg(&tree, drawn_size(&size)?, SvgSettings::default());
        surface.pop();
        surface.pop();
        y += drawn.height() + 26.0;

        println!(
            "{file}: {} ({} x {} svg units)",
            match kind {
                Kind::Diagram => "renders",
                Kind::Gap => "gap",
                Kind::Closed => "GAP NOW RENDERS",
                Kind::Broken => "BROKEN",
            },
            round(size.width()),
            round(size.height()),
        );
    }

    surface.finish();
    page.finish();

    std::fs::create_dir_all("target")?;
    let pdf = document.finish()?;
    std::fs::write("target/showcase.pdf", &pdf)?;
    println!("wrote target/showcase.pdf ({} bytes)", pdf.len());

    for path in &closed {
        eprintln!("warning: {path} renders now; move it out of examples/gaps/");
    }
    if !failures.is_empty() {
        return Err(format!("samples failed to render:\n{}", failures.join("\n")).into());
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Kind {
    Diagram,
    Gap,
    /// A gap sample that renders: either the gap closed or the sample is wrong.
    Closed,
    /// A sample that should render and doesn't.
    Broken,
}

/// The cover: one row per upstream diagram type.
fn matrix(
    document: &mut Document,
    pen: &Pen,
    samples: &[Sample],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut page = document.start_page_with(page_settings()?);
    let mut surface = page.surface();
    let mut y = MARGIN + 24.0;

    text(
        &mut surface,
        &pen.bold,
        22.0,
        "mmd — coverage",
        MARGIN,
        y,
        INK,
    );
    y += 28.0;
    let intro = "Every diagram type upstream mermaid knows, and how mmd handles it. \
                 The pages that follow show each type's samples, rendered by mmd to \
                 SVG and placed here, then its known gaps: real mermaid input that \
                 mmd rejects, shown as the error it reports. Lines marked ≈ are where \
                 a drawing knowingly differs from mermaid's.";
    for line in wrap(intro, 84) {
        text(
            &mut surface,
            &pen.regular,
            BODY_SIZE,
            &line,
            MARGIN,
            y,
            MUTED,
        );
        y += LEADING;
    }
    y += 12.0;

    let columns = [0.0, 128.0, 236.0, 292.0, 336.0];
    for (x, head) in columns
        .iter()
        .zip(["Type", "Status", "Samples", "Gaps", "Approximations"])
    {
        text(&mut surface, &pen.bold, 9.5, head, MARGIN + x, y, INK);
    }
    y += 15.0;

    let (mut rendered, mut total) = (0, 0);
    for (index, row) in UPSTREAM.iter().enumerate() {
        let (status, color) = status(row);
        total += 1;
        if color == OK {
            rendered += 1;
        }
        let mine = samples.iter().filter(|s| s.row == Some(index));
        let (mut count, mut gaps, mut approx) = (0, 0, 0);
        for sample in mine {
            if sample.gap {
                gaps += 1;
            } else {
                count += 1;
                approx += sample.approx.len();
            }
        }
        let cells = [
            row[0].to_string(),
            status.to_string(),
            count.to_string(),
            gaps.to_string(),
            approx.to_string(),
        ];
        for (i, (x, cell)) in columns.iter().zip(cells).enumerate() {
            let paint = match i {
                1 => color,
                3 if gaps > 0 => GAP,
                4 if approx > 0 => APPROX,
                _ => INK,
            };
            text(&mut surface, &pen.regular, 9.5, &cell, MARGIN + x, y, paint);
        }
        y += 15.0;
    }
    y += 8.0;
    let summary = format!(
        "{rendered} of {total} types render. Samples not tied to a known type are \
         listed last."
    );
    text(
        &mut surface,
        &pen.regular,
        BODY_SIZE,
        &summary,
        MARGIN,
        y,
        MUTED,
    );

    surface.finish();
    page.finish();
    Ok(())
}

/// A card standing in for a diagram: the source's first lines and the error.
fn error_card(source: &str, error: &mmd::Error, color: (u8, u8, u8)) -> String {
    let width = 480.0;
    let mut lines: Vec<(String, bool)> = vec![(format!("mmd: {error}"), true)];
    // The source without its showcase captions, keeping line numbers so the
    // failing line can be marked.
    let body: Vec<(usize, &str)> = source
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim_start().starts_with("%%"))
        .map(|(i, l)| (i + 1, l))
        .collect();
    let shown = body.len().min(14);
    for (number, line) in &body[..shown] {
        let mark = if *number == error.line { "▶ " } else { "  " };
        lines.push((format!("{mark}{line}"), false));
    }
    if body.len() > shown {
        lines.push((format!("  … {} more lines", body.len() - shown), false));
    }
    let mut wrapped = Vec::new();
    for (line, error) in lines {
        // Prose wraps at words; source wraps where it must, keeping its
        // indentation visible.
        let parts = if error {
            wrap(&line, 82)
        } else {
            wrap_hard(&line, 86)
        };
        for (n, part) in parts.into_iter().enumerate() {
            wrapped.push((if n == 0 { part } else { format!("    {part}") }, error));
        }
    }
    let height = 16.0 + wrapped.len() as f32 * 13.0 + 6.0;
    let (r, g, b) = color;
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect x="0.5" y="0.5" width="{w}" height="{h}" rx="4" fill="#fdf3f2" stroke="rgb({r},{g},{b})"/>"##,
        w = width - 1.0,
        h = height - 1.0,
    );
    for (i, (line, error)) in wrapped.iter().enumerate() {
        let y = 18.0 + i as f32 * 13.0;
        let (fill, family, weight) = if *error {
            (format!("rgb({r},{g},{b})"), "Libertinus Serif", "bold")
        } else {
            ("#1f2328".to_string(), "DejaVu Sans Mono", "normal")
        };
        svg.push_str(&format!(
            r#"<text x="10" y="{y}" font-family="{family}" font-size="9.5" font-weight="{weight}" fill="{fill}" xml:space="preserve">{}</text>"#,
            escape(line)
        ));
    }
    svg.push_str("</svg>");
    svg
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn text(
    surface: &mut Surface,
    font: &Font,
    size: f32,
    line: &str,
    x: f32,
    y: f32,
    color: (u8, u8, u8),
) {
    surface.set_fill(Some(fill(color)));
    surface.draw_text(
        Point::from_xy(x, y),
        font.clone(),
        size,
        line,
        false,
        TextDirection::Auto,
    );
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

fn fill((r, g, b): (u8, u8, u8)) -> Fill {
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

/// Wrap at exactly `columns` characters, keeping indentation: for source.
fn wrap_hard(line: &str, columns: usize) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![String::new()];
    }
    chars.chunks(columns).map(|c| c.iter().collect()).collect()
}

fn round(value: f32) -> i32 {
    value.round() as i32
}
