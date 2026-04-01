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
    heading_level: u8,        // 0 = not a heading, 1-9 = heading depth
    indent: i8,               // -1 = just finished list item, 0 = none, >0 = depth
    num_id: Option<String>,   // w:numId for the current paragraph
    order_counters: Vec<u32>, // per-depth counters for ordered lists
}

struct DocxContext {
    markdown: String,

    para: ParaStyle,
    run: RunStyle,

    in_table: bool,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,

    active_hyperlink_rid: Option<String>,
    hyperlink_text: String,
    relationships: HashMap<String, String>,

    images: HashMap<String, (String, String)>,
    /// (numId, ilvl) -> is_ordered
    numbering: HashMap<(String, String), bool>,
}

impl DocxContext {
    fn new(
        relationships: HashMap<String, String>,
        images: HashMap<String, (String, String)>,
        numbering: HashMap<(String, String), bool>,
    ) -> Self {
        Self {
            markdown: String::new(),
            para: ParaStyle::default(),
            run: RunStyle::default(),
            in_table: false,
            table_rows: Vec::new(),
            current_row: Vec::new(),
            active_hyperlink_rid: None,
            hyperlink_text: String::new(),
            relationships,
            images,
            numbering,
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
        if self.in_table {
            self.current_row.push(text.to_string());
        } else if self.active_hyperlink_rid.is_some() {
            self.hyperlink_text.push_str(text);
        } else {
            self.markdown.push_str(text);
        }
    }

    fn end_paragraph(&mut self) {
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

pub fn parse_docx(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
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

    let mut ctx = DocxContext::new(rels, images, numbering);
    let mut reader = Reader::from_str(&xml_content);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:tbl" => {
                    ctx.in_table = true;
                }
                b"w:hyperlink" => {
                    ctx.active_hyperlink_rid = get_attr(&e, b"r:id");
                    ctx.hyperlink_text.clear();
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
                b"a:blip" => {
                    if let Some(rid) = get_attr(&e, b"r:embed")
                        && let Some((media_type, b64)) = ctx.images.get(&rid)
                    {
                        let img = format!("![Image](data:{};base64,{})", media_type, b64);
                        ctx.push_text(&img);
                    }
                }
                _ => {}
            },

            Ok(Event::Text(e)) => {
                let raw = parse_text(&*e)?;
                let styled = ctx.apply_run_style(&raw);
                ctx.reset_run_style();

                if ctx.in_table || ctx.active_hyperlink_rid.is_some() {
                    ctx.push_text(&styled);
                } else if ctx.para.title {
                    ctx.markdown.push_str(&format!("# {}", styled));
                    ctx.para.title = false;
                } else if ctx.para.heading_level > 0 {
                    let hashes = "#".repeat(ctx.para.heading_level as usize + 1);
                    ctx.markdown.push_str(&format!("{} {}", hashes, styled));
                    ctx.para.heading_level = 0;
                } else if ctx.para.indent > 0 {
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
                        ctx.markdown
                            .push_str(&format!("{}{}. {}", indent_str, n, styled));
                    } else {
                        ctx.markdown
                            .push_str(&format!("{}- {}", indent_str, styled));
                    }
                    ctx.para.indent = -1;
                } else {
                    ctx.markdown.push_str(&styled);
                }
            }

            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:tbl" => {
                    ctx.flush_table();
                    ctx.in_table = false;
                    ctx.para = ParaStyle::default();
                }
                b"w:tr" => {
                    ctx.table_rows.push(std::mem::take(&mut ctx.current_row));
                }
                b"w:p" => {
                    ctx.end_paragraph();
                }
                b"w:hyperlink" => {
                    if let Some(rid) = ctx.active_hyperlink_rid.take() {
                        let text = std::mem::take(&mut ctx.hyperlink_text);
                        if let Some(url) = ctx.relationships.get(&rid) {
                            let link = format!("[{}]({})", text.trim(), url);
                            ctx.push_text(&link);
                        } else {
                            ctx.push_text(&text);
                        }
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

    Ok(ctx.markdown)
}

#[cfg(test)]
mod tests {
    use crate::docx::parse_docx;
    static FIXTURE: &[u8] = include_bytes!("../fixtures/fixture.docx");

    fn parse() -> String {
        parse_docx(FIXTURE).expect("fixture should parse without error")
    }

    // ## Headings ##

    #[test]
    fn heading_title() {
        let md = parse();
        assert!(
            md.contains("# Comprehensive DOCX Fixture"),
            "title heading missing"
        );
    }

    #[test]
    fn heading_levels() {
        let md = parse();
        assert!(md.contains("## Heading 1"), "H1 missing");
        assert!(md.contains("### Heading 2"), "H2 missing");
        assert!(md.contains("#### Heading 3"), "H3 missing");
    }

    // ## Inline formatting ##

    #[test]
    fn bold() {
        let md = parse();
        assert!(md.contains("**Bold.**"), "bold missing");
    }

    #[test]
    fn italic() {
        let md = parse();
        assert!(md.contains("*Italic.*"), "italic missing");
    }

    #[test]
    fn bold_italic() {
        let md = parse();
        // bold wraps italic or vice-versa depending on run order
        assert!(
            md.contains("***Bold-italic.***") || md.contains("**_Bold-italic._**"),
            "bold-italic missing"
        );
    }

    #[test]
    fn strikethrough() {
        let md = parse();
        assert!(md.contains("~~Strikethrough.~~"), "strikethrough missing");
    }

    #[test]
    fn underline() {
        let md = parse();
        assert!(md.contains("<u>Underline.</u>"), "underline missing");
    }

    // ## Paragraph formatting ##

    #[test]
    fn plain_paragraph_text() {
        let md = parse();
        assert!(md.contains("Normal text."), "normal paragraph text missing");
    }

    // ## Unordered lists ##

    #[test]
    fn unordered_list_items() {
        let md = parse();
        assert!(md.contains("- Bullet item one"), "bullet item 1 missing");
        assert!(md.contains("- Bullet item two"), "bullet item 2 missing");
        assert!(md.contains("- Bullet item three"), "bullet item 3 missing");
    }

    #[test]
    fn unordered_list_nested() {
        let md = parse();
        assert!(
            md.contains("  - Nested bullet A"),
            "nested bullet A missing"
        );
        assert!(
            md.contains("  - Nested bullet B"),
            "nested bullet B missing"
        );
        assert!(
            md.contains("    - Deeply nested bullet"),
            "deeply nested bullet missing"
        );
    }

    // ## Ordered lists ##

    #[test]
    fn ordered_list_items() {
        let md = parse();
        assert!(md.contains("1. First item"), "ordered item 1 missing");
        assert!(md.contains("2. Second item"), "ordered item 2 missing");
        assert!(md.contains("3. Third item"), "ordered item 3 missing");
    }

    #[test]
    fn ordered_list_nested() {
        let md = parse();
        assert!(md.contains("  - Sub-item a"), "ordered sub-item a missing");
        assert!(md.contains("  - Sub-item b"), "ordered sub-item b missing");
    }

    // ## Hyperlinks ##

    #[test]
    fn hyperlink_example_com() {
        let md = parse();
        assert!(
            md.contains("[example.com](https://example.com)"),
            "hyperlink to example.com missing"
        );
    }

    #[test]
    fn hyperlink_github() {
        let md = parse();
        assert!(
            md.contains("[python-docx on GitHub](https://github.com/python-openxml/python-docx)"),
            "github hyperlink missing"
        );
    }

    // ## Tables ##

    #[test]
    fn table_headers() {
        let md = parse();
        assert!(md.contains("Name"), "table header 'Name' missing");
        assert!(md.contains("Age"), "table header 'Age' missing");
        assert!(md.contains("City"), "table header 'City' missing");
        assert!(md.contains("Score"), "table header 'Score' missing");
    }

    #[test]
    fn table_rows() {
        let md = parse();
        assert!(md.contains("Alice"), "table row Alice missing");
        assert!(md.contains("New York"), "table row New York missing");
        assert!(md.contains("Bob"), "table row Bob missing");
        assert!(md.contains("Carol"), "table row Carol missing");
    }

    #[test]
    fn table_markdown_separator() {
        let md = parse();
        // markdown tables require a separator row of |---|
        assert!(md.contains("|---"), "table markdown separator missing");
    }

    // ## Inline image ##

    #[test]
    fn inline_image_base64() {
        let md = parse();
        assert!(
            md.contains("![Image](data:image/png;base64,"),
            "inline base64 image missing"
        );
    }

    // ## Page break ##

    #[test]
    fn page_break() {
        let md = parse();
        assert!(md.contains("---"), "page break (---) missing");
    }

    // ## Line break ##

    #[test]
    fn soft_line_break() {
        let md = parse();
        assert!(
            md.contains("Line one."),
            "line break content 'line one' missing"
        );
        assert!(
            md.contains("Line two (soft break, same paragraph)."),
            "line break content 'line two' missing"
        );
        // soft break emits "  \n"
        assert!(
            md.contains("  \n"),
            "soft line break (two spaces + newline) missing"
        );
    }

    // ## Tab ##

    #[test]
    fn tab_character() {
        let md = parse();
        assert!(md.contains("Column A\tColumn B"), "tab character missing");
    }

    // ## Unicode ##

    #[test]
    fn unicode_content() {
        let md = parse();
        assert!(md.contains("中文"), "CJK unicode missing");
        assert!(md.contains("שלום"), "Hebrew unicode missing");
    }

    // ## Mixed cell content ##

    #[test]
    fn hyperlink_inside_table() {
        let md = parse();
        assert!(
            md.contains("Hyperlink in cell"),
            "hyperlink text inside table cell missing"
        );
    }

    // ## Empty paragraphs ##

    #[test]
    fn content_around_empty_paragraphs() {
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

    // ## No crash / valid output ##

    #[test]
    fn output_is_nonempty() {
        let md = parse();
        assert!(!md.trim().is_empty(), "output should not be empty");
    }

    #[test]
    fn no_raw_xml_in_output() {
        let md = parse();
        // No XML tags should leak into output
        assert!(
            !md.contains("<w:"),
            "raw WordprocessingML XML leaked into output"
        );
        assert!(!md.contains("<a:"), "raw DrawingML XML leaked into output");
    }
}
