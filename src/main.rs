mod image_extended;
mod inline_image;
mod inline_image_reader;
mod iterm_encoder;
mod kitty_encoder;
mod sixel_encoder;
mod term_misc;
mod video;

#[macro_use]
extern crate lazy_static;

use std::path::Path;

use clap::{
    builder::{styling::AnsiColor, Styles},
    error::ErrorKind,
    Arg, ColorChoice, Command,
};
use image_extended::{parse_resize_mode, ResizeMode};
use inline_image::InlineImgOpts;
use inline_image_reader::InlineImgReader;
use iterm_encoder::is_iterm_capable;
use kitty_encoder::is_kitty_capable;
use sixel_encoder::is_sixel_capable;
use term_misc::{
    break_filter_string, break_size_string, dim_to_px, init_winsize, EnvIdentifiers, Filters, Size,
};

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
                .default_value("80%"),
        )
        .arg(
            Arg::new("height")
                .long("height")
                .help("the new height: [<usize> / <usize>px / <usize>c / <usize>%]")
                .default_value("60%"),
        )
        .arg(
            Arg::new("resize-mode")
                .short('m')
                .long("resize-mode")
                .help("the technique to use for resizing")
                .value_parser(|mode: &str| {
                    parse_resize_mode(mode).ok_or(clap::Error::new(ErrorKind::InvalidValue))
                })
                .default_value("fit"),
        )
        .arg(
            Arg::new("resize-video")
                .short('r')
                .long("resize-video")
                .help("tries to resize video as well (doesn't respect crop)")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-center")
                .short('C')
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
        .arg(
            Arg::new("spx")
                .long("spx")
                .help("the size of the screen in px (fallback) <width>x<height>x<force> for instance 1920x1080xfalse")
                .value_parser(|spx: &str| {
                    break_size_string(spx).map_err(|e|clap::Error::raw(ErrorKind::InvalidValue, e))
                })
                .default_value("1920x1080"),
        )
        .arg(
            Arg::new("sc")
                .long("sc")
                .help("the size of the screen in cells (fallback) <width>x<height>x<force> for instance 100x20xtrue")
                .value_parser(|spx: &str| {
                    break_size_string(spx).map_err(|e|clap::Error::raw(ErrorKind::InvalidValue, e))
                })
                .default_value("100x20"),
        )
        .arg(
            Arg::new("filter")
                .long("filter")
                .help("filters to apply: [scale, blur, invert, rotate, grayscale, brighten, unsharpen, hue_rotate, contrast]. filter1=value,filter2=value (will extend the default)")
                .value_parser(|filter: &str| {
                    break_filter_string(filter).map_err(|e|clap::Error::raw(ErrorKind::InvalidValue, e))
                })
        )
        .arg(
            Arg::new("save")
                .long("save")
                .help("path to save the image into, will save instead of printing the image")
                .value_parser(|path: &str| {
                    let p = Path::new(path);
                    if p.extension().is_some_and(|f| f == "png") {
                        return Ok(path.to_string());
                    } else {
                        return Err(clap::Error::raw(ErrorKind::InvalidValue, "path must be a png file"));
                    }
                })
        )
        .get_matches();

    let path = opts.get_one::<String>("input").unwrap();
    let mut format = opts.get_one::<String>("format").unwrap().as_str();
    let resize_mode = opts.get_one::<ResizeMode>("resize-mode").unwrap();
    let width = opts.get_one::<String>("width").unwrap();
    let height = opts.get_one::<String>("height").unwrap();
    let center = !opts.get_flag("no-center");
    let resize_video = opts.get_flag("resize-video");
    let cache = opts.get_flag("cache");
    let spx = opts.get_one::<Size>("spx").unwrap();
    let sc = opts.get_one::<Size>("sc").unwrap();
    let filter = opts.get_one::<Filters>("filter");
    let save = opts.get_one::<String>("save");

    let _ = init_winsize(&spx, &sc, filter.and_then(|f| f.scale));
    let width = dim_to_px(&width, term_misc::SizeDirection::WIDTH).unwrap_or_else(|_| {
        eprintln!("invalid width format, please see mcat --help");
        std::process::exit(1);
    }) as u16;
    let height = dim_to_px(&height, term_misc::SizeDirection::HEIGHT).unwrap_or_else(|_| {
        eprintln!("invalid height format, please see mcat --help");
        std::process::exit(1);
    }) as u16;

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
    let img = match InlineImgReader::open(
        &img_path,
        cache,
        format != "sixel",
        InlineImgOpts {
            width,
            height,
            resize_mode: resize_mode.clone(),
            center,
            resize_video,
        },
        filter,
        save,
    ) {
        Ok(img) => img,
        Err(e) => {
            let status = e.to_string() == "file saved";
            eprintln!("{}", e);
            std::process::exit(!status as i32)
        }
    };

    match format {
        "iterm" => match iterm_encoder::encode_image(&img) {
            Ok(item) => println!("{}", item),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1)
            }
        },
        "kitty" => match kitty_encoder::encode_image(&img) {
            Ok(item) => println!("{}", item),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1)
            }
        },
        "sixel" => match sixel_encoder::encode_image(&img) {
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
