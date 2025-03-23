use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};
use which::which;

use base64::{engine::general_purpose, Engine};
use image::{DynamicImage, ImageReader};

pub struct ImgCache {
    pub id: String,
}
impl ImgCache {
    pub fn new(id: &str, is_path: bool) -> Self {
        let id = if is_path {
            let path = Path::new(id);
            match path.canonicalize() {
                Ok(path) => path.to_string_lossy().to_string(),
                Err(_) => id.to_owned(),
            }
        } else {
            id.to_owned()
        };

        ImgCache { id }
    }
    pub fn get_png(&self) -> Result<DynamicImage, Box<dyn std::error::Error>> {
        let path = self.get_cache_path();
        if !path.exists() {
            return Err(From::from("cache doesn't exists"));
        }

        let img = ImageReader::open(path)?.decode()?;
        Ok(img)
    }
    pub fn get_cache_path(&self) -> PathBuf {
        let tmp_dir = env::temp_dir();
        let digested_name = general_purpose::URL_SAFE.encode(&self.id) + ".png";
        tmp_dir.join(&digested_name)
    }
}

pub fn is_document(input: &Path) -> bool {
    let supported_extensions = [
        "docx", "xlsx", "pdf", "pptx", "odf", "odp", "ods", "odt", "html",
    ];

    input
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| supported_extensions.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn open_document(
    input: &Path,
    cache: bool,
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let office_path =
        find_libreoffice_path().ok_or("libreoffice isn't installed or in the path")?;

    let img_cache = ImgCache::new(&input.to_string_lossy().to_string(), true);
    if cache {
        if let Ok(img) = img_cache.get_png() {
            return Ok(img);
        }
    }

    // where the file will be located
    let tmp_dir = env::temp_dir();
    let base_name = input
        .with_extension("png")
        .file_name()
        .ok_or("failed to get filename")?
        .to_string_lossy()
        .to_string();
    let path = tmp_dir.join(base_name);

    let is_html = input.extension().is_some_and(|f| f == "html");
    let filter = if is_html {
        "png:writer_png_Export"
    } else {
        "png"
    };

    Command::new(office_path)
        .arg("--headless")
        .arg("--norestore")
        .arg("--convert-to")
        .arg(filter)
        .arg(input)
        .arg("--outdir")
        .arg(tmp_dir)
        .output()?;

    if !path.exists() {
        return Err(From::from("failed to convert using libreoffice"));
    }

    //renaming for the caching
    fs::rename(path, img_cache.get_cache_path())?;
    let img = img_cache.get_png()?;

    Ok(img)
}

fn find_libreoffice_path() -> Option<PathBuf> {
    let paths = [
        "C:\\Program Files\\LibreOffice\\program\\soffice.com",
        "/usr/bin/libreoffice",
    ];
    for path in paths {
        let p = Path::new(path);
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    let names = ["soffice", "libreoffice"];
    for name in names {
        if let Ok(path) = which(name) {
            return Some(path);
        }
    }

    None
}
