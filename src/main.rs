mod image_extended;
mod iterm_encoder;
mod kitty_encoder;
mod sixel_encoder;
mod term_misc;

#[macro_use]
extern crate lazy_static;

use std::path::Path;

use clap::{
    builder::{styling::AnsiColor, Styles},
    error::ErrorKind,
    Arg, ColorChoice, Command,
};
use image::ImageReader;
use image_extended::{parse_resize_mode, DocumentReader, InlineImage, ResizeMode};
use iterm_encoder::is_iterm_capable;
use kitty_encoder::is_kitty_capable;
use sixel_encoder::is_sixel_capable;
use term_misc::{dim_to_px, EnvIdentifiers};

fn main() {
    let opts = Command::new("mcat")
        .version("0.1")
        .about("A blazingly fast media cat tool")
        .color(ColorChoice::Always)
        .styles(
            Styles::styled()
                .header(AnsiColor::Green.on_default().bold())
                .literal(AnsiColor::Blue.on_default()),
        )
        .arg(
            Arg::new("input")
                .index(1)
                .help("Path / Url to the media file")
                .required(true),
        )
        .arg(
            Arg::new("format")
                .short('f')
                .long("format")
                .help("the protocol to use for the encoding")
                .value_parser(["sixel", "kitty", "iterm", "auto"])
                .default_value("auto"),
        )
        .arg(
            Arg::new("width")
                .long("width")
                .help("the new width: [<usize> / <usize>px / <usize>c / <usize>%]")
                .default_value("80%")
                .value_parser(|dim_str: &str| {
                    dim_to_px(dim_str, term_misc::SizeDirection::WIDTH)
                        .map_err(|_| clap::Error::new(ErrorKind::InvalidValue))
                }),
        )
        .arg(
            Arg::new("height")
                .long("height")
                .help("the new height: [<usize> / <usize>px / <usize>c / <usize>%]")
                .default_value("60%")
                .value_parser(|dim_str: &str| {
                    dim_to_px(dim_str, term_misc::SizeDirection::HEIGHT)
                        .map_err(|_| clap::Error::new(ErrorKind::InvalidValue))
                }),
        )
        .arg(
            Arg::new("resizeMode")
                .short('m')
                .long("resizeMode")
                .help("the technique to use for resizing")
                .value_parser(|mode: &str| {
                    parse_resize_mode(mode).ok_or(clap::Error::new(ErrorKind::InvalidValue))
                })
                .default_value("fit"),
        )
        .arg(
            Arg::new("no-center")
                .long("no-center")
                .help("disable centering for the image with the remaining space")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("cache")
                .short('c')
                .long("cache")
                .help("enable caching for document files / urls")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let path = opts.get_one::<String>("input").unwrap();
    let mut format = opts.get_one::<String>("format").unwrap().as_str();
    let resize_mode = opts.get_one::<ResizeMode>("resizeMode").unwrap();
    let width = *opts.get_one::<u32>("width").unwrap() as u16;
    let height = *opts.get_one::<u32>("height").unwrap() as u16;
    let center = !opts.get_flag("no-center");
    let cache = opts.get_flag("cache");

    if format == "auto" {
        let env = &EnvIdentifiers::new();
        let kitty_capable = is_kitty_capable(env);
        let iterm_capable = is_iterm_capable(env);
        let sixel_capable = is_sixel_capable(env);

        if iterm_capable {
            format = "iterm"
        } else if kitty_capable {
            format = "kitty"
        } else if sixel_capable {
            format = "sixel"
        }
    }

    let img_path = Path::new(path).to_path_buf();
    let img = match ImageReader::open_inline_image(&img_path, cache) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1)
        }
    };
    let (img, offset) = match img.resize_into_png(width, height, resize_mode, center) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1)
        }
    };

    match format {
        "iterm" => match iterm_encoder::encode_image(&img, offset) {
            Ok(item) => println!("{}", item),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1)
            }
        },
        "kitty" => match kitty_encoder::encode_image(&img, offset) {
            Ok(item) => println!("{}", item),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1)
            }
        },
        "sixel" => match sixel_encoder::encode_image(&img, offset) {
            Ok(item) => println!("{}", item),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1)
            }
        },
        _ => {
            eprintln!("how did you reach here?");
            std::process::exit(1)
        }
    }
}
