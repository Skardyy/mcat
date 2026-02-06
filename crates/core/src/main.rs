mod catter;
mod cdp;
mod concater;
mod config;
mod converter;
mod fetch_manager;
mod image_viewer;
mod inspector;
mod markdown_viewer;
mod prompter;
mod scrapy;

use clap::{
    Arg, ColorChoice, Command,
    builder::{Styles, styling::AnsiColor},
};
use clap_complete::{Generator, Shell, generate};
use config::McatConfig;
use crossterm::tty::IsTty;
use dirs::home_dir;
use rasteroid::term_misc;
use scrapy::MediaScrapeOptions;
use std::{
    io::{BufWriter, Read, Write},
    path::Path,
};

fn print_completions<G: Generator>(gene: G, cmd: &mut Command) {
    generate(
        gene,
        cmd,
        cmd.get_name().to_string(),
        &mut std::io::stdout(),
    );
}

fn build_core_args() -> Vec<Arg> {
    vec![
        Arg::new("output")
            .long("output")
            .short('o')
            .value_name("type")
            .help("Output format")
            .value_parser(["html", "md", "image", "video", "inline", "interactive"]),
        Arg::new("theme")
            .long("theme")
            .short('t')
            .help("Color theme [default: github]")
            .value_parser([
                "catppuccin",
                "nord",
                "monokai",
                "dracula",
                "gruvbox",
                "one_dark",
                "solarized",
                "tokyo_night",
                "makurai_light",
                "makurai_dark",
                "ayu",
                "ayu_mirage",
                "github",
                "synthwave",
                "material",
                "rose_pine",
                "kanagawa",
                "vscode",
                "everforest",
                "autumn",
                "spring",
            ]),
    ]
}

fn build_markdown_viewer_args() -> Vec<Arg> {
    vec![
        Arg::new("no-linenumbers")
            .long("no-linenumbers")
            .help("Disable line numbers in code blocks")
            .action(clap::ArgAction::SetTrue),
        Arg::new("md-image")
            .long("md-image")
            .value_name("mode")
            .help("what images to render in the markdown [default: auto]")
            .value_parser(["all", "small", "none", "auto"]),
        Arg::new("fast")
            .short('f')
            .help("sets md-image to none, for speed.")
            .action(clap::ArgAction::SetTrue),
        Arg::new("header")
            .long("header")
            .help("shows YAML headers too")
            .action(clap::ArgAction::SetTrue),
        Arg::new("color") // doesn't have default because of env extending
            .long("color")
            .value_name("mode")
            .help("Control ANSI formatting [default: auto]")
            .value_parser(["never", "always", "auto"]),
        Arg::new("color-never")
            .short('C')
            .help("Shortcut for --color never")
            .action(clap::ArgAction::SetTrue),
        Arg::new("color-always")
            .short('c')
            .help("Shortcut for --color always")
            .action(clap::ArgAction::SetTrue),
        Arg::new("pager")
            .long("pager")
            .value_name("command")
            .help("Modify the default pager [default: 'less -r']"),
        Arg::new("paging") // doesn't have default because of env extending
            .long("paging")
            .value_name("mode")
            .help("Control paging behavior [default: auto]")
            .value_parser(["never", "always", "auto"]),
        Arg::new("paging-never")
            .short('P')
            .help("Shortcut for --paging never")
            .action(clap::ArgAction::SetTrue),
        Arg::new("paging-always")
            .short('p')
            .help("Shortcut for --paging always")
            .action(clap::ArgAction::SetTrue),
    ]
}
fn build_image_viewer_args() -> Vec<Arg> {
    vec![
        Arg::new("inline")
            .short('i')
            .help("Shortcut for --output inline")
            .action(clap::ArgAction::SetTrue),
        Arg::new("interactive")
            .short('I')
            .help("Shortcut for --output interactive")
            .action(clap::ArgAction::SetTrue),
        Arg::new("style-html")
            .long("style-html")
            .short('s')
            .help("Add style to HTML output (when HTML is the output)")
            .action(clap::ArgAction::SetTrue),
        Arg::new("report")
            .long("report")
            .action(clap::ArgAction::SetTrue)
            .help("Reports image/video dimensions and additional info"),
        Arg::new("silent")
            .long("silent")
            .action(clap::ArgAction::SetTrue)
            .help("Removes loading bars"),
        Arg::new("kitty")
            .long("kitty")
            .help("Use Kitty image protocol")
            .action(clap::ArgAction::SetTrue),
        Arg::new("iterm")
            .long("iterm")
            .help("Use iTerm2 image protocol")
            .action(clap::ArgAction::SetTrue),
        Arg::new("sixel")
            .long("sixel")
            .help("Use Sixel image protocol")
            .action(clap::ArgAction::SetTrue),
        Arg::new("ascii")
            .long("ascii")
            .help("Use ASCII art output")
            .action(clap::ArgAction::SetTrue),
        Arg::new("horizontal")
            .long("hori")
            .action(clap::ArgAction::SetTrue)
            .help("Concatenate images horizontally"),
        Arg::new("delete-all-images")
            .long("delete-images")
            .help("Delete all images (Kitty only)")
            .action(clap::ArgAction::SetTrue),
        Arg::new("inline-options").long("opts").help(
            "Options for --output inline:\n\
                     *  center=<bool>\n\
                     *  inline=<bool>\n\
                     *  width=<string>\n\
                     *  height=<string>\n\
                     *  scale=<f32>\n\
                     *  scalex=<f32>\n\
                     *  scaley=<f32>\n\
                     *  spx=<string>\n\
                     *  sc=<string>\n\
                     *  zoom=<usize>\n\
                     *  x=<int>\n\
                     *  y=<int>\n\
                     Example: --opts 'center=false,inline=true,width=80%,height=20c,scale=0.5,spx=1920x1080,sc=100x20xforce,zoom=2,x=16,y=8'",
        ),
    ]
}
fn build_fetcher_args() -> Vec<Arg> {
    vec![
        Arg::new("generate-completions")
            .long("generate")
            .value_name("shell")
            .help("Generate shell completions")
            .value_parser(["bash", "zsh", "fish", "powershell"]),
        Arg::new("fetch-chromium")
            .long("fetch-chromium")
            .help("Download and prepare chromium")
            .action(clap::ArgAction::SetTrue),
        Arg::new("fetch-ffmpeg")
            .long("fetch-ffmpeg")
            .help("Download and prepare ffmpeg")
            .action(clap::ArgAction::SetTrue),
        Arg::new("fetch-clean")
            .long("fetch-clean")
            .help("Clean up local binaries")
            .action(clap::ArgAction::SetTrue),
    ]
}
fn build_ls_args() -> Vec<Arg> {
    vec![
        Arg::new("hidden")
            .long("hidden")
            .short('a')
            .help("Include hidden files")
            .action(clap::ArgAction::SetTrue),
        Arg::new("hyprlink")
            .long("hyprlink")
            .help("adds hyprlink to the text in the ls command")
            .action(clap::ArgAction::SetTrue),
        Arg::new("sort") // doesn't have default because of env extending
            .long("sort")
            .value_parser(["name", "size", "time", "type"])
            .help("sort method for the ls command [default: name]"),
        Arg::new("sort-type")
            .short('X')
            .help("shortcut for --sort type")
            .action(clap::ArgAction::SetTrue),
        Arg::new("sort-size")
            .short('S')
            .help("shortcut for --sort size")
            .action(clap::ArgAction::SetTrue),
        Arg::new("reverse")
            .short('r')
            .long("reverse")
            .help("reverse the order of items in the ls command")
            .action(clap::ArgAction::SetTrue),
        Arg::new("ls-options")
            .long("ls-opts")
            .help(
            "Options for directory listings:\n\
                 *  x_padding=<string>\n\
                 *  y_padding=<string>\n\
                 *  min_width=<string>\n\
                 *  max_width=<string>\n\
                 *  height=<string>\n\
                 *  items_per_row=<usize>\n\
                 Example: --ls-opts 'x_padding=4c,y_padding=2c,min_width=4c,max_width=16c,height=8%,items_per_row=12'",
        ),
    ]
}
fn build_input_arg(stdin_streamed: bool) -> Arg {
    let mut input_arg = Arg::new("input")
        .index(1)
        .num_args(1..)
        .help("Input source (file/dir/url/ls)");

    if !stdin_streamed {
        input_arg = input_arg.required_unless_present_any([
            "fetch-clean",
            "fetch-chromium",
            "fetch-ffmpeg",
            "report",
            "generate-completions",
            "delete-all-images",
        ]);
    }
    input_arg
}

fn build_cli(stdin_streamed: bool) -> Command {
    Command::new("mcat")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about("A powerful extended cat command - cat all the things you couldn't before")
        .color(ColorChoice::Always)
        .styles(
            Styles::styled()
                .header(AnsiColor::Green.on_default().bold())
                .literal(AnsiColor::Blue.on_default()),
        )
        // Core arguments and input
        .arg(build_input_arg(stdin_streamed))
        .next_help_heading("Core Options")
        .args(build_core_args())
        // Markdown viewing options
        .next_help_heading("Markdown Viewing")
        .args(build_markdown_viewer_args())
        // Image/Media viewing options
        .next_help_heading("Image/Video Viewing")
        .args(build_image_viewer_args())
        // Directory listing options
        .next_help_heading("Directory Listing")
        .args(build_ls_args())
        // System operations
        .next_help_heading("System Operations")
        .args(build_fetcher_args())
}

fn main() {
    let stdin_streamed = !std::io::stdin().is_tty();
    let stdout = std::io::stdout().lock();
    let mut out = BufWriter::new(stdout);
    let opts = build_cli(stdin_streamed).get_matches();

    let mut config = McatConfig::default();
    config.extend_from_env();
    config.extend_from_args(&opts);

    // setting the winsize
    let spx = term_misc::break_size_string(config.inline_options.spx.as_ref()).unwrap_or_exit();
    let sc = term_misc::break_size_string(config.inline_options.sc.as_ref()).unwrap_or_exit();
    let _ = term_misc::init_wininfo(
        &spx,
        &sc,
        config.inline_options.scalex,
        config.inline_options.scaley,
        config.is_tmux,
        config.inline_options.inline,
    );

    // fn and leave
    if let Some(fn_and_leave) = config.fn_and_leave {
        match fn_and_leave {
            config::FnAndLeave::ShellGenerate(shell) => {
                let mut cmd = build_cli(stdin_streamed);
                match shell.as_str() {
                    "bash" => print_completions(Shell::Bash, &mut cmd),
                    "zsh" => print_completions(Shell::Zsh, &mut cmd),
                    "fish" => print_completions(Shell::Fish, &mut cmd),
                    "powershell" => print_completions(Shell::PowerShell, &mut cmd),
                    _ => unreachable!(),
                }
            }
            config::FnAndLeave::DeleteImages => {
                rasteroid::kitty_encoder::delete_all_images(&mut out).unwrap_or_exit()
            }
            config::FnAndLeave::FetchChromium => fetch_manager::fetch_chromium().unwrap_or_exit(),
            config::FnAndLeave::FetchFfmpeg => fetch_manager::fetch_ffmpeg().unwrap_or_exit(),
            config::FnAndLeave::FetchClean => fetch_manager::clean().unwrap_or_exit(),
            config::FnAndLeave::Report => report_full(),
        };
        return;
    };

    // if ls
    if config.is_ls {
        let d = ".".to_string();
        let input = config.input.get(1).unwrap_or(&d);
        if config.is_tmux {
            rasteroid::set_tmux_passthrough(true);
        }
        converter::lsix(input, &mut out, &config.ls_options, &config.inline_encoder)
            .unwrap_or_exit();
        return;
    }

    // gathering all the inputs
    let mut tmp_files = Vec::new(); //for lifetime
    let mut path_bufs = Vec::new();
    // if stdin is streamed into
    if stdin_streamed {
        let mut buffer = Vec::new();
        std::io::stdin().read_to_end(&mut buffer).unwrap_or_exit();

        let inter = inspector::InspectedBytes::from_bytes(&buffer).unwrap_or_exit();
        match inter {
            inspector::InspectedBytes::File(named_temp_file) => {
                let path = named_temp_file.path().to_path_buf();
                path_bufs.push(path);
                tmp_files.push(named_temp_file);
            }
            inspector::InspectedBytes::Path(path_buf) => path_bufs.push(path_buf),
        };
    }
    let mut scraper_opts = MediaScrapeOptions::default();
    scraper_opts.silent = config.silent;
    for i in config.input.iter() {
        if i.starts_with("https://") {
            if let Ok(tmp) = scrapy::scrape_biggest_media(&i, &scraper_opts) {
                let path = tmp.path().to_path_buf();
                tmp_files.push(tmp);
                path_bufs.push(path);
            } else {
                eprintln!("{} didn't contain any supported media", i);
            }
        } else {
            let i = expand_tilde(&i);
            let path = Path::new(&i);
            if !path.exists() {
                eprintln!("{} doesn't exists", path.display());
                std::process::exit(1);
            }
            if path.is_dir() {
                let mut selected_files =
                    prompter::prompt_for_files(path, config.hidden).unwrap_or_exit();
                selected_files.sort();
                path_bufs.extend_from_slice(&selected_files);
            } else {
                path_bufs.push(path.to_path_buf());
            }
        }
    }

    // concating and printing the result
    let formats = concater::check_unified_format(&path_bufs);
    let is_interactive = config.output.clone().unwrap_or_default() == "interactive";
    let contains_video = formats.contains(&"video");
    let contains_image = formats.contains(&"image");
    let contains_text = formats.contains(&"text");
    match (
        contains_video,
        contains_image,
        contains_text,
        is_interactive,
    ) {
        // images and videos and interactive
        (false, true | false, true | false, true) => {
            let paths = path_bufs.iter().map(|v| v.as_path()).collect();
            catter::cat(paths, &mut out, &config).unwrap_or_exit();
        }
        // only text
        (false, false, true, _) => {
            if path_bufs.len() == 1 {
                catter::cat(vec![&path_bufs[0]], &mut out, &config).unwrap_or_exit();
            } else {
                let tmp = concater::concat_text(path_bufs);
                catter::cat(vec![tmp.path()], &mut out, &config).unwrap_or_exit();
            }
        }
        // only videos
        (true, false, false, _) => {
            match config.inline_encoder {
                rasteroid::InlineEncoder::Ascii | rasteroid::InlineEncoder::Sixel => {}
                _ => {
                    if config.is_tmux {
                        rasteroid::set_tmux_passthrough(true);
                    }
                }
            }
            if path_bufs.len() == 1 {
                catter::cat(vec![&path_bufs[0]], &mut out, &config).unwrap_or_exit();
            } else {
                #[allow(unused_variables)]
                let (dir, path) = concater::concat_video(&path_bufs).unwrap_or_exit();
                catter::cat(vec![&path], &mut out, &config).unwrap_or_exit();
            }
        }
        // only images
        (false, true, false, _) => {
            match config.inline_encoder {
                rasteroid::InlineEncoder::Ascii => {}
                _ => {
                    if config.is_tmux {
                        rasteroid::set_tmux_passthrough(true);
                    }
                }
            }
            if path_bufs.len() == 1 {
                catter::cat(vec![&path_bufs[0]], &mut out, &config).unwrap_or_exit();
            } else {
                let img = concater::concat_images(path_bufs, config.horizontal_image_stacking)
                    .unwrap_or_exit();
                catter::cat(vec![img.path()], &mut out, &config).unwrap_or_exit();
            }
        }
        _ => {
            eprintln!("Error: Cannot have 2 different formats [text / images / videos]");
            std::process::exit(1);
        }
    }
    out.flush().unwrap();
}

trait UnwrapOrExit<T> {
    fn unwrap_or_exit(self) -> T;
}

impl<T, E: std::fmt::Display> UnwrapOrExit<T> for Result<T, E> {
    fn unwrap_or_exit(self) -> T {
        match self {
            Ok(value) => value,
            Err(err) => {
                eprintln!("{}", err);
                std::process::exit(1);
            }
        }
    }
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~") {
        if let Some(home) = home_dir() {
            return path.replace("~", &home.to_string_lossy().into_owned());
        }
    }
    path.to_string()
}

fn report_full() {
    let is_chromium_installed = fetch_manager::is_chromium_installed();
    let is_ffmpeg_installed = fetch_manager::is_ffmpeg_installed();
    let is_poppler_installed = fetch_manager::is_poppler_installed();
    let mut env = term_misc::EnvIdentifiers::new();
    let kitty = rasteroid::kitty_encoder::is_kitty_capable(&mut env);
    let iterm = rasteroid::iterm_encoder::is_iterm_capable(&mut env);
    let sixel = rasteroid::sixel_encoder::is_sixel_capable(&mut env);
    let ascii = true; //not sure what doesn't support it
    let winsize = term_misc::get_wininfo();
    let tmux = winsize.is_tmux;
    let inline = winsize.needs_inline;
    let os = env.data.get("OS").map(|f| f.as_str()).unwrap_or("Unknown");
    let term = if tmux {
        env.data
            .get("TMUX_ORIGINAL_TERM")
            .map(|f| f.as_str())
            .unwrap_or("Unknonwn")
    } else {
        env.data
            .get("TERM")
            .map(|f| f.as_str())
            .unwrap_or("Unknonwn")
    };
    let tmux_program = if tmux {
        env.data
            .get("TMUX_ORIGINAL_SPEC")
            .map(|f| f.as_str())
            .unwrap_or("Unknown")
    } else {
        env.data
            .get("TERM_PROGRAM")
            .map(|f| f.as_str())
            .unwrap_or("Unknown")
    };
    let ver = env!("CARGO_PKG_VERSION");

    println!("┌────────────────────────────────────────────────────┐");
    println!("│               SYSTEM CAPABILITIES                  │");
    println!("├────────────────────────────────────────────────────┤");

    fn green(text: &str) -> String {
        format!("\x1b[32m{}\x1b[0m", text)
    }
    fn red(text: &str) -> String {
        format!("\x1b[31m{}\x1b[0m", text)
    }
    fn format_status(status: bool) -> String {
        if status {
            green("✓ INSTALLED")
        } else {
            red("× MISSING")
        }
    }
    fn format_capability(status: bool) -> String {
        if status {
            green("✓ SUPPORTED")
        } else {
            red("× UNSUPPORTED")
        }
    }
    fn format_info(status: bool) -> String {
        if status {
            green("✓ YES")
        } else {
            red("× NO")
        }
    }

    // required dependencies
    println!("│ Optional Dependencies:                             │");
    println!(
        "│   Chromium: {:<47} │",
        format_status(is_chromium_installed)
    );
    println!("│   FFmpeg:   {:<47} │", format_status(is_ffmpeg_installed));
    println!(
        "│   Poppler:  {:<47} │",
        format_status(is_poppler_installed)
    );

    // terminal capabilities
    println!("├────────────────────────────────────────────────────┤");
    println!("│ Terminal Graphics Support:                         │");
    println!("│   Kitty:    {:<47} │", format_capability(kitty));
    println!("│   iTerm2:   {:<47} │", format_capability(iterm));
    println!("│   Sixel:    {:<47} │", format_capability(sixel));
    println!("│   ASCII:    {:<47} │", format_capability(ascii));

    // terminal dimensions
    println!("├────────────────────────────────────────────────────┤");
    println!("│ Terminal Info:                                     │");
    println!("│   Width:          {:<32} │", winsize.sc_width);
    println!("│   Height:         {:<32} │", winsize.sc_height);
    println!("│   Pixel Width:    {:<32} │", winsize.spx_width);
    println!("│   Pixel Height:   {:<32} │", winsize.spx_height);

    // Others
    println!("├────────────────────────────────────────────────────┤");
    println!("│ Others:                                            │");
    println!("│   Tmux:       {:<45} │", format_info(tmux));
    println!("│   Inline:     {:<45} │", format_info(inline));
    println!("│   OS:         {:<36} │", os);
    println!("│   TERM:       {:<36} │", term);
    println!("│   TERMTYPE:   {:<36} │", tmux_program);
    println!("│   Version:    {:<36} │", ver);

    println!("└────────────────────────────────────────────────────┘");
}
