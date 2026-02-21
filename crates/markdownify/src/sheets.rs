use std::io::{BufRead, Cursor};

use calamine::Reader;

use crate::error::ParsingError;

fn detect_delimiter(line: &str) -> u8 {
    let candidates = [',', ';', '\t', '|'];
    candidates
        .iter()
        .map(|&c| (c, line.matches(c).count()))
        .max_by_key(|&(_, count)| count)
        .map(|(c, _)| c as u8)
        .unwrap_or(b',')
}

pub fn to_markdown_table(headers: &[String], rows: &[Vec<String>]) -> String {
    let mut output = String::new();
    output += &format!("| {} |\n", headers.join(" | "));
    output += &format!("|{}|\n", vec!["---"; headers.len()].join("|"));

    for row in rows {
        output += &format!("| {} |\n", row.join(" | "));
    }

    output
}

pub fn parse_sheets(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let cursor = Cursor::new(content.as_ref());
    let mut workbook = calamine::open_workbook_auto_from_rs(cursor)
        .map_err(|e| ParsingError::ParsingError(e.to_string()))?;
    let mut output = String::new();

    for sheet_name in workbook.sheet_names() {
        if let Ok(range) = workbook.worksheet_range(&sheet_name) {
            let mut rows = range.rows();
            if let Some(header_row) = rows.next() {
                let headers = header_row
                    .iter()
                    .map(|cell| cell.to_string())
                    .collect::<Vec<_>>();
                let body = rows
                    .map(|r| r.iter().map(|cell| cell.to_string()).collect::<Vec<_>>())
                    .collect::<Vec<_>>();

                output += &format!("## {}\n\n", sheet_name);
                output += &to_markdown_table(&headers, &body);
                output += "\n";
            }
        }
    }

    Ok(output)
}

pub fn parse_csv(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let mut cursor = Cursor::new(&content);
    let mut first_line = String::new();
    cursor.read_line(&mut first_line)?;

    let delimiter = detect_delimiter(&first_line);
    cursor.set_position(0);

    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .from_reader(cursor);

    let headers = reader
        .headers()
        .map_err(|e| ParsingError::ParsingError(e.to_string()))?
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

    let rows = reader
        .records()
        .map(|r| r.map(|rec| rec.iter().map(|s| s.to_string()).collect::<Vec<_>>()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ParsingError::ParsingError(e.to_string()))?;

    Ok(to_markdown_table(&headers, &rows))
}
