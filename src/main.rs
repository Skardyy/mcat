mod iterm_encoder;
mod kitty_encoder;
mod media_encoder;
mod photo_media;
mod sixel_encoder;
mod term_misc;
mod video_media;

#[macro_use]
extern crate lazy_static;

use clap::{
    builder::{styling::AnsiColor, Styles},
    error::ErrorKind,
    Arg, ColorChoice, Command,
};
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
                .value_parser(["fit", "crop", "strech"])
                .default_value("fit"),
        )
        .arg(
            Arg::new("no-center")
                .long("no-center")
                .help("disable centering for the image with the remaining space")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-cache")
                .long("no-cache")
                .help("disable for cache libreoffice convertions")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let path = opts.get_one::<String>("input").unwrap();
    let mut format = opts.get_one::<String>("format").unwrap().as_str();
    let resize_mode = opts.get_one::<String>("resizeMode").unwrap().as_str();
    let width = opts.get_one::<u32>("width").unwrap();
    let height = opts.get_one::<u32>("height").unwrap();
    let center = !opts.get_flag("no-center");
    let cache = !opts.get_flag("no-cache");

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
    match format {
        "iterm" => {
            let item = iterm_encoder::encode(path, *width, *height, resize_mode, center, cache);
            println!("{}", item)
        }
        "kitty" => {
            let item = kitty_encoder::encode(path, *width, *height, resize_mode, center, cache);
            println!("{}", item)
        }
        "sixel" => {
            let item = sixel_encoder::encode(path, *width, *height, resize_mode, center, cache);
            println!("{}", item)
        }
        _ => {}
    }
}
