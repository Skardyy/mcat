use anyhow::{Context, Result};
use futures::StreamExt;
use regex::Regex;
use reqwest::{Client, Response};
use scraper::Html;
use std::sync::OnceLock;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::{
    mcat_file::{McatFile, McatKind},
    prompter::MultiBar,
    prompter::get_rt,
};

static GITHUB_BLOB_URL: OnceLock<Regex> = OnceLock::new();
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();

#[derive(Default)]
pub struct MediaScrapeOptions {
    pub max_content_length: Option<u64>,
}

pub fn scrape_biggest_media(
    url: &str,
    options: &MediaScrapeOptions,
    bar: Option<&MultiBar>,
) -> Result<McatFile> {
    let client = HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default()
    });
    let re =
        GITHUB_BLOB_URL.get_or_init(|| Regex::new(r"^.*github\.com.*[\\\/]blob[\\\/].*$").unwrap());

    let url = if re.is_match(url) {
        url.replace("github.com", "raw.githubusercontent.com")
            .replace("/blob/", "/")
    } else {
        url.to_string()
    };

    get_rt().block_on(async {
        let response = get_response(client, &url, bar).await?;

        let mime = get_mime(&response);
        let format = mime.as_deref().and_then(|m| {
            if m == "application/octet-stream" {
                ext_from_url(&url).as_deref().and_then(McatKind::from_ext)
            } else {
                format_from_mime(m)
            }
        });

        match format {
            // html, try to scrape for something
            Some(McatKind::Html) => {
                let html = response.text().await?;
                let result = scrape_html(client, &url, &html, options, bar).await;
                match &result {
                    Ok(file) => {
                        info!(url = %url, kind = ?file.kind, size = file.bytes.len(), "scraped media")
                    }
                    Err(e) => warn!(url = %url, error = %e, "no media found on page"),
                }
                result
            },
            // known type, just download
            Some(fmt) => {
                let data = download(response, options, bar).await?;
                let mut file = McatFile::from_bytes(data, None, ext_from_url(&url), Some(url), true)?;
                if file.kind == McatKind::PreMarkdown {
                    file.kind = fmt;
                }
                info!(url = %file.id.as_deref().unwrap_or_default(), kind = ?file.kind, size = file.bytes.len(), "downloaded media");
                Ok(file)
            },
            None => anyhow::bail!("no media found at {}", url),
        }

    })
}

fn ext_from_url(url: &str) -> Option<String> {
    url.split('?')
        .next()
        .and_then(|u| u.split('/').next_back())
        .and_then(|f| f.split('.').next_back())
        .map(|e| e.to_string())
}

fn get_mime(response: &Response) -> Option<String> {
    response
        .headers()
        .get("Content-Type")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
}

fn get_content_length(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("content-length")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse().ok())
}

async fn download(
    response: Response,
    options: &MediaScrapeOptions,
    bar: Option<&MultiBar>,
) -> Result<Vec<u8>> {
    let content_length = get_content_length(&response);

    if let (Some(max), Some(len)) = (options.max_content_length, content_length) {
        anyhow::ensure!(len <= max, "content length {len} exceeds max {max}");
    }

    let handle = bar.map(|b| b.add(content_length, None));

    let mut data = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        data.extend_from_slice(&chunk);
        if let Some(max) = options.max_content_length {
            anyhow::ensure!(
                data.len() as u64 <= max,
                "download exceeded max content length"
            );
        }
        if let Some(ref h) = handle {
            h.set_position(data.len() as u64);
        }
    }

    if let Some(h) = handle {
        h.finish();
    }
    Ok(data)
}

async fn scrape_html(
    client: &Client,
    base_url: &str,
    html: &str,
    options: &MediaScrapeOptions,
    bar: Option<&MultiBar>,
) -> Result<McatFile> {
    let document = Html::parse_document(html);
    let base = reqwest::Url::parse(base_url)?;

    // collect candidates as (url, area)
    let mut candidates: Vec<(String, u64)> = Vec::new();

    for selector in &[
        "img[src]",
        "video[src]",
        "video source[src]",
        "object[type='image/svg+xml'][data]",
        "embed[type='image/svg+xml'][src]",
    ] {
        if let Ok(sel) = scraper::Selector::parse(selector) {
            for el in document.select(&sel) {
                let src = el.value().attr("src").or_else(|| el.value().attr("data"));
                let Some(src) = src else { continue };
                let Ok(url) = base.join(src) else { continue };

                let (w, h) = resolve_dimensions(&el);
                let area = w * h;

                candidates.push((url.to_string(), area));
            }
        }
    }

    anyhow::ensure!(!candidates.is_empty(), "no media found on page");

    let best_url = candidates
        .iter()
        .max_by_key(|(_, area)| *area)
        .map(|(url, _)| url.clone())
        .context("no valid media found on page")?;
    debug!(url = %best_url, "selected best candidate");

    let response = get_response(client, &best_url, bar).await?;
    let mime = get_mime(&response);
    let format = mime.as_deref().and_then(format_from_mime);
    let data = download(response, options, bar).await?;

    let mut file = McatFile::from_bytes(
        data,
        None,
        ext_from_url(&best_url),
        Some(base_url.to_owned()),
        true,
    )?;
    if let Some(fmt) = format {
        file.kind = fmt;
    }

    Ok(file)
}

async fn get_response(client: &Client, url: &str, bar: Option<&MultiBar>) -> Result<Response> {
    let handle = bar.map(|b| b.add(None, Some(&format!("Fetching {url}..."))));

    let request = client.get(url).send();
    tokio::pin!(request);

    let response = tokio::select! {
        result = &mut request => result?,
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
            if let Some(ref h) = handle {
                h.enable_steady_tick(Duration::from_millis(100));
            }
            request.await?
        }
    };

    if let Some(h) = handle {
        h.finish();
    }
    anyhow::ensure!(response.status().is_success(), response.status());

    Ok(response)
}

fn resolve_dimensions(el: &scraper::ElementRef) -> (u64, u64) {
    let (w_style, h_style) = extract_style_dims(el.value().attr("style"));
    let w_raw = el.value().attr("width").or(w_style.as_deref());
    let h_raw = el.value().attr("height").or(h_style.as_deref());

    let (w_raw, h_raw) = match (w_raw, h_raw) {
        (Some(w), Some(h)) => (w, h),
        _ => return (0, 0),
    };

    let parent_w = resolve_parent_dim(el, "width");
    let parent_h = resolve_parent_dim(el, "height");

    let w = dim_to_px(w_raw, parent_w);
    let h = dim_to_px(h_raw, parent_h);
    (w, h)
}

fn extract_style_dims(style: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(style) = style else {
        return (None, None);
    };
    let mut w = None;
    let mut h = None;
    for prop in style.split(';') {
        let prop = prop.trim();
        if let Some((key, val)) = prop.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            match key {
                "width" | "max-width" if w.is_none() => w = Some(val.to_string()),
                "height" | "max-height" if h.is_none() => h = Some(val.to_string()),
                _ => {}
            }
        }
    }
    (w, h)
}

fn get_element_dim(element: &scraper::node::Element, dim: &str) -> Option<String> {
    // check attribute first, then style
    if let Some(v) = element.attr(dim) {
        return Some(v.to_string());
    }
    let style = element.attr("style")?;
    for prop in style.split(';') {
        let prop = prop.trim();
        if let Some((key, val)) = prop.split_once(':') {
            let key = key.trim();
            let val = val.trim();
            if key == dim || key == format!("max-{dim}") {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn get_style_prop(element: &scraper::node::Element, prop: &str) -> Option<String> {
    let style = element.attr("style")?;
    for part in style.split(';') {
        let part = part.trim();
        if let Some((key, val)) = part.split_once(':')
            && key.trim() == prop
        {
            return Some(val.trim().to_string());
        }
    }
    None
}

fn resolve_parent_dim(el: &scraper::ElementRef, dim: &str) -> f64 {
    let default = if dim == "width" { 1920.0 } else { 1080.0 };
    let mut node = el.parent();
    while let Some(n) = node {
        if let Some(element) = n.value().as_element() {
            if let Some(v) = get_element_dim(element, dim)
                && let Some(px) = try_parse_absolute(&v)
            {
                return px;
            }
            // if no explicit dim, try to derive from aspect-ratio + the other dim
            if let Some(ar) = get_style_prop(element, "aspect-ratio")
                && let Ok(ar) = ar.parse::<f64>()
                && ar > 0.0
            {
                let other_dim = if dim == "height" { "width" } else { "height" };
                if let Some(v) = get_element_dim(element, other_dim)
                    && let Some(other_px) = try_parse_absolute(&v)
                {
                    // aspect-ratio = width / height
                    return if dim == "height" {
                        other_px / ar
                    } else {
                        other_px * ar
                    };
                }
            }
        }
        node = n.parent();
    }
    default
}

fn try_parse_absolute(value: &str) -> Option<f64> {
    let s = value.trim();
    let font_size: f64 = 14.0; // yeah just assumming..

    if s.ends_with('%') || s.ends_with("vw") || s.ends_with("vh") {
        return None; // relative, keep walking up
    }

    let px = if let Some(v) = s.strip_suffix("rem") {
        v.parse::<f64>().ok()? * font_size
    } else if let Some(v) = s.strip_suffix("em") {
        v.parse::<f64>().ok()? * font_size
    } else {
        s.strip_suffix("px").unwrap_or(s).parse::<f64>().ok()?
    };

    if px > 0.0 { Some(px) } else { None }
}

fn dim_to_px(value: &str, parent: f64) -> u64 {
    let s = value.trim();
    let font_size: f64 = 14.0;

    let px = if let Some(v) = s.strip_suffix('%') {
        v.parse::<f64>().unwrap_or(0.0) / 100.0 * parent
    } else if let Some(v) = s.strip_suffix("rem") {
        v.parse::<f64>().unwrap_or(0.0) * font_size
    } else if let Some(v) = s.strip_suffix("em") {
        v.parse::<f64>().unwrap_or(0.0) * font_size
    } else if let Some(v) = s.strip_suffix("vw") {
        v.parse::<f64>().unwrap_or(0.0) / 100.0 * 1920.0
    } else if let Some(v) = s.strip_suffix("vh") {
        v.parse::<f64>().unwrap_or(0.0) / 100.0 * 1080.0
    } else {
        s.strip_suffix("px")
            .unwrap_or(s)
            .parse::<f64>()
            .unwrap_or(0.0)
    };

    px.max(0.0) as u64
}

fn format_from_mime(mime: &str) -> Option<McatKind> {
    let mime = mime.split(';').next()?.trim();
    match mime {
        "image/gif" => Some(McatKind::Gif),
        "image/svg+xml" => Some(McatKind::Svg),
        "image/png"
        | "image/jpeg"
        | "image/webp"
        | "image/tiff"
        | "image/bmp"
        | "image/x-icon"
        | "image/vnd.microsoft.icon"
        | "image/avif"
        | "image/vnd.radiance"
        | "image/x-exr"
        | "image/qoi"
        | "image/x-portable-anymap"
        | "image/farbfeld"
        | "image/vnd.ms-dds" => Some(McatKind::Image),
        "video/mp4" | "video/webm" | "video/matroska" | "video/quicktime" | "video/avi"
        | "video/x-msvideo" | "video/x-ms-wmv" | "video/x-flv" | "video/mpeg" | "video/ogg"
        | "video/3gpp" | "video/x-m4v" => Some(McatKind::Video),
        "application/pdf" => Some(McatKind::Pdf),
        "text/x-tex" => Some(McatKind::Tex),
        "text/html" => Some(McatKind::Html),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        | "application/vnd.ms-excel"
        | "application/vnd.oasis.opendocument.text"
        | "application/vnd.oasis.opendocument.presentation"
        | "application/vnd.oasis.opendocument.spreadsheet"
        | "application/zip"
        | "application/x-tar"
        | "application/gzip"
        | "application/x-xz"
        | "application/json"
        | "application/x-yaml" => Some(McatKind::PreMarkdown),
        _ if mime.starts_with("text/") => Some(McatKind::PreMarkdown),
        _ => None,
    }
}
