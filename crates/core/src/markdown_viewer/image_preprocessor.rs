use std::{collections::HashMap, path::Path};

use anyhow::Context;
use anyhow::Result;
use base64::Engine;
use comrak::nodes::{AstNode, NodeValue};
use image::GenericImageView;
use itertools::Itertools;
use rasteroid::Encoder;
use rasteroid::term_misc::Wininfo;
use rasteroid::{
    RasterEncoder,
    image_extended::InlineImage,
    term_misc::{self},
};
use rayon::iter::IndexedParallelIterator;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;

use tracing::{info, warn};

use crate::mcat_file::McatFile;
use crate::{
    config::{McatConfig, MdImageMode},
    scrapy::{MediaScrapeOptions, scrape_biggest_media},
};

use super::render::UNDERLINE_OFF;

fn is_local_path(url: &str) -> bool {
    !url.starts_with("http://") && !url.starts_with("https://") && !url.starts_with("data:")
}

fn handle_data_uri(url: &str) -> Option<McatFile> {
    let rest = url.strip_prefix("data:")?;
    let (_, data) = rest.split_once("base64,")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    McatFile::from_bytes(bytes, None, None, None).ok()
}

fn handle_local_image(path: &str, markdown_file_dir: Option<&Path>) -> Result<McatFile> {
    let original_path = Path::new(path);

    // Try absolute or CWD-relative path first
    if original_path.exists() {
        return McatFile::from_path(original_path);
    }

    // If that fails and we have a markdown file directory, try relative to that
    if let Some(md_dir) = markdown_file_dir {
        let relative_path = md_dir.join(path);
        if relative_path.exists() {
            return McatFile::from_path(relative_path);
        } else {
            anyhow::bail!(
                "Local image file not found: {} (tried {} and {})",
                path,
                path,
                relative_path.display()
            )
        }
    }

    anyhow::bail!("Local image file not found: {}", path)
}

pub struct ImagePreprocessor {
    pub mapper: HashMap<String, ImageElement>,
}

impl ImagePreprocessor {
    pub fn new<'a>(
        node: &'a AstNode<'a>,
        conf: &McatConfig,
        markdown_file_path: Option<&Path>,
    ) -> Result<Self> {
        let mut urls = Vec::new();
        extract_image_urls(node, &mut urls);
        let encoder = conf
            .encoder
            .as_ref()
            .context("this is likely a bug, encoder isn't set at ImagePreprocessor new")?;
        let wininfo = conf
            .wininfo
            .as_ref()
            .context("this is likely a bug, wininfo isn't set at ImagePreprocessor new")?;

        let render_mode = if conf.md_image != MdImageMode::Auto {
            &conf.md_image
        } else {
            match *encoder {
                RasterEncoder::Kitty => &MdImageMode::All,
                RasterEncoder::Iterm => &MdImageMode::Small,
                RasterEncoder::Sixel => &MdImageMode::Small,
                RasterEncoder::Ascii => &MdImageMode::None,
            }
        };
        info!(
            image_count = urls.len(),
            ?render_mode,
            "preprocessing markdown images"
        );
        let markdown_dir = markdown_file_path.and_then(|p| p.parent());
        let scrape_opts = MediaScrapeOptions {
            max_content_length: match render_mode {
                MdImageMode::All => None,
                _ => Some(50_000), // filter complex images - won't scale down good
            },
        };

        if render_mode == &MdImageMode::None {
            return Ok(ImagePreprocessor {
                mapper: HashMap::new(),
            });
        }

        let mapper: HashMap<String, ImageElement> = urls
            .par_iter()
            .enumerate()
            .filter_map(|(i, url)| {
                let tmp = if url.base_url.starts_with("data:") {
                    handle_data_uri(&url.base_url)?
                } else if is_local_path(&url.base_url) {
                    match handle_local_image(&url.base_url, markdown_dir) {
                        Ok(f) => Some(f),
                        Err(e) => {
                            warn!(%e);
                            None
                        }
                    }?
                } else {
                    scrape_biggest_media(&url.base_url, &scrape_opts, conf.bar.as_ref()).ok()?
                };

                let img = match tmp.to_image(conf, false, false) {
                    Ok(img) => img,
                    Err(e) => {
                        warn!(url = %url.base_url, error = %e, "failed to convert image");
                        return None;
                    }
                };

                let (width, height) = img.dimensions();
                let width = url.width.map(|v| v as u32).unwrap_or(width);
                let height = url.height.map(|v| v as u32).unwrap_or(height);
                let width_fm = if width as f32 > wininfo.spx_width as f32 * 0.8 {
                    "80%"
                } else {
                    &format!("{width}px")
                };
                let height_fm = if render_mode == &MdImageMode::Small {
                    let px = wininfo
                        .dim_to_px("1c", term_misc::SizeDirection::Height)
                        .unwrap_or_default()
                        .saturating_sub(1); // it ceils, so we must make sure 1c
                    &format!("{px}px")
                } else if height as f32 > wininfo.spx_height as f32 * 0.4 {
                    "40%"
                } else {
                    &format!("{height}px")
                };

                let img =
                    match img.resize_plus(wininfo, Some(width_fm), Some(height_fm), false, false) {
                        Ok(img) => img,
                        Err(e) => {
                            warn!(url = %url.base_url, error = %e, "failed to resize image");
                            return None;
                        }
                    };

                let mut buffer = Vec::new();
                if let Err(e) = encoder.encode_image(&img, &mut buffer, wininfo, None, None) {
                    warn!(url = %url.original_url, error = %e, "failed to encode image");
                    return None;
                }

                let img_str = String::from_utf8(buffer).unwrap_or_default();
                let placeholder = create_placeholder(wininfo, &img_str, i, encoder, img.width());

                Some((
                    url.original_url.clone(),
                    ImageElement {
                        is_ok: true,
                        placeholder,
                        img: img_str,
                    },
                ))
            })
            .collect();

        Ok(ImagePreprocessor { mapper })
    }
}

fn create_placeholder(
    wininfo: &Wininfo,
    img: &str,
    id: usize,
    inline_encoder: &RasterEncoder,
    width: u32,
) -> String {
    let fg_color = 16 + (id % 216);
    let bg_color = 16 + ((id / 216) % 216);

    let (width, height) = match inline_encoder {
        RasterEncoder::Kitty => {
            let placeholder = "\u{10EEEE}";
            let first_line = img.lines().next().unwrap_or("");
            let width = first_line.matches(placeholder).count();
            let count = img.lines().count();
            (width, count)
        }
        _ => {
            let width = wininfo
                .dim_to_cells(&format!("{width}px"), term_misc::SizeDirection::Width)
                .unwrap_or(1) as usize;
            (width, 1)
        }
    };

    let line = format!(
        "\x1b[38;5;{}m\x1b[48;5;{}m{}\x1b[0m",
        fg_color,
        bg_color,
        "█".repeat(width)
    );
    vec![line; height].join("\n")
}

pub struct ImageElement {
    pub is_ok: bool,
    pub placeholder: String,
    pub img: String,
}

impl ImageElement {
    pub fn insert_into_text(&self, text: &mut String) {
        if !self.is_ok {
            return;
        }

        let img = self
            .img
            .lines()
            .map(|line| format!("{UNDERLINE_OFF}{}", line))
            .join("\n");
        let placeholder_line = self.placeholder.lines().nth(0).unwrap_or_default();

        loop {
            if !text.contains(placeholder_line) {
                break;
            }
            for img_line in img.lines() {
                *text = text.replacen(placeholder_line, img_line, 1);
            }
        }
    }
}

#[derive(Debug)]
struct ImageUrl {
    base_url: String,
    original_url: String,
    width: Option<u16>,
    height: Option<u16>,
}
fn extract_image_urls<'a>(node: &'a AstNode<'a>, urls: &mut Vec<ImageUrl>) {
    let data = node.data.borrow();

    if let NodeValue::Image(image_node) = &data.value {
        // regex for; <URL>#<Width>x<Height>
        // width and height are optional.
        let regex = Regex::new(r"^(.+?)(?:#(\d+)?x(\d+)?)?$").unwrap();
        if let Some(captures) = regex.captures(&image_node.url)
            && let Some(base_url) = captures.get(1)
        {
            let width = captures.get(2).and_then(|v| v.as_str().parse::<u16>().ok());
            let height = captures.get(3).and_then(|v| v.as_str().parse::<u16>().ok());
            urls.push(ImageUrl {
                base_url: base_url.as_str().to_owned(),
                original_url: image_node.url.clone(),
                width,
                height,
            });
        }
    }

    for child in node.children() {
        extract_image_urls(child, urls);
    }
}
