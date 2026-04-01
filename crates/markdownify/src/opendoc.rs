use crate::{error::ParsingError, parse_text};

use super::sheets;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[derive(Default, Clone)]
struct TextStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
}

#[derive(Clone)]
struct ListContext {
    ordered: bool,
    depth: u8,
    counters: Vec<u32>,
}

struct OpendocContext {
    markdown: String,
    inline: bool,

    text_styles: HashMap<String, TextStyle>,
    list_styles: HashMap<String, bool>,

    active_span_style: Option<TextStyle>,
    active_link_href: Option<String>,
    link_text: String,

    list_stack: Vec<ListContext>,
    next_item_is_list_para: bool,

    in_table: bool,
    in_cell: bool,
    table_rows: Vec<Vec<String>>,
    current_row: Vec<String>,

    para_buf: String,
    in_para: bool,
    in_heading: Option<u8>,

    images: HashMap<String, (String, String)>,
}

impl OpendocContext {
    fn new(inline: bool, images: HashMap<String, (String, String)>) -> Self {
        Self {
            markdown: String::new(),
            inline,
            text_styles: HashMap::new(),
            list_styles: HashMap::new(),
            active_span_style: None,
            active_link_href: None,
            link_text: String::new(),
            list_stack: Vec::new(),
            next_item_is_list_para: false,
            in_table: false,
            in_cell: false,
            table_rows: Vec::new(),
            current_row: Vec::new(),
            para_buf: String::new(),
            in_para: false,
            in_heading: None,
            images,
        }
    }

    fn apply_style(&self, text: &str, style: &TextStyle) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return text.to_string();
        }
        let lead = if text.starts_with(' ') { " " } else { "" };
        let trail = if text.ends_with(' ') { " " } else { "" };
        let s = match (style.bold, style.italic) {
            (true, true) => format!("***{}***", trimmed),
            (true, false) => format!("**{}**", trimmed),
            (false, true) => format!("*{}*", trimmed),
            _ => trimmed.to_string(),
        };
        let s = if style.strike {
            format!("~~{}~~", s.trim())
        } else {
            s
        };
        let s = if style.underline {
            format!("<u>{}</u>", s.trim())
        } else {
            s
        };
        format!("{}{}{}", lead, s, trail)
    }

    fn push_text(&mut self, text: &str) {
        if self.in_cell {
            if let Some(last) = self.current_row.last_mut() {
                last.push_str(text);
            }
        } else if self.active_link_href.is_some() {
            self.link_text.push_str(text);
        } else {
            self.para_buf.push_str(text);
        }
    }

    fn flush_para(&mut self) {
        let text = std::mem::take(&mut self.para_buf);
        let trimmed = text.trim();

        if let Some(level) = self.in_heading {
            if !trimmed.is_empty() {
                let hashes = "#".repeat(level as usize);
                self.markdown
                    .push_str(&format!("{} {}\n\n", hashes, trimmed));
            }
            self.in_heading = None;
            self.in_para = false;
            return;
        }

        if self.next_item_is_list_para {
            self.next_item_is_list_para = false;
            if let Some(ctx) = self.list_stack.last_mut() {
                let depth = ctx.depth;
                let indent = "  ".repeat((depth - 1) as usize);
                if ctx.ordered {
                    while ctx.counters.len() < depth as usize {
                        ctx.counters.push(0);
                    }
                    ctx.counters[depth as usize - 1] += 1;
                    let n = ctx.counters[depth as usize - 1];
                    self.markdown
                        .push_str(&format!("{}{}. {}\n", indent, n, trimmed));
                } else {
                    self.markdown
                        .push_str(&format!("{}- {}\n", indent, trimmed));
                }
            }
            self.in_para = false;
            return;
        }

        if self.in_para {
            self.markdown.push_str(&format!("{}\n\n", trimmed));
        }
        self.in_para = false;
    }

    fn flush_table(&mut self) {
        if !self.table_rows.is_empty() {
            let headers = self.table_rows[0].clone();
            let data = if self.table_rows.len() > 1 {
                self.table_rows[1..].to_vec()
            } else {
                Vec::new()
            };
            self.markdown
                .push_str(&sheets::to_markdown_table(&headers, &data));
            self.markdown.push('\n');
            self.table_rows.clear();
        }
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

fn load_images(archive: &mut ZipArchive<Cursor<&[u8]>>) -> HashMap<String, (String, String)> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut map = HashMap::new();
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.by_index(i).ok().map(|f| f.name().to_string()))
        .collect();
    for name in names {
        let media_type = if name.ends_with(".png") {
            "image/png"
        } else if name.ends_with(".jpg") || name.ends_with(".jpeg") {
            "image/jpeg"
        } else if name.ends_with(".gif") {
            "image/gif"
        } else if name.ends_with(".svg") {
            "image/svg+xml"
        } else {
            continue;
        };
        if let Ok(mut f) = archive.by_name(&name) {
            let mut bytes = Vec::new();
            if f.read_to_end(&mut bytes).is_ok() {
                map.insert(name, (media_type.to_string(), STANDARD.encode(&bytes)));
            }
        }
    }
    map
}

fn parse_styles(xml: &str) -> (HashMap<String, TextStyle>, HashMap<String, bool>) {
    let mut text_styles: HashMap<String, TextStyle> = HashMap::new();
    let mut list_styles: HashMap<String, bool> = HashMap::new();

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut current_style_name: Option<String> = None;
    let mut current_list_name: Option<String> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => match e.name().as_ref() {
                b"style:style" => {
                    let name = get_attr(&e, b"style:name").unwrap_or_default();
                    let family = get_attr(&e, b"style:family").unwrap_or_default();
                    if family == "text" {
                        current_style_name = Some(name.clone());
                        text_styles.entry(name).or_default();
                    }
                }
                b"style:text-properties" => {
                    if let Some(ref name) = current_style_name {
                        let entry = text_styles.entry(name.clone()).or_default();
                        if get_attr(&e, b"fo:font-weight").as_deref() == Some("bold") {
                            entry.bold = true;
                        }
                        if get_attr(&e, b"fo:font-style").as_deref() == Some("italic") {
                            entry.italic = true;
                        }
                        if matches!(get_attr(&e, b"style:text-line-through-style").as_deref(),
                            Some(s) if s != "none")
                        {
                            entry.strike = true;
                        }
                        if matches!(get_attr(&e, b"style:text-underline-style").as_deref(),
                            Some(s) if s != "none")
                        {
                            entry.underline = true;
                        }
                    }
                }
                b"text:list-style" => {
                    let name = get_attr(&e, b"style:name").unwrap_or_default();
                    current_list_name = Some(name.clone());
                    list_styles.entry(name).or_insert(false);
                }
                b"text:list-level-style-number" => {
                    if let Some(ref name) = current_list_name {
                        list_styles.insert(name.clone(), true);
                    }
                }
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"style:style" => {
                    current_style_name = None;
                }
                b"text:list-style" => {
                    current_list_name = None;
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    (text_styles, list_styles)
}

pub fn parse_opendoc(content: impl AsRef<[u8]>, inline: bool) -> Result<String, ParsingError> {
    let bytes = content.as_ref();
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

    let images = load_images(&mut archive);

    let mut xml_content = String::new();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;
        if file.name() == "content.xml" {
            file.read_to_string(&mut xml_content)?;
            break;
        }
    }

    let (text_styles, list_styles) = parse_styles(&xml_content);
    let mut ctx = OpendocContext::new(inline, images);
    ctx.text_styles = text_styles;
    ctx.list_styles = list_styles;

    let mut reader = Reader::from_str(&xml_content);
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"text:h" => {
                    let level = get_attr(&e, b"text:outline-level")
                        .and_then(|v| v.parse::<u8>().ok())
                        .unwrap_or(1);
                    ctx.in_heading = Some(level);
                    ctx.in_para = true;
                }
                b"text:p" => {
                    ctx.in_para = true;
                }
                b"text:span" => {
                    let style_name = get_attr(&e, b"text:style-name").unwrap_or_default();
                    ctx.active_span_style = ctx.text_styles.get(&style_name).cloned();
                }
                b"text:a" => {
                    ctx.active_link_href = get_attr(&e, b"xlink:href");
                    ctx.link_text.clear();
                }
                b"text:list" => {
                    let style_name = get_attr(&e, b"text:style-name").unwrap_or_default();
                    let ordered = ctx.list_styles.get(&style_name).copied().unwrap_or(false);
                    let depth = ctx.list_stack.last().map(|c| c.depth + 1).unwrap_or(1);
                    ctx.list_stack.push(ListContext {
                        ordered,
                        depth,
                        counters: Vec::new(),
                    });
                }
                b"text:list-item" => {
                    ctx.next_item_is_list_para = true;
                }
                b"table:table" => {
                    ctx.in_table = true;
                    ctx.table_rows.clear();
                }
                b"table:table-row" => {
                    ctx.current_row.clear();
                }
                b"table:table-cell" => {
                    ctx.in_cell = true;
                    ctx.current_row.push(String::new());
                }
                _ => {}
            },

            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"text:line-break" => {
                    ctx.push_text("\n");
                }
                b"text:tab" => {
                    ctx.push_text("\t");
                }
                b"draw:image" => {
                    if let Some(href) = get_attr(&e, b"xlink:href")
                        && let Some((mt, b64)) = ctx.images.get(&href)
                    {
                        let img = if ctx.inline {
                            format!("![Image](data:{};base64,{})", mt, b64)
                        } else {
                            "![Image]()".to_string()
                        };
                        ctx.push_text(&img);
                    }
                }
                _ => {}
            },

            Ok(Event::Text(e)) => {
                let raw = parse_text(&*e)?;
                if raw.trim().is_empty() && !ctx.in_cell {
                    ctx.push_text(&raw);
                    continue;
                }
                let styled = if let Some(ref style) = ctx.active_span_style.clone() {
                    ctx.apply_style(&raw, style)
                } else {
                    raw.clone()
                };
                ctx.push_text(&styled);
            }

            Ok(Event::End(e)) => match e.name().as_ref() {
                b"text:h" | b"text:p" => {
                    ctx.flush_para();
                }
                b"text:span" => {
                    ctx.active_span_style = None;
                }
                b"text:a" => {
                    if let Some(href) = ctx.active_link_href.take() {
                        let text = std::mem::take(&mut ctx.link_text);
                        let link = format!("[{}]({})", text.trim(), href);
                        ctx.push_text(&link);
                    }
                }
                b"text:list" => {
                    ctx.list_stack.pop();
                    if ctx.list_stack.is_empty() {
                        ctx.markdown.push('\n');
                    }
                }
                b"table:table" => {
                    ctx.flush_table();
                    ctx.in_table = false;
                }
                b"table:table-row" => {
                    ctx.table_rows.push(std::mem::take(&mut ctx.current_row));
                }
                b"table:table-cell" => {
                    ctx.in_cell = false;
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

    Ok(ctx.markdown.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_opendoc;

    static FIXTURE: &[u8] = include_bytes!("../fixtures/fixture.odt");

    fn parse() -> String {
        parse_opendoc(FIXTURE, true).expect("fixture should parse without error")
    }

    #[test]
    fn headings() {
        let md = parse();
        assert!(md.contains("# Comprehensive ODT Fixture"), "title missing");
        assert!(md.contains("## Heading 1"), "H1 missing");
        assert!(md.contains("### Heading 2"), "H2 missing");
        assert!(md.contains("#### Heading 3"), "H3 missing");
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
        assert!(md.contains("| 87 |"), "Bob score missing");
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
    fn image_base64() {
        let md = parse();
        assert!(
            md.contains("![Image](data:image/png;base64,"),
            "base64 image missing"
        );
    }

    #[test]
    fn soft_line_break() {
        let md = parse();
        let pos1 = md.find("Line one.").expect("'Line one.' missing");
        let pos2 = md
            .find("Line two (soft break, same paragraph).")
            .expect("'Line two' missing");
        assert!(pos2 > pos1, "line two must come after line one");
        assert!(
            &md[pos1..pos2].contains('\n'),
            "line one and two are joined without a break"
        );
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
        assert!(md.contains("مرحبا"), "Arabic missing");
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
        assert!(!md.contains("<text:"), "ODF text XML leaked");
        assert!(!md.contains("<table:"), "ODF table XML leaked");
        assert!(!md.contains("<draw:"), "ODF draw XML leaked");
        assert!(!md.contains("<style:"), "ODF style XML leaked");
    }
}
