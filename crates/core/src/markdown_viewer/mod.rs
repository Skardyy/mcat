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
use utils::limit_newlines;

use crate::{UnwrapOrExit, config::McatConfig};
use std::path::Path;

pub fn md_to_ansi(md: &str, config: &McatConfig, markdown_file_path: Option<&Path>) -> String {
    let res = &html_preprocessor::process(md);
    let md = &res.content;

    let arena = Arena::new();
    let opts = comrak_options();
    let root = comrak::parse_document(&arena, md, &opts);

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
        centered_lines: &res.centered_lines,
        term_width: term_misc::get_wininfo().sc_width as usize,
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
    output.push_str(&parse_node(root, &mut ctx).trim_matches('\n'));

    // making sure its wrapped to fit into the termianl size
    let lines: Vec<String> = textwrap::wrap(&output, term_misc::get_wininfo().sc_width as usize)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect();
    let res = lines
        .join("\n")
        .replace(RESET, &format!("{RESET}{}", &ctx.theme.foreground.fg));

    // force at max 2 \n at a row (we're adding newlines based on sourcepos)
    let mut res = limit_newlines(&res).to_string();

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
