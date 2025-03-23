// use image::ImageFormat;
// use scraper::{Html, Selector};
// use std::{
//     error::Error,
//     io::{Cursor, Read},
// };
//
// use crate::{
//     image_extended::PNGImage,
//     inline_image::{self, InlineImage, InlineImageOpts},
//     inline_video::InlineVideo,
//     media_reader::{apply_filters, load_svg},
//     term_misc::Filters,
// };
//
// enum Mime {
//     Md,
//     Svg,
//     Gif,
//     Image(ImageFormat),
//     NotSupported,
//     Html,
// }
// fn get_and_infer_url_content(url: &str) -> Result<(Mime, Vec<u8>), Box<dyn Error>> {
//     let url_without_params = match url.rfind('?') {
//         Some(i) => &url[..i],
//         None => url,
//     };
//     let base_url_name = match url_without_params.rfind('/') {
//         Some(i) => &url_without_params[i..],
//         None => url_without_params,
//     };
//     let ext = match base_url_name.rfind('.') {
//         Some(i) => &base_url_name[i + 1..],
//         None => "",
//     };
//     let ext: &str = &ext.to_lowercase();
//
//     let mime = match ext {
//         "svg" => Mime::Svg,
//         "gif" => Mime::Gif,
//         "" => Mime::Html,
//         _ => match ImageFormat::from_extension(ext) {
//             Some(f) => Mime::Image(f),
//             None => Mime::NotSupported,
//         },
//     };
//     let (mime, content) = match mime {
//         Mime::Html => handle_html(url)?,
//         Mime::NotSupported => (mime, Vec::new()),
//         _ => {
//             let response = ureq::get(url).call()?;
//             let mut bytes = Vec::new();
//             response.into_body().into_reader().read_to_end(&mut bytes)?;
//
//             (mime, bytes)
//         }
//     };
//
//     Ok((mime, content))
// }
//
// fn parse_dimension(value: Option<&str>, reference_size: i32) -> i32 {
//     match value {
//         Some(dim) => {
//             if dim.ends_with('%') {
//                 // Handle percentage
//                 if let Ok(percent) = dim.trim_end_matches('%').parse::<f32>() {
//                     (reference_size as f32 * percent / 100.0) as i32
//                 } else {
//                     reference_size // Default to reference size if parsing fails
//                 }
//             } else if dim == "auto" {
//                 reference_size // Assume full size for "auto"
//             } else if let Ok(num) = dim.parse::<i32>() {
//                 num // Use numeric value
//             } else {
//                 reference_size // Default to reference size for other cases
//             }
//         }
//         None => reference_size, // Default to reference size if not specified
//     }
// }
//
// fn handle_html(url: &str) -> Result<(Mime, Vec<u8>), Box<dyn Error>> {
//     let html_string = ureq::get(url).call()?.into_body().read_to_string()?;
//     let document = Html::parse_document(&html_string);
//
//     let img_selector = Selector::parse("img")?;
//     let img_elements: Vec<_> = document.select(&img_selector).collect();
//
//     if !img_elements.is_empty() {
//         const REF_WIDTH: i32 = 1920;
//         const REF_HEIGHT: i32 = 1080;
//         let mut largest_img_src = None;
//         let mut largest_area = 0;
//
//         for img in img_elements {
//             let src = match img.value().attr("src") {
//                 Some(s) => s,
//                 None => continue, // Skip images without src
//             };
//
//             let width = parse_dimension(img.value().attr("width"), REF_WIDTH);
//             let height = parse_dimension(img.value().attr("height"), REF_HEIGHT);
//             let area = width * height;
//
//             // Check for largest image
//             if area > largest_area {
//                 largest_area = area;
//                 largest_img_src = Some(src);
//             }
//         }
//
//         // If we found an image src, fetch and return it
//         if let Some(src) = largest_img_src {
//             let (img_mime, img_bytes) = get_and_infer_url_content(src)?;
//             return Ok((img_mime, img_bytes));
//         }
//     }
//
//     // Check for top-level SVG
//     let svg_selector = Selector::parse("svg")?;
//     if let Some(svg) = document.select(&svg_selector).next() {
//         return Ok((Mime::Svg, svg.html().as_bytes().to_vec()));
//     }
//
//     Err("url doesn't contain a top level svg / img".into())
// }
//
// pub fn handle_url(
//     url: &str,
//     opts: InlineImageOpts,
//     try_video: bool,
//     filter: Option<&Filters>,
// ) -> Result<InlineImage, Box<dyn Error>> {
//     let (mime_type, content) = get_and_infer_url_content(url)?;
//     let mut img = match mime_type {
//         Mime::Svg => {
//             let cursor = Cursor::new(content);
//             load_svg(cursor)?
//         }
//         Mime::Gif => {
//             if try_video {
//                 let vid = InlineVideo::new(content);
//                 let offset = vid.get_offset_for_center(opts.center)?;
//                 let inline_img = InlineImage::from_raw(
//                     vid.data,
//                     inline_image::InlineImageFormat::Gif,
//                     Some(offset),
//                 );
//                 return Ok(inline_img);
//             } else {
//                 image::load_from_memory_with_format(&content, ImageFormat::Gif)?
//             }
//         }
//         Mime::Image(image_format) => image::load_from_memory_with_format(&content, image_format)?,
//         Mime::NotSupported => return Err("url type is not supported".into()),
//         Mime::Html => return Err("couldn't find anything to turn into image in the url".into()),
//     };
//
//     if let Some(filter) = filter {
//         apply_filters(&mut img, filter);
//     };
//
//     let inline_img = img.into_inline_img(opts)?;
//     Ok(inline_img)
// }
