use ignore::WalkBuilder;
use image::{DynamicImage, GenericImage, ImageFormat, Rgba, RgbaImage};
use itertools::Itertools;
use rasteroid::{
    image_extended::InlineImage,
    term_misc::{self, SizeDirection},
};
use rayon::prelude::*;
use std::io::Write;
use std::{
    error,
    fs::{self},
    io::Cursor,
    path::Path,
};

use crate::{
    catter,
    config::{LsixOptions, SortMode},
    markdown_viewer::utils::string_len,
};

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
    inline_encoder: &rasteroid::RasterEncoder,
) -> Result<(), Box<dyn error::Error>> {
    let dir_path = Path::new(input.as_ref());
    let walker = WalkBuilder::new(dir_path)
        .standard_filters(!ctx.hidden)
        .hidden(!ctx.hidden)
        .max_depth(Some(1))
        .follow_links(true)
        .build();
    let resize_for_ascii = matches!(inline_encoder, rasteroid::RasterEncoder::Ascii);
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
        let base_dir_order = b_is_dir.cmp(&a_is_dir);
        let dir_order = if ctx.reverse {
            base_dir_order.reverse()
        } else {
            base_dir_order
        };

        match dir_order {
            std::cmp::Ordering::Equal => {
                let order = match ctx.sort_mode {
                    SortMode::Name => {
                        let a_str = a.0.to_string_lossy().to_lowercase();
                        let b_str = b.0.to_string_lossy().to_lowercase();
                        a_str.cmp(&b_str)
                    }
                    SortMode::Size => {
                        let a_size = a.0.metadata().ok().map(|m| m.len()).unwrap_or(0);
                        let b_size = b.0.metadata().ok().map(|m| m.len()).unwrap_or(0);
                        a_size.cmp(&b_size)
                    }
                    SortMode::Time => {
                        let a_time = a.0.metadata().ok().and_then(|m| m.modified().ok());
                        let b_time = b.0.metadata().ok().and_then(|m| m.modified().ok());
                        a_time.cmp(&b_time)
                    }
                    SortMode::Type => {
                        let a_ext =
                            a.0.extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                        let b_ext =
                            b.0.extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("")
                                .to_lowercase();

                        match a_ext.cmp(&b_ext) {
                            std::cmp::Ordering::Equal => {
                                let a_str = a.0.to_string_lossy().to_lowercase();
                                let b_str = b.0.to_string_lossy().to_lowercase();
                                a_str.cmp(&b_str)
                            }
                            ext_order => ext_order,
                        }
                    }
                };

                if ctx.reverse { order.reverse() } else { order }
            }
            dir_order => dir_order,
        }
    });

    // Process images in parallel
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
                let tpath = truncate_filename((*f.1).clone(), width, &f.4, ctx.create_hyprlink);
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
