use anyhow::{Context, Ok, Result};
use file_format::{FileFormat, Kind};
use image::DynamicImage;
use markdownify::MarkdownifyInput;
use pelite::PeFile;
use rasteroid::{Frame, image_extended::InlineImage, term_misc::Wininfo};
use reqwest::Url;
use resvg::{
    tiny_skia,
    usvg::{self, Options, Tree},
};
use std::{
    fs::{self},
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

use crate::{cdp::ChromeHeadless, fetch_manager};

pub struct McatFile {
    pub bytes: Vec<u8>,
    pub path: Option<PathBuf>,
    pub format: FileFormat,
    pub kind: Kind,
}

impl McatFile {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let bytes = fs::read(&path)?;
        let format = FileFormat::from_bytes(&bytes);

        Ok(Self {
            bytes,
            path: Some(path),
            format,
            kind: format.kind(),
        })
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        let format = FileFormat::from_bytes(&bytes);

        Self {
            bytes,
            path: None,
            format,
            kind: format.kind(),
        }
    }

    pub fn to_image(
        &self,
        wininfo: &Wininfo,
        width: Option<&str>,
        height: Option<&str>,
        is_ascii: bool,
    ) -> Result<(Vec<u8>, u32, u32)> {
        let img = match self.format {
            FileFormat::ScalableVectorGraphics => {
                return svg_to_image(&self.bytes, wininfo, width, height);
            }
            FileFormat::PortableExecutable => exe_to_image(&self.bytes)?,
            FileFormat::WindowsShortcut => lnk_to_image(&self.bytes)?,
            FileFormat::HypertextMarkupLanguage => html_to_image(self)?,
            _ if image::guess_format(&self.bytes).is_ok() => image::load_from_memory(&self.bytes)?,
            _ => match self.extension().as_deref() {
                Some("url") => url_to_image(&self.bytes)?,
                _ => return Err(anyhow::anyhow!("unsupported format: {:?}", self.format)),
            },
        };

        let (img, width, height) = img.resize_plus(wininfo, width, height, is_ascii, false)?;

        Ok((img, width, height))
    }

    pub fn to_markdown_input(&self) -> MarkdownifyInput {
        let mut input = MarkdownifyInput::from_bytes(
            self.bytes.clone(),
            self.path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        input.path = self.path.clone();
        input.ext = self
            .path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase());
        input
    }

    pub fn to_frames(&self) -> Result<Box<dyn Iterator<Item = VideoFrames>>> {
        let input = if let Some(path) = &self.path {
            path.to_string_lossy().to_string()
        } else {
            let mut tmp_file = NamedTempFile::with_suffix(".mp4")?;
            tmp_file.write_all(&self.bytes)?;
            tmp_file.path().to_string_lossy().to_string()
        };

        let mut command = fetch_manager::get_ffmpeg().context(
            "ffmpeg isn't installed. either install it manually, or call `mcat --fetch-ffmpeg`",
        )?;

        command.hwaccel("auto").input(&input).rawvideo();
        let mut child = command.spawn()?;
        let frames = child.iter()?.filter_frames().map(|f| VideoFrames {
            timestamp: f.timestamp,
            img: f.data,
            width: f.width as u16,
            height: f.height as u16,
        });

        Ok(Box::new(frames))
    }

    fn extension(&self) -> Option<String> {
        self.path
            .as_ref()?
            .extension()?
            .to_str()
            .map(|e| e.to_lowercase())
    }
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

// converting methods.

pub fn svg_to_image(
    bytes: &[u8],
    wininfo: &Wininfo,
    width: Option<&str>,
    height: Option<&str>,
) -> Result<(Vec<u8>, u32, u32)> {
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
        Some(w) => wininfo.dim_to_px(w, rasteroid::term_misc::SizeDirection::Width)?,
        None => src_width as u32,
    };
    let height = match height {
        Some(h) => wininfo.dim_to_px(h, rasteroid::term_misc::SizeDirection::Height)?,
        None => src_height as u32,
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

    Ok((pixmap.data().to_vec(), target_width, target_height))
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

    let icon_file = McatFile::from_path(icon_path)?;
    match icon_file.format {
        FileFormat::PortableExecutable => exe_to_image(&icon_file.bytes),
        FileFormat::WindowsIcon => Ok(image::load_from_memory(&icon_file.bytes)?),
        _ => anyhow::bail!("unsupported icon format: {:?}", icon_file.format),
    }
}

pub fn html_to_image(source: &McatFile) -> Result<DynamicImage> {
    let url = if let Some(path) = &source.path {
        Url::from_file_path(path)
            .map_err(|_| anyhow::anyhow!("failed to create url for chromium"))?
    } else {
        let html = std::str::from_utf8(&source.bytes)?;
        let mut tmp_file = NamedTempFile::with_suffix(".html")?;
        tmp_file.write_all(html.as_bytes())?;
        Url::from_file_path(tmp_file.path())
            .map_err(|_| anyhow::anyhow!("failed to create url for chromium"))?
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let img_bytes: Vec<u8> = rt.block_on(async {
        let browser = ChromeHeadless::new(url.as_str()).await?;
        browser.capture_screenshot().await
    })?;

    Ok(image::load_from_memory(&img_bytes)?)
}
