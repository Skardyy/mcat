use ignore::WalkBuilder;
use inquire::MultiSelect;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub fn prompt_for_files(
    dir: &Path,
    hidden: bool,
) -> Result<Vec<(PathBuf, Option<String>)>, String> {
    let mut all_paths = collect_gitignored_paths(dir, hidden)?;
    all_paths.sort(); // Ensures folders come before contents

    let tree_view = format_file_list(&all_paths, dir);

    let index_map: HashMap<String, PathBuf> = tree_view
        .iter()
        .cloned()
        .zip(all_paths.iter().cloned())
        .collect();

    let selected = MultiSelect::new("Select files or folders", tree_view)
        .with_page_size(20)
        .with_vim_mode(true)
        .prompt()
        .map_err(|e| e.to_string())?;

    let selected_paths: HashSet<PathBuf> = selected
        .into_iter()
        .filter_map(|label| index_map.get(&label).cloned())
        .collect();

    // Avoid duplicates: if a folder is selected, skip its inner files
    let mut final_files = HashSet::new();

    for path in &selected_paths {
        if path.is_file() {
            // Only include files not covered by a selected folder
            let covered = selected_paths
                .iter()
                .any(|other| other.is_dir() && path.starts_with(other));
            if !covered {
                final_files.insert((path.clone(), None));
            }
        } else if path.is_dir() {
            for file in all_paths
                .iter()
                .filter(|p| p.is_file() && p.starts_with(path))
            {
                final_files.insert((file.clone(), None));
            }
        }
    }

    Ok(final_files.into_iter().collect())
}

fn collect_gitignored_paths(dir: &Path, hidden: bool) -> Result<Vec<PathBuf>, String> {
    let walker = WalkBuilder::new(dir)
        .standard_filters(!hidden)
        .hidden(!hidden)
        .follow_links(true)
        .max_depth(None)
        .build();

    let mut paths = vec![];

    for result in walker {
        match result {
            Ok(entry) => {
                let path = entry.path().to_path_buf();
                if path != dir {
                    paths.push(path);
                }
            }
            Err(_) => continue,
        }
    }

    Ok(paths)
}

fn format_file_list(paths: &[PathBuf], base: &Path) -> Vec<String> {
    let mut formatted = vec![];

    for (i, path) in paths.iter().enumerate() {
        let rel = path.strip_prefix(base).unwrap_or(path);
        let depth = rel.components().count().saturating_sub(1);
        let name = path.file_name().unwrap_or_default().to_string_lossy();

        let mut line = String::new();
        if depth > 0 {
            line.push_str(&"│   ".repeat(depth - 1));
            let is_last = paths
                .get(i + 1)
                .map(|next| {
                    let next_rel = next.strip_prefix(base).unwrap_or(next);
                    next_rel.components().count().saturating_sub(1) < depth
                })
                .unwrap_or(true);
            line.push_str(if is_last { "└── " } else { "├── " });
        }

        line.push_str(&name);
        if path.is_dir() {
            line.push('/');
        }

        // Add invisible unique suffix to make each label distinct
        line.push_str(&encode_invisible_id(i));

        formatted.push(line);
    }

    formatted
}

fn encode_invisible_id(id: usize) -> String {
    let charset = ['\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}'];
    let mut encoded = String::new();
    let mut n = id;
    if n == 0 {
        encoded.push(charset[0]);
    } else {
        while n > 0 {
            encoded.push(charset[n % 4]);
            n /= 4;
        }
    }
    encoded
}
