use crate::{Converter, error::ParsingError};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::Path,
};
use tar::Archive;
use zip::ZipArchive;

pub fn parse_zip(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let mut archive = ZipArchive::new(Cursor::new(content))
        .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;
    let mut files = BTreeMap::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

        if entry.is_dir() {
            continue;
        }

        let name = entry.name().to_string();

        if name.starts_with("__MACOSX/")
            || name.contains("/._")
            || Path::new(&name)
                .file_name()
                .map_or(false, |f| f.to_string_lossy().starts_with("._"))
        {
            continue;
        }

        let ext = Path::new(&name)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

        let convert_type = Converter::from_path(name.clone(), ext.clone());
        let text = crate::convert_from_bytes(contents, convert_type)?;
        files.insert(name, text);
    }

    build_output(files)
}

pub fn parse_tar(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let mut archive = Archive::new(Cursor::new(content));
    let mut files = BTreeMap::new();
    for entry in archive
        .entries()
        .map_err(|e| ParsingError::ArchiveError(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| ParsingError::ArchiveError(e.to_string()))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let path = entry
            .path()
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;
        let name = path.to_string_lossy().to_string();
        let ext = Path::new(&name)
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

        let convert_type = Converter::from_path(name.clone(), ext.clone());
        let text = crate::convert_from_bytes(contents, convert_type)?;

        files.insert(name, text);
    }

    build_output(files)
}

pub(crate) fn build_output(files: BTreeMap<String, String>) -> Result<String, ParsingError> {
    let mut output = String::new();
    build_tree(&files, &mut output);
    output.push_str("\n\n");
    for (name, content) in files {
        output.push_str("# File: ");
        output.push_str(&name);
        output.push_str("\n\n");
        output.push_str(&content);
        output.push_str("\n\n");
    }
    Ok(output)
}

pub(crate) fn build_tree(files: &BTreeMap<String, String>, output: &mut String) {
    let mut all_paths: Vec<&str> = Vec::new();
    for path in files.keys() {
        let parts: Vec<&str> = path.split('/').collect();
        for i in 0..parts.len() {
            let partial = parts[..=i].join("/");
            if !all_paths.contains(&partial.as_str()) {
                all_paths.push(partial.leak());
            }
        }
    }
    all_paths.sort();
    for path in all_paths {
        let depth = path.matches('/').count();
        let name = path.split('/').last().unwrap_or(path);
        let indent = "│   ".repeat(depth);
        let is_file = files.contains_key(path);
        let marker = if is_file { "├── " } else { "├── " };
        output.push_str(&indent);
        output.push_str(marker);
        output.push_str(name);
        if !is_file {
            output.push('/');
        }
        output.push('\n');
    }
}
