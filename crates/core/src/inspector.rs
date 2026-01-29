use std::{
    io::Write,
    path::{Path, PathBuf},
};

use tempfile::{Builder, NamedTempFile};

pub enum InspectedBytes {
    File(NamedTempFile),
    Path(PathBuf),
}

impl InspectedBytes {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if let Some(ext) = detect_video(bytes) {
            return Ok(write_with_ext(bytes, ext));
        }

        if image::guess_format(bytes).is_ok() {
            // its does guess later again, so ext doesn't matter.
            return Ok(write_with_ext(bytes, "png"));
        }

        if is_pdf(bytes) {
            return Ok(write_with_ext(bytes, "pdf"));
        }

        if let Some(ext) = detect_zip_based_doc(bytes) {
            return Ok(write_with_ext(bytes, ext));
        }

        if is_html(bytes) {
            return Ok(write_with_ext(bytes, "html"));
        }

        if let Ok(s) = std::str::from_utf8(bytes) {
            let path = Path::new(s.trim());
            if path.exists() {
                return Ok(InspectedBytes::Path(path.to_path_buf()));
            } else {
                // defaults to markdown because its close enough to txt
                return Ok(write_with_ext(bytes, "md"));
            }
        }

        Err("Couldn't figure out the file type".to_string())
    }
}

fn write_with_ext(bytes: &[u8], ext: &str) -> InspectedBytes {
    let mut file = Builder::new()
        .suffix(&format!(".{}", ext))
        .tempfile()
        .expect("Failed to create temp file");
    file.write_all(bytes).expect("Failed to write to temp file");
    InspectedBytes::File(file)
}

fn detect_video(bytes: &[u8]) -> Option<&'static str> {
    if bytes.get(4..8) == Some(b"ftyp") {
        if bytes
            .get(8..12)
            .map_or(false, |b| matches!(b, b"isom" | b"mp42" | b"iso2"))
        {
            return Some("mp4");
        } else if bytes.get(8..12) == Some(b"m4v ") {
            return Some("m4v");
        } else if bytes.get(8..12) == Some(b"qt  ") {
            return Some("mov");
        }
    }

    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some("mkv"); // or webm - needs deeper check of EBML DocType
    }

    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"AVI ") {
        return Some("avi");
    }

    if bytes.starts_with(&[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        return Some("wmv");
    }

    if bytes.starts_with(b"FLV") {
        return Some("flv");
    }

    if bytes.get(0) == Some(&0x47) {
        return Some("ts");
    }

    if bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a") {
        return Some("gif");
    }

    None
}

fn is_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF")
}

fn detect_zip_based_doc(bytes: &[u8]) -> Option<&'static str> {
    if !bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return None;
    }

    // Try checking for known files in ZIP that indicate the type
    use std::io::Cursor;
    use zip::ZipArchive;

    let reader = Cursor::new(bytes);
    if let Ok(mut zip) = ZipArchive::new(reader) {
        for i in 0..zip.len().min(10) {
            if let Ok(file) = zip.by_index(i) {
                let name = file.name();
                if name.starts_with("word/") {
                    return Some("docx");
                } else if name.starts_with("ppt/") {
                    return Some("pptx");
                } else if name.starts_with("xl/") {
                    return Some("xlsx");
                } else if name.ends_with(".odt") {
                    return Some("odt");
                } else if name.ends_with(".ods") {
                    return Some("ods");
                } else if name.ends_with(".odp") {
                    return Some("odp");
                }
            }
        }

        // fallback to generic zip
        return Some("zip");
    }

    None
}

fn is_html(bytes: &[u8]) -> bool {
    if let Ok(s) = std::str::from_utf8(&bytes[..bytes.len().min(512)]) {
        let lower = s.to_ascii_lowercase();
        return lower.contains("<!doctype html") || lower.contains("<html");
    }
    false
}
