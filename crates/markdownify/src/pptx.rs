/// yeah pptx is pretty bad format, not impl everything here, since it also not structured like
/// docx for instance..
use crate::{error::ParsingError, parse_text};

use super::sheets;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io::{BufRead, Cursor, Read};
use zip::ZipArchive;

#[derive(Default, Clone)]
struct RunStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
}

#[derive(Default)]
struct ParaState {
    is_bullet: bool,
    level: u8,
    ppr_seen: bool,
}

struct PptxContext {
    slide_md: String,
    inline: bool,

    run: RunStyle,
    para: ParaState,

    in_tx_body: bool,
    in_para_pr: bool,
    in_run_pr: bool,

    in_title_shape: bool,

    active_hlink_rid: Option<String>,
    hlink_run_text: Option<String>,

    in_table: bool,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    in_cell: bool,
    cell_text: String,

    para_buf: String,

    relationships: HashMap<String, String>,
    images: HashMap<String, (String, String)>,
}

impl PptxContext {
    fn new(
        relationships: HashMap<String, String>,
        images: HashMap<String, (String, String)>,
        inline: bool,
    ) -> Self {
        Self {
            slide_md: String::new(),
            inline,
            run: RunStyle::default(),
            para: ParaState::default(),
            in_tx_body: false,
            in_para_pr: false,
            in_run_pr: false,
            in_title_shape: false,
            active_hlink_rid: None,
            hlink_run_text: None,
            in_table: false,
            table_rows: Vec::new(),
            current_row: Vec::new(),
            in_cell: false,
            cell_text: String::new(),
            para_buf: String::new(),
            relationships,
            images,
        }
    }

    fn apply_run_style(&self, text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return text.to_string();
        }
        let lead = if text.starts_with(' ') { " " } else { "" };
        let trail = if text.ends_with(' ') { " " } else { "" };
        let s = match (self.run.bold, self.run.italic) {
            (true, true) => format!("***{}***", trimmed),
            (true, false) => format!("**{}**", trimmed),
            (false, true) => format!("*{}*", trimmed),
            _ => trimmed.to_string(),
        };
        let s = if self.run.strike {
            format!("~~{}~~", s.trim())
        } else {
            s
        };
        let s = if self.run.underline {
            format!("<u>{}</u>", s.trim())
        } else {
            s
        };
        format!("{}{}{}", lead, s, trail)
    }

    fn push_run_text(&mut self, styled: &str, raw: &str) {
        if self.in_cell {
            self.cell_text.push_str(styled);
        } else if let Some(ref mut buf) = self.hlink_run_text {
            buf.push_str(raw.trim());
        } else {
            self.para_buf.push_str(styled);
        }
    }

    fn flush_para(&mut self) {
        let text = std::mem::take(&mut self.para_buf);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            self.slide_md.push('\n');
        } else if self.in_title_shape {
            let clean = trimmed.trim_matches('*').trim_matches('~').trim();
            self.slide_md.push_str(&format!("# {}\n", clean));
        } else if self.para.is_bullet {
            let indent = "  ".repeat(self.para.level as usize);
            self.slide_md
                .push_str(&format!("{}- {}\n", indent, trimmed));
        } else {
            self.slide_md.push_str(&format!("{}\n", trimmed));
        }
        self.para = ParaState::default();
    }

    fn flush_table(&mut self) {
        if !self.table_rows.is_empty() {
            let headers = self.table_rows[0].clone();
            let data = if self.table_rows.len() > 1 {
                self.table_rows[1..].to_vec()
            } else {
                Vec::new()
            };
            self.slide_md
                .push_str(&sheets::to_markdown_table(&headers, &data));
            self.slide_md.push('\n');
            self.table_rows.clear();
        }
    }

    fn emit_image(&mut self, rid: &str) {
        if let Some((mt, b64)) = self.images.get(rid) {
            let img = if self.inline {
                format!("![Image](data:{};base64,{})\n", mt, b64)
            } else {
                "![Image]()\n".to_string()
            };
            self.slide_md.push_str(&img);
        }
    }
}

fn get_attr(
    e: &quick_xml::events::BytesStart,
    key: &[u8],
    reader: &Reader<impl BufRead>,
) -> Option<String> {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == key {
            return Some(
                attr.decode_and_unescape_value(reader.decoder())
                    .ok()?
                    .into_owned(),
            );
        }
    }
    None
}

// yeah im just guessing..
fn mar_l_to_level(mar_l: u32) -> u8 {
    match mar_l {
        0..=400_000 => 0,
        400_001..=800_000 => 1,
        _ => 2,
    }
}

fn load_slide_rels(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    slide_path: &str,
) -> HashMap<String, String> {
    let file_name = slide_path.split('/').next_back().unwrap_or("");
    let rels_path = format!("ppt/slides/_rels/{}.rels", file_name);
    let mut xml = String::new();
    if let Ok(mut f) = archive.by_name(&rels_path) {
        let _ = f.read_to_string(&mut xml);
    }
    let mut map = HashMap::new();
    if xml.is_empty() {
        return map;
    }

    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) if e.name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(target)) = (
                    get_attr(&e, b"Id", &reader),
                    get_attr(&e, b"Target", &reader),
                ) {
                    map.insert(id, target);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    map
}

fn load_slide_images(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    rels: &HashMap<String, String>,
) -> HashMap<String, (String, String)> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut map = HashMap::new();
    for (rid, target) in rels {
        let path = if target.starts_with("../") {
            format!("ppt/{}", &target.strip_prefix("../").unwrap_or(target))
        } else {
            target.clone()
        };
        let media_type = if path.ends_with(".png") {
            "image/png"
        } else if path.ends_with(".jpg") || path.ends_with(".jpeg") {
            "image/jpeg"
        } else if path.ends_with(".gif") {
            "image/gif"
        } else if path.ends_with(".svg") {
            "image/svg+xml"
        } else {
            continue;
        };
        if let Ok(mut f) = archive.by_name(&path) {
            let mut bytes = Vec::new();
            if f.read_to_end(&mut bytes).is_ok() {
                map.insert(
                    rid.clone(),
                    (media_type.to_string(), STANDARD.encode(&bytes)),
                );
            }
        }
    }
    map
}

fn load_notes(archive: &mut ZipArchive<Cursor<&[u8]>>, slide_index: usize) -> Option<String> {
    let path = format!("ppt/notesSlides/notesSlide{}.xml", slide_index);
    let mut xml = String::new();
    archive.by_name(&path).ok()?.read_to_string(&mut xml).ok()?;

    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();
    let mut in_notes_body = false;
    let mut in_tx_body = false;
    let mut ph_is_body = false;
    let mut text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"p:sp" => {
                    ph_is_body = false;
                }
                b"p:txBody" => {
                    if ph_is_body {
                        in_notes_body = true;
                    }
                    in_tx_body = true;
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"p:ph" {
                    ph_is_body = get_attr(&e, b"type", &reader).as_deref() == Some("body");
                }
            }
            Ok(Event::Text(e)) => {
                if in_notes_body
                    && in_tx_body
                    && let Ok(t) = parse_text(&*e)
                {
                    let t = t.trim();
                    if !t.is_empty() {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(t);
                    }
                }
            }
            Ok(Event::GeneralRef(e)) => {
                if in_notes_body
                    && in_tx_body
                    && let Ok(name) = parse_text(&*e)
                {
                    let replacement = match name.as_ref() {
                        "gt" => ">",
                        "lt" => "<",
                        "amp" => "&",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => continue,
                    };
                    text.push_str(replacement);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"p:sp" => {
                    in_notes_body = false;
                    ph_is_body = false;
                }
                b"p:txBody" => {
                    in_tx_body = false;
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    if text.is_empty() { None } else { Some(text) }
}

fn collect_slide_paths(archive: &mut ZipArchive<Cursor<&[u8]>>) -> Vec<String> {
    let mut paths: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            let name = archive.by_index(i).ok()?.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    paths.sort_by_key(|p| {
        p.trim_start_matches("ppt/slides/slide")
            .trim_end_matches(".xml")
            .parse::<u32>()
            .unwrap_or(0)
    });
    paths
}

fn detect_title_shape_id(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current_sp_id: Option<String> = None;
    let mut best_id: Option<String> = None;
    let mut best_sz: u32 = 0;
    let mut in_ph = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => match e.name().as_ref() {
                b"p:sp" => {
                    current_sp_id = None;
                    in_ph = false;
                }
                b"p:cNvPr" => {
                    current_sp_id = get_attr(&e, b"id", &reader);
                }
                b"p:ph" => {
                    let t = get_attr(&e, b"type", &reader);
                    if matches!(t.as_deref(), Some("title") | Some("ctrTitle")) {
                        return current_sp_id;
                    }
                    in_ph = true;
                }
                b"a:rPr" => {
                    if !in_ph
                        && let Some(sz) =
                            get_attr(&e, b"sz", &reader).and_then(|v| v.parse::<u32>().ok())
                        && sz > best_sz
                    {
                        best_sz = sz;
                        best_id = current_sp_id.clone();
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    best_id
}

fn parse_slide(xml: &str, mut ctx: PptxContext) -> Result<String, ParsingError> {
    let title_shape_id = detect_title_shape_id(xml);

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"p:sp" => {
                    ctx.in_title_shape = false;
                }
                b"p:cNvPr" => {
                    let id = get_attr(&e, b"id", &reader);
                    ctx.in_title_shape = title_shape_id.is_some() && id == title_shape_id;
                }
                b"p:ph" => {
                    let t = get_attr(&e, b"type", &reader);
                    ctx.in_title_shape = matches!(t.as_deref(), Some("title") | Some("ctrTitle"));
                }
                b"p:txBody" => {
                    ctx.in_tx_body = true;
                }
                b"a:pPr" => {
                    if ctx.in_tx_body && !ctx.para.ppr_seen {
                        ctx.in_para_pr = true;
                        ctx.para.ppr_seen = true;
                        let mar_l = get_attr(&e, b"marL", &reader)
                            .and_then(|v| v.parse::<u32>().ok())
                            .unwrap_or(0);
                        if mar_l > 0 {
                            ctx.para.is_bullet = true;
                            ctx.para.level = mar_l_to_level(mar_l);
                        }
                    }
                }
                b"a:rPr" => {
                    if ctx.in_tx_body {
                        ctx.in_run_pr = true;
                        ctx.run.bold = get_attr(&e, b"b", &reader).as_deref() == Some("1");
                        ctx.run.italic = get_attr(&e, b"i", &reader).as_deref() == Some("1");
                        ctx.run.strike = matches!(
                            get_attr(&e, b"strike", &reader).as_deref(),
                            Some("sngStrike") | Some("dblStrike")
                        );
                        ctx.run.underline = matches!(get_attr(&e, b"u", &reader).as_deref(),
                                                Some(u) if u != "none");
                    }
                }
                b"a:hlinkClick" => {
                    ctx.active_hlink_rid = get_attr(&e, b"r:id", &reader);
                    ctx.hlink_run_text = Some(String::new());
                }
                b"a:tbl" => {
                    ctx.in_table = true;
                    ctx.table_rows.clear();
                }
                b"a:tr" => {
                    if ctx.in_table {
                        ctx.current_row.clear();
                    }
                }
                b"a:tc" => {
                    if ctx.in_table {
                        ctx.in_cell = true;
                        ctx.cell_text.clear();
                    }
                }
                b"a:blip" => {
                    if let Some(rid) = get_attr(&e, b"r:embed", &reader) {
                        ctx.emit_image(&rid);
                    }
                }
                _ => {}
            },

            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"p:cNvPr" => {
                    let id = get_attr(&e, b"id", &reader);
                    ctx.in_title_shape = title_shape_id.is_some() && id == title_shape_id;
                }
                b"p:ph" => {
                    let t = get_attr(&e, b"type", &reader);
                    ctx.in_title_shape = matches!(t.as_deref(), Some("title") | Some("ctrTitle"));
                }
                b"a:buChar" | b"a:buAutoNum" => {
                    ctx.para.is_bullet = true;
                }
                b"a:buNone" => {
                    if ctx.in_para_pr {
                        ctx.para.is_bullet = false;
                        ctx.para.level = 0;
                    }
                }
                b"a:hlinkClick" => {
                    ctx.active_hlink_rid = get_attr(&e, b"r:id", &reader);
                    ctx.hlink_run_text = Some(String::new());
                }
                b"a:blip" => {
                    if let Some(rid) = get_attr(&e, b"r:embed", &reader) {
                        ctx.emit_image(&rid);
                    }
                }
                _ => {}
            },

            Ok(Event::Text(e)) => {
                if !ctx.in_tx_body && !ctx.in_cell {
                    continue;
                }
                let raw = parse_text(&*e)?;
                let styled = if raw.trim().is_empty() {
                    ctx.run = RunStyle::default();
                    raw.clone()
                } else {
                    let s = ctx.apply_run_style(&raw);
                    if ctx.hlink_run_text.is_none() {
                        ctx.run = RunStyle::default();
                    }
                    s
                };
                ctx.push_run_text(&styled, &raw);
            }

            Ok(Event::GeneralRef(e)) => {
                if !ctx.in_tx_body && !ctx.in_cell {
                    continue;
                }
                let name = parse_text(&*e)?;
                let replacement = match name.as_ref() {
                    "gt" => ">",
                    "lt" => "<",
                    "amp" => "&",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => continue,
                };
                ctx.push_run_text(replacement, replacement);
            }

            Ok(Event::End(e)) => match e.name().as_ref() {
                b"p:sp" => {
                    ctx.in_title_shape = false;
                }
                b"p:txBody" => {
                    ctx.in_tx_body = false;
                    ctx.slide_md.push('\n');
                }
                b"a:pPr" => {
                    ctx.in_para_pr = false;
                }
                b"a:rPr" => {
                    ctx.in_run_pr = false;
                }
                b"a:r" => {
                    if let Some(htext) = ctx.hlink_run_text.take() {
                        let url = ctx
                            .active_hlink_rid
                            .take()
                            .and_then(|rid| ctx.relationships.get(&rid))
                            .cloned()
                            .unwrap_or_default();
                        ctx.para_buf
                            .push_str(&format!("[{}]({})", htext.trim(), url));
                    }
                    ctx.active_hlink_rid = None;
                    ctx.run = RunStyle::default();
                }
                b"a:p" => {
                    if ctx.in_tx_body {
                        ctx.flush_para();
                    }
                }
                b"a:tbl" => {
                    ctx.flush_table();
                    ctx.in_table = false;
                }
                b"a:tr" => {
                    if ctx.in_table {
                        ctx.table_rows.push(std::mem::take(&mut ctx.current_row));
                    }
                }
                b"a:tc" => {
                    if ctx.in_table {
                        ctx.current_row.push(ctx.cell_text.trim().to_string());
                        ctx.cell_text.clear();
                        ctx.in_cell = false;
                    }
                }
                _ => {}
            },

            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(ParsingError::ParsingError(format!(
                    "Error at position {}: {:?}",
                    reader.buffer_position(),
                    e
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(ctx.slide_md)
}

pub fn parse_pptx(content: impl AsRef<[u8]>, inline: bool) -> Result<String, ParsingError> {
    let bytes = content.as_ref();
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

    let slide_paths = collect_slide_paths(&mut archive);
    let mut slides: Vec<String> = Vec::new();

    for (idx, path) in slide_paths.iter().enumerate() {
        let slide_num = idx + 1;
        let rels = load_slide_rels(&mut archive, path);
        let images = load_slide_images(&mut archive, &rels);

        let mut xml = String::new();
        archive
            .by_name(path)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?
            .read_to_string(&mut xml)?;

        let ctx = PptxContext::new(rels, images, inline);
        let mut slide_md = parse_slide(&xml, ctx)?;

        if let Some(notes) = load_notes(&mut archive, slide_num) {
            slide_md.push_str(&format!("\n> {}\n", notes));
        }

        let trimmed = slide_md.trim().to_string();
        if !trimmed.is_empty() {
            slides.push(trimmed);
        }
    }

    Ok(slides.join("\n\n---\n\n"))
}

#[cfg(test)]
mod tests {
    use super::parse_pptx;
    static FIXTURE: &[u8] = include_bytes!("../fixtures/fixture.pptx");

    fn parse() -> String {
        parse_pptx(FIXTURE, true).expect("fixture should parse without error")
    }

    #[test]
    fn slide_structure() {
        let md = parse();
        assert_eq!(
            md.matches("\n---\n").count(),
            6,
            "expected 6 slide separators"
        );
        assert!(
            !md.trim_end().ends_with("---"),
            "must not end with separator"
        );
    }

    #[test]
    fn slide1_title_and_subtitle() {
        let md = parse();
        assert!(md.contains("# Comprehensive PPTX Fixture"), "title missing");
        assert!(
            md.contains("Subtitle text on title slide"),
            "subtitle missing"
        );
    }

    #[test]
    fn speaker_notes() {
        let md = parse();
        assert!(
            md.contains("Speaker notes for slide one."),
            "slide 1 notes missing"
        );
        assert!(
            md.contains("These are the speaker notes for slide seven."),
            "slide 7 notes missing"
        );
    }

    #[test]
    fn inline_formatting() {
        let md = parse();
        assert!(md.contains("Normal text."), "plain text missing");
        assert!(md.contains("**Bold.**"), "bold missing");
        assert!(md.contains("*Italic.*"), "italic missing");
        assert!(
            md.contains("***Bold-italic.***") || md.contains("**_Bold-italic._**"),
            "bold-italic missing"
        );
        assert!(md.contains("~~Strikethrough.~~"), "strikethrough missing");
        assert!(md.contains("<u>Underline.</u>"), "underline missing");
    }

    #[test]
    fn hyperlinks() {
        let md = parse();
        assert!(
            md.contains("[example.com](https://example.com)"),
            "example.com hyperlink missing"
        );
        assert!(
            md.contains("[python-docx on GitHub](https://github.com/python-openxml/python-docx)"),
            "github hyperlink missing"
        );
    }

    #[test]
    fn bullet_lists() {
        let md = parse();
        assert!(md.contains("- Bullet item one"), "bullet 1 missing");
        assert!(md.contains("- Bullet item two"), "bullet 2 missing");
        assert!(md.contains("- Bullet item three"), "bullet 3 missing");
        assert!(md.contains("  - Nested bullet A"), "nested A missing");
        assert!(md.contains("  - Nested bullet B"), "nested B missing");
        assert!(
            md.contains("    - Deeply nested bullet"),
            "deeply nested missing"
        );
    }

    #[test]
    fn table_structure() {
        let md = parse();
        assert!(md.contains("|---"), "table GFM separator missing");
        assert!(
            md.contains("| Name |") || md.contains("| **Name**"),
            "table header missing"
        );
        assert!(md.contains("| Alice |"), "Alice row missing");
        assert!(md.contains("| New York |"), "New York cell missing");
        assert!(md.contains("| Bob |"), "Bob row missing");
        assert!(md.contains("| Carol |"), "Carol row missing");
    }

    #[test]
    fn image_base64() {
        let md = parse();
        assert!(
            md.contains("![Image](data:image/png;base64,"),
            "base64 image missing"
        );
    }

    #[test]
    fn unicode() {
        let md = parse();
        assert!(md.contains("中文"), "CJK missing");
        assert!(md.contains("שלום"), "Hebrew missing");
    }

    #[test]
    fn no_raw_xml() {
        let md = parse();
        assert!(!md.contains("<p:"), "PresentationML leaked");
        assert!(!md.contains("<a:"), "DrawingML leaked");
    }
}
