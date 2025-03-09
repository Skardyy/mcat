mod iterm_encoder;
mod kitty_encoder;
mod media_encoder;
mod sixel_encoder;
mod term_misc;

use clap::{
    builder::{styling::AnsiColor, Styles},
    Arg, ColorChoice, Command,
};
use iterm_encoder::is_iterm_capable;
use kitty_encoder::is_kitty_capable;
use sixel_encoder::is_sixel_capable;

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
            Arg::new("path")
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
        .get_matches();

    let path = opts.get_one::<String>("path").unwrap();
    let format = opts.get_one::<String>("format").unwrap().to_lowercase();
    let mut format = format.as_str();

    if format == "auto" {
        let kitty_capable = is_kitty_capable();
        let iterm_capable = is_iterm_capable();
        let sixel_capable = is_sixel_capable();

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
            if let Ok(item) = iterm_encoder::encode(path) {
                println!("{}", item)
            }
        }
        "kitty" => {
            if let Ok(item) = kitty_encoder::encode(path) {
                println!("{}", item)
            }
        }
        "sixel" => {
            if let Ok(item) = sixel_encoder::encode(path) {
                println!("{}", item)
            }
        }
        _ => {}
    }
}
