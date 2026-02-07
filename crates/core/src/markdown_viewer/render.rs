use comrak::nodes::{
    AstNode, NodeAlert, NodeCode, NodeCodeBlock, NodeHeading, NodeHtmlBlock, NodeLink, NodeMath,
    NodeValue, NodeWikiLink,
};
use itertools::Itertools;
use regex::Regex;
use syntect::parsing::SyntaxSet;

use crate::markdown_viewer::utils::{get_title_box, string_len, trim_ansi_string, wrap_lines};

use super::{
    image_preprocessor::ImagePreprocessor,
    themes::CustomTheme,
    utils::{
        format_code_box, format_code_full, format_code_simple, format_tb, limit_newlines,
        wrap_char_based,
    },
};

pub const RESET: &str = "\x1B[0m";
pub const BOLD: &str = "\x1B[1m";
const ITALIC: &str = "\x1B[3m";
const UNDERLINE: &str = "\x1B[4m";
const STRIKETHROUGH: &str = "\x1B[9m";
const FAINT: &str = "\x1b[2m";
const NORMAL: &str = "\x1B[22m";
const ITALIC_OFF: &str = "\x1B[23m";
const STRIKETHROUGH_OFF: &str = "\x1B[29m";
pub const UNDERLINE_OFF: &str = "\x1B[24m";
const INDENT: usize = 2;

pub struct AnsiContext<'a> {
    pub ps: SyntaxSet,
    pub theme: CustomTheme,
    pub hide_line_numbers: bool,
    pub show_frontmatter: bool,
    pub centered_lines: &'a [usize],
    pub term_width: usize,
    pub image_preprocessor: &'a ImagePreprocessor,

    pub blockquote_fenced_offset: Option<usize>,
    pub is_multi_block_quote: bool,
    pub paragraph_collecting_line: Option<usize>,
    pub collecting_depth: usize,
    pub under_header: bool,
    pub force_simple_code_block: usize,
    pub list_depth: usize,
}

impl<'a> AnsiContext<'a> {
    pub fn should_indent(&self) -> bool {
        // root level element, and under an header
        self.under_header && self.collecting_depth == 0
    }
}

fn collect<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    ctx.collecting_depth += 1;
    let content = node
        .children()
        .map(|child| parse_node(child, ctx))
        .collect();
    ctx.collecting_depth -= 1;

    content
}

pub fn parse_node<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let data = node.data.borrow();
    let mut buffer = match ctx.paragraph_collecting_line {
        Some(line) => {
            if data.sourcepos.start.line != line {
                ctx.paragraph_collecting_line = Some(data.sourcepos.start.line);
                "\n"
            } else {
                ""
            }
        }
        None => "",
    }
    .to_owned();

    let content = match &data.value {
        NodeValue::Document => render_document(node, ctx),
        NodeValue::FrontMatter(_) => render_front_matter(node, ctx),
        NodeValue::BlockQuote => render_block_quote(node, ctx),
        NodeValue::List(_) => render_list(node, ctx),
        NodeValue::Item(_) => render_item(node, ctx),
        NodeValue::CodeBlock(_) => render_code_block(node, ctx),
        NodeValue::HtmlBlock(_) => render_html_block(node, ctx),
        NodeValue::Paragraph => render_paragraph(node, ctx),
        NodeValue::Heading(_) => render_heading(node, ctx),
        NodeValue::ThematicBreak => render_thematic_break(node, ctx),
        NodeValue::Table(_) => render_table(node, ctx),
        NodeValue::Strong => render_strong(node, ctx),
        NodeValue::Emph => render_emph(node, ctx),
        NodeValue::Strikethrough => render_strikethrough(node, ctx),
        NodeValue::Link(_) => render_link(node, ctx),
        NodeValue::Image(_) => render_image(node, ctx),
        NodeValue::Code(_) => render_code(node, ctx),
        NodeValue::TaskItem(_) => render_task_item(node, ctx),
        NodeValue::HtmlInline(_) => render_html_inline(node, ctx),
        NodeValue::Superscript => render_superscript(node, ctx),
        NodeValue::MultilineBlockQuote(_) => render_multiline_block_quote(node, ctx),
        NodeValue::WikiLink(_) => render_wiki_link(node, ctx),
        NodeValue::SpoileredText => render_spoilered_text(node, ctx),
        NodeValue::Alert(_) => render_alert(node, ctx),
        NodeValue::FootnoteDefinition(_) => render_footnote_def(node, ctx),
        NodeValue::FootnoteReference(_) => render_footnote_ref(node, ctx),
        // leave as is
        NodeValue::Text(literal) => literal.to_owned(),
        NodeValue::Raw(literal) => literal.to_owned(),
        NodeValue::Math(NodeMath { literal, .. }) => literal.to_owned(),
        NodeValue::SoftBreak => " ".to_owned(),
        // Ignore
        NodeValue::LineBreak => String::new(),
        NodeValue::TableRow(_) => String::new(),
        NodeValue::TableCell => String::new(),
        NodeValue::Escaped => String::new(),
        NodeValue::DescriptionList => String::new(),
        NodeValue::DescriptionItem(_) => String::new(),
        NodeValue::DescriptionTerm => String::new(),
        NodeValue::DescriptionDetails => String::new(),
        NodeValue::EscapedTag(_) => String::new(),
        NodeValue::Underline => String::new(),
        NodeValue::Subscript => String::new(),
    };
    buffer.push_str(&content);

    buffer
}

fn render_document<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    node.children()
        .map(|child| parse_node(child, ctx))
        .collect()
}

fn render_front_matter<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::FrontMatter(ref literal) = node.data.borrow().value else {
        panic!()
    };

    if !ctx.show_frontmatter {
        return String::new();
    }

    let content = literal
        .trim()
        .strip_prefix("---")
        .unwrap_or(literal.trim())
        .strip_suffix("---")
        .unwrap_or(literal.trim())
        .trim();

    format_code_box(content, "yaml", "Document Metadata", ctx)
}

fn render_footnote_def<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::FootnoteDefinition(ref item) = node.data.borrow().value else {
        panic!()
    };

    let content = collect(node, ctx);
    let cyan = &ctx.theme.cyan.fg;
    let content = format!("{cyan}[{}]{RESET}: {content}", item.name);
    let sps = node.data.borrow().sourcepos;

    let suffix = if ctx.collecting_depth == 0 {
        "\n\n"
    } else {
        "\n"
    };

    if ctx.centered_lines.contains(&sps.start.line) {
        content
            .lines()
            .map(|line| {
                let line = trim_ansi_string(line.into());
                let le = string_len(&line);
                // 1 based index
                let offset = sps.start.column.saturating_sub(1);
                let offset = (ctx.term_width - offset)
                    .saturating_sub(le)
                    .saturating_div(2);
                format!("{}{line}", " ".repeat(offset))
            })
            .join("\n")
            + suffix
    } else {
        content
            .lines()
            .map(|line| {
                if ctx.should_indent() {
                    wrap_lines(&line, false, INDENT, "", "")
                } else {
                    line.into()
                }
            })
            .join("\n")
            + suffix
    }
}

fn render_footnote_ref<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::FootnoteReference(ref item) = node.data.borrow().value else {
        panic!()
    };

    let cyan = &ctx.theme.cyan.fg;
    format!("{cyan}[{}]{RESET}", item.name)
}

fn render_block_quote<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let guide = ctx.theme.guide.fg.clone();
    let comment = ctx.theme.comment.fg.clone();
    ctx.force_simple_code_block += 1;
    let content = collect(node, ctx).replace(RESET, &format!("{RESET}{comment}"));
    let content = limit_newlines(&content);
    ctx.force_simple_code_block -= 1;
    let content = content.trim_matches('\n');
    let fence_offset = ctx.blockquote_fenced_offset.unwrap_or_default();

    let content = content
        .lines()
        .map(|line| {
            let offset = " ".repeat(fence_offset + 1);
            format!("{guide}▌{offset}{comment}{line}{RESET}")
        })
        .join("\n");

    let content = if ctx.should_indent() {
        wrap_char_based(&content, '▌', INDENT, "", "")
    } else {
        content.to_owned()
    };

    format!("\n\n{content}\n\n")
}

fn render_list<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::List(..) = node.data.borrow().value else {
        panic!()
    };

    ctx.list_depth += 1;
    let content = collect(node, ctx);
    ctx.list_depth -= 1;
    let content = if ctx.should_indent() {
        wrap_lines(&content, true, INDENT, "", "  ") // 2 space extra because of the bullet
    } else {
        content
    };

    if ctx.is_multi_block_quote {
        content
    } else {
        content + "\n"
    }
}

fn render_item<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Item(ref item) = node.data.borrow().value else {
        panic!()
    };

    let yellow = ctx.theme.yellow.fg.clone();
    let content = collect(node, ctx);
    let content = content.trim();
    let depth = ctx.list_depth - 1;

    let bullets = ["●", "○", "◆", "◇"];
    let bullet = match item.list_type {
        comrak::nodes::ListType::Bullet => bullets[depth % 4],
        comrak::nodes::ListType::Ordered => &format!("{}.", item.start),
    };

    format!(
        "{}{yellow}{bullet}{RESET} {content}\n",
        " ".repeat(depth * 4)
    )
}

fn render_task_item<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::TaskItem(ref task) = node.data.borrow().value else {
        panic!()
    };

    let offset = " ".repeat(node.data.borrow().sourcepos.start.column - 1);
    let content = collect(node, ctx);
    let content = content.trim();
    let (icon, colour) = match task.map(|c| c.to_ascii_lowercase()) {
        Some('x') => ("󰱒", &ctx.theme.green.fg),
        Some('-') | Some('~') => ("󰛲", &ctx.theme.yellow.fg),
        Some('!') => ("󰳤", &ctx.theme.red.fg),
        _ => ("󰄱", &ctx.theme.red.fg),
    };

    format!("{offset}{colour}{icon}{RESET}  {content}\n")
}

fn render_code_block<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::CodeBlock(NodeCodeBlock {
        ref literal,
        ref info,
        ..
    }) = node.data.borrow().value
    else {
        panic!()
    };

    let info = if info.trim().is_empty() { "text" } else { info };

    // force_simple_code_block is a number because it may be recursive
    if info == "file-tree" {
        render_file_tree(literal, ctx)
    } else if literal.lines().count() <= 10
        || ctx.force_simple_code_block > 0
        || ctx.hide_line_numbers
    {
        let indent = if ctx.should_indent() { INDENT } else { 0 };
        format_code_simple(literal, info, ctx, indent)
    } else {
        format_code_full(literal, info, ctx)
    }
}

fn render_file_tree(tree: &str, ctx: &mut AnsiContext) -> String {
    let tree_chars = Regex::new(r"[│├└─]").unwrap();
    let folders = Regex::new(r"([a-zA-Z0-9_\-]+/)").unwrap();
    let files = Regex::new(r"([a-zA-Z0-9_\-]+\.[a-zA-Z0-9]+)").unwrap();

    let tree_char_color = &ctx.theme.guide.fg;
    let folder_color = &ctx.theme.blue.fg;
    let file_color = &ctx.theme.foreground.fg;

    let mut result = tree.to_string();

    result = tree_chars
        .replace_all(&result, |caps: &regex::Captures| {
            format!("{tree_char_color}{}{RESET}", &caps[0])
        })
        .to_string();

    result = folders
        .replace_all(&result, |caps: &regex::Captures| {
            format!("{folder_color}{}{RESET}", &caps[1])
        })
        .to_string();

    result = files
        .replace_all(&result, |caps: &regex::Captures| {
            format!("{file_color}{}{RESET}", &caps[1])
        })
        .to_string();

    result
}

fn render_html_block<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::HtmlBlock(NodeHtmlBlock { ref literal, .. }) = node.data.borrow().value else {
        panic!()
    };

    if let Some(title) = get_title_box(literal) {
        let text_size = string_len(title);
        let border_width = text_size + 4;
        let center_padding = (ctx.term_width - border_width) / 2;

        let fg_yellow = ctx.theme.yellow.fg.clone();
        let border_line = "─".repeat(border_width);
        let spaces = " ".repeat(center_padding);

        return format!(
            "{spaces}┌{border_line}┐\n{spaces}│  {fg_yellow}{BOLD}{title}{RESET}  │\n{spaces}└{border_line}┘\n"
        );
    }

    let sps = node.data.borrow().sourcepos;
    if literal.contains("<!--HR-->") {
        return format_tb(ctx, sps.start.column);
    }

    let comment = &ctx.theme.comment.fg;
    let result = literal
        .lines()
        .map(|line| format!("{comment}{line}{RESET}"))
        .join("\n");
    let result = wrap_lines(&result, true, INDENT, "", "");
    format!("\n\n{result}\n\n")
}

fn render_paragraph<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let sps = node.data.borrow().sourcepos;
    ctx.paragraph_collecting_line = Some(sps.start.line);
    let lines = collect(node, ctx);
    ctx.paragraph_collecting_line = None;

    let suffix = if ctx.collecting_depth == 0 {
        "\n\n"
    } else {
        "\n"
    };

    if ctx.centered_lines.contains(&sps.start.line) {
        lines
            .lines()
            .map(|line| {
                let line = trim_ansi_string(line.into());
                let le = string_len(&line);
                // 1 based index
                let offset = sps.start.column.saturating_sub(1);
                let offset = (ctx.term_width - offset)
                    .saturating_sub(le)
                    .saturating_div(2);
                format!("{}{line}", " ".repeat(offset))
            })
            .join("\n")
            + suffix
    } else {
        lines
            .lines()
            .map(|line| {
                if ctx.should_indent() {
                    wrap_lines(&line, false, INDENT, "", "")
                } else {
                    line.into()
                }
            })
            .join("\n")
            + suffix
    }
}

fn render_heading<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Heading(NodeHeading { level, .. }) = node.data.borrow().value else {
        panic!()
    };

    ctx.under_header = true;
    let content = collect(node, ctx);
    let content = content.trim();
    let content = match level {
        1 => format!(" 󰎤 {content}"),
        2 => format!(" 󰎧 {content}"),
        3 => format!(" 󰎬 {content}"),
        4 => format!(" 󰎮 {content}"),
        5 => format!(" 󰎰 {content}"),
        6 => format!(" 󰎵 {content}"),
        _ => unreachable!(),
    };
    let bg = &ctx.theme.keyword_bg.bg;
    let main_color = &ctx.theme.keyword.fg;
    let content = content.replace(RESET, &format!("{RESET}{bg}"));
    let sps = &node.data.borrow().sourcepos;

    let mut header = if !ctx.centered_lines.contains(&sps.start.line) {
        let padding = " ".repeat(
            ctx.term_width
                .saturating_sub(string_len(&content) as usize)
                .into(),
        );
        format!("{main_color}{bg}{content}{padding}{RESET}")
    } else {
        // center here
        let le = string_len(&content);
        let left_space = ctx.term_width.saturating_sub(le);
        let padding_left = left_space.saturating_div(2);
        let padding_rigth = left_space - padding_left;
        format!(
            "{main_color}{bg}{}{content}{}{RESET}",
            " ".repeat(padding_left),
            " ".repeat(padding_rigth)
        )
    };
    header.push_str("\n\n");
    if sps.start.line != 1 {
        format!("\n\n{header}")
    } else {
        header
    }
}

fn render_thematic_break<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let offset = node.data.borrow().sourcepos.start.column;
    let offset = if ctx.should_indent() {
        offset
    } else {
        offset + INDENT
    };
    format_tb(ctx, offset) + "\n"
}

fn render_table<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Table(ref table) = node.data.borrow().value else {
        panic!()
    };

    let alignments = &table.alignments;
    let mut rows: Vec<Vec<Vec<String>>> = Vec::new();
    let mut row_heights: Vec<usize> = Vec::new();

    // collect all cell contents and calculate row heights
    for child in node.children() {
        let mut row_cells: Vec<Vec<String>> = Vec::new();
        let mut max_lines_in_row = 1;

        for cell_node in child.children() {
            let cell_content = collect(cell_node, ctx);
            let cell_lines: Vec<String> =
                cell_content.lines().map(|s| s.trim().to_string()).collect();
            max_lines_in_row = max_lines_in_row.max(cell_lines.len());
            row_cells.push(cell_lines);
        }

        rows.push(row_cells);
        row_heights.push(max_lines_in_row);
    }

    // Calculate column widths based on the longest line in any cell of the column
    let mut column_widths: Vec<usize> = vec![0; alignments.len()];
    for row in &rows {
        for (i, cell) in row.iter().enumerate() {
            let max_width_in_cell = cell.iter().map(|line| string_len(line)).max().unwrap_or(0);
            if max_width_in_cell > column_widths[i] {
                column_widths[i] = max_width_in_cell;
            }
        }
    }

    let color = &ctx.theme.border.fg;
    let header_color = &ctx.theme.yellow.fg;
    let mut result = String::new();
    let is_only_headers = rows.len() == 1;

    if !rows.is_empty() {
        let cols = column_widths.len();

        let build_line = |left: &str, mid: &str, right: &str, fill: &str| -> String {
            let mut line = String::new();
            line.push_str(color);
            line.push_str(left);
            for (i, &width) in column_widths.iter().enumerate() {
                line.push_str(&fill.repeat(width + 2));
                if i < cols - 1 {
                    line.push_str(mid);
                }
            }
            line.push_str(right);
            line.push_str(RESET);
            line
        };

        let top_border = build_line("╭", "┬", "╮", "─");
        let bottom_border = build_line("╰", "┴", "╯", "─");
        let middle_border = if is_only_headers {
            bottom_border.clone()
        } else {
            build_line("├", "┼", "┤", "─")
        };

        result.push_str(&top_border);
        result.push('\n');

        for (row_idx, row) in rows.iter().enumerate() {
            let text_color = if row_idx == 0 { header_color } else { "" };
            let row_height = row_heights[row_idx];

            for line_idx in 0..row_height {
                result.push_str(&format!("{color}│{RESET}"));

                for (col_idx, cell) in row.iter().enumerate() {
                    let width = column_widths[col_idx];
                    let cell_line = cell.get(line_idx).map(|s| s.as_str()).unwrap_or("");

                    let padding = width.saturating_sub(string_len(cell_line));
                    let (left_pad, right_pad) = if row_idx == 0 {
                        // Header row - always center
                        (padding / 2, padding - (padding / 2))
                    } else {
                        match alignments[col_idx] {
                            comrak::nodes::TableAlignment::Center => {
                                (padding / 2, padding - (padding / 2))
                            }
                            comrak::nodes::TableAlignment::Right => (padding, 0),
                            _ => (0, padding),
                        }
                    };

                    result.push_str(&format!(
                        " {}{text_color}{}{} {color}│{RESET}",
                        " ".repeat(left_pad),
                        cell_line,
                        " ".repeat(right_pad)
                    ));
                }
                result.push('\n');
            }

            if row_idx == 0 {
                result.push_str(&middle_border);
                result.push('\n');
            }
        }

        if !is_only_headers {
            result.push_str(&bottom_border);
        }
    }

    let sps = node.data.borrow().sourcepos;
    let result = if ctx.centered_lines.contains(&sps.start.line) {
        let le = string_len(result.lines().nth(1).unwrap_or_default());
        let offset = sps.start.column.saturating_sub(1);
        let offset = (ctx.term_width - offset)
            .saturating_sub(le)
            .saturating_div(2);

        result
            .lines()
            .map(|line| format!("{}{line}", " ".repeat(offset)))
            .join("\n")
    } else if ctx.should_indent() {
        result
            .lines()
            .map(|line| format!("{}{line}", " ".repeat(INDENT)))
            .join("\n")
    } else {
        result
    };

    format!("\n\n{result}\n\n")
}

fn render_strong<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx);
    format!("{BOLD}{content}{NORMAL}")
}

fn render_emph<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx);
    format!("{ITALIC}{content}{ITALIC_OFF}")
}

fn render_strikethrough<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx);
    format!("{STRIKETHROUGH}{content}{STRIKETHROUGH_OFF}")
}

fn render_link<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Link(NodeLink { ref url, .. }) = node.data.borrow().value else {
        panic!()
    };

    let content = collect(node, ctx);
    let cyan = ctx.theme.cyan.fg.clone();
    let osc8_start = format!("\x1b]8;;{}\x1b\\", url);
    let osc8_end = "\x1b]8;;\x1b\\";
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("{osc8_start}{UNDERLINE}{cyan}\u{f0339} {line}{RESET}{osc8_end}")
            } else {
                format!("  {osc8_start}{UNDERLINE}{cyan}{line}{RESET}{osc8_end}")
            }
        })
        .join("\n")
}

fn render_image<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Image(NodeLink { ref url, .. }) = node.data.borrow().value else {
        panic!()
    };

    if let Some(img) = ctx.image_preprocessor.mapper.get(url) {
        if img.is_ok {
            return img.placeholder.clone();
        }
    }

    let content = collect(node, ctx);
    let cyan = ctx.theme.cyan.fg.clone();
    format!("{UNDERLINE}{cyan}\u{f0976} {}{RESET}", content)
}

fn render_code<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Code(NodeCode { ref literal, .. }) = node.data.borrow().value else {
        panic!()
    };

    let fg = &ctx.theme.green.fg;
    format!("{fg}{}{RESET}", literal)
}

fn render_html_inline<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::HtmlInline(ref literal) = node.data.borrow().value else {
        panic!()
    };

    let string_color = ctx.theme.string.fg.clone();
    format!("{string_color}{literal}{RESET}")
}

fn render_superscript<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    // no real thing I can do
    collect(node, ctx)
}

fn render_multiline_block_quote<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::MultilineBlockQuote(ref multiline_block_quote) = node.data.borrow().value else {
        panic!()
    };
    let fenced_offset = multiline_block_quote.fence_offset;
    ctx.blockquote_fenced_offset = Some(fenced_offset);
    ctx.is_multi_block_quote = true;

    let res = render_block_quote(node, ctx);
    ctx.blockquote_fenced_offset = None;
    ctx.is_multi_block_quote = false;

    res
}

fn render_wiki_link<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::WikiLink(NodeWikiLink { .. }) = node.data.borrow().value else {
        panic!()
    };

    let content = collect(node, ctx);
    let cyan = &ctx.theme.cyan.fg;
    format!("{cyan}\u{f15d6} {}{RESET}", content)
}

fn render_spoilered_text<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx);
    let comment = &ctx.theme.comment.fg;
    format!("{FAINT}{comment}{content}{RESET}")
}

fn render_alert<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Alert(NodeAlert { ref alert_type, .. }) = node.data.borrow().value else {
        panic!()
    };

    let kind = alert_type;
    let blue = ctx.theme.blue.fg.clone();
    let red = ctx.theme.red.fg.clone();
    let green = ctx.theme.green.fg.clone();
    let cyan = ctx.theme.cyan.fg.clone();
    let yellow = ctx.theme.yellow.fg.clone();

    let (prefix, color) = match kind {
        comrak::nodes::AlertType::Note => ("\u{f05d6} NOTE", blue),
        comrak::nodes::AlertType::Tip => ("\u{f400} TIP", green),
        comrak::nodes::AlertType::Important => ("\u{f017e} INFO", cyan),
        comrak::nodes::AlertType::Warning => ("\u{ea6c} WARNING", yellow),
        comrak::nodes::AlertType::Caution => ("\u{f0ce6} DANGER", red),
    };

    let mut result = format!("\n\n{}▌ {BOLD}{}{RESET}", color, prefix);

    ctx.force_simple_code_block += 1;
    let alert_content = collect(node, ctx);
    let alert_content = limit_newlines(&alert_content);
    ctx.force_simple_code_block -= 1;
    let alert_content = alert_content.trim();
    if alert_content.is_empty() {
        return result;
    }

    result.push('\n');
    let alert_content = alert_content
        .lines()
        .map(|line| format!("{color}▌{RESET} {line}"))
        .join("\n");
    result.push_str(&alert_content);

    let content = if ctx.should_indent() {
        wrap_char_based(&result, '▌', INDENT, "", "")
    } else {
        result
    };

    content + "\n\n"
}
