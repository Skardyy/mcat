use crate::{error::ParsingError, parse_text};

use super::sheets;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[derive(Default, Clone)]
struct RunStyle {
    bold: bool,
    italics: bool,
    strike: bool,
    underline: bool,
}

#[derive(Default)]
struct ParaStyle {
    title: bool,
    heading_level: u8,
    indent: i8,
    num_id: Option<String>,
    order_counters: Vec<u32>,
}

struct DocxContext {
    markdown: String,
    para: ParaStyle,
    run: RunStyle,
    run_buf: String,
    in_table: bool,
    in_cell: bool,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,
    active_hyperlink_rid: Option<String>,
    hyperlink_text: String,
    relationships: HashMap<String, String>,
    images: HashMap<String, (String, String)>,
    numbering: HashMap<(String, String), bool>,
    inline: bool,
    in_drawing: bool,
}

impl DocxContext {
    fn new(
        relationships: HashMap<String, String>,
        images: HashMap<String, (String, String)>,
        numbering: HashMap<(String, String), bool>,
        inline: bool,
    ) -> Self {
        Self {
            markdown: String::new(),
            para: ParaStyle::default(),
            run: RunStyle::default(),
            run_buf: String::new(),
            in_table: false,
            in_cell: false,
            table_rows: Vec::new(),
            current_row: Vec::new(),
            active_hyperlink_rid: None,
            hyperlink_text: String::new(),
            relationships,
            images,
            numbering,
            inline,
            in_drawing: false,
        }
    }

    fn flush_table(&mut self) {
        if !self.table_rows.is_empty() {
            let headers = self.table_rows[0].clone();
            let data_rows = if self.table_rows.len() > 1 {
                self.table_rows[1..].to_vec()
            } else {
                Vec::new()
            };
            self.markdown
                .push_str(&sheets::to_markdown_table(&headers, &data_rows));
            self.markdown.push('\n');
            self.table_rows = Vec::new();
        }
    }

    fn push_text(&mut self, text: &str) {
        if self.active_hyperlink_rid.is_some() {
            self.hyperlink_text.push_str(text);
        } else if self.in_table {
            if self.in_cell
                && let Some(last) = self.current_row.last_mut()
            {
                last.push_str(text);
                return;
            }
            self.current_row.push(text.to_string());
        } else {
            self.markdown.push_str(text);
        }
    }

    fn end_paragraph(&mut self) {
        if self.in_table {
            return;
        }
        if self.para.indent == -1 {
            self.para.indent = 0;
            self.markdown.push_str("  \n");
        } else {
            self.markdown.push_str("\n\n");
        }
        let counters = std::mem::take(&mut self.para.order_counters);
        self.para = ParaStyle::default();
        self.para.order_counters = counters;
    }

    fn apply_run_style(&self, raw: &str) -> String {
        let mut text = raw.to_string();
        if self.run.bold && self.run.italics {
            text = format!("***{}*** ", text.trim());
        } else if self.run.bold {
            text = format!("**{}** ", text.trim());
        } else if self.run.italics {
            text = format!("*{}* ", text.trim());
        }
        if self.run.underline {
            text = format!("<u>{}</u> ", text.trim());
        }
        if self.run.strike {
            text = format!("~~{}~~ ", text.trim());
        }
        text
    }

    fn reset_run_style(&mut self) {
        self.run = RunStyle::default();
    }
}

fn get_attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == key {
            return Some(attr.unescape_value().ok()?.into_owned());
        }
    }
    None
}

fn load_relationships(archive: &mut ZipArchive<Cursor<&[u8]>>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut xml = String::new();
    for path in &["word/_rels/document.xml.rels", "_rels/.rels"] {
        if let Ok(mut f) = archive.by_name(path) {
            let _ = f.read_to_string(&mut xml);
            break;
        }
    }
    if xml.is_empty() {
        return map;
    }
    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) if e.name().as_ref() == b"Relationship" => {
                if let (Some(id), Some(target)) = (get_attr(&e, b"Id"), get_attr(&e, b"Target")) {
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

fn load_numbering(archive: &mut ZipArchive<Cursor<&[u8]>>) -> HashMap<(String, String), bool> {
    let mut xml = String::new();
    if let Ok(mut f) = archive.by_name("word/numbering.xml") {
        let _ = f.read_to_string(&mut xml);
    }
    if xml.is_empty() {
        return HashMap::new();
    }
    let mut abstract_level_ordered: HashMap<(String, String), bool> = HashMap::new();
    let mut num_to_abstract: HashMap<String, String> = HashMap::new();
    let mut reader = Reader::from_str(&xml);
    let mut buf = Vec::new();
    let mut current_abstract_id: Option<String> = None;
    let mut current_ilvl: Option<String> = None;
    let mut current_num_id: Option<String> = None;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:abstractNum" => {
                    current_abstract_id = get_attr(&e, b"w:abstractNumId");
                }
                b"w:lvl" => {
                    current_ilvl = get_attr(&e, b"w:ilvl");
                }
                b"w:num" => {
                    current_num_id = get_attr(&e, b"w:numId");
                }
                _ => {}
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:numFmt" => {
                    if let (Some(abs_id), Some(ilvl), Some(val)) = (
                        current_abstract_id.as_ref(),
                        current_ilvl.as_ref(),
                        get_attr(&e, b"w:val"),
                    ) {
                        abstract_level_ordered
                            .insert((abs_id.clone(), ilvl.clone()), val == "decimal");
                    }
                }
                b"w:abstractNumId" => {
                    if let (Some(num_id), Some(abs_id)) =
                        (current_num_id.as_ref(), get_attr(&e, b"w:val"))
                    {
                        num_to_abstract.insert(num_id.clone(), abs_id);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:abstractNum" => current_abstract_id = None,
                b"w:lvl" => current_ilvl = None,
                b"w:num" => current_num_id = None,
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    let mut result = HashMap::new();
    for (num_id, abs_id) in &num_to_abstract {
        for ((a_id, ilvl), ordered) in &abstract_level_ordered {
            if a_id == abs_id {
                result.insert((num_id.clone(), ilvl.clone()), *ordered);
            }
        }
    }
    result
}

fn load_images(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    rels: &HashMap<String, String>,
) -> HashMap<String, (String, String)> {
    let mut map = HashMap::new();
    for (rid, target) in rels {
        let path = if target.starts_with("media/") {
            format!("word/{}", target)
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
                use base64::{Engine as _, engine::general_purpose::STANDARD};
                map.insert(
                    rid.clone(),
                    (media_type.to_string(), STANDARD.encode(&bytes)),
                );
            }
        }
    }
    map
}

pub fn parse_docx(content: impl AsRef<[u8]>, inline: bool) -> Result<String, ParsingError> {
    let bytes = content.as_ref();
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

    let rels = load_relationships(&mut archive);
    let images = load_images(&mut archive, &rels);
    let numbering = load_numbering(&mut archive);

    let mut xml_content = String::new();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;
        if file.name() == "word/document.xml" {
            file.read_to_string(&mut xml_content)?;
            break;
        }
    }

    let mut ctx = DocxContext::new(rels, images, numbering, inline);
    let mut reader = Reader::from_str(&xml_content);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:tbl" => {
                    ctx.in_table = true;
                }
                b"w:tc" => {
                    ctx.in_cell = true;
                    ctx.current_row.push(String::new());
                }
                b"w:hyperlink" => {
                    ctx.active_hyperlink_rid = get_attr(&e, b"r:id");
                    ctx.hyperlink_text.clear();
                }
                b"w:drawing" => {
                    ctx.in_drawing = true;
                }
                _ => {}
            },

            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:b" => {
                    ctx.run.bold = match get_attr(&e, b"w:val").as_deref() {
                        Some(v) => v == "true",
                        None => true,
                    };
                }
                b"w:i" => {
                    ctx.run.italics = match get_attr(&e, b"w:val").as_deref() {
                        Some(v) => v == "true",
                        None => true,
                    };
                }
                b"w:strike" => {
                    ctx.run.strike = match get_attr(&e, b"w:val").as_deref() {
                        Some(v) => v == "true",
                        None => true,
                    };
                }
                b"w:u" => {
                    ctx.run.underline = true;
                }
                b"w:pStyle" => {
                    if let Some(val) = get_attr(&e, b"w:val") {
                        let lower = val.to_lowercase();
                        if lower.contains("title") {
                            ctx.para.title = true;
                            ctx.para.heading_level = 0;
                            ctx.para.indent = 0;
                        } else if lower.contains("heading") {
                            let level: u8 = lower
                                .chars()
                                .rev()
                                .find(|c| c.is_ascii_digit())
                                .and_then(|c| c.to_digit(10))
                                .unwrap_or(1) as u8;
                            ctx.para.heading_level = level;
                            ctx.para.indent = 0;
                        }
                    }
                }
                b"w:ilvl" => {
                    if ctx.para.heading_level > 0 || ctx.para.title {
                    } else if let Some(val) = get_attr(&e, b"w:val")
                        && let Ok(v) = val.parse::<i8>()
                    {
                        ctx.para.indent = v + 1;
                    }
                }
                b"w:numId" => {
                    if let Some(val) = get_attr(&e, b"w:val") {
                        ctx.para.num_id = Some(val);
                    }
                }
                b"w:tab" => {
                    ctx.push_text("\t");
                }
                b"w:br" => {
                    ctx.push_text("\n");
                }
                b"a:blip" => {
                    if let Some(rid) = get_attr(&e, b"r:embed")
                        && let Some((media_type, b64)) = ctx.images.get(&rid)
                    {
                        let img = if ctx.inline {
                            format!("![Image](data:{};base64,{})", media_type, b64)
                        } else {
                            "![Image]()".to_string()
                        };
                        ctx.push_text(&img);
                    }
                }
                _ => {}
            },

            Ok(Event::Text(e)) => {
                if ctx.in_drawing {
                    continue;
                }
                let raw = parse_text(&*e)?;
                ctx.run_buf.push_str(&raw);
            }

            Ok(Event::GeneralRef(e)) => {
                if ctx.in_drawing {
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
                ctx.run_buf.push_str(replacement);
            }

            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:tbl" => {
                    ctx.flush_table();
                    ctx.in_table = false;
                    ctx.para = ParaStyle::default();
                }
                b"w:tc" => {
                    ctx.in_cell = false;
                }
                b"w:tr" => {
                    ctx.table_rows.push(std::mem::take(&mut ctx.current_row));
                }
                b"w:r" => {
                    let raw = std::mem::take(&mut ctx.run_buf);
                    if !raw.is_empty() {
                        let styled = ctx.apply_run_style(&raw);

                        if !ctx.in_table && ctx.para.indent > 0 {
                            let depth = ctx.para.indent as usize;
                            let ilvl = (depth - 1).to_string();
                            let ordered = ctx
                                .para
                                .num_id
                                .as_ref()
                                .and_then(|nid| ctx.numbering.get(&(nid.clone(), ilvl)))
                                .copied()
                                .unwrap_or(false);
                            let indent_str = "  ".repeat(depth);
                            if ordered {
                                while ctx.para.order_counters.len() < depth {
                                    ctx.para.order_counters.push(0);
                                }
                                ctx.para.order_counters[depth - 1] += 1;
                                let n = ctx.para.order_counters[depth - 1];
                                ctx.markdown.push_str(&format!("{}{}. ", indent_str, n));
                            } else {
                                ctx.markdown.push_str(&format!("{}- ", indent_str));
                            }
                            ctx.para.indent = -1;
                        }

                        if ctx.in_table || ctx.active_hyperlink_rid.is_some() {
                            ctx.push_text(&styled);
                        } else if ctx.para.title {
                            ctx.markdown.push_str(&format!("# {}", styled));
                            ctx.para.title = false;
                        } else if ctx.para.heading_level > 0 {
                            let hashes = "#".repeat(ctx.para.heading_level as usize);
                            ctx.markdown.push_str(&format!("{} {}", hashes, styled));
                            ctx.para.heading_level = 0;
                        } else {
                            ctx.markdown.push_str(&styled);
                        }
                    }
                    ctx.reset_run_style();
                }
                b"w:p" => {
                    ctx.end_paragraph();
                }
                b"w:hyperlink" => {
                    if let Some(rid) = ctx.active_hyperlink_rid.take() {
                        let text = std::mem::take(&mut ctx.hyperlink_text);
                        if let Some(url) = ctx.relationships.get(&rid) {
                            let link = format!("[{}]({})", text, url);
                            ctx.push_text(&link);
                        } else {
                            ctx.push_text(&text);
                        }
                    }
                }
                b"w:drawing" => {
                    ctx.in_drawing = false;
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

    Ok(ctx.markdown)
}

#[cfg(test)]
mod tests {
    use crate::docx::parse_docx;
    static FIXTURE: &[u8] = include_bytes!("../fixtures/fixture.docx");

    fn parse() -> String {
        parse_docx(FIXTURE, true).expect("fixture should parse without error")
    }

    #[test]
    fn headings() {
        let md = parse();
        assert!(md.contains("# Comprehensive DOCX Fixture"), "title missing");
        assert!(md.contains("# Heading 1"), "H1 missing");
        assert!(md.contains("## Heading 2"), "H2 missing");
        assert!(md.contains("### Heading 3"), "H3 missing");
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
    fn unordered_lists() {
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
    fn ordered_lists() {
        let md = parse();
        assert!(md.contains("1. First item"), "ordered item 1 missing");
        assert!(md.contains("2. Second item"), "ordered item 2 missing");
        assert!(md.contains("3. Third item"), "ordered item 3 missing");
        assert!(md.contains("  - Sub-item a"), "sub-item a missing");
        assert!(md.contains("  - Sub-item b"), "sub-item b missing");
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
    fn table_structure() {
        let md = parse();
        assert!(
            md.contains("| Name |") || md.contains("| **Name**"),
            "table header missing"
        );
        assert!(md.contains("|---"), "table GFM separator missing");
        assert!(md.contains("| Alice |"), "Alice row missing");
        assert!(md.contains("| New York |"), "New York cell missing");
        assert!(md.contains("| Bob |"), "Bob row missing");
        assert!(md.contains("| Carol |"), "Carol row missing");
        assert!(md.contains("| 95 |"), "Alice score missing");
        assert!(
            md.contains("| 87 |") || md.contains("| 88 |"),
            "Bob score missing"
        );
        assert!(md.contains("| 92 |"), "Carol score missing");
    }

    #[test]
    fn table_no_blank_lines_inside() {
        let md = parse();
        let table_start = md
            .find("| Name |")
            .or_else(|| md.find("| **Name**"))
            .expect("table header not found");
        let table_end = md[table_start..]
            .find("\n\n")
            .map(|i| table_start + i)
            .unwrap_or(md.len());
        let table = &md[table_start..table_end];
        assert!(
            !table.contains("\n\n"),
            "blank line found inside table:\n{}",
            table
        );
    }

    #[test]
    fn hyperlink_inside_table() {
        let md = parse();
        assert!(
            md.contains("[Hyperlink in cell](https://example.com)"),
            "hyperlink in table cell missing or malformed"
        );
    }

    #[test]
    fn inline_image_base64() {
        let md = parse();
        assert!(
            md.contains("![Image](data:image/png;base64,"),
            "inline base64 image missing"
        );
    }

    #[test]
    fn page_break() {
        let md = parse();
        assert!(md.contains("---"), "page break separator missing");
    }

    #[test]
    fn soft_line_break() {
        let md = parse();
        let pos1 = md.find("Line one.").expect("'Line one.' missing");
        let pos2 = md
            .find("Line two (soft break, same paragraph).")
            .expect("'Line two' missing");
        assert!(pos2 > pos1, "line two should come after line one");
        assert!(
            &md[pos1..pos2].contains('\n'),
            "line one and line two are joined without a break"
        );
        assert!(md.contains("  \n"), "markdown soft line break missing");
    }

    #[test]
    fn tab_character() {
        let md = parse();
        assert!(md.contains("Column A\tColumn B"), "tab character missing");
    }

    #[test]
    fn unicode() {
        let md = parse();
        assert!(md.contains("中文"), "CJK missing");
        assert!(md.contains("שלום"), "Hebrew missing");
    }

    #[test]
    fn empty_paragraphs() {
        let md = parse();
        assert!(
            md.contains("Paragraph before empty."),
            "pre-empty paragraph missing"
        );
        assert!(
            md.contains("Paragraph after two empty paragraphs."),
            "post-empty paragraph missing"
        );
    }

    #[test]
    fn no_raw_xml() {
        let md = parse();
        assert!(!md.contains("<w:"), "WordprocessingML XML leaked");
        assert!(!md.contains("<a:"), "DrawingML XML leaked");
    }
}
