use comrak::nodes::{
    AstNode, NodeCode, NodeHeading, NodeHtmlBlock, NodeMath, NodeValue, NodeWikiLink,
};
use itertools::Itertools;
use rasteroid::term_misc::Wininfo;
use regex::Regex;
use strip_ansi_escapes::strip_str;
use syntect::parsing::SyntaxSet;

use crate::{
    markdown_viewer::utils::{string_len, to_superscript, trim_ansi_string, wrap_lines},
    themes::CustomTheme,
};

use super::{
    image_preprocessor::ImagePreprocessor,
    utils::{
        format_code_box, format_code_full, format_code_simple, format_tb, wrap_char_based,
        wrap_highlighted_line,
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

pub struct AnsiContext {
    pub ps: SyntaxSet,
    pub theme: CustomTheme,
    pub wininfo: Wininfo,
    pub hide_line_numbers: bool,
    pub show_frontmatter: bool,
    pub center: bool,
    pub image_preprocessor: ImagePreprocessor,

    pub blockquote_fenced_offset: Option<usize>,
    pub is_multi_block_quote: bool,
    pub paragraph_collecting_line: Option<usize>,
    pub collecting_depth: usize,
    pub under_header: bool,
    pub force_simple_code_block: usize,
    pub list_depth: usize,
}

impl AnsiContext {
    pub fn should_wrap(&self) -> bool {
        // root level element
        self.collecting_depth == 0
    }
    pub fn indent(&self) -> usize {
        if self.under_header { 2 } else { 0 }
    }
}

fn collect<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext, sep: &str) -> String {
    ctx.collecting_depth += 1;
    let content = node
        .children()
        .map(|child| parse_node(child, ctx))
        .filter(|s| !s.is_empty())
        .join(sep);
    ctx.collecting_depth -= 1;
    content
}

fn center_lines(text: &str, start_col: usize, sc_width: u16) -> String {
    text.lines()
        .map(|line| {
            let line = trim_ansi_string(line.into());
            let le = string_len(&line);
            let offset = start_col.saturating_sub(1);
            let offset = (sc_width as usize - offset)
                .saturating_sub(le)
                .saturating_div(2);
            format!("{}{line}", " ".repeat(offset))
        })
        .join("\n")
}

pub fn parse_node<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let data = node.data.borrow();

    match &data.value {
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
        NodeValue::Highlight => render_highlight(node, ctx),
        NodeValue::DescriptionList => render_description_list(node, ctx),
        NodeValue::DescriptionItem(_) => render_description_item(node, ctx),
        NodeValue::DescriptionTerm => render_description_term(node, ctx),
        NodeValue::DescriptionDetails => render_description_details(node, ctx),
        NodeValue::Text(literal) => literal.to_string(),
        NodeValue::Raw(literal) => literal.to_owned(),
        NodeValue::Math(NodeMath { literal, .. }) => literal.to_owned(),
        NodeValue::ShortCode(sc) => sc.emoji.to_string(),
        NodeValue::SoftBreak => " ".to_owned(),
        NodeValue::LineBreak => "\n".to_owned(),
        NodeValue::TableRow(_) => String::new(), // handled in table
        NodeValue::TableCell => String::new(),   // handled in table
        NodeValue::Escaped => String::new(),     // feature not enabled per char
        NodeValue::EscapedTag(_) => String::new(), // i think its only html
        NodeValue::Underline => String::new(),   // feature not enabled
        NodeValue::Subscript => String::new(),   // feature not enabled (can't render them)
        NodeValue::HeexBlock(_) => String::new(), // feature not enabled
        NodeValue::HeexInline(_) => String::new(), // feature not enabled
        NodeValue::Subtext => String::new(),     // feature not enabled (can't render this)
        NodeValue::Insert => String::new(),      // too niche, not enabled
        NodeValue::BlockDirective(_) => String::new(), // too niche, not enabled
    }
}

fn render_document<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    node.children()
        .map(|child| parse_node(child, ctx))
        .filter(|s| !s.is_empty())
        .join("\n\n")
}

fn render_description_list<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    collect(node, ctx, "\n\n")
}

fn render_description_item<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    collect(node, ctx, "\n")
}

fn render_description_term<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "").replace(RESET, &format!("{RESET}{BOLD}"));
    format!("{BOLD}{content}{NORMAL}")
}

fn render_description_details<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "");
    content.lines().map(|l| format!("  {l}")).join("\n")
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

    let content = collect(node, ctx, "\n\n");
    let cyan = &ctx.theme.cyan.fg;
    let content = format!("{cyan}[{}]{RESET}: {content}", item.name);

    // not sure if this is possible to center a footnote ref.. leaving this non centered
    content
        .lines()
        .map(|line| {
            let indent = ctx.indent();
            if ctx.should_wrap() {
                wrap_lines(ctx, line, false, indent, "", "", false)
            } else {
                line.into()
            }
        })
        .join("\n")
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
    let content = collect(node, ctx, "\n\n").replace(RESET, &format!("{RESET}{comment}"));
    ctx.force_simple_code_block -= 1;
    let fence_offset = ctx.blockquote_fenced_offset.unwrap_or_default();

    let offset = " ".repeat(fence_offset + 1);
    let content = content
        .lines()
        .map(|line| format!("{guide}▌{offset}{comment}{line}{RESET}"))
        .join("\n");

    let indent = ctx.indent();
    if ctx.should_wrap() {
        wrap_char_based(ctx, &content, '▌', indent, "", "")
    } else {
        content.to_owned()
    }
}

fn render_list<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::List(ref list) = node.data.borrow().value else {
        panic!()
    };
    let sep = if list.tight { "\n" } else { "\n\n" };

    ctx.list_depth += 1;
    let content = collect(node, ctx, sep);
    ctx.list_depth -= 1;

    // we indent sub lines based on the length of the bullet
    let sub_prefix = content
        .lines()
        .next()
        .map(|line| {
            strip_str(line)
                .trim()
                .chars()
                .take_while(|c| !c.is_whitespace())
                .count()
                + 1
        })
        .unwrap_or(2);
    let sub_prefix = " ".repeat(sub_prefix);

    let indent = ctx.indent();
    if ctx.should_wrap() {
        wrap_lines(ctx, &content, true, indent, "", &sub_prefix, true)
    } else {
        // this part only gets called inside other blocky elements. e.g. blockquote and alert
        let indent_width = indent * 2;
        let blockquote_prefix =
            2 * ctx.force_simple_code_block + ctx.blockquote_fenced_offset.unwrap_or(0);
        let bullet_width = 2;
        let sub_prefix_width = 2;
        let prefix_width =
            (indent_width + blockquote_prefix + bullet_width + sub_prefix_width) as u16;

        ctx.wininfo.sc_width = ctx.wininfo.sc_width.saturating_sub(prefix_width);
        let result = wrap_lines(ctx, &content, true, 0, "", &sub_prefix, true);
        ctx.wininfo.sc_width += prefix_width;
        result
    }
}

fn render_item<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Item(ref item) = node.data.borrow().value else {
        panic!()
    };

    let yellow = ctx.theme.yellow.fg.clone();
    let content = collect(node, ctx, "\n");
    let content = content.trim();
    let depth = ctx.list_depth - 1;

    let bullets = ["●", "○", "◆", "◇"];
    let bullet = match item.list_type {
        comrak::nodes::ListType::Bullet => bullets[depth % 4],
        comrak::nodes::ListType::Ordered => &format!("{}.", item.start),
    };

    // indent new lines to allign with the first line
    let bullet_count = bullet.chars().count() + 1;
    let content = content.replace("\n", &format!("\n{}", " ".repeat(bullet_count)));

    format!("{}{yellow}{bullet}{RESET} {content}", " ".repeat(depth * 4))
}

fn render_task_item<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::TaskItem(ref task) = node.data.borrow().value else {
        panic!()
    };

    let offset = " ".repeat(node.data.borrow().sourcepos.start.column - 1);
    let content = collect(node, ctx, "\n\n");
    let content = content.trim();
    let (icon, colour) = match task.symbol.map(|c| c.to_ascii_lowercase()) {
        Some('x') => ("󰱒", &ctx.theme.green.fg),
        Some('-') | Some('~') => ("󰛲", &ctx.theme.yellow.fg),
        Some('!') => ("󰳤", &ctx.theme.red.fg),
        _ => ("󰄱", &ctx.theme.red.fg),
    };

    format!("{offset}{colour}{icon}{RESET}  {content}")
}

fn render_code_block<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::CodeBlock(ref node_code_block) = node.data.borrow().value else {
        panic!()
    };
    let literal = &node_code_block.literal;
    let info = &node_code_block.info;

    let info = if info.trim().is_empty() { "text" } else { info };

    if matches!(info, "mermaid" | "mmd")
        && let Some(img) = ctx.image_preprocessor.mapper.get(literal)
        && img.is_ok
    {
        let placeholder = &img.placeholder;
        if ctx.center {
            let sps = node.data.borrow().sourcepos;
            return center_lines(placeholder, sps.start.column, ctx.wininfo.sc_width);
        }
        placeholder.clone()
    } else if info == "file-tree" {
        render_file_tree(literal, ctx)
    } else if literal.lines().count() <= 10
        || ctx.force_simple_code_block > 0
        || ctx.hide_line_numbers
    {
        let indent = ctx.indent();
        format_code_simple(literal, info, ctx, indent)
    } else {
        format_code_full(literal, info, ctx)
    }
}

fn render_file_tree(tree: &str, ctx: &mut AnsiContext) -> String {
    let tree_chars = Regex::new(r"[│├└─]").unwrap();
    let folders = Regex::new(r"([a-zA-Z0-9_\-]+/)").unwrap();
    let files = Regex::new(r"([a-zA-Z0-9_\-]+\.[a-zA-Z0-9]+)").unwrap();
    let urls = Regex::new(r"https?://[^\s]+").unwrap();

    let tree_char_color = &ctx.theme.guide.fg;
    let folder_color = &ctx.theme.blue.fg;
    let file_color = &ctx.theme.foreground.fg;
    let url_color = &ctx.theme.magenta.fg;

    let mut result = tree.trim().to_string();

    result = tree_chars
        .replace_all(&result, |caps: &regex::Captures| {
            format!("{tree_char_color}{}{RESET}", &caps[0])
        })
        .to_string();

    result = result
        .lines()
        .map(|line| {
            let mut l = line.to_string();
            if urls.is_match(line) {
                l = urls
                    .replace_all(&l, |caps: &regex::Captures| {
                        format!("{url_color}{}{RESET}", &caps[0])
                    })
                    .to_string();
            } else {
                l = folders
                    .replace_all(&l, |caps: &regex::Captures| {
                        format!("{folder_color}{}{RESET}", &caps[1])
                    })
                    .to_string();
                l = files
                    .replace_all(&l, |caps: &regex::Captures| {
                        format!("{file_color}{}{RESET}", &caps[1])
                    })
                    .to_string();
            }
            l
        })
        .collect::<Vec<_>>()
        .join("\n");

    result
}

fn render_html_block<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::HtmlBlock(NodeHtmlBlock { ref literal, .. }) = node.data.borrow().value else {
        panic!()
    };

    if literal.contains("<!--CENTER_ON-->") {
        ctx.center = true;
        return String::new();
    }
    if literal.contains("<!--CENTER_OFF-->") {
        ctx.center = false;
        return String::new();
    }

    let comment = &ctx.theme.comment.fg;
    let result = literal
        .lines()
        .map(|line| format!("{comment}{line}{RESET}"))
        .join("\n");
    wrap_lines(ctx, &result, true, INDENT, "", "", false)
}

fn render_paragraph<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let sps = node.data.borrow().sourcepos;
    ctx.paragraph_collecting_line = Some(sps.start.line);
    let lines = collect(node, ctx, "");
    ctx.paragraph_collecting_line = None;

    if ctx.center {
        center_lines(&lines, sps.start.column, ctx.wininfo.sc_width)
    } else {
        lines
            .lines()
            .map(|line| {
                let indent = ctx.indent();
                if ctx.should_wrap() {
                    wrap_lines(ctx, line, false, indent, "", "", false)
                } else {
                    line.into()
                }
            })
            .join("\n")
    }
}

fn render_heading<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Heading(NodeHeading { level, .. }) = node.data.borrow().value else {
        panic!()
    };

    ctx.under_header = true;
    let content = collect(node, ctx, "");
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

    if !ctx.center {
        let padding = " ".repeat(
            ctx.wininfo
                .sc_width
                .saturating_sub(string_len(&content) as u16)
                .into(),
        );
        format!("{main_color}{bg}{content}{padding}{RESET}")
    } else {
        // center here
        let le = string_len(&content);
        let left_space = (ctx.wininfo.sc_width as usize).saturating_sub(le);
        let padding_left = left_space.saturating_div(2);
        let padding_rigth = left_space - padding_left;
        format!(
            "{main_color}{bg}{}{content}{}{RESET}",
            " ".repeat(padding_left),
            " ".repeat(padding_rigth)
        )
    }
}

fn render_thematic_break<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let offset = node.data.borrow().sourcepos.start.column;
    // each level of blockquote adds 4 char prefix..
    let extra_offset = ctx.force_simple_code_block * 4;
    format_tb(ctx, offset + extra_offset + ctx.indent())
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
            let cell_content = collect(cell_node, ctx, "");
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

    // Cap column widths to fit within available terminal width.
    // Match the three-way branch in the existing post-render offset logic:
    // centered uses source-position offset, indented uses ctx.indent(),
    // default uses full width.
    let available_width = if ctx.center {
        let offset = node.data.borrow().sourcepos.start.column.saturating_sub(1);
        (ctx.wininfo.sc_width as usize).saturating_sub(offset)
    } else if ctx.should_wrap() {
        (ctx.wininfo.sc_width as usize).saturating_sub(ctx.indent())
    } else {
        ctx.wininfo.sc_width as usize
    };

    let border_overhead = 3 * column_widths.len() + 1;
    let total_content_width: usize = column_widths.iter().sum();
    let total_table_width = total_content_width + border_overhead;

    if total_table_width > available_width && total_content_width > 0 {
        let target_content_width = available_width.saturating_sub(border_overhead);
        // Waterfall algorithm: keep narrow columns at their natural width,
        // shrink only the wider columns. Iterate from narrowest to widest:
        // if a column fits within its equal share of remaining space, grant
        // it its natural width; otherwise distribute remaining space equally
        // among the remaining (wider) columns.
        let mut indices: Vec<usize> = (0..column_widths.len()).collect();
        indices.sort_by_key(|&i| column_widths[i]);

        let mut new_widths: Vec<usize> = vec![0; column_widths.len()];
        let mut remaining_budget = target_content_width;
        let mut remaining_cols = column_widths.len();

        for &i in &indices {
            let fair_share = if remaining_cols > 0 {
                remaining_budget / remaining_cols
            } else {
                0
            };
            if column_widths[i] <= fair_share {
                // This column fits within its share; keep natural width
                new_widths[i] = column_widths[i];
            } else {
                // This column (and all wider ones) must share the remaining budget
                new_widths[i] = fair_share.max(1);
            }
            remaining_budget = remaining_budget.saturating_sub(new_widths[i]);
            remaining_cols -= 1;
        }

        // Distribute any leftover due to integer division
        let assigned: usize = new_widths.iter().sum();
        if assigned < target_content_width {
            let mut remainder = target_content_width - assigned;
            // Give extra to widest columns first
            indices.sort_by(|&a, &b| column_widths[b].cmp(&column_widths[a]));
            for &i in &indices {
                if remainder == 0 {
                    break;
                }
                new_widths[i] += 1;
                remainder -= 1;
            }
        }

        column_widths = new_widths;
    }

    // Re-wrap cell contents to fit capped column widths and recalculate row heights
    for (row_idx, row) in rows.iter_mut().enumerate() {
        let mut max_lines_in_row: usize = 1;

        for (col_idx, cell) in row.iter_mut().enumerate() {
            let col_width = column_widths[col_idx];
            let mut new_lines: Vec<String> = Vec::new();

            for line in cell.iter() {
                if string_len(line) > col_width {
                    let wrapped =
                        wrap_highlighted_line(line.clone(), col_width, col_width, "", false);
                    new_lines.extend(wrapped.split('\n').map(|s| s.to_string()));
                } else {
                    new_lines.push(line.clone());
                }
            }

            max_lines_in_row = max_lines_in_row.max(new_lines.len());
            *cell = new_lines;
        }

        row_heights[row_idx] = max_lines_in_row;
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
        } else {
            result = result.trim_end_matches("\n").to_owned();
        }
    }

    let sps = node.data.borrow().sourcepos;
    if ctx.center {
        let le = string_len(result.lines().nth(1).unwrap_or_default());
        let offset = sps.start.column.saturating_sub(1);
        let offset = (ctx.wininfo.sc_width as usize - offset)
            .saturating_sub(le)
            .saturating_div(2);

        result
            .lines()
            .map(|line| format!("{}{line}", " ".repeat(offset)))
            .join("\n")
    } else if ctx.should_wrap() {
        let indent = ctx.indent();
        result
            .lines()
            .map(|line| format!("{}{line}", " ".repeat(indent)))
            .join("\n")
    } else {
        result
    }
}

fn render_strong<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "").replace(RESET, &format!("{RESET}{BOLD}"));
    format!("{BOLD}{content}{NORMAL}")
}

fn render_emph<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "").replace(RESET, &format!("{RESET}{ITALIC}"));
    format!("{ITALIC}{content}{ITALIC_OFF}")
}

fn render_highlight<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "");
    let color = &ctx.theme.yellow.bg;
    let fg = &ctx.theme.black.fg;
    let start = format!("{fg}{color}");
    let content = content.replace(RESET, &format!("{RESET}{start}"));
    format!("{start}{content}{RESET}")
}

fn render_strikethrough<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "").replace(RESET, &format!("{RESET}{STRIKETHROUGH}"));
    format!("{STRIKETHROUGH}{content}{STRIKETHROUGH_OFF}")
}

fn render_link<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Link(ref node_link) = node.data.borrow().value else {
        panic!()
    };
    let url = &node_link.url;

    let content = collect(node, ctx, "");
    let cyan = &ctx.theme.cyan.fg;
    let start = format!("{UNDERLINE}{cyan}");
    let content = content.replace(RESET, &format!("{RESET}{start}"));
    let osc8_start = format!("\x1b]8;;{}\x1b\\", url);
    let osc8_end = "\x1b]8;;\x1b\\";
    content
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                format!("{osc8_start}{start}\u{f0339} {line}{RESET}{osc8_end}")
            } else {
                format!("\u{00A0}\u{00A0}{osc8_start}{start}{line}{RESET}{osc8_end}")
            }
        })
        .join("\n")
}

fn render_image<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Image(ref node_link) = node.data.borrow().value else {
        panic!()
    };
    let url = &node_link.url;

    if let Some(img) = ctx.image_preprocessor.mapper.get(url)
        && img.is_ok
    {
        return img.placeholder.clone();
    }

    let content = collect(node, ctx, "");
    let cyan = &ctx.theme.cyan.fg;
    let start = format!("{UNDERLINE}{cyan}");
    let content = content.replace(RESET, &format!("{RESET}{start}"));
    format!("{start}\u{f0976} {}{RESET}", content)
}

fn render_code<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Code(NodeCode { ref literal, .. }) = node.data.borrow().value else {
        panic!()
    };

    // special formatting, we'll treat code blocks wrapped [] to be kbd.
    if literal.starts_with('[') && literal.ends_with(']') {
        let inner = &literal[1..literal.len() - 1];
        let fg = &ctx.theme.foreground.fg;
        let bg = &ctx.theme.surface.bg;
        let cap_fg = &ctx.theme.surface.fg;
        return format!("{cap_fg}\u{e0b6}{fg}{bg}{inner}{RESET}{cap_fg}\u{e0b4}{RESET}");
    }

    let fg = &ctx.theme.green.fg;
    format!("{fg}{}{RESET}", literal)
}

fn render_html_inline<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::HtmlInline(ref literal) = node.data.borrow().value else {
        panic!()
    };
    let string_color = ctx.theme.string.fg.clone();
    match literal.to_lowercase().as_str() {
        "<u>" | "<ins>" => UNDERLINE.to_owned(),
        "</u>" | "</ins>" => UNDERLINE_OFF.to_owned(),

        "<br>" | "<br/>" | "<br />" => "\n".to_owned(),

        _ => format!("{string_color}{literal}{RESET}"),
    }
}

fn render_superscript<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "");
    let trimmed = content.trim();

    let mut result = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        match to_superscript(ch) {
            Some(sup) => result.push(sup),
            None => return format!("^({})", trimmed),
        }
    }
    result
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

    let content = collect(node, ctx, "");
    let cyan = &ctx.theme.cyan.fg;
    let content = content.replace(RESET, &format!("{RESET}{cyan}"));
    format!("{cyan}\u{f15d6} {}{RESET}", content)
}

fn render_spoilered_text<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let content = collect(node, ctx, "");
    let comment = &ctx.theme.comment.fg;
    let start = format!("{FAINT}{comment}");
    let content = content.replace(RESET, &format!("{RESET}{start}"));
    format!("{start}{content}{RESET}")
}

fn render_alert<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) -> String {
    let NodeValue::Alert(ref node_alert) = node.data.borrow().value else {
        panic!()
    };

    ctx.force_simple_code_block += 1;
    let alert_content = collect(node, ctx, "\n");
    ctx.force_simple_code_block -= 1;

    let alert_type = &node_alert.alert_type;
    let kind = alert_type;
    let blue = &ctx.theme.blue.fg;
    let red = &ctx.theme.red.fg;
    let green = &ctx.theme.green.fg;
    let cyan = &ctx.theme.cyan.fg;
    let yellow = &ctx.theme.yellow.fg;

    let (prefix, color) = match kind {
        comrak::nodes::AlertType::Note => ("\u{f05d6} NOTE", blue),
        comrak::nodes::AlertType::Tip => ("\u{f400} TIP", green),
        comrak::nodes::AlertType::Important => ("\u{f017e} INFO", cyan),
        comrak::nodes::AlertType::Warning => ("\u{ea6c} WARNING", yellow),
        comrak::nodes::AlertType::Caution => ("\u{f0ce6} DANGER", red),
    };

    let mut result = format!("{}▌ {BOLD}{}{RESET}", color, prefix);

    result.push('\n');
    let alert_content = alert_content
        .lines()
        .map(|line| format!("{color}▌{RESET} {line}"))
        .join("\n");
    result.push_str(&alert_content);

    let indent = ctx.indent();
    if ctx.should_wrap() {
        wrap_char_based(ctx, &result, '▌', indent, "", "")
    } else {
        result
    }
}
