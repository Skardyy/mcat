use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use hayro::{RenderCache, hayro_syntax::Pdf};
use image::{DynamicImage, GenericImage};
use infer::{
    app::is_exe,
    archive::is_pdf,
    image::{is_gif, is_jxl},
    is_video as infer_is_video,
};
use lzma_rust2::XzReader;
use markdownify::MarkdownifyInput;
use pelite::PeFile;
use rasteroid::{
    RasterEncoder,
    image_extended::InlineImage,
    term_misc::{SizeDirection, Wininfo},
};
use reqwest::Url;
use resvg::{
    tiny_skia,
    usvg::{self, Options, Tree},
};
use std::{
    fs::{self},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};
use tempfile::NamedTempFile;

use tracing::{debug, info};

use crate::{
    cdp::ChromeHeadless,
    config::{McatConfig, Theme},
    fetch_manager, markdown_viewer,
    prompter::RUNTIME,
};

#[derive(Clone, Default, Debug, PartialEq)]
pub enum McatKind {
    #[default]
    PreMarkdown, // is the most common ones, just something that is passed into markdownify
    Markdown,
    Html,

    Video,
    Gif, // have different logic on iterm

    Image,
    Mermaid,
    Svg, // svg is handled manually, since its not supported by the image crate
    JpegXL,

    Url,
    Exe,
    Lnk,

    // has some manual handling
    Pdf,
    Tex,
    Typst,
}

impl McatKind {
    pub fn detect(bytes: Option<&[u8]>, ext: Option<&str>) -> Option<Self> {
        let ext = ext.unwrap_or("").to_lowercase();
        let ext = ext.as_str();

        // doesn't go well into our map
        if ext == "mmd"
            && let Some(b) = bytes
            && is_mermaid(b)
        {
            return Some(Self::Mermaid);
        }

        let handlers: &[(Checker, &[&str], McatKind)] = &[
            (is_pdf, &["pdf"], Self::Pdf),
            (is_gif, &["gif"], Self::Gif), // gif most be before video check.
            (
                |b| image::guess_format(b).is_ok(),
                &[
                    "png", "jpg", "jpeg", "webp", "tiff", "tif", "bmp", "ico", "avif", "exr",
                    "qoi", "hdr", "dds", "farbfeld", "pnm", "pbm", "pgm", "ppm", "pam", "tga",
                    "pcx",
                ],
                Self::Image,
            ),
            (
                is_video,
                &[
                    "mp4", "webm", "mkv", "mov", "avi", "wmv", "flv", "mpeg", "ogg", "m4v",
                ],
                Self::Video,
            ),
            (is_exe, &["exe"], Self::Exe),
            (is_jxl, &["jxl"], Self::JpegXL),
            (is_svg, &["svg"], Self::Svg),
            (|_| false, &["mermaid"], Self::Mermaid),
            (|_| false, &["html", "htm"], Self::Html),
            (
                // markdown exts need to be synced with markdownfiy.
                |_| false,
                &[
                    "md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdx", "mdc", "qmd", "rmd",
                    "mmd",
                ],
                Self::Markdown,
            ),
            (|_| false, &["tex"], Self::Tex),
            (|_| false, &["typ"], Self::Typst),
            (|_| false, &["lnk"], Self::Lnk),
            (|_| false, &["url"], Self::Url),
        ];

        // 2 pass to make content beat ext.
        handlers
            .iter()
            .find(|(check, _, _)| bytes.map(check).unwrap_or(false))
            .or_else(|| handlers.iter().find(|(_, exts, _)| exts.contains(&ext)))
            .map(|(_, _, kind)| kind.clone())
    }

    pub fn from_ext(ext: &str) -> Option<Self> {
        Self::detect(None, Some(ext))
    }
}

type Checker = fn(&[u8]) -> bool;

pub struct McatFile {
    pub bytes: Vec<u8>,

    pub kind: McatKind,

    pub path: Option<PathBuf>,
    pub ext: Option<String>,
    pub id: Option<String>,
}

impl McatFile {
    pub fn from_path(path: impl AsRef<Path>, decompress: bool) -> Result<Self> {
        let path = path.as_ref();
        let pathbuf = path.to_path_buf();
        let ext = path.extension().map(|v| v.to_string_lossy().to_string());
        let bytes = fs::read(path)?;

        let s = Self::from_bytes(bytes, Some(pathbuf), ext, None, decompress)?;
        info!(path = %path.display(), kind = ?s.kind, "loaded file");
        Ok(s)
    }

    pub fn from_image(img: DynamicImage, path: Option<PathBuf>, id: Option<String>) -> Self {
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Pnm)
            .expect("PAM encode should never fail");
        Self {
            bytes: buf,
            kind: McatKind::Image,
            path,
            ext: None,
            id,
        }
    }

    pub fn from_bytes(
        bytes: Vec<u8>,
        path: Option<PathBuf>,
        ext: Option<String>,
        id: Option<String>,
        decompress: bool,
    ) -> Result<Self> {
        let bytes: Vec<u8> = if decompress && infer::archive::is_gz(&bytes) {
            let mut decoder = GzDecoder::new(bytes.as_slice());
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            out
        } else if decompress && infer::archive::is_xz(&bytes) {
            let mut decoder = XzReader::new(bytes.as_slice(), true);
            let mut out = Vec::new();
            decoder.read_to_end(&mut out)?;
            out
        } else {
            bytes
        };
        let kind = McatKind::detect(Some(&bytes), ext.as_deref()).unwrap_or_default();

        Ok(Self {
            bytes,
            kind,
            path,
            ext,
            id,
        })
    }

    pub fn to_html(&self, theme_for_style: Option<Theme>, inline_images: bool) -> Result<String> {
        let md = self.to_markdown_input(inline_images)?.convert()?;
        let should_style = theme_for_style.is_some();
        let html =
            markdown_viewer::md_to_html(&md, &theme_for_style.unwrap_or_default(), should_style);

        Ok(html)
    }

    pub fn to_image(&self, config: &McatConfig, pad: bool, resize: bool) -> Result<DynamicImage> {
        debug!(kind = ?self.kind, pad, resize, "converting to image");
        let wininfo = config
            .wininfo
            .as_ref()
            .context("this is likely a bug, tried to convert to image and wininfo is None")?;
        let width: Option<&str> = Some(&config.img_width);
        let height: Option<&str> = Some(&config.img_height);
        let is_ascii = config
            .encoder
            .map(|v| v == RasterEncoder::Ascii)
            .unwrap_or(false);

        let img: DynamicImage = match self.kind {
            McatKind::PreMarkdown | McatKind::Markdown => {
                let theme = config.theme.clone();
                let html = self.to_html(Some(theme), config.inline_images_in_md)?;
                let file = McatFile::from_bytes(
                    html.into_bytes(),
                    self.path.clone(),
                    Some("html".to_owned()),
                    self.id.clone(),
                    true,
                )?;
                html_to_image(&file)?
            }
            McatKind::Mermaid => {
                let theme = config.theme.to_custom().to_mermaid_theme();
                let mut opts = mermaid_rs_renderer::RenderOptions::modern();
                opts.theme = theme;
                let svg =
                    mermaid_rs_renderer::render_with_options(str::from_utf8(&self.bytes)?, opts)?;
                svg_to_image(
                    svg.as_bytes(),
                    wininfo,
                    width,
                    height,
                    is_ascii,
                    pad,
                    resize,
                )?
            }
            McatKind::Html => html_to_image(self)?,
            McatKind::Video => anyhow::bail!(
                "Cannot turn video format to image, this is most likely a bug and should not reach here."
            ),
            McatKind::Gif => image::load_from_memory(&self.bytes)?,
            McatKind::Image => image::load_from_memory(&self.bytes)?,
            McatKind::Svg => {
                return svg_to_image(&self.bytes, wininfo, width, height, is_ascii, pad, resize);
            }
            McatKind::Url => url_to_image(&self.bytes)?,
            McatKind::Exe => exe_to_image(&self.bytes)?,
            McatKind::Lnk => lnk_to_image(&self.bytes)?,
            McatKind::Pdf => pdf_to_image(&self.bytes, 0)?,
            McatKind::Tex => return self.tex_to_pdf()?.to_image(config, pad, resize),
            McatKind::Typst => return self.typst_to_pdf()?.to_image(config, pad, resize),
            McatKind::JpegXL => {
                let decoder =
                    jxl_oxide::integration::JxlDecoder::new(Cursor::new(self.bytes.clone()))?;
                image::DynamicImage::from_decoder(decoder)?
            }
        };

        if resize {
            Ok(img.resize_plus(wininfo, width, height, is_ascii, pad)?)
        } else {
            Ok(img)
        }
    }

    pub fn to_markdown_input(&self, inline_images: bool) -> Result<MarkdownifyInput> {
        let mut input = MarkdownifyInput::from_bytes(
            self.bytes.clone(),
            self.path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        )?;
        input.allow_inline_images = inline_images;
        input.path = self.path.clone();
        input.ext = self.ext.clone();
        input.id = self.id.clone().unwrap_or_default();

        Ok(input)
    }

    pub fn to_album(&self, config: &McatConfig) -> Result<Vec<DynamicImage>> {
        match self.kind {
            McatKind::PreMarkdown
            | McatKind::Markdown
            | McatKind::Html
            | McatKind::Gif
            | McatKind::Image
            | McatKind::Svg
            | McatKind::Url
            | McatKind::Exe
            | McatKind::JpegXL
            | McatKind::Mermaid
            | McatKind::Lnk => {
                let img = self.to_image(config, false, false)?;
                Ok(vec![img])
            }
            McatKind::Pdf => pdf_to_album(&self.bytes),
            McatKind::Tex => self.tex_to_pdf()?.to_album(config),
            McatKind::Typst => self.typst_to_pdf()?.to_album(config),
            McatKind::Video => anyhow::bail!("interactive mode isn't supported with videos"),
        }
    }

    fn tex_to_pdf(&self) -> Result<McatFile> {
        let _temp_input;
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                _temp_input = NamedTempFile::with_suffix(".tex")?;
                fs::write(_temp_input.path(), &self.bytes)?;
                _temp_input.path().to_path_buf()
            }
        };

        let temp_dir = tempfile::tempdir()?;
        let name = path.file_stem().context("no file stem")?.to_string_lossy();
        let temp_pdf = temp_dir.path().join(format!("{name}.pdf"));

        // try tectonic first
        let mut last_stderr = String::new();
        let compiled = match Command::new("tectonic")
            .args([
                "--outdir",
                &temp_dir.path().to_string_lossy(),
                &path.to_string_lossy(),
            ])
            .output()
        {
            Ok(o) if o.status.success() && temp_pdf.exists() => true,
            Ok(o) => {
                last_stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                false
            }
            Err(_) => false,
        };

        // fallback to pdflatex
        let compiled = compiled
            || match Command::new("pdflatex")
                .args([
                    &format!("-output-directory={}", temp_dir.path().to_string_lossy()),
                    "-interaction=nonstopmode",
                    &path.to_string_lossy(),
                ])
                .output()
            {
                Ok(o) if o.status.success() && temp_pdf.exists() => true,
                Ok(o) => {
                    last_stderr = String::from_utf8_lossy(&o.stderr).into_owned();
                    false
                }
                Err(_) => false,
            };

        if !compiled {
            if last_stderr.is_empty() {
                anyhow::bail!("failed to compile tex to pdf. install tectonic or pdflatex");
            } else {
                anyhow::bail!("failed to compile tex to pdf:\n{last_stderr}");
            }
        }

        let bytes = fs::read(&temp_pdf)?;
        Ok(McatFile {
            bytes,
            kind: McatKind::Pdf,
            path: self.path.clone(),
            ext: Some("pdf".to_owned()),
            id: self.id.clone(),
        })
    }

    fn typst_to_pdf(&self) -> Result<McatFile> {
        let _temp_input;
        let path = match &self.path {
            Some(p) => p.clone(),
            None => {
                _temp_input = NamedTempFile::with_suffix(".typ")?;
                fs::write(_temp_input.path(), &self.bytes)?;
                _temp_input.path().to_path_buf()
            }
        };

        let temp_pdf = NamedTempFile::with_suffix(".pdf")?;
        let output_path = temp_pdf.path().to_path_buf();

        let output = Command::new("typst")
            .args([
                "compile",
                "--format",
                "pdf",
                &path.to_string_lossy(),
                &output_path.to_string_lossy(),
            ])
            .output();

        match output {
            Ok(o) if o.status.success() && output_path.exists() => {}
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr);
                if stderr.is_empty() {
                    anyhow::bail!("typst failed to compile");
                } else {
                    anyhow::bail!("typst failed to compile:\n{stderr}");
                }
            }
            Err(_) => anyhow::bail!("failed to run typst. is it installed?"),
        }

        let bytes = fs::read(&output_path)?;
        Ok(McatFile {
            bytes,
            kind: McatKind::Pdf,
            path: self.path.clone(),
            ext: Some("pdf".to_owned()),
            id: self.id.clone(),
        })
    }

    pub fn to_frames(
        &self,
        enc: RasterEncoder,
    ) -> Result<(Box<dyn Iterator<Item = rasteroid::VideoFrame>>, u32, u32)> {
        self.spawn_frames(enc, None)
    }

    fn spawn_frames(
        &self,
        enc: RasterEncoder,
        map: Option<u32>,
    ) -> Result<(Box<dyn Iterator<Item = rasteroid::VideoFrame>>, u32, u32)> {
        let mut command = fetch_manager::get_ffmpeg().context(
            "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`",
        )?;

        command.hwaccel("auto");
        if enc != RasterEncoder::Kitty {
            // kitty doesn't need loop, rasteroid impl
            command.arg("-stream_loop").arg("-1");
        }
        match &self.path {
            Some(path) => command.input(path.to_string_lossy()),
            None => command.input("pipe:0"),
        };
        command.no_audio();
        if let Some(idx) = map {
            command.arg("-map").arg(format!("0:{idx}"));
        }
        command.rawvideo();

        let mut child = command.spawn()?;

        if self.path.is_none() {
            let stdin = child.take_stdin().context("failed to get ffmpeg stdin")?;
            let bytes = self.bytes.clone();
            std::thread::spawn(move || {
                let mut stdin = stdin;
                let _ = stdin.write_all(&bytes);
            });
        }

        let mut iter = child.iter()?;
        let meta = iter.collect_metadata()?;

        let out = meta
            .output_streams
            .iter()
            .find_map(|s| s.video_data())
            .context("no video stream in ffmpeg output")?;

        // a 1-fps pick is a still image, never a real track. only then look for a
        // better candidate: best resolution among the tracks that aren't stills.
        if map.is_none() && out.fps <= 1.0 {
            let better = meta
                .input_streams
                .iter()
                .filter_map(|s| s.video_data().map(|v| (s.stream_index, v)))
                .filter(|(_, v)| v.fps > 1.0)
                .max_by_key(|(_, v)| v.width * v.height);

            if let Some((idx, v)) = better {
                debug!(
                    idx,
                    fps = v.fps,
                    w = v.width,
                    h = v.height,
                    "still image, remapping"
                );
                let _ = child.kill();
                return self.spawn_frames(enc, Some(idx));
            }
        }

        let (width, height) = (out.width, out.height);
        debug!(width, height, fps = out.fps, "reading frames");

        let frames = iter.filter_frames().map(|f| {
            let rgb = image::RgbImage::from_raw(f.width, f.height, f.data).unwrap_or_default();
            (image::DynamicImage::ImageRgb8(rgb), f.timestamp)
        });

        Ok((Box::new(frames), width, height))
    }
}

// converting methods.

pub fn svg_to_image(
    bytes: &[u8],
    wininfo: &Wininfo,
    width: Option<&str>,
    height: Option<&str>,
    is_ascii: bool,
    pad: bool,
    needs_resize: bool,
) -> Result<DynamicImage> {
    let mut opt = Options::default();

    // allowing text
    let mut fontdb = fontdb::Database::new();
    fontdb.load_system_fonts();
    opt.fontdb = std::sync::Arc::new(fontdb);
    opt.text_rendering = usvg::TextRendering::OptimizeLegibility;

    let tree = Tree::from_data(bytes, &opt)?;
    let pixmap_size = tree.size();
    let src_width = pixmap_size.width();
    let src_height = pixmap_size.height();

    let width = match width {
        Some(w) if needs_resize => match is_ascii {
            true => wininfo.dim_to_cells(w, SizeDirection::Width)?,
            false => wininfo.dim_to_px(w, SizeDirection::Width)?,
        },
        _ => (src_width as u32).min(wininfo.spx_width as u32),
    };
    let height = match height {
        Some(h) if needs_resize => match is_ascii {
            true => wininfo.dim_to_cells(h, SizeDirection::Height)? * 2,
            false => wininfo.dim_to_px(h, SizeDirection::Height)?,
        },
        _ => (src_height as u32).min(wininfo.spx_height as u32),
    };
    let (target_width, target_height) =
        rasteroid::image_extended::calc_fit(src_width as u32, src_height as u32, width, height);
    let scale_x = target_width as f32 / src_width;
    let scale_y = target_height as f32 / src_height;
    let scale = scale_x.min(scale_y);

    let mut pixmap = tiny_skia::Pixmap::new(target_width, target_height)
        .context("Failed to create pixmap for svg")?;
    let transform = tiny_skia::Transform::from_scale(scale, scale);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let img = image::RgbaImage::from_raw(target_width, target_height, pixmap.data().to_vec())
        .context("Failed to create image buffer from svg pixmap")?;
    let dyn_img = DynamicImage::ImageRgba8(img);

    if pad && (target_width != width || target_height != height) {
        let mut new_img = DynamicImage::new_rgba8(width, height);
        let x_offset = if width == target_width {
            0
        } else {
            (width - target_width) / 2
        };
        let y_offset = if height == target_height {
            0
        } else {
            (height - target_height) / 2
        };
        new_img.copy_from(&dyn_img, x_offset, y_offset)?;
        return Ok(new_img);
    }

    Ok(dyn_img)
}

fn render_pdf_page<'a>(
    pdf: &'a Pdf,
    page_index: usize,
    cache: Option<&'a RenderCache<'a>>,
) -> Result<DynamicImage> {
    let pages = pdf.pages();
    let page = pages
        .get(page_index)
        .context("PDF page index out of bounds")?;

    let render_settings = hayro::RenderSettings {
        bg_color: hayro::vello_cpu::color::AlphaColor::WHITE,
        ..Default::default()
    };
    let cache = match cache {
        Some(v) => v,
        None => &RenderCache::new(),
    };
    let pixmap = hayro::render(
        page,
        cache,
        &hayro::hayro_interpret::InterpreterSettings::default(),
        &render_settings,
    );

    let width = pixmap.width() as u32;
    let height = pixmap.height() as u32;
    let rgba: Vec<u8> = pixmap
        .data()
        .iter()
        .flat_map(|p| {
            let a = p.a;
            if a == 0 {
                [0, 0, 0, 0]
            } else {
                // unpremultiply
                let r = ((p.r as u16 * 255) / a as u16) as u8;
                let g = ((p.g as u16 * 255) / a as u16) as u8;
                let b = ((p.b as u16 * 255) / a as u16) as u8;
                [r, g, b, a]
            }
        })
        .collect();

    let img = image::RgbaImage::from_raw(width, height, rgba)
        .context("failed to create image from PDF pixmap")?;
    Ok(DynamicImage::ImageRgba8(img))
}

fn pdf_to_image(bytes: &[u8], page_index: usize) -> Result<DynamicImage> {
    let pdf = Pdf::new(Arc::new(bytes.to_vec()))
        .map_err(|e| anyhow::anyhow!("failed to load PDF: {e:?}"))?;
    render_pdf_page(&pdf, page_index, None)
}

fn pdf_to_album(bytes: &[u8]) -> Result<Vec<DynamicImage>> {
    let pdf = Pdf::new(Arc::new(bytes.to_vec()))
        .map_err(|e| anyhow::anyhow!("failed to load PDF: {e:?}"))?;
    let page_count = pdf.pages().len();
    let cache = RenderCache::new();
    (0..page_count)
        .map(|i| render_pdf_page(&pdf, i, Some(&cache)))
        .collect()
}

pub fn exe_to_image(bytes: &[u8]) -> Result<DynamicImage> {
    let pe = PeFile::from_bytes(bytes)?;
    let resources = pe.resources()?;

    let (_name, icon_group) = resources
        .icons()
        .next()
        .context("no icons found in exe")??;

    let best_entry = icon_group
        .entries()
        .iter()
        .max_by_key(|e| {
            let width = if e.bWidth == 0 { 256 } else { e.bWidth as u32 };
            let height = if e.bHeight == 0 {
                256
            } else {
                e.bHeight as u32
            };
            (width * height, e.wBitCount as u32)
        })
        .context("no icon entries found")?;

    let icon_data = icon_group.image(best_entry.nId)?;

    let mut ico_file = Vec::new();
    // ICO header
    ico_file.extend_from_slice(&[0, 0, 1, 0, 1, 0]);
    ico_file.push(best_entry.bWidth);
    ico_file.push(best_entry.bHeight);
    ico_file.push(best_entry.bColorCount);
    ico_file.push(0);
    ico_file.extend_from_slice(&best_entry.wPlanes.to_le_bytes());
    ico_file.extend_from_slice(&best_entry.wBitCount.to_le_bytes());
    ico_file.extend_from_slice(&(icon_data.len() as u32).to_le_bytes());
    ico_file.extend_from_slice(&22u32.to_le_bytes());
    ico_file.extend_from_slice(icon_data);

    Ok(image::load_from_memory(&ico_file)?)
}

pub fn lnk_to_image(bytes: &[u8]) -> Result<DynamicImage> {
    // Rather lazy tbh, just checking for target and not to icon if set.
    // Most will likely just target an exe which we can take the icon from.

    let link_flags = u32::from_le_bytes([bytes[0x14], bytes[0x15], bytes[0x16], bytes[0x17]]);
    anyhow::ensure!(link_flags & 0x02 != 0, "lnk has no link info");

    let mut offset = 0x4C;
    if link_flags & 0x01 != 0 {
        let id_list_size = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2 + id_list_size as usize;
    }

    let local_base_path_offset = u32::from_le_bytes([
        bytes[offset + 0x10],
        bytes[offset + 0x11],
        bytes[offset + 0x12],
        bytes[offset + 0x13],
    ]) as usize;

    anyhow::ensure!(local_base_path_offset != 0, "lnk has no local base path");

    let path_offset = offset + local_base_path_offset;
    let end = bytes[path_offset..]
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(260);

    let target = String::from_utf8(bytes[path_offset..path_offset + end].to_vec())?;
    let target = Path::new(&target);

    anyhow::ensure!(target.exists(), "lnk target does not exist");
    anyhow::ensure!(
        target
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .as_deref()
            == Some("exe"),
        "lnk target is not an exe"
    );

    let exe_bytes = fs::read(target)?;
    exe_to_image(&exe_bytes)
}

pub fn url_to_image(bytes: &[u8]) -> Result<DynamicImage> {
    let content = std::str::from_utf8(bytes)?;
    let icon_path = content
        .lines()
        .find_map(|line| line.strip_prefix("IconFile="))
        .map(|s| s.trim())
        .context("no IconFile entry in url file")?;

    let icon_path = Path::new(icon_path);
    anyhow::ensure!(icon_path.exists(), "icon path does not exist");

    let icon_file = McatFile::from_path(icon_path, true)?;
    match icon_file.kind {
        McatKind::Image => Ok(image::load_from_memory(&icon_file.bytes)?),
        McatKind::Exe => exe_to_image(&icon_file.bytes),
        _ => anyhow::bail!("unsupported icon format: {:?}", icon_file.kind),
    }
}

pub fn html_to_image(source: &McatFile) -> Result<DynamicImage> {
    let (html, _, _) = encoding_rs::UTF_8.decode(&source.bytes);
    let mut tmp_file = NamedTempFile::with_suffix(".html")?;
    tmp_file.write_all(html.as_bytes())?;
    let url = Url::from_file_path(tmp_file.path())
        .map_err(|_| anyhow::anyhow!("failed to create url for chromium"))?;

    let img_bytes: Vec<u8> = RUNTIME.block_on(async {
        let browser = ChromeHeadless::new(url.as_str()).await?;
        browser.capture_screenshot().await
    })?;

    Ok(image::load_from_memory(&img_bytes)?)
}

fn is_svg(b: &[u8]) -> bool {
    let head = &b[..b.len().min(2048)];
    let s = String::from_utf8_lossy(head);
    let mut rest = s.as_ref();

    loop {
        rest = rest.trim_start();
        let Some(after_lt) = rest.strip_prefix('<') else {
            return false;
        };
        let Some(gt) = after_lt.find('>') else {
            return false;
        };
        let tag = &after_lt[..gt];

        if tag.starts_with('?') || tag.starts_with('!') {
            rest = &after_lt[gt + 1..];
            continue;
        }

        return tag.starts_with("svg")
            && matches!(
                tag.as_bytes().get(3),
                None | Some(b' ' | b'\t' | b'\n' | b'\r' | b'/')
            );
    }
}

#[rustfmt::skip]
fn is_mermaid(b: &[u8]) -> bool {
    const KEYWORDS: &[&str] = &[
        "graph", "flowchart", "sequenceDiagram", "classDiagram",
        "stateDiagram-v2", "stateDiagram", "erDiagram", "journey",
        "gantt", "pie", "gitGraph", "mindmap", "timeline",
        "quadrantChart", "requirementDiagram", "C4Context",
        "sankey-beta", "xychart-beta", "block-beta",
    ];

    let head = &b[..b.len().min(2048)];
    let s = String::from_utf8_lossy(head);
    let mut lines = s.lines().peekable();

    // skip YAML frontmatter
    if lines.peek().map(|l| l.trim()) == Some("---") {
        lines.next();
        for line in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
        }
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }

        // first token
        let first = line.split_whitespace().next().unwrap_or("");
        let first = first.split('(').next().unwrap_or(first);
        return KEYWORDS.contains(&first);
    }
    false
}

fn is_video(buf: &[u8]) -> bool {
    if infer_is_video(buf) {
        return true;
    }

    // animated avif
    if buf.len() < 16 || &buf[4..8] != b"ftyp" {
        return false;
    }
    let Ok(size) = buf[0..4].try_into().map(u32::from_be_bytes) else {
        return false;
    };
    let end = (size as usize).clamp(16, buf.len()) & !3;
    buf[8..end]
        .chunks_exact(4)
        .any(|b| b == b"avis" || b == b"msf1")
}
