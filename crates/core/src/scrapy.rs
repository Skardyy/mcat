use clap::error::Result;
use futures::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex::Regex;
use reqwest::{Client, Response};
use scraper::Html;
use std::io::Write;
use std::sync::OnceLock;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::runtime::{Builder, Runtime};

use crate::catter;

static GITHUB_BLOB_URL: OnceLock<Regex> = OnceLock::new();
static HTTP_CLIENT: OnceLock<Client> = OnceLock::new();
static RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn extension_from_mime(mime: &str) -> Option<&'static str> {
    if mime.contains("image/avif") {
        Some("avif")
    } else if mime.contains("image/jpeg") {
        Some("jpg")
    } else if mime.contains("image/png") {
        Some("png")
    } else if mime.contains("image/apng") {
        Some("apng")
    } else if mime.contains("image/gif") {
        Some("gif")
    } else if mime.contains("image/webp") {
        Some("webp")
    } else if mime.contains("image/tiff") {
        Some("tiff")
    } else if mime.contains("image/x-tga") {
        Some("tga")
    } else if mime.contains("image/vnd.ms-dds") {
        Some("dds")
    } else if mime.contains("image/bmp") {
        Some("bmp")
    } else if mime.contains("image/x-icon") || mime.contains("image/vnd.microsoft.icon") {
        Some("ico")
    } else if mime.contains("image/vnd.radiance") {
        Some("hdr")
    } else if mime.contains("image/aces") || mime.contains("image/exr") {
        Some("exr")
    } else if mime.contains("image/x-portable-bitmap") {
        Some("pbm")
    } else if mime.contains("image/x-portable-graymap") {
        Some("pgm")
    } else if mime.contains("image/x-portable-pixmap") {
        Some("ppm")
    } else if mime.contains("image/x-portable-anymap") {
        Some("pam")
    } else if mime.contains("image/x-farbfeld") {
        Some("ff")
    } else if mime.contains("image/x-qoi") {
        Some("qoi")
    } else if mime.contains("image/x-pcx") {
        Some("pcx")
    } else if mime.contains("image/svg+xml") {
        Some("svg")

        // Video
    } else if mime.contains("video/mp4") {
        Some("mp4")
    } else if mime.contains("video/webm") {
        Some("webm")
    } else if mime.contains("video/x-msvideo") {
        Some("avi")
    } else if mime.contains("video/x-matroska") {
        Some("mkv")
    } else if mime.contains("video/quicktime") {
        Some("mov")

        // Documents
    } else if mime.contains("application/pdf") {
        Some("pdf")
    } else if mime.contains("text/markdown") || mime.contains("text/x-markdown") {
        Some("md")
    } else if mime.contains("application/msword") {
        Some("doc")
    } else if mime
        .contains("application/vnd.openxmlformats-officedocument.wordprocessingml.document")
    {
        Some("docx")
    } else if mime.contains("application/vnd.oasis.opendocument.text") {
        Some("odt")
    } else if mime.contains("application/vnd.ms-excel") {
        Some("xls")
    } else if mime.contains("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet") {
        Some("xlsx")
    } else if mime.contains("application/vnd.ms-excel.sheet.macroenabled.12") {
        Some("xlsm")
    } else if mime.contains("application/vnd.ms-excel.sheet.binary.macroenabled.12") {
        Some("xlsb")
    } else if mime.contains("application/vnd.ms-excel.addin.macroenabled.12") {
        Some("xlam")
    } else if mime.contains("application/vnd.ms-excel.addin") {
        Some("xla")
    } else if mime.contains("application/vnd.oasis.opendocument.spreadsheet") {
        Some("ods")
    } else if mime.contains("application/vnd.ms-powerpoint") {
        Some("ppt")
    } else if mime
        .contains("application/vnd.openxmlformats-officedocument.presentationml.presentation")
    {
        Some("pptx")
    } else if mime.contains("application/vnd.oasis.opendocument.presentation") {
        Some("odp")
    } else if mime.contains("text/csv") || mime.contains("application/csv") {
        Some("csv")

        // Archives
    } else if mime.contains("application/zip") {
        Some("zip")
    } else if mime.contains("application/x-rar-compressed") {
        Some("rar")
    } else if mime.contains("application/x-7z-compressed") {
        Some("7z")
    } else if mime.contains("application/x-tar") {
        Some("tar")
    } else if mime.contains("application/gzip") {
        Some("gz")
    } else if mime.contains("application/x-bzip2") {
        Some("bz2")
    } else {
        None
    }
}

static GLOBAL_MULTI_PROGRESS: OnceLock<MultiProgress> = OnceLock::new();

fn get_global_multi_progress() -> &'static MultiProgress {
    GLOBAL_MULTI_PROGRESS.get_or_init(|| MultiProgress::new())
}

pub struct MediaScrapeOptions {
    pub silent: bool,
    pub max_content_length: Option<u64>,
    pub images: bool,
    pub videos: bool,
    pub documents: bool,
}

impl Default for MediaScrapeOptions {
    fn default() -> Self {
        MediaScrapeOptions {
            silent: false,
            max_content_length: None,
            images: true,
            videos: true,
            documents: true,
        }
    }
}

#[derive(Debug, PartialEq)]
enum MediaType {
    Image,
    Video,
    Document,
}

fn get_media_type_from_ext(ext: &str) -> MediaType {
    if catter::is_video(ext) {
        MediaType::Video
    } else if ext == "svg" || image::ImageFormat::from_extension(ext).is_some() {
        MediaType::Image
    } else {
        MediaType::Document
    }
}

fn is_media_type_allowed(media_type: &MediaType, options: &MediaScrapeOptions) -> bool {
    match media_type {
        MediaType::Image => options.images,
        MediaType::Video => options.videos,
        MediaType::Document => options.documents,
    }
}

pub fn scrape_biggest_media(
    url: &str,
    options: &MediaScrapeOptions,
) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let client = HTTP_CLIENT.get_or_init(|| {
        Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .build().unwrap_or_default()
    });
    let rt = RUNTIME.get_or_init(|| Builder::new_current_thread().enable_all().build().unwrap());

    let re =
        GITHUB_BLOB_URL.get_or_init(|| Regex::new(r"^.*github\.com.*[\\\/]blob[\\\/].*$").unwrap());
    let url = if re.is_match(url) && !url.contains("?raw=true") {
        &format!("{url}?raw=true")
    } else {
        url
    };

    rt.block_on(async {
        let response = get_response(&client, url, options).await?;

        // Check content length before proceeding
        if let Some(max_length) = options.max_content_length {
            if let Some(content_length) = get_content_length(&response) {
                if content_length > max_length {
                    return Err(format!(
                        "Content length ({} bytes) exceeds maximum allowed ({} bytes)",
                        content_length, max_length
                    )
                    .into());
                }
            }
        }

        // Direct file download if mime type is recognized
        if let Some(ext) = get_ext_from_response(&response) {
            let media_type = get_media_type_from_ext(ext);
            if is_media_type_allowed(&media_type, options) {
                let data = download_media(response, options).await?;
                return write_to_tmp_file(&data, ext);
            } else {
                return Err(format!(
                    "Media type {:?} is not allowed by current options",
                    media_type
                )
                .into());
            }
        }

        // Process HTML content for embedded media
        process_html(&client, response, url, options).await
    })
}

fn get_content_length(response: &Response) -> Option<u64> {
    response
        .headers()
        .get("content-length")
        .and_then(|cl| cl.to_str().ok())
        .and_then(|cl| cl.parse::<u64>().ok())
}

fn get_ext_from_response<'a>(response: &Response) -> Option<&'a str> {
    response
        .headers()
        .get("Content-Type")
        .and_then(|h| h.to_str().ok())
        .and_then(extension_from_mime)
}

fn write_to_tmp_file(data: &[u8], ext: &str) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let mut tmp_file = NamedTempFile::with_suffix(&format!(".{}", ext))?;
    tmp_file.write_all(&data)?;
    return Ok(tmp_file);
}

async fn download_media(
    response: Response,
    options: &MediaScrapeOptions,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let content_length = get_content_length(&response);

    // Check max content length before downloading
    if let (Some(max_length), Some(actual_length)) = (options.max_content_length, content_length) {
        if actual_length > max_length {
            return Err(format!(
                "Content length ({} bytes) exceeds maximum allowed ({} bytes)",
                actual_length, max_length
            )
            .into());
        }
    }

    // Setup progress bar if not silent and content length is known
    let progress_bar =
        if !options.silent && content_length.is_some() && content_length.unwrap() > 500_000 {
            let pb = get_global_multi_progress().add(ProgressBar::new(content_length.unwrap()));
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

    let mut stream = response.bytes_stream();
    let mut file_data = Vec::new();
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result?;
        file_data.extend_from_slice(&chunk);

        // Check if we've exceeded max content length during download
        if let Some(max_length) = options.max_content_length {
            if file_data.len() as u64 > max_length {
                if let Some(pb) = progress_bar {
                    pb.finish_and_clear();
                }
                return Err(format!(
                    "Downloaded content ({} bytes) exceeds maximum allowed ({} bytes)",
                    file_data.len(),
                    max_length
                )
                .into());
            }
        }

        // Update progress bar if we have one
        if let Some(pb) = &progress_bar {
            pb.set_position(file_data.len() as u64);
        }
    }
    // Finish the progress bar if we have one
    if let Some(pb) = progress_bar {
        pb.finish_and_clear();
    }

    Ok(file_data)
}

async fn process_html(
    client: &Client,
    response: Response,
    url: &str,
    options: &MediaScrapeOptions,
) -> Result<NamedTempFile, Box<dyn std::error::Error>> {
    let html_content = response.text().await?;
    let document = Html::parse_document(&html_content);

    // first lets find the potential media
    let mut potential_media: Vec<(String, String)> = Vec::new();

    // Find image tags (only if images are enabled)
    if options.images {
        let image_selector = scraper::Selector::parse("img[src]").unwrap();
        for element in document.select(&image_selector) {
            if let Some(src) = element.value().attr("src") {
                potential_media.push((src.to_string(), "image".to_string()));
            }
        }

        // Find object and embed tags for SVGs
        let svg_selectors = vec![
            scraper::Selector::parse("object[type='image/svg+xml'][data]").unwrap(),
            scraper::Selector::parse("embed[type='image/svg+xml'][src]").unwrap(),
            scraper::Selector::parse("img[src$='.svg']").unwrap(),
        ];
        for selector in svg_selectors {
            for element in document.select(&selector) {
                if let Some(src) = element
                    .value()
                    .attr("data")
                    .or_else(|| element.value().attr("src"))
                {
                    potential_media.push((src.to_string(), "svg".to_string()));
                }
            }
        }
    }

    // Find video tags (only if videos are enabled)
    if options.videos {
        let video_selector = scraper::Selector::parse("video[src]").unwrap();
        for element in document.select(&video_selector) {
            if let Some(src) = element.value().attr("src") {
                potential_media.push((src.to_string(), "video".to_string()));
            }
        }

        // Find source tags within video tags
        let video_source_selector = scraper::Selector::parse("video source[src]").unwrap();
        for element in document.select(&video_source_selector) {
            if let Some(src) = element.value().attr("src") {
                potential_media.push((src.to_string(), "video".to_string()));
            }
        }
    }

    // then lets get the biggest valid one of them
    let mut biggest_media: Option<(usize, Vec<u8>, String)> = None;
    for (media_url, media_type) in potential_media.iter() {
        // parse url
        let resolved_url = match reqwest::Url::parse(url).and_then(|base| base.join(&media_url)) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // get response
        let media_response = match get_response(client, resolved_url.as_str(), options).await {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Check content length before downloading
        if let Some(max_length) = options.max_content_length {
            if let Some(content_length) = get_content_length(&media_response) {
                if content_length > max_length {
                    continue; // Skip this media item
                }
            }
        }

        // check if its an ext we support
        let ext = match get_ext_from_response(&media_response) {
            Some(v) => v,
            None => continue,
        };

        let is_valid = match media_type.as_str() {
            "svg" => ext == "svg",
            "video" => catter::is_video(&ext),
            "image" => image::ImageFormat::from_extension(&ext).is_some(),
            _ => false,
        };
        if !is_valid {
            continue;
        }

        // Check if this media type is allowed
        let detected_media_type = get_media_type_from_ext(ext);
        if !is_media_type_allowed(&detected_media_type, options) {
            continue;
        }

        // download the media
        let media_data = match download_media(media_response, options).await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let media_size = media_data.len();

        if biggest_media.is_none() || media_size > biggest_media.as_ref().unwrap().0 {
            biggest_media = Some((media_size, media_data, ext.to_owned()));
        }
    }

    match biggest_media {
        Some((_, data, ext)) => write_to_tmp_file(&data, &ext),
        None => {
            Err("No significant and valid media found that matches the specified options.".into())
        }
    }
}

async fn get_response(
    client: &Client,
    url: &str,
    options: &MediaScrapeOptions,
) -> Result<Response, Box<dyn std::error::Error>> {
    let initial_spinner = if !options.silent {
        let pb = get_global_multi_progress().add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::default_spinner()
                .template(&format!("{{spinner:.green}} Fetching {url}..."))?
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
        );
        Some(pb)
    } else {
        None
    };

    let request_future = client.get(url).send();
    tokio::pin!(request_future);

    let response = tokio::select! {
        result = &mut request_future => {
            result
        }
            _ = tokio::time::sleep(Duration::from_millis(300)) => {
            // Request is taking too long, start spinner and wait for it
            if let Some(ref spinner) = initial_spinner {
                spinner.enable_steady_tick(Duration::from_millis(100));
            }
            request_future.await
        }
    }?;

    if let Some(spinner) = initial_spinner {
        spinner.finish_and_clear();
    }
    if !response.status().is_success() {
        return Err(format!("Failed to retrieve URL: {}", response.status()).into());
    }

    Ok(response)
}
