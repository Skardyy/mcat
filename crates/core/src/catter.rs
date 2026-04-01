use anyhow::{Context, Result};
use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode},
    tty::IsTty,
};
use image::DynamicImage;
use rasteroid::{Encoder, RasterEncoder, image_extended::InlineImage, term_misc};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use std::{
    io::{Cursor, Write, stdout},
    process::{Command, Stdio},
};

use tracing::{info, warn};

use crate::{
    config::{ColorMode, McatConfig, OutputFormat},
    image_viewer::{clear_screen, run_interactive_viewer, show_help_prompt},
    markdown_viewer,
    mcat_file::{McatFile, McatKind},
};

pub fn cat(files: Vec<McatFile>, out: &mut impl Write, config: &McatConfig) -> Result<()> {
    let mf = files
        .first()
        .context("this is likely a bug, mcat cat command was passed with 0 files")?;
    let encoder = config
        .encoder
        .context("this is likely a bug, encoder wasn't set at the cat command")?;
    let wininfo = config
        .wininfo
        .as_ref()
        .context("this is likely a bug, wininfo isn't set when inlining a video")?;

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
                .map(|v| v.to_image(config, false, false))
                .collect::<Result<Vec<_>>>()?;

            interact_with_image(images, config, out)?;
            return Ok(());
        }
        let images = mf.to_album(config)?;
        interact_with_image(images, config, out)?;
        return Ok(());
    }

    let mcat_file = if files.len() > 1 {
        if config.output.as_ref() == Some(&OutputFormat::Image) {
            anyhow::bail!("Cannot turn multiple files into an image.")
        };
        if files.iter().any(|v| v.kind == McatKind::Video) {
            anyhow::bail!("Cannot view multiple files if 1 of them is a video.")
        }

        // turns things that cannot be represented to images.
        let files = files
            .into_par_iter()
            .map(|v| match v.kind {
                McatKind::PreMarkdown => Ok(v),
                McatKind::Markdown => Ok(v),
                McatKind::Html => Ok(v),
                McatKind::Video => unreachable!(),
                McatKind::Gif
                | McatKind::Svg
                | McatKind::Exe
                | McatKind::Lnk
                | McatKind::Pdf
                | McatKind::Tex
                | McatKind::Url
                | McatKind::Typst => {
                    let img = v.to_image(config, false, true)?;
                    let f = McatFile::from_image(img);
                    Ok(f)
                }
                McatKind::Image => Ok(v),
            })
            .collect::<Result<Vec<_>>>()?;

        let files = files
            .iter()
            .map(|v| v.to_markdown_input(config.inline_images_in_md))
            .collect::<Result<Vec<_>>>()?;
        let md = markdownify::convert_files(files)?;
        &McatFile::from_bytes(md.into_bytes(), Some("md"))?
    } else {
        mf
    };

    // force certain things to be inline.
    let output = match config.output.clone() {
        Some(v) => Some(v),
        None => match mcat_file.kind {
            McatKind::Video
            | McatKind::Gif
            | McatKind::Image
            | McatKind::Svg
            | McatKind::Pdf
            | McatKind::Exe
            | McatKind::Lnk => Some(OutputFormat::Inline),
            _ => None,
        },
    };
    // converting
    match output {
        Some(OutputFormat::Html) => {
            let html = mcat_file.to_html(Some(config.theme.clone()), config.inline_images_in_md)?;
            out.write_all(html.as_bytes())?
        }
        Some(OutputFormat::Md) => {
            let md = mcat_file
                .to_markdown_input(config.inline_images_in_md)?
                .convert()?;
            out.write_all(md.as_bytes())?
        }
        Some(OutputFormat::Image) => {
            let img = mcat_file.to_image(config, false, true)?;
            let mut buf = Vec::new();
            img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)?;
            out.write_all(&buf)?;
        }
        Some(OutputFormat::Inline) => {
            let is_ascii = config
                .encoder
                .map(|v| v == RasterEncoder::Ascii)
                .unwrap_or(false);
            match mcat_file.kind {
                McatKind::Video | McatKind::Gif => {
                    let (mut frames, mut width, _) = mcat_file.to_frames()?;
                    // frames don't give width according to the encoder
                    if is_ascii {
                        width = wininfo
                            .dim_to_cells(&format!("{width}px"), term_misc::SizeDirection::Width)?;
                    }
                    let offset = wininfo.center_offset(width as u16, is_ascii);
                    encoder.encode_frames(&mut frames, out, wininfo, Some(offset), None)?;
                }
                _ => {
                    let img = mcat_file.to_image(config, false, true)?;
                    let offset = wininfo.center_offset(img.width() as u16, is_ascii);
                    encoder.encode_image(&img, out, wininfo, Some(offset), None)?;
                }
            }
        }
        Some(OutputFormat::Interactive) => unreachable!(),
        None => {
            let md = mcat_file
                .to_markdown_input(config.inline_images_in_md)?
                .convert()?;

            let is_tty = stdout().is_tty();
            let use_color = match config.color {
                ColorMode::Never => false,
                ColorMode::Always => true,
                ColorMode::Auto => is_tty,
            };
            let content = match use_color {
                true => {
                    markdown_viewer::md_to_ansi(&md, config.clone(), mcat_file.path.as_deref())?
                }
                false => md,
            };

            let use_pager = match config.paging {
                crate::config::PagingMode::Never => false,
                crate::config::PagingMode::Always => true,
                crate::config::PagingMode::Auto => {
                    is_tty && content.lines().count() > wininfo.sc_height as usize
                }
            };

            if use_pager {
                if let Some(pager) = Pager::new(&config.pager) {
                    info!(pager = %config.pager, "using pager");
                    if pager.page(&content).is_err() {
                        warn!(pager = %config.pager, "pager failed, writing directly");
                        out.write_all(content.as_bytes())?;
                    }
                } else {
                    warn!(pager = %config.pager, "pager not found, writing directly");
                    out.write_all(content.as_bytes())?;
                }
            } else {
                out.write_all(content.as_bytes())?;
            }
        }
    }

    Ok(())
}

fn interact_with_image(
    images: Vec<DynamicImage>,
    opts: &McatConfig,
    out: &mut impl Write,
) -> Result<()> {
    if images.is_empty() {
        anyhow::bail!("Most likely a bug - interact_with_image received 0 paths");
    }
    let wininfo = opts
        .wininfo
        .as_ref()
        .context("this is likely a bug, wininfo isn't set at interact_with_image")?;
    let encoder = opts
        .encoder
        .as_ref()
        .context("this is likely a bug encoder wasn't set at interact_with_image")?;

    let mut img = &images[0];
    let container_width = wininfo.spx_width as u32;
    let container_height = wininfo.spx_height as u32;
    let image_width = img.width();
    let image_height = img.height();

    let resize_for_ascii = encoder == &RasterEncoder::Ascii;

    let height = wininfo.sc_height - 4;
    let should_disable_raw_mode = match encoder {
        RasterEncoder::Kitty => wininfo.is_tmux,
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
            let new_img = vp.apply_to_image(img);
            let img = new_img
                .resize_plus(
                    wininfo,
                    Some("80%"),
                    Some(&format!("{height}c")),
                    resize_for_ascii,
                    false,
                )
                .ok()?;
            let center = wininfo.center_offset(img.width() as u16, resize_for_ascii);
            let img_height_cells = wininfo
                .dim_to_cells(
                    &format!("{}px", img.height()),
                    term_misc::SizeDirection::Height,
                )
                .unwrap_or(height as u32);
            let v_pad = (height as u32).saturating_sub(img_height_cells) / 2;
            if should_disable_raw_mode {
                disable_raw_mode().ok()?;
            }

            let mut buf = Vec::new();
            buf.write_all("\n".repeat(v_pad as usize).as_bytes()).ok()?;
            encoder
                .encode_image(
                    &img,
                    &mut buf,
                    wininfo,
                    if opts.no_center { None } else { Some(center) },
                    None,
                )
                .ok()?;

            show_help_prompt(
                &mut buf,
                wininfo.sc_width,
                wininfo.sc_height,
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

pub struct Pager {
    command: String,
    args: Vec<String>,
}

impl Pager {
    pub fn command_and_args_from_string(full: &str) -> Option<(String, Vec<String>)> {
        let parts = shell_words::split(full).ok()?;
        let (cmd, args) = parts.split_first()?;
        Some((cmd.clone(), args.to_vec()))
    }
    pub fn new(def_command: &str) -> Option<Self> {
        let (command, args) = Pager::command_and_args_from_string(def_command)?;
        if which::which(&command).is_ok() {
            return Some(Self { command, args });
        }
        None
    }

    pub fn page(&self, content: &str) -> Result<()> {
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
