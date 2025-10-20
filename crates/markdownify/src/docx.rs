use crate::get_encoding;

use super::sheets;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

struct Styles {
    title: bool,     //w:pStyle empty w:val="includes title"
    header: bool,    // w:pStyle empty w:val="includes heading"
    bold: bool,      //w:b empty
    strike: bool,    //w:strike
    underline: bool, //w:u
    italics: bool,   //w:i
    indent: i8,      // w:ilvl w:val="0" (add 1 to it and -1 was indented)
    table: bool,     //w:tbl
}

impl Styles {
    pub fn default() -> Self {
        Styles {
            title: false,
            header: false,
            strike: false,
            italics: false,
            underline: false,
            bold: false,
            indent: 0,
            table: false,
        }
    }
}

fn get_attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == key {
            let buf = &attr.value;
            let encoding = get_encoding(buf, 4096);
            let mut decoder = encoding.new_decoder_with_bom_removal();
            let mut text = String::default();
            let _ = decoder.decode_to_string(&buf, &mut text, true);
            return Some(text);
        }
    }
    None
}

/// convert docx into markdown
/// usuage:
/// ```rs
/// let path = Path::new("path/to/file.docx");
/// let md = docx_convert(&path).unwrap();
/// println!("{}", md);
/// ```
pub fn docx_convert(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let cursor = Cursor::new(data);

    let mut archive = ZipArchive::new(cursor)?;
    let mut xml_content = String::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name() == "word/document.xml" {
            file.read_to_string(&mut xml_content)?;
            break;
        }
    }

    let mut reader = Reader::from_str(&xml_content);
    let mut buf = Vec::new();
    let mut markdown = String::new();

    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut styles = Styles::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:tbl" => styles.table = true,
                _ => {
                    continue;
                }
            },
            Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:b" => {
                    if let Some(val) = get_attr(&e, b"w:val") {
                        if val == "true" {
                            styles.bold = true;
                        }
                    } else {
                        styles.bold = true;
                    }
                }
                b"w:i" => {
                    if let Some(val) = get_attr(&e, b"w:val") {
                        if val == "true" {
                            styles.italics = true;
                        }
                    } else {
                        styles.italics = true;
                    }
                }
                b"w:strike" => {
                    if let Some(val) = get_attr(&e, b"w:val") {
                        if val == "true" {
                            styles.strike = true;
                        }
                    } else {
                        styles.strike = true;
                    }
                }
                b"w:u" => {
                    styles.underline = true;
                }
                b"w:pStyle" => {
                    if let Some(val) = get_attr(&e, b"w:val") {
                        if val.to_lowercase().contains("title") {
                            styles.title = true;
                            styles.indent = 0;
                        } else if val.to_lowercase().contains("heading") {
                            styles.header = true;
                            styles.indent = 0;
                        }
                    }
                }
                b"w:ilvl" => {
                    if styles.header || styles.title {
                        continue;
                    }
                    if let Some(val) = get_attr(&e, b"w:val") {
                        if let Ok(val) = val.parse::<i8>() {
                            styles.indent = val + 1
                        }
                    }
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                let encoding = get_encoding(&e, 4096);
                let mut decoder = encoding.new_decoder_with_bom_removal();
                let mut text = String::default();
                let _ = decoder.decode_to_string(&e, &mut text, true);
                if styles.bold {
                    text = format!("**{}** ", text.trim());
                    styles.bold = false;
                }
                if styles.underline {
                    text = format!("<u>{}</u> ", text.trim());
                    styles.underline = false;
                }
                if styles.strike {
                    text = format!("~~{}~~ ", text.trim());
                    styles.strike = false;
                }
                if styles.italics {
                    text = format!("*{}* ", text.trim());
                    styles.italics = false;
                }

                if styles.table {
                    current_row.push(text);
                    continue;
                }
                if styles.title {
                    markdown.push_str(&format!("## {}", text));
                    styles.title = false;
                    continue;
                }
                if styles.header {
                    markdown.push_str(&format!("### {}", text));
                    styles.header = false;
                    continue;
                }
                if styles.indent > 0 {
                    let indent = "  ".repeat(styles.indent as usize);
                    markdown.push_str(&format!("{}* {}", indent, text));
                    styles.indent = -1;
                    continue;
                }
                markdown.push_str(&text);
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:tbl" => {
                    if !table_rows.is_empty() {
                        let headers = table_rows[0].clone();
                        let data_rows = if table_rows.len() > 1 {
                            table_rows[1..].to_vec()
                        } else {
                            Vec::new()
                        };
                        markdown.push_str(&sheets::to_markdown_table(&headers, &data_rows));
                        markdown.push('\n');
                        table_rows = Vec::new();
                        styles = Styles::default();
                    }
                }
                b"w:tr" => {
                    table_rows.push(current_row);
                    current_row = Vec::new();
                }
                b"w:p" => {
                    if styles.indent == -1 {
                        styles.indent = 0;
                        markdown.push_str("  \n");
                    } else {
                        markdown.push_str("\n\n");
                    }
                }
                _ => {}
            },
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(
                    format!("Error at position {}: {:?}", reader.buffer_position(), e).into(),
                );
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(format(&markdown))
}

// Same format function from your ODT implementation
fn format(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut newline_count = 0;
    let mut spaces_count = 0;

    for line in input.lines() {
        if line.trim() == "" {
            result.push('\n');
        } else {
            result.push_str(&format!("{}\n", line));
        }
    }
    let input = &result;
    let mut result = String::with_capacity(input.len());

    for c in input.chars() {
        if c == ' ' {
            spaces_count += 1;
        }
        if c == '\n' {
            newline_count += 1;
            if spaces_count >= 2 {
                newline_count += 1;
            }
            spaces_count = 0;
            if newline_count <= 2 {
                result.push(c);
            }
        } else {
            newline_count = 0;
            spaces_count = 0;
            result.push(c);
        }
    }

    result
}
