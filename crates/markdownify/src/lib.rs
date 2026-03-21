pub mod archives;
pub mod docx;
pub mod error;
pub mod opendoc;
pub mod pptx;
pub mod sheets;

use std::{
    io::Read,
    path::{Path, PathBuf},
};

pub use file_format::FileFormat;
use file_format::Kind;
use flate2::read::GzDecoder;
use lzma_rust2::XzReader;

use crate::{archives::FileTree, error::ParsingError};

pub struct MarkdownifyInput {
    pub bytes: Vec<u8>,
    pub format: FileFormat,
    pub id: String,
    pub path: Option<PathBuf>,
    pub ext: Option<String>,
}

impl MarkdownifyInput {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ParsingError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(ParsingError::UnreadableFile)?;
        let mut input = Self::from_bytes(bytes, path.to_string_lossy().to_string());
        input.path = Some(path.to_path_buf());
        input.ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        Ok(input)
    }

    pub fn from_bytes(bytes: Vec<u8>, id: String) -> Self {
        let format = FileFormat::from_bytes(&bytes);
        let (bytes, format) = match format {
            FileFormat::Xz => {
                let mut out = Vec::new();
                if XzReader::new(bytes.as_slice(), true)
                    .read_to_end(&mut out)
                    .is_ok()
                {
                    let fmt = FileFormat::from_bytes(&out);
                    (out, fmt)
                } else {
                    (bytes, format)
                }
            }
            FileFormat::Gzip => {
                let mut out = Vec::new();
                if GzDecoder::new(bytes.as_slice())
                    .read_to_end(&mut out)
                    .is_ok()
                {
                    let fmt = FileFormat::from_bytes(&out);
                    (out, fmt)
                } else {
                    (bytes, format)
                }
            }
            _ => (bytes, format),
        };

        Self {
            bytes,
            format,
            id,
            path: None,
            ext: None,
        }
    }

    pub fn convert(&self) -> Result<String, ParsingError> {
        let result = match self.format {
            FileFormat::TapeArchive => archives::parse_tar(&self.bytes)?,
            FileFormat::Zip => archives::parse_zip(&self.bytes)?,
            FileFormat::HypertextMarkupLanguage | FileFormat::PlainText => {
                parse_utf8(self.bytes.clone())?
            }
            FileFormat::OfficeOpenXmlDocument | FileFormat::MicrosoftWordDocument => {
                docx::parse_docx(&self.bytes)?
            }
            FileFormat::OfficeOpenXmlPresentation | FileFormat::MicrosoftPowerpointPresentation => {
                pptx::parse_pptx(&self.bytes)?
            }
            FileFormat::OpendocumentText | FileFormat::OpendocumentPresentation => {
                opendoc::parse_opendoc(&self.bytes)?
            }
            FileFormat::OfficeOpenXmlSpreadsheet
            | FileFormat::MicrosoftExcelSpreadsheet
            | FileFormat::OpendocumentSpreadsheet => sheets::parse_sheets(&self.bytes)?,
            _ => {
                // extension based for formats with no magic bytes
                match self.ext.as_deref() {
                    Some("csv") => sheets::parse_csv(&self.bytes)?,
                    Some("md") => parse_utf8(self.bytes.clone())?,
                    // i think xlsx should catch them, but just in case.
                    Some("xlsm") | Some("xlsb") | Some("xla") | Some("xlam") => {
                        sheets::parse_sheets(&self.bytes)?
                    }
                    // bigger catchers for fallback
                    _ => match self.format.kind() {
                        Kind::Image => image_fallback(self.id.clone()),
                        Kind::Video => video_fallback(self.id.clone()),
                        Kind::Audio => audio_fallback(self.id.clone()),
                        _ => match parse_utf8(self.bytes.clone()) {
                            Ok(text) => file_fallback(text, self.format.extension().to_string()),
                            Err(_) => binary_fallback(
                                self.id.clone(),
                                self.format.extension().to_string(),
                            ),
                        },
                    },
                }
            }
        };
        Ok(result)
    }
}

pub fn convert_files(files: Vec<MarkdownifyInput>) -> Result<String, ParsingError> {
    if files.is_empty() {
        return Ok(String::new());
    }

    let common_root: PathBuf = files
        .iter()
        .filter_map(|f| f.path.as_ref()?.parent().map(|p| p.to_path_buf()))
        .fold(None::<PathBuf>, |acc, path| {
            Some(match acc {
                None => path,
                Some(common) => common
                    .components()
                    .zip(path.components())
                    .take_while(|(a, b)| a == b)
                    .map(|(a, _)| a)
                    .collect(),
            })
        })
        .unwrap_or_default();
    let common_root = common_root.parent().unwrap_or(&common_root);

    let mut tree = FileTree::new();

    for input in files {
        let key = input
            .path
            .as_ref()
            .map(|p| {
                p.strip_prefix(common_root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .unwrap_or_else(|| input.id.clone());

        let content = input.convert()?;
        tree.add_file(key, content);
    }

    tree.render()
}

pub fn parse_utf8(content: Vec<u8>) -> Result<String, ParsingError> {
    String::from_utf8(content).map_err(|_| ParsingError::ParsingError("Invalid UTF-8".to_string()))
}

pub fn file_fallback(content: String, ext: String) -> String {
    return format!("```{ext}\n{content}\n```");
}

pub fn image_fallback(path: String) -> String {
    return format!("![Image]({path})");
}

pub fn video_fallback(path: String) -> String {
    return format!("![Video]({path})");
}

pub fn audio_fallback(path: String) -> String {
    return format!("<audio controls src=\"{path}\"></audio>");
}

pub fn binary_fallback(path: String, ext: String) -> String {
    return format!("[{ext} File]({path})");
}
