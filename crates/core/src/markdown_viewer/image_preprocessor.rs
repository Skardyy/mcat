use std::{collections::HashMap, fs};

use comrak::nodes::{AstNode, NodeValue};
use image::{DynamicImage, GenericImageView, ImageFormat};
use rasteroid::{
    InlineEncoder,
    image_extended::InlineImage,
    inline_an_image,
    term_misc::{self},
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use regex::Regex;
use tempfile::NamedTempFile;

use crate::{
    config::{McatConfig, MdImageRender},
    converter::svg_to_image,
    scrapy::{MediaScrapeOptions, scrape_biggest_media},
};

use super::render::UNDERLINE_OFF;

pub struct ImagePreprocessor {
    pub mapper: HashMap<String, ImageElement>,
}

impl ImagePreprocessor {
    pub fn new<'a>(node: &'a AstNode<'a>, conf: &McatConfig) -> Self {
        let mut urls = Vec::new();
        extract_image_urls(node, &mut urls);

        let render_mode = if conf.md_image_render != MdImageRender::Auto {
            conf.md_image_render
        } else {
            match conf.inline_encoder {
                InlineEncoder::Kitty => MdImageRender::All,
                InlineEncoder::Iterm => MdImageRender::Small,
                InlineEncoder::Sixel => MdImageRender::Small,
                InlineEncoder::Ascii => MdImageRender::None,
            }
        };
        let mut scrape_opts = MediaScrapeOptions::default();
        scrape_opts.silent = conf.silent;
        scrape_opts.videos = false;
        scrape_opts.documents = false;
        scrape_opts.max_content_length = match render_mode {
            MdImageRender::All => None,
            _ => Some(50_000), // filter complex images -- won't scale down good
        };

        let items: Vec<(&ImageUrl, Vec<u8>, u32)> = urls
            .par_iter()
            .filter_map(|url| {
                // fail everything early if needed.
                if render_mode == MdImageRender::None {
                    return None;
                }

                let tmp = scrape_biggest_media(&url.base_url, &scrape_opts).ok()?;
                let img = render_image(tmp, url.width, url.height)?;

                let (width, height) = img.dimensions();
                let width = url.width.map(|v| v as u32).unwrap_or(width);
                let height = url.height.map(|v| v as u32).unwrap_or(height);
                let width_fm = if width as f32 > term_misc::get_wininfo().spx_width as f32 * 0.8 {
                    "80%"
                } else {
                    &format!("{width}px")
                };
                let height_fm = if render_mode == MdImageRender::Small {
                    let px = term_misc::dim_to_px("1c", term_misc::SizeDirection::Height)
                        .unwrap_or_default()
                        .saturating_sub(1); // it ceils, so we must make sure 1c
                    &format!("{px}px")
                } else if height as f32 > term_misc::get_wininfo().spx_height as f32 * 0.4 {
                    "40%"
                } else {
                    &format!("{height}px")
                };

                let (img, _, new_width, _) = img
                    .resize_plus(Some(&width_fm), Some(&height_fm), false, false)
                    .ok()?;

                return Some((url, img, new_width));
            })
            .collect();

        let mut mapper: HashMap<String, ImageElement> = HashMap::new();
        for (i, (url, img, width)) in items.iter().enumerate() {
            let mut buffer = Vec::new();
            if inline_an_image(&img, &mut buffer, None, None, &conf.inline_encoder).is_ok() {
                let img = String::from_utf8(buffer).unwrap_or_default();
                let img = ImageElement {
                    is_ok: true,
                    placeholder: create_placeholder(&img, i, &conf.inline_encoder, width.clone()),
                    img,
                };
                mapper.insert(url.original_url.clone(), img);
            }
        }

        ImagePreprocessor { mapper }
    }
}

fn create_placeholder(img: &str, id: usize, inline_encoder: &InlineEncoder, width: u32) -> String {
    let fg_color = 16 + (id % 216);
    let bg_color = 16 + ((id / 216) % 216);

    let (width, height) = match inline_encoder {
        InlineEncoder::Kitty => {
            let placeholder = "\u{10EEEE}";
            let first_line = img.lines().next().unwrap_or("");
            let width = first_line.matches(placeholder).count();
            let count = img.lines().count();
            (width, count)
        }
        _ => {
            let width =
                term_misc::dim_to_cells(&format!("{width}px"), term_misc::SizeDirection::Width)
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

fn render_image(
    tmp: NamedTempFile,
    width: Option<u16>,
    height: Option<u16>,
) -> Option<DynamicImage> {
    let width = width.map(|v| v.to_string());
    let height = height.map(|v| v.to_string());
    let ext = tmp.path().extension().unwrap_or_default().to_string_lossy();
    let dyn_img = if ext == "svg" {
        let buf = fs::read(tmp).ok()?;
        svg_to_image(buf.as_slice(), width.as_deref(), height.as_deref()).ok()?
    } else if ImageFormat::from_extension(ext.as_ref()).is_some() {
        let buf = fs::read(tmp).ok()?;
        image::load_from_memory(&buf).ok()?
    } else {
        return None;
    };

    Some(dyn_img)
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

        let img = format!("{UNDERLINE_OFF}{}", self.img);

        let placeholder_line = self.placeholder.lines().nth(0).unwrap_or_default();
        for img_line in img.lines() {
            *text = text.replacen(placeholder_line, img_line, 1);
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
        if let Some(captures) = regex.captures(&image_node.url) {
            if let Some(base_url) = captures.get(1) {
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
    }

    for child in node.children() {
        extract_image_urls(child, urls);
    }
}
