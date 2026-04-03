use anyhow::Result;
use ignore::WalkBuilder;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use inquire::MultiSelect;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::AtomicBool;
use std::sync::{
    Arc, Weak,
    atomic::{AtomicU64, Ordering},
};
use tokio::runtime::{Builder, Runtime};

use crate::markdown_viewer::utils::get_lang_icon_and_color;

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub fn get_rt() -> &'static Runtime {
    RUNTIME.get_or_init(|| Builder::new_current_thread().enable_all().build().unwrap())
}

#[derive(Clone)]
pub enum MultiBar {
    Indicatif(MultiProgress),
    Ghostty(Arc<GhosttyBar>),
}

pub enum BarHandle {
    Indicatif(ProgressBar),
    Ghostty(GhosttyBarHandle),
}

impl MultiBar {
    pub fn indicatif() -> Self {
        Self::Indicatif(MultiProgress::new())
    }

    pub fn ghostty() -> Self {
        Self::Ghostty(GhosttyBar::new())
    }

    pub fn add(&self, total: Option<u64>, msg: Option<&str>) -> BarHandle {
        match self {
            Self::Indicatif(multi) => {
                let pb = match total {
                    Some(n) => {
                        let pb = multi.add(ProgressBar::new(n));
                        pb.set_style(
                            ProgressStyle::default_bar()
                                .template("{spinner:.green} [{bar:50.blue/white}] {bytes}/{total_bytes} ({percent}%)")
                                .unwrap()
                                .progress_chars("█▓▒░"),
                        );
                        pb
                    }
                    None => {
                        let pb = multi.add(ProgressBar::new_spinner());
                        pb.set_style(
                            ProgressStyle::default_spinner()
                                .template(&format!("{{spinner:.green}} {}", msg.unwrap_or("{msg}")))
                                .unwrap()
                                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
                        );
                        pb
                    }
                };
                BarHandle::Indicatif(pb)
            }
            Self::Ghostty(ghostty) => BarHandle::Ghostty(ghostty.add(total)),
        }
    }
}

impl BarHandle {
    pub fn set_position(&self, pos: u64) {
        match self {
            Self::Indicatif(pb) => pb.set_position(pos),
            Self::Ghostty(h) => h.set_position(pos),
        }
    }

    pub fn enable_steady_tick(&self, duration: std::time::Duration) {
        match self {
            Self::Indicatif(pb) => pb.enable_steady_tick(duration),
            Self::Ghostty(_) => {} // ghostty handles indeterminate via print
        }
    }

    pub fn finish(&self) {
        match self {
            Self::Indicatif(pb) => pb.finish_and_clear(),
            Self::Ghostty(h) => h.finish(),
        }
    }
}

#[derive(Clone)]
pub struct GhosttyBar {
    bars: boxcar::Vec<(Arc<AtomicU64>, Option<u64>, Arc<AtomicBool>)>, // (current, total, done)
}

pub struct GhosttyBarHandle {
    current: Arc<AtomicU64>,
    manager: Weak<GhosttyBar>,
}

impl GhosttyBarHandle {
    pub fn set_position(&self, pos: u64) {
        self.current.store(pos, Ordering::Relaxed);
        if let Some(mgr) = self.manager.upgrade() {
            mgr.print();
        }
    }

    pub fn finish(&self) {
        if let Some(mgr) = self.manager.upgrade() {
            mgr.finish(&self.current);
        }
    }
}

impl GhosttyBar {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            bars: boxcar::Vec::new(),
        })
    }

    pub fn add(self: &Arc<Self>, total: Option<u64>) -> GhosttyBarHandle {
        let current = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicBool::new(false));
        self.bars
            .push((Arc::clone(&current), total, Arc::clone(&done)));
        GhosttyBarHandle {
            current,
            manager: Arc::downgrade(self),
        }
    }

    pub fn print(&self) {
        let mut total_pct = 0.0f64;
        let mut count = 0usize;

        for (_, (current, total, done)) in &self.bars {
            if done.load(Ordering::Relaxed) {
                continue;
            }
            let cur = current.load(Ordering::Relaxed);
            if let Some(total) = total {
                let pct = (cur as f64 / *total as f64 * 100.0).min(100.0);
                total_pct += pct;
                count += 1;
            }
        }

        let all_done = self.bars.count() > 0
            && self
                .bars
                .iter()
                .all(|(_, (_, _, done))| done.load(Ordering::Relaxed));

        if all_done {
            eprint!("\x1b]9;4;0\x07");
        } else if count == 0 {
            eprint!("\x1b]9;4;3\x07");
        } else {
            let avg = (total_pct / count as f64) as u32;
            eprint!("\x1b]9;4;1;{avg}\x07");
        }
    }

    fn finish(&self, current: &Arc<AtomicU64>) {
        if let Some((_, (_, _, done))) = self
            .bars
            .iter()
            .find(|(_, (c, _, _))| Arc::ptr_eq(c, current))
        {
            done.store(true, Ordering::Relaxed);
        }
        self.print();
    }
}

pub fn prompt_for_files(dir: &Path, hidden: bool) -> Result<Vec<PathBuf>> {
    let mut all_paths = collect_gitignored_paths(dir, hidden)?;
    all_paths.sort();

    let tree_view = format_file_list(&all_paths, dir);

    let index_map: HashMap<String, PathBuf> = tree_view
        .iter()
        .cloned()
        .zip(all_paths.iter().cloned())
        .collect();

    let selected = MultiSelect::new("Select files or folders", tree_view)
        .with_page_size(20)
        .with_vim_mode(true)
        .prompt()?;

    let selected_paths: HashSet<PathBuf> = selected
        .into_iter()
        .filter_map(|label| index_map.get(&label).cloned())
        .collect();

    // if a folder is selected, skip its inner files
    let mut final_files = HashSet::new();

    for path in &selected_paths {
        if path.is_file() {
            // only include files not covered by a selected folder
            let covered = selected_paths
                .iter()
                .any(|other| other.is_dir() && path.starts_with(other));
            if !covered {
                final_files.insert(path.clone());
            }
        } else if path.is_dir() {
            for file in all_paths
                .iter()
                .filter(|p| p.is_file() && p.starts_with(path))
            {
                final_files.insert(file.clone());
            }
        }
    }

    Ok(final_files.into_iter().collect())
}

fn collect_gitignored_paths(dir: &Path, hidden: bool) -> Result<Vec<PathBuf>> {
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
    let reset = "\x1b[0m";
    let bold = "\x1b[1m";
    let blue = "\x1b[34m";
    let purple = "\x1b[35m";
    let dir_color = &format!("{bold}{blue}");
    let link_color = purple;

    for (i, path) in paths.iter().enumerate() {
        let rel = path.strip_prefix(base).unwrap_or(path);
        let depth = rel.components().count().saturating_sub(1);
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let ext = path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        let is_dir = path.is_dir();
        let is_link = path.is_symlink();

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
            line.push_str(reset);
        }

        let name_color = if is_link {
            link_color
        } else if is_dir {
            dir_color
        } else {
            ""
        };
        if is_dir {
            line.push_str(&format!("{name_color}\u{f024b} {name}/{reset}"));
        } else if let Some((icon, color)) = get_lang_icon_and_color(&ext) {
            line.push_str(&format!("{color}{icon}{reset} {name_color}{name}{reset}"));
        } else {
            line.push_str(&format!("{name_color}{name}{reset}"));
        }

        // add invisible unique suffix to make each label distinct
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
