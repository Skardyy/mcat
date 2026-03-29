use anyhow::{Context, Result};
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::{Client, Response};
use scraper::Html;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::runtime::{Builder, Runtime};

use tracing::{debug, info, warn};

use crate::mcat_file::{McatFile, McatKind};

static GITHUB_BLOB_URL: OnceLock<Regex> = OnceLock::new();
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
// TODO: remove that
static RUNTIME: OnceLock<Runtime> = OnceLock::new();
static GLOBAL_MULTI_PROGRESS: OnceLock<MultiProgress> = OnceLock::new();

fn get_multi_progress() -> &'static MultiProgress {
    GLOBAL_MULTI_PROGRESS.get_or_init(MultiProgress::new)
}

#[derive(Default)]
pub struct MediaScrapeOptions {
    pub silent: bool,
    pub max_content_length: Option<u64>,
}

pub fn scrape_biggest_media(url: &str, options: &MediaScrapeOptions) -> Result<McatFile> {
    let client = HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default()
    });
    let rt = RUNTIME.get_or_init(|| Builder::new_current_thread().enable_all().build().unwrap());
    let re =
        GITHUB_BLOB_URL.get_or_init(|| Regex::new(r"^.*github\.com.*[\\\/]blob[\\\/].*$").unwrap());

    let url = if re.is_match(url) && !url.contains("?raw=true") {
        format!("{url}?raw=true")
    } else {
        url.to_string()
    };

    rt.block_on(async {
        let response = get_response(client, &url, options).await?;

        let mime = get_mime(&response);
        let format = mime.as_deref().and_then(format_from_mime);
        // if we know its not html, download directly
        if let Some(fmt) = format
            && fmt != McatKind::Html
        {
            let data = download(response, options).await?;
            let mut file = McatFile::from_bytes(data, None)?;
            file.kind = fmt;
            info!(url = %url, kind = ?file.kind, size = file.bytes.len(), "scraped media");
            return Ok(file);
        }

        // otherwise scrape html for biggest media
        let html = response.text().await?;
        let result = scrape_html(client, &url, &html, options).await;
        match &result {
            Ok(file) => {
                info!(url = %url, kind = ?file.kind, size = file.bytes.len(), "scraped media")
            }
            Err(e) => warn!(url = %url, error = %e, "no media found on page"),
        }
        result
    })
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

async fn download(response: Response, options: &MediaScrapeOptions) -> Result<Vec<u8>> {
    let content_length = get_content_length(&response);

    if let (Some(max), Some(len)) = (options.max_content_length, content_length) {
        anyhow::ensure!(len <= max, "content length {len} exceeds max {max}");
    }

    let pb = if !options.silent && content_length.map(|l| l > 500_000).unwrap_or(false) {
        let pb = get_multi_progress().add(ProgressBar::new(content_length.unwrap()));
        pb.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{bar:50.blue/white}] {bytes}/{total_bytes} ({percent}%)",
                )?
                .progress_chars("█▓▒░"),
        );
        Some(pb)
    } else {
        None
    };

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
        if let Some(ref pb) = pb {
            pb.set_position(data.len() as u64);
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }
    Ok(data)
}

async fn scrape_html(
    client: &Client,
    base_url: &str,
    html: &str,
    options: &MediaScrapeOptions,
) -> Result<McatFile> {
    let document = Html::parse_document(html);
    let base = reqwest::Url::parse(base_url)?;

    // collect all candidate media urls
    let mut candidates: Vec<String> = Vec::new();

    for selector in &[
        "img[src]",
        "video[src]",
        "video source[src]",
        "object[type='image/svg+xml'][data]",
        "embed[type='image/svg+xml'][src]",
    ] {
        if let Ok(sel) = scraper::Selector::parse(selector) {
            for el in document.select(&sel) {
                if let Some(src) = el.value().attr("src").or_else(|| el.value().attr("data"))
                    && let Ok(url) = base.join(src)
                {
                    candidates.push(url.to_string());
                }
            }
        }
    }

    anyhow::ensure!(!candidates.is_empty(), "no media found on page");

    // download all candidates, keep biggest
    let mut biggest: Option<McatFile> = None;
    for url in candidates {
        let Ok(response) = get_response(client, &url, options).await else {
            continue;
        };
        let mime = get_mime(&response);
        let format = mime.as_deref().and_then(format_from_mime);

        // only keep image/video/svg
        let is_media = format
            .as_ref()
            .map(|f| {
                matches!(
                    f,
                    McatKind::Gif | McatKind::Image | McatKind::Svg | McatKind::Video
                )
            })
            .unwrap_or(false);
        if !is_media {
            continue;
        }

        let Ok(data) = download(response, options).await else {
            warn!(url = %url, "failed to download candidate");
            continue;
        };
        let size = data.len();

        let mut file = McatFile::from_bytes(data, None)?;
        if let Some(fmt) = format {
            file.kind = fmt;
        }

        debug!(url = %url, size, kind = ?file.kind, "downloaded candidate");
        if biggest
            .as_ref()
            .map(|b| size > b.bytes.len())
            .unwrap_or(true)
        {
            biggest = Some(file);
        }
    }

    biggest.context("no valid media found on page")
}

async fn get_response(
    client: &Client,
    url: &str,
    options: &MediaScrapeOptions,
) -> Result<Response> {
    let spinner = if !options.silent {
        let pb = get_multi_progress().add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template(&format!("{{spinner:.green}} Fetching {url}..."))?
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
        );
        Some(pb)
    } else {
        None
    };

    let request = client.get(url).send();
    tokio::pin!(request);

    let response = tokio::select! {
        result = &mut request => result?,
        _ = tokio::time::sleep(Duration::from_millis(300)) => {
            if let Some(ref pb) = spinner { pb.enable_steady_tick(Duration::from_millis(100)); }
            request.await?
        }
    };

    if let Some(pb) = spinner {
        pb.finish_and_clear();
    }
    anyhow::ensure!(
        response.status().is_success(),
        "failed to fetch {url}: {}",
        response.status()
    );

    Ok(response)
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
