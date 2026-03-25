pub mod html_preprocessor;
pub mod image_preprocessor;
pub mod render;
pub mod themes;
pub mod utils;

use comrak::{
    Arena, markdown_to_html_with_plugins, options, plugins::syntect::SyntectAdapterBuilder,
};
use image_preprocessor::ImagePreprocessor;
use itertools::Itertools;
use render::{AnsiContext, RESET, parse_node};
use syntect::highlighting::ThemeSet;
use themes::CustomTheme;

use crate::config::{McatConfig, Theme};
use anyhow::{Context, Result};
use std::path::Path;

pub fn md_to_ansi(
    md: &str,
    mut config: McatConfig,
    markdown_file_path: Option<&Path>,
) -> Result<String> {
    let md = html_preprocessor::process(md);

    let arena = Arena::new();
    let opts = comrak_options();
    let root = comrak::parse_document(&arena, &md, &opts);

    let padding = config.padding as usize;

    // changing to forced inline in case of images rendered
    let wininfo = config
        .wininfo
        .as_mut()
        .context("this is likely a bug, wininfo isn't set at the md_to_ansi")?;
    wininfo.needs_inline = true;
    wininfo.sc_width = wininfo.sc_width.saturating_sub((padding * 2) as u16);

    let ps = two_face::syntax::extra_newlines();
    let theme = CustomTheme::from(&config.theme);
    let image_preprocessor = ImagePreprocessor::new(root, &config, markdown_file_path)?;
    let mut ctx = AnsiContext {
        ps,
        theme,
        wininfo: &config.wininfo.unwrap(),
        hide_line_numbers: config.no_linenumbers,
        center: false,
        image_preprocessor: &image_preprocessor,
        show_frontmatter: config.header,

        blockquote_fenced_offset: None,
        is_multi_block_quote: false,
        paragraph_collecting_line: None,
        collecting_depth: 0,
        under_header: false,
        force_simple_code_block: 0,
        list_depth: 0,
    };

    let mut output = String::new();
    output.push_str(&ctx.theme.foreground.fg);
    output.push_str(&parse_node(root, &mut ctx));

    let mut res = output.replace(RESET, &format!("{RESET}{}", &ctx.theme.foreground.fg));

    // replace images
    for (_, img) in image_preprocessor.mapper {
        img.insert_into_text(&mut res);
    }

    // apply horizontal padding
    if padding > 0 {
        let pad = " ".repeat(padding);
        res = res.lines().map(|line| format!("{pad}{line}")).join("\n");
    }

    Ok(res)
}

pub fn md_to_html(markdown: &str, theme: &Theme, style: bool) -> String {
    let options = comrak_options();

    let theme = CustomTheme::from(theme);
    let mut theme_set = ThemeSet::load_defaults();
    let mut plugins = options::Plugins::default();
    theme_set
        .themes
        .insert("dark".to_string(), theme.to_syntect_theme());
    let adapter = SyntectAdapterBuilder::new()
        .theme("dark")
        .theme_set(theme_set)
        .build();
    if style {
        plugins.render.codefence_syntax_highlighter = Some(&adapter);
    }

    let full_css = match style {
        true => Some(theme.to_html_style()),
        false => None,
    };

    let html = markdown_to_html_with_plugins(markdown, &options, &plugins);
    match full_css {
        Some(css) => format!(
            r#"
<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <style>{}</style>
</head>
<body>
  {}
</body>
</html>
"#,
            css, html
        ),
        None => html,
    }
}

fn comrak_options<'a>() -> options::Options<'a> {
    let mut options = options::Options::default();
    options.extension.strikethrough = true;
    options.extension.footnotes = true;
    options.extension.front_matter_delimiter = Some("---".to_owned());
    options.extension.superscript = true;
    options.extension.tagfilter = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.description_lists = true;
    options.extension.math_code = true;
    options.extension.alerts = true;
    options.extension.wikilinks_title_after_pipe = true;
    options.extension.spoiler = true;
    options.extension.multiline_block_quotes = true;

    options.parse.smart = true;
    options.parse.relaxed_tasklist_matching = true;

    options.render.r#unsafe = true;

    options
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McatConfig;

    fn render(md: &str) -> String {
        use clap::Parser;
        let mut config = McatConfig::parse_from(["mcat", "-"]);
        config.finalize().unwrap();
        let result = md_to_ansi(md, config, None).unwrap();
        strip_ansi_escapes::strip_str(&result).to_string()
    }

    #[test]
    fn list_item_with_code_block_on_separate_lines() {
        let md = "1. Step one:\n\n        echo hello\n";
        let output = render(md);

        // The code block header (file icon + "text") must not
        // appear on the same line as the list item text.
        let step_line = output.lines().find(|l| l.contains("Step one"));
        assert!(step_line.is_some(), "should contain \'Step one\'");
        let step_line = step_line.unwrap();

        assert!(
            !step_line.contains("\u{f15c}") && !step_line.contains("text"),
            "code block header should not be on the same line as list item text, got: {:?}",
            step_line,
        );
    }

    #[test]
    fn html_tags_in_backticks_rendered_literally() {
        let md = "This has `<div>` and `<script>` in backticks.\n";
        let output = render(md);
        let lines: Vec<&str> = output.lines()
            .filter(|l| !l.trim().is_empty())
            .collect();

        // Should be a single line with tags rendered literally
        assert_eq!(
            lines.len(), 1,
            "should be one line, got {}:\n{}",
            lines.len(), output,
        );
        assert!(
            lines[0].contains("<div>"),
            "should contain literal <div>, got: {:?}",
            lines[0],
        );
        assert!(
            lines[0].contains("<script>"),
            "should contain literal <script>, got: {:?}",
            lines[0],
        );
    }
}
