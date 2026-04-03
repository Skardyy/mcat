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
        wininfo: config.wininfo.unwrap(),
        hide_line_numbers: config.no_linenumbers,
        center: false,
        image_preprocessor,
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
    for (_, img) in ctx.image_preprocessor.mapper {
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
    use unicode_width::UnicodeWidthStr;

    fn render(md: &str) -> String {
        use clap::Parser;
        let mut config = McatConfig::parse_from(["mcat", "-"]);
        config.finalize().unwrap();
        let result = md_to_ansi(md, config, None).unwrap();
        strip_ansi_escapes::strip_str(&result).to_string()
    }

    fn leading_spaces(s: &str) -> usize {
        s.len() - s.trim_start_matches(' ').len()
    }

    // ---- Hanging indent basics ----

    #[test]
    fn numbered_list_hanging_indent() {
        // "1. " = 3 columns; continuations must indent >= 3
        let long = "word ".repeat(100);
        let md = format!("1. {}\n", long);
        let output = render(&md);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 2, "text should wrap");
        for line in &lines[1..] {
            assert!(
                leading_spaces(line) >= 3,
                "continuation should have >= 3 spaces, got {}: {:?}",
                leading_spaces(line), line,
            );
        }
    }

    #[test]
    fn numbered_list_double_digit_indent() {
        // "10. " = 4 columns
        let long = "word ".repeat(100);
        let md = format!("10. {}\n", long);
        let output = render(&md);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 2, "text should wrap");
        for line in &lines[1..] {
            assert!(
                leading_spaces(line) >= 4,
                "continuation should have >= 4 spaces, got {}: {:?}",
                leading_spaces(line), line,
            );
        }
    }

    #[test]
    fn bullet_list_hanging_indent() {
        let long = "word ".repeat(100);
        let md = format!("- {}\n", long);
        let output = render(&md);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() >= 2, "text should wrap");
        // bullet + space = 2 columns
        for line in &lines[1..] {
            assert!(
                leading_spaces(line) >= 2,
                "continuation should have >= 2 spaces, got {}: {:?}",
                leading_spaces(line), line,
            );
        }
    }

    #[test]
    fn short_list_item_no_wrapping() {
        let md = "1. Short item.\n";
        let output = render(md);
        let lines: Vec<&str> = output.lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        assert_eq!(lines.len(), 1, "short item should be one line");
    }

    // ---- Multi-paragraph items ----

    #[test]
    fn multi_paragraph_item_second_para_indented() {
        let long = "word ".repeat(100);
        let md = format!("1. First paragraph.\n\n   {}\n", long);
        let output = render(&md);
        let lines: Vec<&str> = output.lines()
            .filter(|l| !l.trim().is_empty()
                && !l.contains("First"))
            .collect();
        assert!(!lines.is_empty(),
            "should have second paragraph content");
        for line in &lines {
            assert!(
                leading_spaces(line) >= 3,
                "second para should be indented >= 3, got {}: {:?}",
                leading_spaces(line), line,
            );
        }
    }

    // ---- Nested lists ----

    #[test]
    fn nested_list_is_indented() {
        let md = "- Parent item.\n\n  - Nested item.\n";
        let output = render(&md);
        let nested = output.lines().find(|l| l.contains("Nested"));
        assert!(nested.is_some(), "should contain nested item");
        assert!(
            leading_spaces(nested.unwrap()) >= 4,
            "nested item should be indented >= 4, got {}: {:?}",
            leading_spaces(nested.unwrap()), nested.unwrap(),
        );
    }

    #[test]
    fn nested_list_wrapping_fits_terminal() {
        // Nested item with long text must not overflow
        let long = "word ".repeat(100);
        let md = format!("- Parent.\n\n  - {}\n", long);
        let output = render(&md);
        for line in output.lines() {
            assert!(
                line.width() < 500,
                "line should be wrapped (width={}): {:?}",
                line.width(), line,
            );
        }
    }
}
