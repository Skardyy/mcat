use anyhow::{Context, Result};
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode},
    tty::IsTty,
};
use image::{DynamicImage, ImageFormat};
use rasteroid::{
    RasterEncoder,
    image_extended::{InlineImage, ZoomPanViewport},
    term_misc,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::{
    fs::{self, File},
    io::{Cursor, Write, stdout},
    path::Path,
    process::{Command, Stdio},
};

use crate::{
    config::{McatConfig, OutputFormat},
    converter::{self},
    image_viewer::{clear_screen, run_interactive_viewer, show_help_prompt},
    markdown_viewer,
    mcat_file::{self, McatFile},
};

pub fn get_album(file: &McatFile) -> Option<Vec<DynamicImage>> {
    // let ext = path
    //     .extension()
    //     .unwrap_or_default()
    //     .to_string_lossy()
    //     .into_owned();
    //
    // // pdf
    // if matches!(ext.as_ref(), "pdf" | "tex" | "typ") && converter::get_pdf_command().is_ok() {
    //     let (path, _tmpfile, _tmpfolder) = converter::get_pdf(path);
    //     let images = converter::pdf_to_vec(&path.to_string_lossy().to_string()).ok()?;
    //     if !images.is_empty() {
    //         return Some(images);
    //     }
    // }

    return None;
}

pub fn cat(files: Vec<McatFile>, out: &mut impl Write, config: &McatConfig) -> Result<()> {
    let mf = files
        .get(0)
        .context("this is likely a bug, mcat cat command was passed with 0 files")?;

    // interactive mode
    if config
        .output
        .as_ref()
        .map(|v| v == &OutputFormat::Interactive)
        .unwrap_or(false)
    {
        if files.len() > 1 {
            let images = files
                .par_iter()
                .map(|v| {
                    v.to_image(config).and_then(|v| {
                        image::load_from_memory(&v.0).context("failed to load image from memory")
                    })
                })
                .collect::<Result<Vec<_>>>()?;

            interact_with_image(images, config, out)?;
            return Ok(());
        }
        if let Some(images) = get_album(mf) {
            interact_with_image(images, config, out)?;
            return Ok(());
        }
    }

    // markdown viewer (default)
    // interactive (above)
    // html raw (when specified)
    // md raw (when specified)
    // image raw (when specified)
    // video raw (when specified)
    // inline image/video (only single)

    let mcat_file = if files.len() > 1 {
        let files = files.iter().map(|v| v.to_markdown_input()).collect();
        let md = markdownify::convert_files(files)?;
        &McatFile::from_bytes(md.into_bytes())
    } else {
        mf
    };

    // converting
    match config.output {
        Some(OutputFormat::Html) => todo!(),
        Some(OutputFormat::Md) => todo!(),
        Some(OutputFormat::Image) => todo!(),
        Some(OutputFormat::Video) => todo!(),
        Some(OutputFormat::Inline) => todo!(),
        Some(OutputFormat::Interactive) => todo!(),
        None => {}
    }

    Ok(())
}

fn print_image(out: &mut impl Write, dyn_img: DynamicImage, config: &McatConfig) -> Result<()> {
    let resize_for_ascii = match opts.inline_encoder {
        rasteroid::RasterEncoder::Ascii => true,
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
) -> Result<()> {
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
        rasteroid::RasterEncoder::Ascii => true,
        _ => false,
    };

    let height_cells = term_misc::dim_to_cells(
        opts.inline_options.height.as_deref().unwrap_or(""),
        term_misc::SizeDirection::Height,
    )?;
    let height = (tinfo.sc_height - 3).min(height_cells as u16);
    let should_disable_raw_mode = match opts.inline_encoder {
        RasterEncoder::Kitty => tinfo.is_tmux,
        RasterEncoder::Ascii => true,
        RasterEncoder::Iterm | RasterEncoder::Sixel => false,
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

    pub fn page(&self, content: &str) -> Result {
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
