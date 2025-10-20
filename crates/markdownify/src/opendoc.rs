use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::ZipArchive;

use super::sheets;

/// convert `odt` and `odp` files into markdown
/// # usage:
/// ```
/// use std::path::Path;
/// use markdownify::opendoc::opendoc_convert;
///
/// let path = Path::new("path/to/file.odt");
/// match opendoc_convert(&path) {
///     Ok(md) => println!("{}", md),
///     Err(e) => eprintln!("Error: {}", e)
/// }
/// ```
pub fn opendoc_convert(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let data = std::fs::read(path)?;
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;
    let mut xml_content = String::new();

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if file.name() == "content.xml" {
            file.read_to_string(&mut xml_content)?;
            break;
        }
    }

    let mut reader = Reader::from_str(&xml_content);
    let mut buf = Vec::new();
    let mut markdown = String::new();
    let mut table_rows: Vec<Vec<String>> = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut is_table = false;
    let mut is_list_item = 0;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"text:p" => continue,
                b"text:h" => {
                    is_list_item = 0;
                    markdown.push_str("### ");
                }
                b"text:span" => continue,
                b"table:table" => is_table = true,
                b"table:table-row" => continue,
                b"table:table-cell" => continue,
                b"text:list" => markdown.push_str(""),
                b"text:list-item" => is_list_item = 1,
                b"text:a" => continue,
                _ => {
                    // eprintln!("start {}", String::from_utf8(e.name().0.to_vec())?)
                }
            },
            Ok(Event::Text(e)) => {
                let text = &e.decode()?.into_owned();
                if is_table {
                    current_row.push(text.into());
                } else if is_list_item == 1 {
                    markdown.push_str(&format!(" * {}", text));
                    is_list_item = 2;
                } else {
                    markdown.push_str(text);
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"table:table" => {
                    let headers = table_rows[0].clone();
                    let data_rows = if table_rows.len() > 1 {
                        table_rows[1..].to_vec()
                    } else {
                        Vec::new()
                    };
                    is_table = false;
                    markdown.push_str(&sheets::to_markdown_table(&headers, &data_rows));
                    markdown.push('\n');
                    table_rows = Vec::new();
                }
                b"table:table-row" => {
                    table_rows.push(current_row);
                    current_row = Vec::new();
                }
                b"text:p" => {
                    if is_list_item != 2 {
                        markdown.push_str("\n\n");
                    }
                }
                b"text:h" => markdown.push_str("\n\n"),
                b"text:span" => continue,
                b"table:table-cell" => continue,
                b"text:list" => markdown.push('\n'),
                b"text:list-item" => {
                    is_list_item = 0;
                    markdown.push_str("  \n");
                }
                b"text:a" => continue,
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
