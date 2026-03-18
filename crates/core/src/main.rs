mod config;

use clap::{Command, Parser};
use clap_complete::{Generator, Shell, generate};
use config::McatConfig;
use crossterm::tty::IsTty;
use dirs::home_dir;
use ffmpeg_sidecar::Result;
use rasteroid::{InlineEncoder, term_misc};
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

fn main() -> Result<()> {
    let stdin_streamed = !std::io::stdin().is_tty();
    let stdout = std::io::stdout().lock();
    let mut out = BufWriter::new(stdout);

    let mut config = McatConfig::parse();

    // setup rasteroid, tmp should be changed later.
    let env = term_misc::EnvIdentifiers::new();
    let is_tmux = env.is_tmux();
    let encoder = match config.image_protocol {
        config::ImageProtocol::Auto => InlineEncoder::auto_detect(&env),
        config::ImageProtocol::Kitty => InlineEncoder::Kitty,
        config::ImageProtocol::Iterm => InlineEncoder::Iterm,
        config::ImageProtocol::Sixel => InlineEncoder::Sixel,
        config::ImageProtocol::Ascii => InlineEncoder::Ascii,
    };

    // fn and leave
    if let Some(shell) = config.generate {
        match shell {
            config::Shell::Bash => todo!(),
            config::Shell::Zsh => todo!(),
            config::Shell::Fish => todo!(),
            config::Shell::PowerShell => todo!(),
        }
    }

    Ok(())
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~") {
        if let Some(home) = home_dir() {
            return path.replace("~", &home.to_string_lossy().into_owned());
        }
    }
    path.to_string()
}
