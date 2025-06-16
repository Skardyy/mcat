mod catter;
mod concater;
mod config;
mod converter;
mod fetch_manager;
mod image_viewer;
mod inspector;
mod markdown;
mod prompter;
mod scrapy;
mod wry_screenshot;

use clap::{
    Arg, ColorChoice, Command,
    builder::{Styles, styling::AnsiColor},
};
use clap_complete::{Generator, Shell, generate};
use config::McatConfig;
use crossterm::tty::IsTty;
use dirs::home_dir;
use rasteroid::term_misc;
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

fn build_cli(stdin_streamed: bool) -> Command {
    let mut input_arg = Arg::new("input")
        .index(1)
        .num_args(1..)
        .help("file / dir / url");
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
    Command::new("mcat")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .color(ColorChoice::Always)
        .styles(
            Styles::styled()
                .header(AnsiColor::Green.on_default().bold())
                .literal(AnsiColor::Blue.on_default()),
        )
        .arg(input_arg)
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("the format to output")
                .value_parser(["html", "md",  "image", "video", "inline", "interactive"]),
        )
        .arg(
            Arg::new("theme")
                .long("theme")
                .short('t')
                .help("the theme to use [default: dark]")
                .value_parser(["dark", "light", "catppuccin", "nord", "monokai", "dracula", "gruvbox", "one_dark", "solarized", "tokyo_night"])
        )
        .arg(
            Arg::new("style-html")
                .long("style-html")
                .short('s')
                .help("add style to html too (when html is the output)")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("hidden")
                .long("hidden")
                .short('a')
                .help("include hidden files")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("kitty")
                .long("kitty")
                .help("makes the inline image encoded to kitty")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("iterm")
                .long("iterm")
                .help("makes the inline image encoded to iterm")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("sixel")
                .long("sixel")
                .help("makes the inline image encoded to sixel")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("ascii")
                .long("ascii")
                .help("makes the inline image encoded to ascii")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("inline")
                .short('i')
                .help("shortcut for putting --output inline")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("pager")
                .long("pager")
                .help("modify the default pager used [default: 'less -r']")
        )
        .arg(
            Arg::new("paging")
                .long("paging")
                .help("disable / enable paging forcefully [default: auto]")
                .value_parser(["never", "always", "auto"])
        )
        .arg(
            Arg::new("paging-never")
                .short('P')
                .help("shortcut for putting --paging never")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("paging-always")
                .short('p')
                .help("shortcut for putting --paging always")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("color")
                .long("color")
                .help("disable / enable ANSI formatting forcefully [default: auto]")
                .value_parser(["never", "always", "auto"])
        )
        .arg(
            Arg::new("color-never")
                .short('C')
                .help("shortcut for putting --color never")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("color-always")
                .short('c')
                .help("shortcut for putting --color always")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("horizontal")
                .long("hori")
                .action(clap::ArgAction::SetTrue)
                .help("concat images horizontal instead of vertical"))
        .arg(
            Arg::new("no-linenumbers")
                .long("no-linenumbers")
                .help("changes the format of codeblock in the markdown viewer")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("delete-all-images")
                .long("delete-images")
                .help("deletes all the images, even ones that are not in the scrollview.. currently only works in kitty")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("report")
                .long("report")
                .action(clap::ArgAction::SetTrue)
                .help("reports image / video dimensions when drawing images. along with reporting more info when not drawing images")
        )
        .arg(
            Arg::new("silent")
                .long("silent")
                .action(clap::ArgAction::SetTrue)
                .help("removes loading bars")
        )
        .arg(
            Arg::new("fetch-chromium")
                .long("fetch-chromium")
                .help("download and prepare chromium")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("fetch-ffmpeg")
                .long("fetch-ffmpeg")
                .help("download and prepare ffmpeg")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("fetch-clean")
                .long("fetch-clean")
                .help("Clean up the local binaries")
                .action(clap::ArgAction::SetTrue))
        .arg(
            Arg::new("generate-completions")
                .long("generate")
                .help("Generate shell completions")
                .value_parser(["bash", "zsh", "fish", "powershell"])
        )
        .arg(
            Arg::new("inline-options")
                .long("opts")
                .help(
                    "Options for --output inline:\n\
                     *  center=<bool>\n\
                     *  inline=<bool>\n\
                     *  width=<string>       [only for images]\n\
                     *  height=<string>      [only for images]\n\
                     *  scale=<f32>\n\
                     *  spx=<string>\n\
                     *  sc=<string>\n\
                     *  zoom=<usize>         [only for images]\n\
                     *  x=<int>              [only for images]\n\
                     *  y=<int>              [only for images]\n\
                     *  exmp: --opts 'center=false,inline=true,width=80%,height=20c,scale=0.5,spx=1920x1080,sc=100x20,zoom=2,x=16,y=8'\n"
                )
        )
        .arg(
            Arg::new("ls-options")
                .long("ls-opts")
                .help(
                    "Options for the ls command:\n\
                     *  x_padding=<string>\n\
                     *  y_padding=<string>\n\
                     *  min_width=<string>\n\
                     *  max_width=<string>\n\
                     *  height=<string>\n\
                     *  items_per_row=<usize>\n\
                     *  exmp: --ls-opts 'x_padding=4c,y_padding=2c,min_width=4c,max_width=16c,height=8%,items_per_row=12'\n"
                )
        )
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
    let _ = term_misc::init_wininfo(
        &term_misc::break_size_string(config.inline_options.spx).unwrap_or_exit(),
        &term_misc::break_size_string(config.inline_options.sc).unwrap_or_exit(),
        config.inline_options.scale,
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
        converter::lsix(
            input,
            &mut out,
            &config.ls_options,
            config.hidden,
            &config.inline_encoder,
        )
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
                path_bufs.push((path, Some("stdin input".to_string())));
                tmp_files.push(named_temp_file);
            }
            inspector::InspectedBytes::Path(path_buf) => path_bufs.push((path_buf, None)),
        };
    }
    for i in config.input.iter() {
        if i.starts_with("https://") {
            if let Ok(tmp) = scrapy::scrape_biggest_media(&i, config.silent) {
                let path = tmp.path().to_path_buf();
                tmp_files.push(tmp);
                path_bufs.push((path, Some(i.clone())));
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
                path_bufs.push((path.to_path_buf(), None));
            }
        }
    }

    // concating and printing the result
    let main_format = concater::check_unified_format(&path_bufs);
    match main_format {
        "text" => {
            let path_bufs = concater::assign_names(&path_bufs);
            let tmp = concater::concat_text(path_bufs);
            catter::cat(tmp.path(), &mut out, &config).unwrap_or_exit();
        }
        "video" => {
            match config.inline_encoder {
                rasteroid::InlineEncoder::Ascii | rasteroid::InlineEncoder::Sixel => {}
                _ => {
                    if config.is_tmux {
                        rasteroid::set_tmux_passthrough(true);
                    }
                }
            }
            if path_bufs.len() == 1 {
                catter::cat(&path_bufs[0].0, &mut out, &config).unwrap_or_exit();
            } else {
                #[allow(unused_variables)] //for lifetime
                let (dir, path) = concater::concat_video(&path_bufs).unwrap_or_exit();
                catter::cat(&path, &mut out, &config).unwrap_or_exit();
            }
        }
        "image" => {
            match config.inline_encoder {
                rasteroid::InlineEncoder::Ascii => {}
                _ => {
                    if config.is_tmux {
                        rasteroid::set_tmux_passthrough(true);
                    }
                }
            }
            if path_bufs.len() == 1 {
                catter::cat(&path_bufs[0].0, &mut out, &config).unwrap_or_exit();
            } else {
                let img = concater::concat_images(path_bufs, config.horizontal_image_stacking)
                    .unwrap_or_exit();
                catter::cat(img.path(), &mut out, &config).unwrap_or_exit();
            }
        }
        _ => {}
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

    // Print header with fancy box
    println!("┌────────────────────────────────────────────────────┐");
    println!("│               SYSTEM CAPABILITIES                  │");
    println!("├────────────────────────────────────────────────────┤");

    // Color function helpers
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

    // Print required dependencies
    println!("│ Dependencies:                                      │");
    println!(
        "│   Chromium: {:<47} │",
        format_status(is_chromium_installed)
    );
    println!("│   FFmpeg:   {:<47} │", format_status(is_ffmpeg_installed));

    // Print terminal capabilities
    println!("├────────────────────────────────────────────────────┤");
    println!("│ Terminal Graphics Support:                         │");
    println!("│   Kitty:    {:<47} │", format_capability(kitty));
    println!("│   iTerm2:   {:<47} │", format_capability(iterm));
    println!("│   Sixel:    {:<47} │", format_capability(sixel));
    println!("│   ASCII:    {:<47} │", format_capability(ascii));

    // Print terminal dimensions
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

    // Print footer
    println!("└────────────────────────────────────────────────────┘");
}
