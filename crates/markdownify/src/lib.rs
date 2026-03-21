pub mod archives;
pub mod docx;
pub mod error;
pub mod opendoc;
pub mod pptx;
pub mod sheets;

use std::{
    io::Read,
    iter,
    path::{Path, PathBuf},
};

use base64::Engine;
use flate2::read::GzDecoder;
use infer::{
    archive::{is_tar, is_zip},
    doc::{is_docx, is_pptx, is_xls, is_xlsx},
    is_app, is_audio, is_image as infer_is_image, is_video,
    odf::{is_odp, is_ods, is_odt},
};
use lzma_rust2::XzReader;

use crate::{archives::FileTree, error::ParsingError};

pub struct MarkdownifyInput {
    pub bytes: Vec<u8>,

    id: String,
    path: Option<PathBuf>,
    ext: Option<String>,

    allow_inline_images: bool,
}

type Checker = fn(&[u8]) -> bool;
type Parser = fn(&[u8]) -> Result<String, ParsingError>;

impl MarkdownifyInput {
    /// Wrapper around  [`MarkdownifyInput::from_bytes`]
    /// Reads the file contents, and automatically sets [`set_ext`] and `path` from the given path.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ParsingError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(ParsingError::UnreadableFile)?;
        let mut input = Self::from_bytes(bytes, path.to_string_lossy().to_string())?;

        input.path = Some(path.to_path_buf());
        input.ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        Ok(input)
    }

    /// # Supported formats
    /// - **Documents**: docx, pptx, odt, odp
    /// - **Spreadsheets**: xlsx, xls, xlsm, xlsb, xla, xlam, ods, csv
    /// - **Archives**: tar, zip
    /// - **Text**: html, md
    ///
    /// # Fallbacks
    /// - **Images**: rendered inline as base64 if `allow_inline_images` is set, otherwise as a link
    /// - **Audio/Video**: rendered as a media link
    /// - **Binary**: rendered as a binary link
    /// - **Other**: attempted as plain text and wrapped in a code block, with `ext` used as the
    ///   code block language hint if set via [`MarkdownifyInput::set_ext`]
    ///
    /// # Extension only formats
    /// The following formats cannot be detected from magic bytes and require the file extension to be set via [`MarkdownifyInput::set_ext`]:
    /// - **Spreadsheets**: csv, xlsm, xlsb, xla, xlam
    /// - **Text**: html, md
    ///
    /// To enable inline base64 image embedding, call [`MarkdownifyInput::allow_inline_images`].
    ///
    /// Compressed inputs (gz, xz) are decompressed automatically before processing.
    pub fn from_bytes(bytes: impl Into<Vec<u8>>, id: String) -> Result<Self, ParsingError> {
        let bytes = bytes.into();
        // decompress if needed
        let bytes: Vec<u8> = if infer::archive::is_gz(&bytes) {
            let mut decoder = GzDecoder::new(bytes.as_slice());
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            out
        } else if infer::archive::is_xz(&bytes) {
            let mut decoder = XzReader::new(bytes.as_slice(), true);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            out
        } else {
            bytes
        };

        Ok(Self {
            bytes,
            id,
            path: None,
            ext: None,
            allow_inline_images: false,
        })
    }

    /// setting the ext is helpful for making sure filse will be detected for what they are, its a
    /// fallback for when magic numbers and text parsing cannot tell us what is the type.
    pub fn set_ext(&mut self, ext: String) {
        self.ext = Some(ext);
    }

    /// some images like ones inside zip, don't have a path, thus cannot be written as markdown.
    /// setting this setting to on makes images be inline using base64, which is unreadable.
    /// use that only if you're passing it to a renderer later.
    pub fn allow_inline_images(&mut self, val: bool) {
        self.allow_inline_images = val;
    }

    pub fn convert(&self) -> Result<String, ParsingError> {
        // add more here, also add ext checking in too
        let handlers: &[(Checker, &str, Parser)] = &[
            (is_tar, "tar", |b| archives::parse_tar(b)),
            (is_zip, "zip", |b| archives::parse_zip(b)),
            (is_docx, "docx", |b| docx::parse_docx(b)),
            (is_pptx, "pptx", |b| pptx::parse_pptx(b)),
            (is_odt, "odt", |b| opendoc::parse_opendoc(b)),
            (is_odp, "odp", |b| opendoc::parse_opendoc(b)),
            (is_ods, "ods", |b| sheets::parse_sheets(b)),
            (is_xlsx, "xlsx", |b| sheets::parse_sheets(b)),
            (is_xls, "xls", |b| sheets::parse_sheets(b)),
            (|_| false, "csv", |b| sheets::parse_csv(b)),
            (|_| false, "xlsm", |b| sheets::parse_sheets(b)),
            (|_| false, "xlsb", |b| sheets::parse_sheets(b)),
            (|_| false, "xla", |b| sheets::parse_sheets(b)),
            (|_| false, "xlam", |b| sheets::parse_sheets(b)),
            (|_| false, "html", |b| parse_text(b)),
            (|_| false, "md", |b| parse_text(b)),
        ];

        let ext = self.ext.clone().unwrap_or_default();
        let result = handlers
            .iter()
            .find(|(check, e, _)| check(&self.bytes) || ext == **e)
            .map(|(_, _, parse)| parse(&self.bytes));

        if let Some(result) = result {
            Ok(result?)
        } else {
            if is_image(&self.bytes) {
                return Ok(image_fallback(
                    self.path.as_ref().map(|v| v.to_string_lossy()).as_deref(),
                    if self.allow_inline_images {
                        Some(&self.bytes)
                    } else {
                        None
                    },
                ));
            }
            if is_audio(&self.bytes) {
                return Ok(audio_fallback(
                    self.path.as_ref().map(|v| v.to_string_lossy()).as_deref(),
                ));
            }
            if is_video(&self.bytes) {
                return Ok(video_fallback(
                    self.path.as_ref().map(|v| v.to_string_lossy()).as_deref(),
                ));
            }
            if is_app(&self.bytes) {
                return Ok(binary_fallback(
                    self.path.as_ref().map(|v| v.to_string_lossy()).as_deref(),
                    self.ext.as_deref(),
                ));
            }
            // fallback for other images, just not supported by the image crate
            if infer_is_image(&self.bytes) {
                return Ok(image_fallback(
                    self.path.as_ref().map(|v| v.to_string_lossy()).as_deref(),
                    None,
                ));
            }
            let text = parse_text(&self.bytes).map_err(|e| {
                ParsingError::UnsupportedFormat(format!(
                    "couldn't find a matching format for the file, and file failed to decode as text: {e}"
                ))
            })?;

            Ok(file_fallback(&text, self.ext.as_deref()))
        }
    }
}

// from the image crate, since its the only ones supported by the image crate, which most likely
// will later be used..
fn is_image(buffer: &[u8]) -> bool {
    const MAGIC: &[(&[u8], &[u8])] = &[
        (b"\x89PNG\r\n\x1a\n", b""),
        (&[0xff, 0xd8, 0xff], b""),
        (b"GIF89a", b""),
        (b"GIF87a", b""),
        (b"RIFF\0\0\0\0WEBP", b"\xFF\xFF\xFF\xFF\0\0\0\0"),
        (b"MM\x00*", b""),
        (b"II*\x00", b""),
        (b"DDS ", b""),
        (b"BM", b""),
        (&[0, 0, 1, 0], b""),
        (b"#?RADIANCE", b""),
        (b"\0\0\0\0ftypavif", b"\xFF\xFF\0\0"),
        (&[0x76, 0x2f, 0x31, 0x01], b""),
        (b"qoif", b""),
        (b"P1", b""),
        (b"P2", b""),
        (b"P3", b""),
        (b"P4", b""),
        (b"P5", b""),
        (b"P6", b""),
        (b"P7", b""),
        (b"farbfeld", b""),
    ];

    for &(sig, mask) in MAGIC {
        if mask.is_empty() {
            if buffer.starts_with(sig) {
                return true;
            }
        } else if buffer.len() >= sig.len()
            && buffer
                .iter()
                .zip(sig)
                .zip(mask.iter().chain(iter::repeat(&0xFF)))
                .all(|((&b, &s), &m)| b & m == s)
        {
            return true;
        }
    }
    false
}

/// Converts multiple files to a single markdown string, rendered as a file tree.
///
/// Files are grouped under their common root path, with each file's path relative to that root
/// used as its key in the tree. Files without a path fall back to their `id` as the key.
///
/// See [`MarkdownifyInput::from_bytes`] for supported formats and fallback behavior.
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

    let mut tree = FileTree::default();

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

fn parse_text(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let bytes = content.as_ref();
    let (res, encoding_used, had_errors) = encoding_rs::UTF_8.decode(bytes);
    if had_errors {
        return Err(ParsingError::ParsingError(format!(
            "Failed to decode using {:?}",
            encoding_used
        )));
    }

    Ok(res.into_owned())
}

fn file_fallback(content: &str, ext: Option<&str>) -> String {
    let ext = ext.unwrap_or("");
    format!("```{ext}\n{content}\n```")
}

fn image_fallback(path: Option<&str>, bytes: Option<&[u8]>) -> String {
    if let Some(bytes) = bytes {
        let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
        return format!("![Image](data:image/png;base64,{encoded})");
    }
    let path = path.unwrap_or("");
    format!("![Image]({path})")
}

fn video_fallback(path: Option<&str>) -> String {
    let path = path.unwrap_or("");
    format!("![Video]({path})")
}

fn audio_fallback(path: Option<&str>) -> String {
    let path = path.unwrap_or("");
    format!("<audio controls src=\"{path}\"></audio>")
}

fn binary_fallback(path: Option<&str>, ext: Option<&str>) -> String {
    let path = path.unwrap_or("");
    let ext = ext.unwrap_or("Bin");
    format!("[{ext} file]({path})")
}
