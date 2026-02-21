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
    let mut tree = FileTree::new();

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

        if entry.is_dir() {
            continue;
        }

        let name = entry.name().to_string();

        if should_skip_file(&name) {
            continue;
        }

        let ext = get_extension(&name);
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

        let convert_type = Converter::from_path(name.clone(), ext);
        let text = crate::convert_from_bytes(contents, convert_type)?;
        tree.add_file(name, text);
    }

    tree.render()
}

pub fn parse_tar(content: impl AsRef<[u8]>) -> Result<String, ParsingError> {
    let mut archive = Archive::new(Cursor::new(content));
    let mut tree = FileTree::new();

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
        let ext = get_extension(&name);

        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .map_err(|e| ParsingError::ArchiveError(e.to_string()))?;

        let convert_type = Converter::from_path(name.clone(), ext);
        let text = crate::convert_from_bytes(contents, convert_type)?;

        tree.add_file(name, text);
    }

    tree.render()
}

fn should_skip_file(name: &str) -> bool {
    name.starts_with("__MACOSX/")
        || name.contains("/._")
        || Path::new(name)
            .file_name()
            .map_or(false, |f| f.to_string_lossy().starts_with("._"))
}

fn get_extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}

pub struct FileTree {
    files: BTreeMap<String, String>,
}

impl FileTree {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    pub fn add_file(&mut self, path: String, content: String) {
        self.files.insert(path, content);
    }

    pub fn render(self) -> Result<String, ParsingError> {
        let mut output = String::new();
        output.push_str("```file-tree\n");
        self.render_tree(&mut output);
        output.push_str("```\n\n");

        for (name, content) in self.files {
            output.push_str("# File: ");
            output.push_str(&name);
            output.push_str("\n\n");
            output.push_str(&content);
            output.push_str("\n\n");
        }
        Ok(output)
    }

    fn render_tree(&self, output: &mut String) {
        let mut root: BTreeMap<String, Node> = BTreeMap::new();

        for path in self.files.keys() {
            let parts: Vec<&str> = path.split('/').collect();
            let mut current = &mut root;

            for (i, &part) in parts.iter().enumerate() {
                let is_file = i == parts.len() - 1;
                current = &mut current
                    .entry(part.to_string())
                    .or_insert(Node {
                        children: BTreeMap::new(),
                        is_file,
                    })
                    .children;
            }
        }

        render_tree(&root, output, String::new());
    }
}

struct Node {
    children: BTreeMap<String, Node>,
    is_file: bool,
}

fn render_tree(nodes: &BTreeMap<String, Node>, output: &mut String, prefix: String) {
    let items: Vec<_> = nodes.iter().collect();

    for (idx, (name, node)) in items.iter().enumerate() {
        let is_last = idx == items.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };

        output.push_str(&prefix);
        output.push_str(connector);
        output.push_str(name);
        if !node.is_file {
            output.push('/');
        }
        output.push('\n');

        if !node.children.is_empty() {
            let extension = if is_last { "    " } else { "│   " };
            render_tree(&node.children, output, format!("{}{}", prefix, extension));
        }
    }
}
