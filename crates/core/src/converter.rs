use crossterm::tty::IsTty;
use ffmpeg_sidecar::event::OutputVideoFrame;
use ignore::WalkBuilder;
use image::{DynamicImage, GenericImage, ImageBuffer, ImageFormat, Rgba, RgbaImage};
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use pelite::PeFile;
use rasteroid::{
    Frame,
    image_extended::InlineImage,
    inline_an_image,
    term_misc::{self, SizeDirection, dim_to_cells, dim_to_px, ensure_space},
};
use regex::Regex;
use reqwest::Url;
use resvg::{
    tiny_skia,
    usvg::{self, Options, Tree},
};
use std::{
    error,
    fs::{self, File},
    io::{BufRead, Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Stdio,
};
use std::{
    io::{Write, stdout},
    process::Command,
};
use tempfile::{NamedTempFile, TempDir};

use crate::{
    catter, cdp::ChromeHeadless, config::LsixOptions, fetch_manager,
    markdown_viewer::utils::string_len,
};

pub fn lnk_to_image(path: impl AsRef<Path>) -> Option<DynamicImage> {
    // Rather lazy tbh, just checking for target and not to icon if set.
    // Most will likely just target an exe which we can take the icon from.

    let target = get_lnk_target(path)?;
    let target = Path::new(&target);

    if !target.exists() || target.extension()?.to_str()?.to_lowercase() != "exe" {
        return None;
    }

    exe_to_image(target)
}

fn get_lnk_target(path: impl AsRef<Path>) -> Option<String> {
    let mut file = File::open(path.as_ref()).ok()?;
    let mut buf = vec![0u8; 1024];

    // Quick verify
    file.read(&mut buf).ok()?;
    if &buf[0..4] != &[0x4C, 0x00, 0x00, 0x00] {
        return None;
    }

    // Read LinkFlags at offset 0x14 (20 bytes into file)
    let link_flags = u32::from_le_bytes([buf[0x14], buf[0x15], buf[0x16], buf[0x17]]);

    // Check HasLinkInfo flag (bit 1)
    if link_flags & 0x02 == 0 {
        return None;
    }

    let mut offset = 0x4C; // After header (76 bytes)

    // Skip LinkTargetIDList if present (bit 0)
    if link_flags & 0x01 != 0 {
        let id_list_size = u16::from_le_bytes([buf[offset], buf[offset + 1]]);
        offset += 2 + id_list_size as usize;
    }

    // Ensure we have enough data
    if offset + 28 > buf.len() {
        file.seek(SeekFrom::Start(0)).ok()?;
        buf.resize(offset + 1024, 0);
        file.read(&mut buf).ok()?;
    }

    // Read LocalBasePath offset (at LinkInfo + 0x10)
    let local_base_path_offset = u32::from_le_bytes([
        buf[offset + 0x10],
        buf[offset + 0x11],
        buf[offset + 0x12],
        buf[offset + 0x13],
    ]);

    if local_base_path_offset == 0 {
        return None;
    }

    // Calculate absolute offset to path string
    let path_offset = offset + local_base_path_offset as usize;

    // Ensure buffer has the path
    if path_offset + 260 > buf.len() {
        file.seek(SeekFrom::Start(0)).ok()?;
        buf.resize(path_offset + 1024, 0);
        file.read(&mut buf).ok()?;
    }

    // Read to null char
    let end = buf[path_offset..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(260);

    String::from_utf8(buf[path_offset..path_offset + end].to_vec()).ok()
}

pub fn exe_to_image(path: impl AsRef<Path>) -> Option<DynamicImage> {
    let data = std::fs::read(path).ok()?;
    let pe = PeFile::from_bytes(&data).ok()?;
    let resources = pe.resources().ok()?;

    let (_name, icon_group) = resources.icons().next()?.ok()?;
    let entries = icon_group.entries();
    let best_entry = entries.iter().max_by_key(|e| {
        let width = if e.bWidth == 0 { 256 } else { e.bWidth as u32 };
        let height = if e.bHeight == 0 {
            256
        } else {
            e.bHeight as u32
        };
        let bits = e.wBitCount as u32;

        // Prioritize: size first, then bit depth
        (width * height, bits)
    })?;

    let icon_id = best_entry.nId;
    let icon_data = icon_group.image(icon_id).ok()?;

    let mut ico_file = Vec::new();
    // ICO header
    ico_file.extend_from_slice(&[
        0, 0, // Reserved, must be 0
        1, 0, // Type: 1 for ICO
        1, 0, // Number of images: 1
    ]);
    ico_file.push(best_entry.bWidth); // Width
    ico_file.push(best_entry.bHeight); // Height
    ico_file.push(best_entry.bColorCount); // Color count
    ico_file.push(0); // Reserved
    ico_file.extend_from_slice(&best_entry.wPlanes.to_le_bytes()); // Color planes
    ico_file.extend_from_slice(&best_entry.wBitCount.to_le_bytes()); // Bits per pixel
    ico_file.extend_from_slice(&(icon_data.len() as u32).to_le_bytes()); // Size of img data
    ico_file.extend_from_slice(&22u32.to_le_bytes()); // Offset to img data (6 + 16 = 22)
    ico_file.extend_from_slice(icon_data); // Add the raw img data

    return image::load_from_memory(&ico_file).ok();
}

fn get_icon_path_from_url(path: impl AsRef<Path>) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    content
        .lines()
        .find_map(|line| line.strip_prefix("IconFile="))
        .map(|s| s.trim().to_string())
}
pub fn url_file_to_image(path: impl AsRef<Path>) -> Option<DynamicImage> {
    let path = path.as_ref();
    let icon_path = get_icon_path_from_url(path)?;
    let icon_path = Path::new(&icon_path);

    if !icon_path.exists() {
        return None;
    }

    let ext = icon_path.extension()?.to_str()?.to_lowercase();
    if ext == "exe" {
        return exe_to_image(icon_path);
    }

    if ext != "ico" {
        return None;
    }

    return image::open(icon_path).ok();
}

pub fn svg_to_image(
    mut reader: impl Read,
    width: Option<&str>,
    height: Option<&str>,
) -> Result<DynamicImage, Box<dyn std::error::Error>> {
    let mut svg_data = Vec::new();
    reader.read_to_end(&mut svg_data)?;

    // Create options for parsing SVG
    let mut opt = Options::default();

    // allowing text
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    opt.fontdb = std::sync::Arc::new(fontdb);
    opt.text_rendering = usvg::TextRendering::OptimizeLegibility;

    // Parse SVG
    let tree = Tree::from_data(&svg_data, &opt)?;

    // Get size of the SVG
    let pixmap_size = tree.size();
    let src_width = pixmap_size.width();
    let src_height = pixmap_size.height();
    let width = match width {
        Some(w) => rasteroid::term_misc::dim_to_px(w, rasteroid::term_misc::SizeDirection::Width)?,
        None => src_width as u32,
    };
    let height = match height {
        Some(h) => rasteroid::term_misc::dim_to_px(h, rasteroid::term_misc::SizeDirection::Height)?,
        None => src_height as u32,
    };
    let (target_width, target_height) =
        rasteroid::image_extended::calc_fit(src_width as u32, src_height as u32, width, height);
    let scale_x = target_width as f32 / src_width;
    let scale_y = target_height as f32 / src_height;
    let scale = scale_x.min(scale_y);

    // Create a Pixmap to render to
    let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)
        .ok_or("Failed to create pixmap for svg")?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);

    // Render SVG to Pixmap
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    // Convert Pixmap to ImageBuffer
    let image_buffer =
        ImageBuffer::<Rgba<u8>, _>::from_raw(target_width, target_height, pixmap.data().to_vec())
            .ok_or("Failed to create image buffer for svg")?;

    // Convert ImageBuffer to DynamicImage
    Ok(DynamicImage::ImageRgba8(image_buffer))
}

pub fn latex_to_pdf<P: AsRef<Path>>(input_path: P) -> Option<(TempDir, PathBuf)> {
    let input_path = input_path.as_ref();

    if !input_path.exists() {
        return None;
    }

    let temp_dir = TempDir::new().ok()?;
    let name = input_path.file_stem()?.to_string_lossy().to_string();
    let temp_pdf = temp_dir.path().join(format!("{name}.pdf"));

    // Try Tectonic first
    let tectonic = Command::new("tectonic")
        .args(&["--outdir", temp_dir.path().to_str()?, input_path.to_str()?])
        .output();

    if let Ok(output) = tectonic {
        if output.status.success() && temp_pdf.exists() {
            return Some((temp_dir, temp_pdf));
        }
    }

    // Fallback to pdflatex
    let pdflatex = Command::new("pdflatex")
        .args(&[
            &format!("-output-directory={}", temp_dir.path().to_str()?),
            "-interaction=nonstopmode",
            input_path.to_str()?,
        ])
        .output();

    if let Ok(output) = pdflatex {
        if output.status.success() && temp_pdf.exists() {
            return Some((temp_dir, temp_pdf));
        }
    }

    None
}

pub fn typst_to_pdf<P: AsRef<Path>>(input_path: P) -> Option<NamedTempFile> {
    let input_path = input_path.as_ref();

    if !input_path.exists() {
        return None;
    }

    let temp_pdf = NamedTempFile::with_suffix(".pdf").ok()?;
    let output_path = temp_pdf.path().to_path_buf();

    let result = Command::new("typst")
        .args(&[
            "compile",
            "--format",
            "pdf",
            input_path.to_str()?,
            output_path.to_str()?,
        ])
        .output();

    if let Ok(output) = result {
        if output.status.success() && output_path.exists() {
            return Some(temp_pdf);
        }
    }

    None
}

pub fn html_to_image(html: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut tmp_file = NamedTempFile::with_suffix(".html").expect("failed to create tmp file");
    tmp_file.write_all(html.as_bytes())?;
    let path = tmp_file.path();
    let url =
        Url::from_file_path(path).map_err(|_| "Failed to create a url for the chromium flag")?;
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let browser = ChromeHeadless::new(&url.as_str()).await?;
        let img_data = browser.capture_screenshot().await?;
        Ok(img_data)
    })
}
pub fn get_pdf_command() -> Result<&'static str, String> {
    which::which("pdftocairo")
        .map(|_| "pdftocairo")
        .or_else(|_| which::which("pdftoppm").map(|_| "pdftoppm"))
        .map_err(|_| "Neither pdftocairo nor pdftoppm found in PATH".to_string())
}
pub fn get_pdf(path: &Path) -> (PathBuf, Option<NamedTempFile>, Option<TempDir>) {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    match ext.as_ref() {
        "pdf" => (path.to_path_buf(), None, None),
        "typ" => {
            if let Some(pdf) = typst_to_pdf(path) {
                (pdf.path().to_path_buf(), Some(pdf), None)
            } else {
                (path.to_path_buf(), None, None)
            }
        }
        "tex" => {
            if let Some((tmpdir, p)) = latex_to_pdf(path) {
                (p, None, Some(tmpdir))
            } else {
                (path.to_path_buf(), None, None)
            }
        }
        _ => unreachable!(),
    }
}
pub fn pdf_to_vec(pdf_path: &str) -> Result<Vec<DynamicImage>, Box<dyn std::error::Error>> {
    let temp_dir = TempDir::new()?;
    let temp_path = temp_dir.path().join("page").to_string_lossy().to_string();
    let tool = get_pdf_command()?;

    let output = Command::new(tool)
        .args(&["-jpeg", "-r", "300", pdf_path, &temp_path])
        .output()
        .map_err(|e| format!("{} failed to execute: {}", tool, e))?;
    if !output.status.success() {
        return Err(format!(
            "{} error: {}",
            tool,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let mut entries: Vec<_> = fs::read_dir(temp_dir.path())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "jpg" || ext == "jpeg")
                .unwrap_or(false)
        })
        .collect();
    entries.sort_by_key(|entry| entry.file_name());

    let mut images = Vec::new();
    for entry in entries {
        if let Ok(img) = image::open(entry.path()) {
            images.push(img);
        }
    }

    Ok(images)
}
pub fn pdf_to_image(
    pdf_path: &str,
    page_number: usize,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let tool = get_pdf_command()?;

    let output = Command::new(tool)
        .args(&[
            "-jpeg",
            "-singlefile",
            "-f",
            &page_number.to_string(),
            "-l",
            &page_number.to_string(),
            "-r",
            "300",
            pdf_path,
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("{} failed to execute: {}", tool, e))?;

    if !output.status.success() {
        return Err(format!(
            "{} error: {}",
            tool,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(output.stdout)
}

pub struct VideoFrames {
    timestamp: f32,
    img: Vec<u8>,
    width: u16,
    height: u16,
}
impl Frame for VideoFrames {
    fn timestamp(&self) -> f32 {
        self.timestamp
    }
    fn data(&self) -> &[u8] {
        &self.img
    }
    fn width(&self) -> u16 {
        self.width as u16
    }
    fn height(&self) -> u16 {
        self.height as u16
    }
}

fn truncate_filename(name: String, width: u16, lnk: &Path, create_hyprlink: bool) -> String {
    let width = width as usize;

    let osc8_start = if create_hyprlink {
        std::fs::canonicalize(&lnk)
            .ok()
            .and_then(|abs_path| {
                let abs_path = abs_path.display().to_string();
                let abs_path = abs_path.strip_prefix(r"\\?\").unwrap_or(&abs_path);
                let abs_path = abs_path.replace("\\", "/");
                let uri = format!("file://{}", abs_path);
                Some(format!("\x1b]8;;{}\x1b\\", uri))
            })
            .unwrap_or("".to_owned())
    } else {
        "".to_owned()
    };
    let osc8_end = if create_hyprlink {
        "\x1b]8;;\x1b\\"
    } else {
        ""
    };

    let le = string_len(&name);
    if le <= width {
        let rem_space = width - le;
        let left_spaces = rem_space / 2;
        let right_spaces = rem_space - left_spaces;
        return format!(
            "{}{osc8_start}{}{osc8_end}{}",
            " ".repeat(left_spaces),
            name,
            " ".repeat(right_spaces)
        );
    }

    // sep base and ext
    let dot_pos = name.rfind('.'); // always a single byte so its fine
    let (base, ext) = match dot_pos {
        Some(pos) => {
            let (b, e) = name.split_at(pos);
            (b.into(), format!(".{}", e))
        }
        None => (name, "".into()),
    };

    let ext_len = string_len(&ext);
    let base_len = string_len(&base);

    // if even only the ext can't fit, why..
    if width <= ext_len {
        return if width >= ext_len {
            ext.to_string()
        } else {
            let truncated_ext: String = ext.chars().take(width).collect();
            truncated_ext
        };
    }

    let available_base_width = width - ext_len;

    let front_part = if available_base_width < base_len {
        base.chars().take(available_base_width).collect::<String>()
    } else {
        base
    };

    format!("{osc8_start}{}{}{osc8_end}", front_part, ext)
}

fn calculate_items_per_row(terminal_width: u16, ctx: &LsixOptions) -> Result<usize, String> {
    let min_item_width: u16 = term_misc::dim_to_cells(&ctx.min_width, SizeDirection::Width)? as u16;
    let max_item_width: u16 = term_misc::dim_to_cells(&ctx.max_width, SizeDirection::Width)? as u16;
    let max_items_per_row: usize = ctx.max_items_per_row;

    let min_items = ((terminal_width + max_item_width - 1) / max_item_width) as usize;
    let max_items = (terminal_width / min_item_width) as usize;
    let mut items = min_items;
    items = items.min(max_items);
    items = items.min(max_items_per_row);
    Ok(items.max(1))
}

#[rustfmt::skip]
fn ext_to_svg(ext: &str) -> &'static str {
    let svg = if ext == "IAMADIR" {
        include_str!("../assets//folder.svg")
    } else if catter::is_video(ext) {
        include_str!("../assets/video.svg")
    } else if ext == "" {
        include_str!("../assets/file.svg")
    } else if matches!(ext, 
        "codes" | "py" | "rs" | "js" | "ts" | "java" | "c" | "cpp" | "h" | "hpp" | 
        "go" | "php" | "rb" | "sh" | "pl" | "lua" | "swift" | "kt" | "kts" | 
        "scala" | "dart" | "elm" | "hs" | "ml" | "mli" | "r" | "f" | "f90" | 
        "cs" | "vb" | "asm" | "s" | "clj" | "cljs" | "edn" | "coffee" | "erl" | 
        "hrl" | "ex" | "exs" | "json" | "toml" | "yaml" | "yml" | "xml" | "html" | 
        "css" | "scss" | "less" | "vue" | "svelte" | "md" | "markdown" | "tex" | 
        "nim" | "zig" | "v" | "odin" | "d" | "sql" | "ps1" | "bash" | "zsh" | "fish"
    ) {
        include_str!("../assets/code.svg")
    } else if matches!(ext, 
        "conf" | "config" | "ini" | "cfg" | "cnf" | "properties" | "env" | 
        "gitconfig" | "gitignore" | "npmrc" | "yarnrc" | "editorconfig" | 
        "dockerignore" | "dockerfile" | "makefile" | "mk" | "nginx" | "apache" | 
        "htaccess" | "htpasswd" | "hosts" | "service" | "socket" | "timer" | 
        "mount" | "automount" | "swap" | "target" | "path" | "slice" | "sysctl" | 
        "tmpfiles" | "udev" | "logind" | "resolved" | "timesyncd" | "coredump" | 
        "journald" | "netdev" | "network" | "link" | "netctl" | "wpa" | "pacman" | 
        "mirrorlist" | "vconsole" | "locale" | "fstab" | "crypttab" | "grub" | 
        "syslinux" | "archlinux" | "inputrc" | "bashrc" | "bash_profile" | 
        "bash_logout" | "profile" | "zshenv" | "zshrc" | "zprofile" | "zlogin" | 
        "zlogout" | "fishrc" | "fish_variables" | "fish_config" | "fish_plugins" | 
        "fish_functions" | "fish_completions" | "fish_aliases" | "fish_abbreviations" | 
         "fish_user_init" | "fish_user_paths" | 
        "fish_user_variables" | "fish_user_functions" | "fish_user_completions" | 
        "fish_user_abbreviations" | "fish_user_aliases" | "fish_user_key_bindings"
    ) {
        include_str!("../assets/conf.svg")
    } else if matches!(ext,
        "zip" | "tar" | "gz" | "bz2" | "xz" | "zst" | "lz" | "lzma" | "lzo" | 
        "rz" | "sz" | "7z" | "rar" | "iso" | "dmg" | "pkg" | "deb" | "rpm" | 
        "crx" | "cab" | "msi" | "ar" | "cpio" | "shar" | "lbr" | "mar" | 
        "sbx" | "arc" | "wim" | "swm" | "esd" | "zipx" | "zoo" | "pak" | 
        "kgb" | "ace" | "alz" | "apk" | "arj" | "ba" | "bh" | "cfs" | 
        "cramfs" | "dar" | "dd" | "dgc" | "ear" | "gca" | "ha" | "hki" | 
        "ice" | "jar" | "lzh" | "lha" | "lzx" | "partimg" | "paq6" | 
        "paq7" | "paq8" | "pea" | "pim" | "pit" | "qda" | "rk" | "sda" | 
        "sea" | "sen" | "sfx" | "shk" | "sit" | "sitx" | "sqx" | "tar.Z" | 
        "uc" | "uc0" | "uc2" | "ucn" | "ur2" | "ue2" | "uca" | "uha" | 
        "war" |  "xar" | "xp3" | "yz1" | "zap" |  
        "zz"
    ) {
        include_str!("../assets/archive.svg")
    } else {
        include_str!("../assets/txt.svg")
    };
    svg
}

pub fn lsix(
    input: impl AsRef<str>,
    out: &mut impl Write,
    ctx: &LsixOptions,
    hidden: bool,
    create_hyprlink: bool,
    inline_encoder: &rasteroid::InlineEncoder,
) -> Result<(), Box<dyn error::Error>> {
    let dir_path = Path::new(input.as_ref());
    let walker = WalkBuilder::new(dir_path)
        .standard_filters(!hidden)
        .hidden(!hidden)
        .max_depth(Some(1))
        .follow_links(true)
        .build();
    let resize_for_ascii = matches!(inline_encoder, rasteroid::InlineEncoder::Ascii);
    let ts = rasteroid::term_misc::get_wininfo();
    let items_per_row = calculate_items_per_row(ts.sc_width, &ctx)?;
    let x_padding = term_misc::dim_to_cells(&ctx.x_padding, SizeDirection::Width)? as u16;
    let y_padding = term_misc::dim_to_cells(&ctx.y_padding, SizeDirection::Height)? as u16;
    let width = (ts.sc_width as f32 / items_per_row as f32 + 0.1).round() as u16 - x_padding - 1;
    let width_formatted = format!("{width}c");
    let height = ctx.height.as_ref();
    let px_x_padding = dim_to_px(&format!("{x_padding}c"), SizeDirection::Width)?;

    // Collect all valid paths first
    let mut paths: Vec<_> = walker
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path().to_path_buf();
            if path == dir_path {
                return None;
            }
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            if path.is_dir() {
                return Some((path, "IAMADIR".to_owned(), filename));
            }
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if ext == "" && filename.contains(".") {
                return Some((path, filename.replace(".", ""), filename));
            }
            Some((path, ext, filename))
        })
        .collect();
    paths.sort_by(|a, b| {
        let a_is_dir = a.0.is_dir();
        let b_is_dir = b.0.is_dir();

        match b_is_dir.cmp(&a_is_dir) {
            std::cmp::Ordering::Equal => {
                let a_str = a.0.to_string_lossy().to_lowercase();
                let b_str = b.0.to_string_lossy().to_lowercase();
                a_str.cmp(&b_str)
            }
            dir_order => dir_order,
        }
    });

    // Process images in parallel
    use rayon::prelude::*;
    let images: Vec<_> = paths
        .par_iter()
        .filter_map(|(path, ext, filename)| {
            let dyn_img = if ext == "svg" {
                fs::read(path).ok().and_then(|buf| {
                    svg_to_image(buf.as_slice(), Some(&width_formatted), Some(&height)).ok()
                })
            } else if ImageFormat::from_extension(ext).is_some() {
                fs::read(path)
                    .ok()
                    .and_then(|buf| image::load_from_memory(&buf).ok())
            } else if ext == "url" {
                url_file_to_image(path)
            } else if ext == "exe" {
                exe_to_image(path)
            } else if ext == "lnk" {
                lnk_to_image(path)
            } else {
                None
            };
            let dyn_img = dyn_img.or_else(|| {
                let svg = ext_to_svg(ext);
                let cursor = Cursor::new(svg);
                svg_to_image(cursor, Some(&width_formatted), Some(&height)).ok()
            })?;

            let (img, _, w, h) = dyn_img
                .resize_plus(
                    Some(&width_formatted),
                    Some(&height),
                    resize_for_ascii,
                    true,
                )
                .ok()?;

            Some((img, filename, w, h, path))
        })
        .collect();

    let mut buf = Vec::new();
    buf.write_all(b"\n")?;
    for chunk in &images.into_iter().chunks(items_per_row as usize) {
        let items: Vec<_> = chunk.collect();
        let images: Vec<DynamicImage> = items
            .iter()
            .map(|f| image::load_from_memory(&f.0))
            .flatten()
            .collect();
        let image = combine_images_into_row(
            images,
            if resize_for_ascii {
                x_padding as u32
            } else {
                px_x_padding
            },
        )?;
        let height = dim_to_cells(height.as_ref(), SizeDirection::Height)?;
        ensure_space(&mut buf, height as u16)?;
        // windows for some reason doesn't handle newlines as expected..
        if cfg!(windows) {
            buf.write_all(b"\x1b[s")?;
        }
        inline_an_image(&image, &mut buf, None, None, inline_encoder)?;
        if cfg!(windows) {
            buf.write_all(format!("\x1b[u\x1b[{height}B").as_bytes())?;
        }
        let names: Vec<String> = items
            .iter()
            .map(|f| {
                let tpath = truncate_filename((*f.1).clone(), width, &f.4, create_hyprlink);
                tpath
            })
            .collect();
        let pad_x = " ".repeat(x_padding as usize);
        let pad_y = "\n".repeat(y_padding as usize);
        let names_combined = names.join(&pad_x);
        write!(buf, "\n{pad_x}{names_combined}{pad_x}{pad_y}")?;
    }

    out.write_all(&buf)?;
    out.flush()?;
    Ok(())
}

fn combine_images_into_row(
    images: Vec<DynamicImage>,
    padding: u32,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let background = Rgba([0, 0, 0, 0]);
    if images.is_empty() {
        return Ok(Vec::new());
    }

    let max_height = images.iter().map(|img| img.height()).max().unwrap_or(0);
    let total_image_width: u32 = images.iter().map(|img| img.width()).sum();

    // Total width = left padding + images + padding between images
    let total_width = padding + total_image_width + padding * (images.len() as u32 - 1);
    let mut output = RgbaImage::from_pixel(total_width, max_height, background);

    let mut x_offset = padding;
    for img in images {
        let img_height = img.height();
        let y_offset = (max_height - img_height) / 2;
        output.copy_from(&img, x_offset, y_offset)?;
        x_offset += img.width() + padding;
    }

    let img = DynamicImage::ImageRgba8(output);
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    img.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(buffer)
}

///width and height only needed for ascii videos atm
pub fn inline_a_video(
    input: impl AsRef<str>,
    out: &mut impl Write,
    inline_encoder: &rasteroid::InlineEncoder,
    width: Option<&str>,
    height: Option<&str>,
    center: bool,
    silent: bool,
) -> Result<(), Box<dyn error::Error>> {
    match inline_encoder {
        rasteroid::InlineEncoder::Kitty => {
            let frames = video_to_frames(input)?;
            let mut kitty_frames = frames.map(|f| VideoFrames {
                width: f.width as u16,
                height: f.height as u16,
                img: f.data,
                timestamp: f.timestamp,
            });
            match stdout().is_tty() {
                // the fast function leaks memory, not good if not consumed right away..
                true => unsafe {
                    rasteroid::kitty_encoder::encode_frames_fast(&mut kitty_frames, out, center)?
                },
                false => rasteroid::kitty_encoder::encode_frames(&mut kitty_frames, out, center)?,
            }
            Ok(())
        }
        rasteroid::InlineEncoder::Iterm => {
            let gif = video_to_gif(input, silent)?;
            let dyn_img = image::load_from_memory_with_format(&gif, image::ImageFormat::Gif)?;
            let offset = match center {
                true => Some(rasteroid::term_misc::center_image(
                    dyn_img.width() as u16,
                    false,
                )),
                false => None,
            };
            rasteroid::iterm_encoder::encode_image(&gif, out, offset, None)?;
            Ok(())
        }
        rasteroid::InlineEncoder::Ascii | rasteroid::InlineEncoder::Sixel => {
            let frames = video_to_frames(input)?;
            let mut ascii_frames = frames.map(|f| {
                let rgb_image = image::RgbImage::from_raw(f.width, f.height, f.data.clone())
                    .unwrap_or_default();
                let img = image::DynamicImage::ImageRgb8(rgb_image);
                let (img, _, _, _) = img
                    .resize_plus(width, height, true, false)
                    .unwrap_or_default();
                VideoFrames {
                    timestamp: f.timestamp,
                    img,
                    width: 0,
                    height: 0,
                }
            });
            rasteroid::ascii_encoder::encode_frames(&mut ascii_frames, out, center, true)?;
            Ok(())
        }
    }
}

fn video_to_gif(input: impl AsRef<str>, silent: bool) -> Result<Vec<u8>, Box<dyn error::Error>> {
    let input = input.as_ref();
    if input.ends_with(".gif") {
        let path = Path::new(input);
        let bytes = fs::read(path)?;
        return Ok(bytes);
    }

    // Create indeterminate progress bar since we don't know total frames
    let pb = if !silent {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:50.blue/white}] {pos}/{len} frames ({percent}%)")?
                .progress_chars("█▓▒░"),
        );
        Some(pb)
    } else {
        None
    };

    let mut command =
        match fetch_manager::get_ffmpeg() {
            Some(c) => c,
            None => return Err(
                "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`"
                    .into(),
            ),
        };

    command
        .hwaccel("auto")
        .input(input)
        .format("gif")
        .args(&["-progress", "pipe:2"]) // Request progress output
        .output("-");

    let mut child = command.spawn()?;
    let mut stdout = child
        .take_stdout()
        .ok_or("failed to get stdout for ffmpeg")?;
    let stderr = child
        .take_stderr()
        .ok_or("failed to get stderr for ffmpeg")?;

    // Read stdout in a separate thread
    let output_thread = std::thread::spawn(move || {
        let mut output_bytes = Vec::new();
        stdout.read_to_end(&mut output_bytes).unwrap();
        output_bytes
    });

    // Process stderr for progress updates
    let duration_re = Regex::new(r"Duration: (\d+):(\d+):([\d.]+)")?;
    let fps_re = Regex::new(r"(\d+(?:\.\d+)?) fps")?;
    let frame_re = Regex::new(r"frame=\s*(\d+)")?;

    let mut total_frames = None;
    let mut fps = None;
    let mut duration_secs = None;
    for line in std::io::BufReader::new(stderr).lines() {
        let line = line?;
        if let Some(cap) = duration_re.captures(&line) {
            let hours: f64 = cap[1].parse().unwrap_or(0.0);
            let minutes: f64 = cap[2].parse().unwrap_or(0.0);
            let seconds: f64 = cap[3].parse().unwrap_or(0.0);
            duration_secs = Some(hours * 3600.0 + minutes * 60.0 + seconds);
        }
        if fps.is_none() {
            if let Some(cap) = fps_re.captures(&line) {
                fps = Some(cap[1].parse::<f64>().unwrap_or(0.0));
            }
        }
        if total_frames.is_none() {
            if let (Some(dur), Some(f)) = (duration_secs, fps) {
                let frames = (dur * f).round();
                total_frames = Some(frames);
                if !silent {
                    pb.as_ref().unwrap().set_length(frames as u64);
                }
            }
        }

        // Parse frame count from progress output
        if let Some(cap) = frame_re.captures(&line) {
            let current_frame: usize = cap[1].parse().unwrap_or(0);
            if !silent {
                pb.as_ref().unwrap().set_position(current_frame as u64);
            }
        }
    }

    let output_bytes = output_thread
        .join()
        .map_err(|_| "failed to capture output")?;
    child.wait()?;

    Ok(output_bytes)
}

fn video_to_frames(
    input: impl AsRef<str>,
) -> Result<Box<dyn Iterator<Item = OutputVideoFrame>>, Box<dyn error::Error>> {
    let input = input.as_ref();

    let mut command =
        match fetch_manager::get_ffmpeg() {
            Some(c) => c,
            None => return Err(
                "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`"
                    .into(),
            ),
        };
    command.hwaccel("auto").input(input).rawvideo();

    let mut child = command.spawn()?;
    let frames = child.iter()?.filter_frames();

    Ok(Box::new(frames))
}
