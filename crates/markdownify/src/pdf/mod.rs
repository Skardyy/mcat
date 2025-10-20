use std::{error::Error, path::Path};

use lopdf::Document;
use pdf_element::PdfUnit;
use pdf_page::PdfPage;

mod pdf_element;
mod pdf_page;
mod pdf_state;

/// convert `pdf` into "markdown"
/// because its hard to keep the layout of pdf and add markdown symbols, it really is just a pdf to
/// text function, wrapped inside a pdf codeblock.
///
/// `screen_size` is the screen_size in cells. and the function will project that text with that
/// in consideration. by default it has values that will stop text from overlapping, but if you have a larger
/// buffer to show the text on, increasing the size will produce better looking result.
/// # usage:
/// ```
/// use std::path::Path;
/// use markdownify::pdf::pdf_convert;
///
/// let path = Path::new("path/to/file.pdf");
/// match pdf_convert(&path, None) {
///     Ok(md) => println!("{}", md),
///     Err(e) => eprintln!("Error: {}", e)
/// }
/// ```
pub fn pdf_convert(
    path: &Path,
    screen_size: Option<(u16, u16)>,
) -> Result<String, Box<dyn std::error::Error>> {
    let pdf = Pdf::new(path)?;
    let mut result = String::new();
    let mut i = 0;

    for page in pdf.iter_pages() {
        i += 1;
        result.push_str(&format!("\n\n<!-- S-TITLE: Page number {} -->\n", i));
        let mut page = page?;
        let units = page.handle_stream(page.stream.clone())?;

        // Separate text and lines
        let mut texts = Vec::new();
        let mut lines = Vec::new();

        for unit in units {
            match unit {
                PdfUnit::Text(unit) => texts.push(unit),
                PdfUnit::Line(line) => lines.push(line),
            }
        }

        let max_x = 612.0;
        let max_y = 792.0;

        // making each lower will give more space for more "accurate" projection
        let (cell_width, cell_height) = match screen_size {
            Some((x, y)) => ((max_x / x as f32).min(4.0), (max_y / y as f32).min(10.0)),
            None => (4.0, 10.0),
        };

        let cols = (max_x / cell_width).ceil() as usize + 1;
        let rows = (max_y / cell_height).ceil() as usize + 1;

        let mut matrix = vec![vec![' '; cols]; rows];

        // First, draw all lines
        for line in lines {
            let x1 = (line.from.0 / cell_width).round() as isize;
            let mut y1 = (line.from.1 / cell_height).round() as isize;
            let x2 = (line.to.0 / cell_width).round() as isize;
            let mut y2 = (line.to.1 / cell_height).round() as isize;

            // Flip Y coordinates
            y1 = rows as isize - 1 - y1;
            y2 = rows as isize - 1 - y2;

            if y1 == y2 {
                // Horizontal line
                for x in x1.min(x2)..=x1.max(x2) {
                    if (0..rows as isize).contains(&y1) && (0..cols as isize).contains(&x) {
                        matrix[y1 as usize][x as usize] = '─';
                    }
                }
            } else if x1 == x2 {
                // Vertical line
                for y in y1.min(y2)..=y1.max(y2) {
                    if (0..rows as isize).contains(&y) && (0..cols as isize).contains(&x1) {
                        matrix[y as usize][x1 as usize] = '│';
                    }
                }
            } else {
                // Diagonal line, ignore -- too complex
            }
        }

        // Then, place all text (this will overwrite lines where they conflict)
        for text_unit in texts {
            let col = (text_unit.x / cell_width).round() as usize;
            let row = (text_unit.y / cell_height).round() as usize;
            // Flip Y coordinate
            let row = rows.saturating_sub(row + 1);

            // Place each character of the string
            for (i, ch) in text_unit.text.chars().enumerate() {
                if col + i < cols && row < rows {
                    matrix[row][col + i] = ch;
                }
            }
        }

        let first = matrix.iter().position(|row| row.iter().any(|&c| c != ' '));
        let last = matrix.iter().rposition(|row| row.iter().any(|&c| c != ' '));
        let text = if let (Some(start), Some(end)) = (first, last) {
            matrix[start..=end]
                .iter()
                .map(|row| row.iter().collect::<String>())
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new() // all rows are empty
        };

        result.push_str("```pdf\n");
        result.push_str(&text);
        result.push_str("\n```");
    }
    Ok(result)
}

struct Pdf {
    doc: Document,
}

impl Pdf {
    pub fn new(path: &Path) -> Result<Pdf, Box<dyn Error>> {
        let doc = lopdf::Document::load(path)?;
        let pdf = Pdf { doc };

        Ok(pdf)
    }

    pub fn iter_pages(&'_ self) -> impl Iterator<Item = Result<PdfPage<'_>, Box<dyn Error>>> {
        self.doc
            .page_iter()
            .map(|id| PdfPage::from_object_id(&self.doc, id))
    }
}
