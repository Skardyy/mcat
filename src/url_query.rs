use infer;
use scraper::{Html, Selector};
use std::error::Error;
use ureq;

fn get_and_infer_url_content(url: &str) -> Result<(String, Vec<u8>), Box<dyn Error>> {
    let response = ureq::get(url).call()?;
    let mut bytes = Vec::new();
    response.into_reader().read_to_end(&mut bytes)?;

    if let Some(kind) = infer::get(&bytes) {
        Ok((kind.mime_type().to_string(), bytes))
    } else {
        Ok(("application/octet-stream".to_string(), bytes))
    }
}

fn handle_html(html_string: &str) -> Result<(String, Vec<u8>), Box<dyn Error>> {
    let document = Html::parse_document(html_string);

    // Check for top-level SVG
    let svg_selector = Selector::parse("svg").unwrap();
    if let Some(svg) = document.select(&svg_selector).next() {
        return Ok(("svg".to_string(), svg.html().as_bytes().to_vec()));
    }

    // Check for top-level img tag. Only the src attribute is used.
    let img_selector = Selector::parse("img").unwrap();
    if let Some(img) = document.select(&img_selector).next() {
        if let Some(src) = img.value().attr("src") {
            //Recursively call the main function.
            let (img_mime, img_bytes) = get_and_infer_url_content(src)?;
            return Ok((img_mime, img_bytes));
        }
    }

    // If no top-level SVG or IMG, return the entire HTML
    Ok(("html".to_string(), html_string.as_bytes().to_vec()))
}

fn handle_url(url: &str) -> Result<(String, Vec<u8>), Box<dyn Error>> {
    let (mime_type, content) = get_and_infer_url_content(url)?;

    if mime_type == "image/svg+xml" || url.ends_with(".svg") {
        Ok(("svg".to_string(), content))
    } else if mime_type == "image/gif" || url.ends_with(".gif") {
        Ok(("gif".to_string(), content))
    } else if mime_type == "image/jpeg" || url.ends_with(".jpg") || url.ends_with(".jpeg") {
        Ok(("jpeg".to_string(), content))
    } else if mime_type == "image/png" || url.ends_with(".png") {
        Ok(("png".to_string(), content))
    } else if mime_type == "image/bmp" || url.ends_with(".bmp") {
        Ok(("bmp".to_string(), content))
    } else if mime_type == "text/html" {
        let html_string = String::from_utf8_lossy(&content).to_string();
        handle_html(&html_string)
    } else {
        Ok((mime_type, content))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let svg_url = "https://upload.wikimedia.org/wikipedia/commons/0/02/SVG_logo.svg";
    let png_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/6/6a/PNG_transparency_demonstration_1.png/640px-PNG_transparency_demonstration_1.png";
    let gif_url =
        "https://upload.wikimedia.org/wikipedia/commons/2/2c/Rotating_earth_%28large%29.gif";
    let jpg_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/4/47/PNG_transparency_demonstration_1.png/640px-PNG_transparency_demonstration_1.jpg";
    let bmp_url = "https://upload.wikimedia.org/wikipedia/commons/thumb/a/af/Windows_BMP_file_example.bmp/640px-Windows_BMP_file_example.bmp";
    let html_svg_url = "https://img.shields.io/badge/neovim-1e2029?logo=neovim&logoColor=3CA628&label=built%20for&labelColor=15161b";
    let html_img_url = "https://www.w3schools.com/html/html_images.asp";

    let (svg_type, svg_content) = handle_url(svg_url)?;
    println!("SVG: Type: {}, Length: {}", svg_type, svg_content.len());

    let (png_type, png_content) = handle_url(png_url)?;
    println!("PNG: Type: {}, Length: {}", png_type, png_content.len());

    let (gif_type, gif_content) = handle_url(gif_url)?;
    println!("GIF: Type: {}, Length: {}", gif_type, gif_content.len());

    let (jpg_type, jpg_content) = handle_url(jpg_url)?;
    println!("JPG: Type: {}, Length: {}", jpg_type, jpg_content.len());

    let (bmp_type, bmp_content) = handle_url(bmp_url)?;
    println!("BMP: Type: {}, Length: {}", bmp_type, bmp_content.len());

    let (html_svg_type, html_svg_content) = handle_url(html_svg_url)?;
    println!(
        "HTML SVG: Type: {}, Length: {}",
        html_svg_type,
        html_svg_content.len()
    );

    let (html_img_type, html_img_content) = handle_url(html_img_url)?;
    println!(
        "HTML IMG: Type: {}, Length: {}",
        html_img_type,
        html_img_content.len()
    );

    Ok(())
}
