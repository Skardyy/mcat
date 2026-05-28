pub mod html_preprocessor;
pub mod image_preprocessor;
pub mod render;
pub mod utils;

use crate::{markdown_viewer::render::build_toc, themes::CustomTheme};
use comrak::{
    Arena, format_html_with_plugins,
    nodes::{AstNode, NodeHtmlBlock, NodeValue},
    options,
    plugins::syntect::SyntectAdapterBuilder,
};
use image_preprocessor::ImagePreprocessor;
use itertools::Itertools;
use render::{AnsiContext, RESET, parse_node};
use syntect::highlighting::ThemeSet;

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
    let theme = config.theme.to_custom();
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
        collecting_depth: 0,
        under_header: false,
        force_simple_code_block: 0,
        list_depth: 0,
    };

    let toc = if config.toc {
        build_toc(root, &mut ctx)
    } else {
        String::new()
    };

    let mut output = String::new();
    if !toc.is_empty() {
        output.push_str(&toc);
        output.push_str("\n\n");
    }
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

    if !res.ends_with('\n') {
        res.push('\n');
    }

    Ok(res)
}

pub fn md_to_html(markdown: &str, theme: &Theme, style: bool) -> String {
    let options = comrak_options();

    let arena = Arena::new();
    let root = comrak::parse_document(&arena, markdown, &options);
    mutate_mermaid_blocks(root, theme);

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

    let mut html = String::new();
    format_html_with_plugins(root, &options, &mut html, &plugins).unwrap();
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

fn mutate_mermaid_blocks<'a>(root: &'a AstNode<'a>, theme: &Theme) {
    for node in root.descendants() {
        let literal = {
            let data = node.data.borrow();
            if let NodeValue::CodeBlock(ref block) = data.value {
                if matches!(block.info.as_str(), "mermaid" | "mmd") {
                    Some(block.literal.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(source) = literal {
            if let Some(svg) = render_mermaid_svg(&source, theme) {
                let html = format!("<div class=\"mcat-mermaid\">\n{}\n</div>\n", svg.trim_end());
                node.data.borrow_mut().value = NodeValue::HtmlBlock(NodeHtmlBlock {
                    block_type: 6, // `block_type` follows CommonMark HTML block type 6 semantics; comrak renderers ignore it here since we are mutating post-parse.
                    literal: html,
                });
            }
        }
    }
}

fn render_mermaid_svg(source: &str, theme: &Theme) -> Option<String> {
    let mut opts = mermaid_rs_renderer::RenderOptions::modern();
    opts.theme = theme.to_custom().to_mermaid_theme();
    mermaid_rs_renderer::render_with_options(source, opts).ok()
}

fn comrak_options<'a>() -> options::Options<'a> {
    let mut options = options::Options::default();

    options.extension.strikethrough = true;
    options.extension.footnotes = true;
    options.extension.front_matter_delimiter = Some("---".to_owned());
    options.extension.superscript = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.description_lists = true;
    options.extension.math_code = true;
    options.extension.math_dollars = true;
    options.extension.alerts = true;
    options.extension.wikilinks_title_after_pipe = true;
    options.extension.spoiler = true;
    options.extension.multiline_block_quotes = true;
    options.extension.block_directive = true;
    options.extension.highlight = true;
    options.parse.smart = true;
    options.parse.relaxed_tasklist_matching = true;
    options.extension.shortcodes = true;

    options.extension.tagfilter = true;
    options.render.r#unsafe = true;

    options
}

#[cfg(test)]
mod tests {
    use super::md_to_html;
    use crate::config::Theme;

    #[test]
    fn md_to_html_renders_mermaid_fence_as_svg() {
        let md = "```mermaid\ngraph TD\nA-->B\n```";
        let html = md_to_html(md, &Theme::Github, false);

        assert!(html.contains("<svg"));
        assert!(!html.contains("language-mermaid"));
    }

    #[test]
    fn md_to_html_keeps_non_mermaid_fence_untouched() {
        let md = "```rust\nfn main() {}\n```";
        let html = md_to_html(md, &Theme::Github, false);

        assert!(html.contains("language-rust"));
        assert!(!html.contains("<div class=\"mcat-mermaid\">"));
    }
}
