mod iterm_encoder;
mod kitty_encoder;
mod media_encoder;

use clap::{
    builder::{styling::AnsiColor, Styles},
    Arg, ColorChoice, Command,
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
                .value_parser(["sixel", "kitty", "iterm", "ascii", "auto"])
                .default_value("iterm"),
        )
        .get_matches();

    let path = opts.get_one::<String>("path").unwrap();
    let format = opts.get_one::<String>("format").unwrap().to_lowercase();
    let format = format.as_str();

    match format {
        "iterm" => {
            if let Ok(item) = iterm_encoder::encode(path) {
                println!("{}", item)
            }
            println!("did iterm");
        }
        "kitty" => {
            if let Ok(item) = kitty_encoder::encode(path) {
                println!("{}", item)
            }
            println!("did kitty");
        }
        _ => {
            println!("auto")
        }
    }
}
