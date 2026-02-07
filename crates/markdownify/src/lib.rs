pub mod archives;
pub mod docx;
pub mod error;
pub mod opendoc;
pub mod pptx;
pub mod sheets;

use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use flate2::read::GzDecoder;
use xz2::read::XzDecoder;

use crate::error::ParsingError;

enum Converter {
    Tar,
    Zip,
    Md,
    Docx,
    OpenDoc,
    Csv,
    Calamine,
    Pptx,
    Image(String),          // file path
    Video(String),          // file path
    Binary(String, String), // file path, ext
    Audio(String),          // file path
    RawText(String),        // file ext
}

impl Converter {
    fn from_path(path: String, ext: String) -> Converter {
        match ext.as_ref() {
            "tar" => Converter::Tar,
            "zip" => Converter::Zip,
            "md" | "html" => Converter::Md,
            "docx" => Converter::Docx,
            "csv" => Converter::Csv,
            "pptx" => Converter::Pptx,
            "xlsx" | "xls" | "xlsm" | "xlsb" | "xla" | "xlam" | "ods" => Converter::Calamine,
            "odt" | "odp" => Converter::OpenDoc,

            "jpg" | "jpeg" | "png" | "gif" | "eps" | "svg" | "webp" | "cr2" | "tif" | "tiff"
            | "bmp" | "heif" | "avif" | "jxr" | "psd" | "ico" | "ora" | "djvu" => {
                Converter::Image(path)
            }

            "mp4" | "m4v" | "mkv" | "webm" | "mov" | "avi" | "wmv" | "mpg" | "flv" => {
                Converter::Video(path)
            }

            "mid" | "mp3" | "m4a" | "ogg" | "flac" | "wav" | "amr" | "aac" | "aiff" | "dsf"
            | "ape" => Converter::Audio(path),

            "epub" | "rar" | "bz2" | "bz3" | "7z" | "pdf" | "swf" | "rtf" | "eot" | "ps"
            | "sqlite" | "nes" | "crx" | "cab" | "deb" | "ar" | "Z" | "lz" | "rpm" | "dcm"
            | "zst" | "lz4" | "msi" | "cpio" | "par2" | "woff" | "woff2" | "ttf" | "otf"
            | "wasm" | "exe" | "dll" | "elf" | "bc" | "mach" | "class" | "dex" | "dey" | "der"
            | "obj" => Converter::Binary(path, ext),

            _ => Converter::RawText(ext),
        }
    }
}

fn convert_from_bytes(content: Vec<u8>, convert_type: Converter) -> Result<String, ParsingError> {
    let result = match convert_type {
        Converter::Tar => archives::parse_tar(content)?,
        Converter::Zip => archives::parse_zip(content)?,
        Converter::Md => parse_utf8(content)?,
        Converter::Csv => sheets::parse_csv(content)?,
        Converter::Calamine => sheets::parse_sheets(content)?,
        Converter::Pptx => pptx::parse_pptx(content)?,
        Converter::OpenDoc => opendoc::parse_opendoc(content)?,
        Converter::Docx => docx::parse_docx(content)?,
        Converter::Image(v) => image_fallback(v),
        Converter::Video(v) => video_fallback(v),
        Converter::Binary(p, ext) => binary_fallback(p, ext),
        Converter::Audio(v) => audio_fallback(v),
        Converter::RawText(ext) => file_fallback(parse_utf8(content)?, ext),
    };

    Ok(result)
}

pub fn convert_files(files: Vec<PathBuf>) -> Result<String, ParsingError> {
    if files.is_empty() {
        return Ok(String::new());
    }

    let files: Vec<PathBuf> = files
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();

    let common_root: PathBuf = files
        .iter()
        .filter_map(|p| p.parent())
        .fold(None::<PathBuf>, |acc, path| {
            Some(match acc {
                None => path.to_path_buf(),
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

    let bmap: BTreeMap<String, String> = files
        .into_iter()
        .map(|path| {
            let key = path
                .strip_prefix(&common_root)
                .unwrap_or(&path)
                .to_string_lossy()
                .into_owned();

            convert(&path).map(|content| (key, content))
        })
        .collect::<Result<BTreeMap<String, String>, ParsingError>>()?;

    let content = archives::build_output(bmap)?;

    Ok(content)
}

pub fn convert(path: &Path) -> Result<String, ParsingError> {
    if !path.is_file() {
        return Err(ParsingError::InvalidFile(path.to_string_lossy().to_string()).into());
    }

    // files without exts will just map into the file_fallback method
    let mut ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let mut file = File::open(path).map_err(ParsingError::UnreadableFile)?;
    let mut content = Vec::new();

    match ext.as_str() {
        "xz" => {
            ext = path
                .file_stem()
                .and_then(|s| Path::new(s).extension())
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            XzDecoder::new(file)
                .read_to_end(&mut content)
                .map_err(ParsingError::UnreadableFile)?;
        }
        "gz" => {
            ext = path
                .file_stem()
                .and_then(|s| Path::new(s).extension())
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            GzDecoder::new(file)
                .read_to_end(&mut content)
                .map_err(ParsingError::UnreadableFile)?;
        }
        _ => {
            file.read_to_end(&mut content)
                .map_err(ParsingError::UnreadableFile)?;
        }
    }

    let convert_type = Converter::from_path(path.to_string_lossy().to_string(), ext);

    convert_from_bytes(content, convert_type)
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
