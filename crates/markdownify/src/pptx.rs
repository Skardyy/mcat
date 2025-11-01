use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

use quick_xml::events::Event;
use zip::ZipArchive;

use super::sheets;

/// convert `pptx` files into markdown
/// # usage:
/// ```
/// use std::path::Path;
/// use markdownify::pptx::pptx_converter;
///
/// let path = Path::new("path/to/file.pptx");
/// match pptx_converter(&path) {
///     Ok(md) => println!("{}", md),
///     Err(e) => eprintln!("Error: {}", e)
/// }
/// ```
pub fn pptx_converter(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let data = fs::read(path)?;
    let cursor = Cursor::new(data);
    let mut archive = ZipArchive::new(cursor)?;
    let mut markdown = String::new();
    let mut slide_num = 1;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let file_name = file.name().to_string();

        if file_name.starts_with("ppt/slides/") && file_name.ends_with(".xml") {
            markdown.push_str(&format!(
                "\n\n<!-- S-TITLE: Slide number {} -->\n",
                slide_num
            ));
            slide_num += 1;

            let mut content = String::new();
            file.read_to_string(&mut content)?;
            let mut reader = quick_xml::Reader::from_str(&content);
            let mut buf = Vec::new();
            let mut table_rows: Vec<Vec<String>> = Vec::new();
            let mut current_row: Vec<String> = Vec::new();
            let mut cell_text = String::new();
            let mut in_text_body = false;
            let mut in_title = false;
            let mut in_table = false;
            let mut in_row = false;
            let mut in_cell = false;

            loop {
                match reader.read_event_into(&mut buf) {
                    Ok(Event::Start(ref e)) => match e.name().as_ref() {
                        b"p:txBody" => {
                            in_text_body = true;
                        }
                        b"p:title" => {
                            in_title = true;
                        }
                        b"a:tbl" => {
                            in_table = true;
                            table_rows.clear();
                        }
                        b"a:tr" => {
                            if in_table {
                                in_row = true;
                                current_row = Vec::new();
                            }
                        }
                        b"a:br" => {
                            if in_text_body {
                                markdown.push_str("  \n");
                            }
                        }
                        b"a:tc" => {
                            if in_row {
                                in_cell = true;
                                cell_text.clear();
                            }
                        }
                        _ => {}
                    },
                    Ok(Event::Text(e)) => {
                        if in_text_body {
                            let text = e.decode().unwrap_or_default().to_string();

                            if !text.trim().is_empty() {
                                if in_title {
                                    markdown.push_str(&format!("# {}", text.trim()));
                                } else {
                                    markdown.push_str(&format!("{} ", text.trim()));
                                }
                            }
                        }
                        if in_cell {
                            cell_text.push_str(&e.decode().unwrap_or_default());
                        }
                    }
                    Ok(Event::End(ref e)) => match e.name().as_ref() {
                        b"p:txBody" => {
                            in_text_body = false;
                            markdown.push_str("  \n");
                        }
                        b"p:title" => {
                            in_title = false;
                            markdown.push_str("  \n");
                        }
                        b"a:tbl" => {
                            in_table = false;
                            if !table_rows.is_empty() {
                                let headers = table_rows[0].clone();
                                let data_rows = if table_rows.len() > 1 {
                                    table_rows[1..].to_vec()
                                } else {
                                    Vec::new()
                                };
                                markdown.push_str(&sheets::to_markdown_table(&headers, &data_rows));
                                markdown.push('\n');
                            }
                        }
                        b"a:tr" => {
                            in_row = false;
                            if !current_row.is_empty() {
                                table_rows.push(current_row.clone());
                            }
                        }
                        b"a:tc" => {
                            in_cell = false;
                            if in_row {
                                current_row.push(cell_text.trim().to_string());
                            }
                        }
                        _ => {}
                    },
                    Ok(Event::Eof) => break,
                    Err(e) => return Err(Box::new(e)),
                    _ => {}
                }
                buf.clear();
            }
        }
    }

    Ok(markdown.trim().to_string())
}
