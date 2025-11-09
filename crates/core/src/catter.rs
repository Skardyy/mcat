use std::{
    error::Error,
    fs::{self, File},
    io::{Cursor, Write, stdout},
    path::Path,
    process::{Command, Stdio},
};

use clap::error::Result;
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode},
    tty::IsTty,
};
use image::{DynamicImage, ImageFormat};
use markdownify::ConvertOptions;
use rasteroid::{
    InlineEncoder,
    image_extended::{InlineImage, ZoomPanViewport},
    term_misc,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    config::McatConfig,
    converter::{self},
    image_viewer::{clear_screen, run_interactive_viewer, show_help_prompt},
    markdown_viewer,
};

pub enum CatType {
    Markdown,
    Pretty,
    Html,
    Image,
    Video,
    InlineImage,
    InlineVideo,
    Interactive,
}

pub fn get_album(path: &Path) -> Option<Vec<DynamicImage>> {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();

    // pdf
    if matches!(ext.as_ref(), "pdf" | "tex" | "typ") && converter::get_pdf_command().is_ok() {
        let (path, _tmpfile, _tmpfolder) = converter::get_pdf(path);
        let images = converter::pdf_to_vec(&path.to_string_lossy().to_string()).ok()?;
        if !images.is_empty() {
            return Some(images);
        }
    }

    return None;
}

pub fn cat(
    paths: Vec<&Path>,
    out: &mut impl Write,
    opts: &McatConfig,
) -> Result<CatType, Box<dyn std::error::Error>> {
    let path = paths
        .get(0)
        .ok_or("This is most likely a bug - no paths are included in the cat function")?;

    //interactive mode
    if opts.output.clone().unwrap_or_default() == "interactive" {
        if paths.len() > 1 {
            let mut new_opts = opts.clone();
            new_opts.output = Some("image".to_owned());

            let images = paths
                .par_iter()
                .filter_map(|path| {
                    let mut buffer = Vec::new();
                    cat(vec![path], &mut buffer, &new_opts).ok()?;

                    let dyn_img = image::load_from_memory(&buffer).ok()?;
                    Some(dyn_img)
                })
                .collect();

            interact_with_image(images, opts, out)?;
            return Ok(CatType::Interactive);
        }
        if let Some(images) = get_album(path) {
            interact_with_image(images, opts, out)?;
            return Ok(CatType::Interactive);
        }
    }

    if !path.exists() {
        return Err(format!("invalid path: {}", path.display()).into());
    }

    let (result, from, to) = load(path, out, opts)?;
    let (string_result, image_result) = match result {
        LoadResult::Image(dynamic_image) => (None, Some(dynamic_image)),
        LoadResult::Text(text) => (Some(text), None),
        LoadResult::Handled(cat_type) => return Ok(cat_type),
    };

    // converting
    match (from.as_ref(), to.as_ref()) {
        ("md", "md") => {
            out.write_all(string_result.unwrap().as_bytes())?;
            Ok(CatType::Markdown)
        }
        ("md", "html") => {
            let html = markdown_viewer::md_to_html(&string_result.unwrap(), if opts.style_html {Some(opts.theme.as_ref())} else {None});
            out.write_all(html.as_bytes())?;
            Ok(CatType::Html)
        },
        ("md", "image") => {
            let html = markdown_viewer::md_to_html(&string_result.unwrap(), Some(opts.theme.as_ref()));
            let image = converter::html_to_image(&html)?;
            out.write_all(&image)?;
            Ok(CatType::Image)
        },
        ("md", "inline") => {
            let html = markdown_viewer::md_to_html(&string_result.unwrap(), Some(opts.theme.as_ref()));
            let image = converter::html_to_image(&html)?;
            let dyn_img = image::load_from_memory(&image)?;
            print_image(out, dyn_img, opts)?;
            Ok(CatType::InlineImage)
        },
        ("md", "interactive") => {
            let html = markdown_viewer::md_to_html(&string_result.unwrap(), Some(opts.theme.as_ref()));
            let img_bytes = converter::html_to_image(&html)?;
            let img = image::load_from_memory(&img_bytes)?;
            interact_with_image(vec![img], opts, out)?;
            Ok(CatType::Interactive)
        },
        ("html", "image") => {
            let image = converter::html_to_image(&string_result.unwrap())?;
            out.write_all(&image)?;
            Ok(CatType::Image)
        },
        ("html", "inline") => {
            let image = converter::html_to_image(&string_result.unwrap())?;
            let dyn_img = image::load_from_memory(&image)?;
            print_image(out, dyn_img, opts)?;
            Ok(CatType::InlineImage)
        },
        ("html", "interactive") => {
            let html = &string_result.unwrap();
            let img_bytes = converter::html_to_image(&html)?;
            let img = image::load_from_memory(&img_bytes)?;
            interact_with_image(vec![img], opts, out)?;
            Ok(CatType::Interactive)
        },
        ("image", "image") => {
            let img = image_result.unwrap();
            let mut cursor = Cursor::new(Vec::new());
            img.write_to(&mut cursor, ImageFormat::Png)?;
            out.write_all(&cursor.into_inner())?;
            Ok(CatType::Image)
        },
        ("image", "interactive") => {
            let img = image_result.unwrap();
            interact_with_image(vec![img], opts, out)?;
            Ok(CatType::Interactive)
        },
        ("md" | "html", _) => {
            //default for md, html
            let mut res = string_result.unwrap();
            if from == "html" {
                res = format!("```html\n{res}\n```");
            }
            let is_tty = stdout().is_tty();
            let use_color = opts.color.should_use(is_tty);
            let content = match use_color {
                true => markdown_viewer::md_to_ansi(&res, &opts, Some(path)),
                false => res,
            };
            let use_pager = opts.paging.should_use(is_tty && content.lines().count() > term_misc::get_wininfo().sc_height as usize);
            if use_pager {
                if let Some(pager) = Pager::new(opts.pager.as_ref()) {
                    if pager.page(&content).is_err() {
                        out.write_all(content.as_bytes())?;
                    }
                } else {
                    out.write_all(content.as_bytes())?;
                }
                Ok(CatType::Pretty)
            } else {
                out.write_all(content.as_bytes())?;
                return Ok(CatType::Markdown)
            }
        },
        ("image", _) => {
            // default for image
            print_image(out, image_result.unwrap(), opts)?;
            Ok(CatType::InlineImage)
        },
        _ => Err(format!(
            "converting: {} to: {}, is not supported.\nsupported pipeline is: any -> md -> html -> image -> inline_image / interactive_image\nor video -> inline_video",
            from, to
        ).into()),
    }
}

pub enum LoadResult {
    Image(DynamicImage),
    Text(String),
    Handled(CatType),
}
pub fn load(
    path: &Path,
    out: &mut impl Write,
    opts: &McatConfig,
) -> Result<(LoadResult, String, String), Box<dyn std::error::Error>> {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let to = opts.output.as_deref().unwrap_or("unknown").to_owned();

    //video
    if is_video(&ext) {
        if to == "video" {
            let content = fs::read(path)?;
            out.write_all(&content)?;

            let res = LoadResult::Handled(CatType::Video);
            return Ok((res, "video".to_owned(), to));
        }
        converter::inline_a_video(
            path.to_string_lossy(),
            out,
            &opts.inline_encoder,
            opts.inline_options.width.as_deref(),
            opts.inline_options.height.as_deref(),
            opts.inline_options.center,
            opts.silent,
        )?;

        let res = LoadResult::Handled(CatType::InlineVideo);
        return Ok((res, "video".to_owned(), to));
    }

    // pdf to images
    if matches!(ext.as_ref(), "pdf" | "tex" | "typ")
        && matches!(to.as_ref(), "inline" | "image" | "interactive")
        && converter::get_pdf_command().is_ok()
    {
        let (path, _tmpfile, _tmpfolder) = converter::get_pdf(path);

        // goes back to normal parsing if fails.
        if let Ok(img_data) = converter::pdf_to_image(&path.to_string_lossy().to_owned(), 1) {
            match to.as_ref() {
                "image" => {
                    let res = LoadResult::Handled(CatType::Image);
                    out.write_all(&img_data)?;
                    return Ok((res, "image".to_owned(), to));
                }
                _ => {
                    let dyn_img = image::load_from_memory(&img_data)?;
                    let res = LoadResult::Image(dyn_img);
                    return Ok((res, "image".to_owned(), to));
                }
            }
        }
    }

    //svg
    if ext == "svg" {
        let file = File::open(path)?;
        let dyn_img = converter::svg_to_image(
            file,
            opts.inline_options.width.as_deref(),
            opts.inline_options.height.as_deref(),
        )?;

        let res = LoadResult::Image(dyn_img);
        return Ok((res, "image".to_owned(), to));
    }

    // .url
    if ext == "url" {
        let dyn_img =
            converter::url_file_to_image(path).ok_or("Url file doesn't contain an icon")?;
        let res = LoadResult::Image(dyn_img);
        return Ok((res, "image".to_owned(), to));
    }

    // exe
    if ext == "exe" {
        let dyn_img = converter::exe_to_image(path).ok_or("Failed to get exe icon")?;
        let res = LoadResult::Image(dyn_img);
        return Ok((res, "image".to_owned(), to));
    }

    //image
    if ImageFormat::from_extension(&ext).is_some() {
        let buf = fs::read(path)?;
        let dyn_img = image::load_from_memory(&buf)?;

        let res = LoadResult::Image(dyn_img);
        return Ok((res, "image".to_owned(), to));
    }

    // local file or dir
    match ext.as_ref() {
        "md" | "html" => {
            let r = fs::read_to_string(path)?;

            let res = LoadResult::Text(r);
            return Ok((res, ext, to));
        }
        _ => {
            let screen_size = term_misc::get_wininfo();
            let opts = ConvertOptions::new(path)
                .with_screen_size((screen_size.sc_width, screen_size.sc_height));
            let f = markdownify::convert(opts)?;

            let res = LoadResult::Text(f);
            return Ok((res, "md".to_owned(), to));
        }
    }
}

fn print_image(
    out: &mut impl Write,
    dyn_img: DynamicImage,
    opts: &McatConfig,
) -> Result<(), Box<dyn Error>> {
    let resize_for_ascii = match opts.inline_encoder {
        rasteroid::InlineEncoder::Ascii => true,
        _ => false,
    };

    let dyn_img = apply_pan_zoom_once(dyn_img, &opts);
    let (img, center, _, _) = dyn_img.resize_plus(
        opts.inline_options.width.as_deref(),
        opts.inline_options.height.as_deref(),
        resize_for_ascii,
        false,
    )?;
    if opts.report {
        rasteroid::term_misc::report_size(
            &opts.inline_options.width.as_deref().unwrap_or(""),
            &opts.inline_options.height.as_deref().unwrap_or(""),
        );
    }
    rasteroid::inline_an_image(
        &img,
        out,
        if opts.inline_options.center {
            Some(center)
        } else {
            None
        },
        None,
        &opts.inline_encoder,
    )?;

    Ok(())
}

fn apply_pan_zoom_once(img: DynamicImage, opts: &McatConfig) -> DynamicImage {
    let zoom = opts.inline_options.zoom.unwrap_or(1);
    let x = opts.inline_options.x.unwrap_or_default();
    let y = opts.inline_options.y.unwrap_or_default();
    if zoom == 1 && x == 0 && y == 0 {
        return img;
    }

    let tinfo = term_misc::get_wininfo();
    let container_width = tinfo.spx_width as u32;
    let container_height = tinfo.spx_height as u32;
    let image_width = img.width();
    let image_height = img.height();

    let mut vp = ZoomPanViewport::new(container_width, container_height, image_width, image_height);
    vp.set_zoom(zoom);
    vp.set_pan(x, y);
    vp.apply_to_image(&img)
}

fn interact_with_image(
    images: Vec<DynamicImage>,
    opts: &McatConfig,
    out: &mut impl Write,
) -> Result<(), Box<dyn Error>> {
    if images.is_empty() {
        return Err("Most likely a bug - interact_with_image received 0 paths".into());
    }

    let mut img = &images[0];
    let tinfo = term_misc::get_wininfo();
    let container_width = tinfo.spx_width as u32;
    let container_height = tinfo.spx_height as u32;
    let image_width = img.width();
    let image_height = img.height();

    let resize_for_ascii = match opts.inline_encoder {
        rasteroid::InlineEncoder::Ascii => true,
        _ => false,
    };

    let height_cells = term_misc::dim_to_cells(
        opts.inline_options.height.as_deref().unwrap_or(""),
        term_misc::SizeDirection::Height,
    )?;
    let height = (tinfo.sc_height - 3).min(height_cells as u16);
    let should_disable_raw_mode = match opts.inline_encoder {
        InlineEncoder::Kitty => tinfo.is_tmux,
        InlineEncoder::Ascii => true,
        InlineEncoder::Iterm | InlineEncoder::Sixel => false,
    };
    let mut current_index = 0;
    let max_images = images.len();

    run_interactive_viewer(
        container_width,
        container_height,
        image_width,
        image_height,
        images.len() as u8,
        |vp, current_image| {
            if current_image != current_index {
                current_index = current_image;
                img = &images[current_image as usize];
                let width = img.width();
                let height = img.height();
                vp.update_image_size(width, height);
            }
            let new_img = vp.apply_to_image(&img);
            let (img, center, _, _) = new_img
                .resize_plus(
                    opts.inline_options.width.as_deref(),
                    Some(&format!("{height}c")),
                    resize_for_ascii,
                    false,
                )
                .ok()?;
            if should_disable_raw_mode {
                disable_raw_mode().ok()?;
            }
            let mut buf = Vec::new();
            rasteroid::inline_an_image(
                &img,
                &mut buf,
                if opts.inline_options.center {
                    Some(center)
                } else {
                    None
                },
                None,
                &opts.inline_encoder,
            )
            .ok()?;
            show_help_prompt(
                &mut buf,
                tinfo.sc_width,
                tinfo.sc_height,
                vp,
                current_image,
                max_images as u8,
            )
            .ok()?;
            clear_screen(out, Some(buf)).ok()?;
            out.flush().ok()?;
            if should_disable_raw_mode {
                enable_raw_mode().ok()?;
            }

            Some(())
        },
    )?;
    clear_screen(out, None)?;
    Ok(())
}

pub fn is_video(ext: &str) -> bool {
    matches!(
        ext,
        "mp4" | "mov" | "avi" | "mkv" | "webm" | "wmv" | "flv" | "m4v" | "ts" | "gif"
    )
}

pub struct Pager {
    command: String,
    args: Vec<String>,
}

impl Pager {
    pub fn command_and_args_from_string(full: &str) -> Option<(String, Vec<String>)> {
        let parts = shell_words::split(full).ok()?;
        let (cmd, args) = parts.split_first()?;
        return Some((cmd.clone(), args.to_vec()));
    }
    pub fn new(def_command: &str) -> Option<Self> {
        let (command, args) = Pager::command_and_args_from_string(def_command)?;
        if which::which(&command).is_ok() {
            return Some(Self { command, args });
        }
        None
    }

    pub fn page(&self, content: &str) -> Result<(), Box<dyn Error>> {
        let mut child = Command::new(&self.command)
            .args(&self.args)
            .stdin(Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            // ignoring cuz the pipe will break when the user quits most likely
            let _ = stdin.write_all(content.as_bytes());
        }

        child.wait()?;

        Ok(())
    }
}
