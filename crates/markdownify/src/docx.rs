use crate::error::ParsingError;

use super::sheets;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::{Cursor, Read};
use zip::ZipArchive;

#[derive(Default)]
struct Styles {
    title: bool,     // w:pStyle empty w:val="includes title"
    header: bool,    // w:pStyle empty w:val="includes heading"
    bold: bool,      // w:b empty
    strike: bool,    // w:strike
    underline: bool, // w:u
    italics: bool,   // w:i
    indent: i8,      // w:ilvl w:val="0" (add 1 to it and -1 was indented)
    table: bool,     // w:tbl
}

fn get_attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    for attr in e.attributes().with_checks(false).flatten() {
        if attr.key.as_ref() == key {
            return Some(attr.unescape_value().ok()?.into_owned());
        }
    }
    None
}

pub fn parse_docx(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let mut archive = ZipArchive::new(Cursor::new(content))
        .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;
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
                let mut text = String::from_utf8_lossy(&e).to_string();

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
                return Err(ParsingError::ParsingError(
                    format!("Error at position {}: {:?}", reader.buffer_position(), e).into(),
                ));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(markdown)
}
