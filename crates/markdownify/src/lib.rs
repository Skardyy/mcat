pub mod docx;
pub mod opendoc;
pub mod pdf;
pub mod pptx;
pub mod sheets;

use chardetng::EncodingDetector;
use encoding_rs::Encoding;
use std::{
    borrow::Cow,
    fs::{self, File},
    path::{Path, PathBuf},
};
use tempfile::Builder;
use zip::ZipArchive;

pub struct ConvertOptions<'a> {
    pub path: Cow<'a, Path>,
    pub name_header: Option<&'a str>,
    pub screen_size: Option<(u16, u16)>,
}
impl<'a> ConvertOptions<'a> {
    pub fn new(path: impl Into<ConvertOptions<'a>>) -> Self {
        path.into()
    }
    pub fn with_name_header(mut self, name_header: &'a str) -> Self {
        self.name_header = Some(name_header);
        self
    }
    pub fn with_screen_size(mut self, screen_size: (u16, u16)) -> Self {
        self.screen_size = Some(screen_size);
        self
    }
}
impl<'a> From<&'a str> for ConvertOptions<'a> {
    fn from(value: &'a str) -> Self {
        ConvertOptions {
            path: Cow::Owned(PathBuf::from(value)),
            name_header: None,
            screen_size: None,
        }
    }
}
impl<'a> From<&'a Path> for ConvertOptions<'a> {
    fn from(value: &'a Path) -> Self {
        ConvertOptions {
            path: Cow::Borrowed(value),
            name_header: None,
            screen_size: None,
        }
    }
}
impl<'a> From<PathBuf> for ConvertOptions<'a> {
    fn from(value: PathBuf) -> Self {
        ConvertOptions {
            path: Cow::Owned(value),
            name_header: None,
            screen_size: None,
        }
    }
}

pub fn get_encoding(buffer: &[u8], limit: usize) -> &'static Encoding {
    let mut detector = EncodingDetector::new();
    let len = std::cmp::min(limit, buffer.len());
    detector.feed(&buffer[..len], true);
    detector.guess(None, true)
}

/// Convert any document into markdown.
///
/// `path_or_opts` can be either just a path (&str, &Path, PathBuf) or ConvertOptions.
/// Options allow you to customize the conversion process:
/// - Add a header to the markdown output
/// - Specify screen size for PDF conversion (affects layout detection quality)
///
/// # Supported Formats
/// - **Text/Markup**: `.csv`, `.md`, `.html` (direct conversion)
/// - **Documents**: `.docx`, `.pdf`, `.pptx`, `.odt`, `.odp`
/// - **Spreadsheets**: `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.xla`, `.xlam`, `.ods`
/// - **Archives**: `.zip` (extracts and converts contents)
/// - **Fallback**: Any other text file will be converted with basic formatting
///
/// # Examples
///
/// ## Basic usage with path only:
/// ```
/// use std::path::Path;
/// use markdownify::convert;
///
/// let path = Path::new("path/to/file.docx");
/// match convert(path) {
///     Ok(md) => println!("{}", md),
///     Err(e) => eprintln!("Error: {}", e)
/// }
/// ```
///
/// ## With options for name header and PDF screen size:
/// ```
/// use markdownify::{convert, ConvertOptions};
///
/// let opts = ConvertOptions::new("path/to/file.pdf")
///     .with_name_header("My Document")
///     .with_screen_size((100, 20)); // Better layout detection for PDFs (in cells - width,height)
///
/// match convert(opts) {
///     Ok(md) => println!("{}", md),
///     Err(e) => eprintln!("Error: {}", e)
/// }
/// ```
pub fn convert<'a>(
    path_or_opts: impl Into<ConvertOptions<'a>>,
) -> Result<String, Box<dyn std::error::Error>> {
    let path_or_opts = path_or_opts.into();
    let path = path_or_opts.path;
    if !path.is_file() {
        return Err(format!("Unknown path type for {}", path.display()).into());
    }

    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();

    let result = match ext.as_str() {
        "csv" => sheets::csv_converter(&path)?,
        "docx" => docx::docx_convert(&path)?,
        "pdf" => pdf::pdf_convert(&path, path_or_opts.screen_size)?,
        "pptx" => pptx::pptx_converter(&path)?,
        "xlsx" | "xls" | "xlsm" | "xlsb" | "xla" | "xlam" | "ods" => sheets::sheets_convert(&path)?,
        "zip" => zip_convert(&path)?,
        "odt" => opendoc::opendoc_convert(&path)?,
        "odp" => opendoc::opendoc_convert(&path)?,
        "md" | "html" => {
            let res = fs::read_to_string(path)?;
            format!("{}\n\n", res)
        }
        _ => {
            let content = fs::read_to_string(path)?;
            markitdown_fallback(&content, &ext)
        }
    };

    let result = match path_or_opts.name_header {
        Some(name) => format!("<!-- S-TITLE: {name} -->\n{result}\n---"),
        None => result,
    };

    Ok(result)
}

/// convert `zip` into markdown
/// # usage:
/// ```
/// use std::path::Path;
/// use markdownify::zip_convert;
///
/// let path = Path::new("path/to/archive.zip");
/// match zip_convert(&path) {
///     Ok(md) => println!("{}", md),
///     Err(e) => eprintln!("Error: {}", e)
/// }
/// ```
pub fn zip_convert(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;

    let mut output = String::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        let extension = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if entry.is_dir() {
            continue;
        }

        let mut temp = Builder::new()
            .suffix(&format!(".{}", extension))
            .tempfile()?;
        std::io::copy(&mut entry, &mut temp)?;
        let temp_path = temp.path().to_path_buf(); // clone path before move

        // convert using original convert function
        let md = match convert(temp_path) {
            Ok(result) => result,
            Err(err) => format!("**[Failed Reading: {}]**", err),
        };
        output += &format!("# `{}`\n\n{}\n\n", name, md);
    }

    Ok(output)
}

fn markitdown_fallback(content: &String, ext: &String) -> String {
    format!("```{}\n{}\n```", ext, content)
}
