pub mod html_preprocessor;
pub mod image_preprocessor;
pub mod render;
pub mod themes;
pub mod utils;

use comrak::{
    Arena, markdown_to_html_with_plugins, options, plugins::syntect::SyntectAdapterBuilder,
};
use image_preprocessor::ImagePreprocessor;
use rasteroid::term_misc::{self, break_size_string};
use render::{AnsiContext, RESET, parse_node};
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};
use themes::CustomTheme;

use crate::{UnwrapOrExit, config::McatConfig};
use std::path::Path;

pub fn md_to_ansi(md: &str, config: &McatConfig, markdown_file_path: Option<&Path>) -> String {
    let md = html_preprocessor::process(md);

    let arena = Arena::new();
    let opts = comrak_options();
    let root = comrak::parse_document(&arena, &md, &opts);

    // changing to forced inline in case of images rendered
    let _ = term_misc::init_wininfo(
        &break_size_string(&config.inline_options.spx).unwrap_or_exit(),
        &break_size_string(&config.inline_options.spx).unwrap_or_exit(),
        config.inline_options.scalex,
        config.inline_options.scaley,
        config.is_tmux,
        true,
    );

    let ps = SyntaxSet::load_defaults_newlines();
    let theme = CustomTheme::from(config.theme.as_ref());
    let image_preprocessor = ImagePreprocessor::new(root, config, markdown_file_path);
    let mut ctx = AnsiContext {
        ps,
        theme,
        hide_line_numbers: config.no_linenumbers,
        term_width: term_misc::get_wininfo().sc_width as usize,
        center: false,
        image_preprocessor: &image_preprocessor,
        show_frontmatter: config.yaml_header,

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
    res
}

pub fn md_to_html(markdown: &str, style: Option<&str>) -> String {
    let options = comrak_options();

    let theme = CustomTheme::from(style.unwrap_or_default());
    let mut theme_set = ThemeSet::load_defaults();
    let mut plugins = options::Plugins::default();
    theme_set
        .themes
        .insert("dark".to_string(), theme.to_syntect_theme());
    let adapter = SyntectAdapterBuilder::new()
        .theme("dark")
        .theme_set(theme_set)
        .build();
    if style.is_some() {
        plugins.render.codefence_syntax_highlighter = Some(&adapter);
    }

    let full_css = match style {
        Some(_) => Some(theme.to_html_style()),
        None => None,
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
        let config = McatConfig::default();
        let result = md_to_ansi(md, &config, None);
        strip_ansi_escapes::strip_str(&result).to_string()
    }

    #[test]
    fn list_item_with_code_block_on_separate_lines() {
        let md = "1. Step one:\n\n        echo hello\n";
        let output = render(md);

        // The code block header (file icon + "text") must not appear on the
        // same line as the list item text. Before the fix, collect() joined
        // block-level children with no separator, so "Step one:" and the code
        // block header ended up on one line.
        let step_line = output.lines().find(|l| l.contains("Step one"));
        assert!(step_line.is_some(), "should contain 'Step one'");
        let step_line = step_line.unwrap();

        // \u{f15c} is the file icon used in code block headers
        assert!(
            !step_line.contains("\u{f15c}") && !step_line.contains("text"),
            "code block header should not be on the same line as list item text, got: {:?}",
            step_line,
        );
    }

    #[test]
    fn table_that_fits_is_not_modified() {
        let md = "| A | B |\n| - | - |\n| x | y |\n";
        let output = render(md);
        // Both cells on one line, no wrapping
        let data_line = output.lines().find(|l| l.contains("x") && l.contains("y"));
        assert!(data_line.is_some(), "should have a line with both x and y");
    }

    #[test]
    fn table_wraps_long_cell() {
        // The cell must exceed the actual terminal width to trigger wrapping,
        // so use a very long string that exceeds any reasonable terminal.
        let long = "word ".repeat(100); // 500 chars
        let md = format!("| A | B |\n| - | - |\n| short | {} |\n", long);
        let output = render(&md);
        // "short" and all 500 chars of the long text should NOT be on one line
        let short_line = output.lines().find(|l| l.contains("short"));
        assert!(short_line.is_some());
        let short_line = short_line.unwrap();
        assert!(
            short_line.len() < 500,
            "long cell should be wrapped, not all on one line (len={})",
            short_line.len(),
        );
    }

    #[test]
    fn table_narrow_columns_keep_natural_width() {
        // The "Type" column is narrow (max 6 chars). It should not be shrunk
        // to the point where "build:" wraps.
        let md = concat!(
            "| Type | Purpose |\n",
            "| - | - |\n",
            "| fix: | Short description |\n",
            "| build: | For cases where your change is to sources in the build directory or to the checker script |\n",
        );
        let output = render(md);
        // "build:" must appear as a complete string on one line, not split
        let has_intact_build = output.lines().any(|l| l.contains("build:"));
        assert!(
            has_intact_build,
            "build: should appear intact on a single line, not split across lines.\nOutput:\n{}",
            output,
        );
    }
}
