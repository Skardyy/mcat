mod catter;
mod cdp;
mod config;
mod fetch_manager;
mod image_viewer;
mod lsix;
mod markdown_viewer;
mod mcat_file;
mod prompter;
mod scrapy;

use anyhow::{Context, Result};
use clap::{Command, CommandFactory, Parser};
use clap_complete::{Generator, generate};
use config::McatConfig;
use crossterm::tty::IsTty;
use dirs::home_dir;
use scrapy::MediaScrapeOptions;
use std::{
    io::{BufWriter, Read},
    path::Path,
};
use tracing_subscriber::EnvFilter;

use crate::mcat_file::McatFile;

fn print_completions<G: Generator>(gene: G, cmd: &mut Command) {
    generate(
        gene,
        cmd,
        cmd.get_name().to_string(),
        &mut std::io::stdout(),
    );
}

fn main() -> Result<()> {
    let stdin_streamed = !std::io::stdin().is_tty();
    if stdin_streamed {
        unsafe { std::env::set_var("MCAT_STDIN_PIPED", "true") };
    }

    let stdout = std::io::stdout().lock();
    let mut out = BufWriter::new(stdout);

    let mut config = McatConfig::parse();

    if config.verbose {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::new("mcat=debug"))
            .with_writer(std::io::stderr)
            .init();
    }

    config.finalize()?;

    // fn and leave
    let mut fn_and_leave = false;
    if config.fetch_chromium {
        fn_and_leave = true;
        fetch_manager::fetch_chromium()?;
    }
    if config.fetch_ffmpeg {
        fn_and_leave = true;
        fetch_manager::fetch_ffmpeg()?;
    }
    if config.fetch_clean {
        fn_and_leave = true;
        fetch_manager::clean()?;
    }
    if config.report {
        fn_and_leave = true;
        report_full(&config)?;
    }
    if let Some(shell) = config.generate {
        fn_and_leave = true;
        let mut cmd = McatConfig::command();
        print_completions(shell, &mut cmd);
    }
    if fn_and_leave {
        return Ok(());
    }

    if config
        .wininfo
        .as_ref()
        .context("this is likely a bug, wininfo is None")?
        .is_tmux
    {
        rasteroid::set_tmux_passthrough(true);
    }

    // if ls
    if config
        .input
        .first()
        .is_some_and(|v| v.to_lowercase() == "ls")
    {
        let d = ".".to_string();
        let input = config.input.get(1).cloned().unwrap_or(d);
        lsix::lsix(input, &mut out, config)?;
        return Ok(());
    }

    let mut files: Vec<McatFile> = Vec::new();

    // if stdin is streamed into
    if stdin_streamed && config.input.is_empty() {
        let mut buffer = Vec::new();
        std::io::stdin().read_to_end(&mut buffer)?;

        let file = McatFile::from_bytes(buffer, None)?;
        files.push(file);
    }

    let scraper_opts = MediaScrapeOptions::default();
    for i in config.input.iter() {
        if i.starts_with("https://") || i.starts_with("http://") {
            if let Ok(f) = scrapy::scrape_biggest_media(i, &scraper_opts, config.bar.as_ref()) {
                files.push(f);
            } else {
                eprintln!("{} didn't contain any supported media", i);
            }
        } else {
            let i = expand_tilde(i);
            let path = Path::new(&i);
            if !path.exists() {
                eprintln!("{} doesn't exists", path.display());
                std::process::exit(1);
            }

            if path.is_dir() {
                let mut selected_files = prompter::prompt_for_files(path, config.hidden)?;
                selected_files.sort();
                let new_files = selected_files
                    .iter()
                    .map(McatFile::from_path)
                    .collect::<Result<Vec<_>, _>>()?;
                files.extend(new_files);
            } else {
                files.push(McatFile::from_path(path)?);
            }
        }
    }

    catter::cat(files, &mut out, &config)?;

    Ok(())
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~")
        && let Some(home) = home_dir()
    {
        return path.replace("~", &home.to_string_lossy());
    }
    path.to_string()
}

fn report_full(config: &McatConfig) -> Result<()> {
    let is_chromium_installed = fetch_manager::is_chromium_installed();
    let is_ffmpeg_installed = fetch_manager::is_ffmpeg_installed();
    let env = config
        .env_id
        .as_ref()
        .context("this is likely a bug, env id is None")?;
    let kitty = rasteroid::kitty_encoder::is_kitty_capable(env);
    let iterm = rasteroid::iterm_encoder::is_iterm_capable(env);
    let sixel = rasteroid::sixel_encoder::is_sixel_capable(env);
    let ascii = true; //not sure what doesn't support it
    let wininfo = config
        .wininfo
        .as_ref()
        .context("this is likely a bug, wininfo is None")?;
    let tmux = wininfo.is_tmux;
    let inline = wininfo.needs_inline;
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
    println!("│   Width:          {:<32} │", wininfo.sc_width);
    println!("│   Height:         {:<32} │", wininfo.sc_height);
    println!("│   Pixel Width:    {:<32} │", wininfo.spx_width);
    println!("│   Pixel Height:   {:<32} │", wininfo.spx_height);

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

    Ok(())
}
