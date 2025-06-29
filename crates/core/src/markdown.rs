use core::str;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::atomic::{AtomicUsize, Ordering},
};

use comrak::{
    Arena, ComrakOptions, ComrakPlugins, markdown_to_html_with_plugins,
    nodes::{AstNode, NodeMath, NodeValue, Sourcepos},
    plugins::syntect::SyntectAdapterBuilder,
};
use rasteroid::term_misc;
use regex::Regex;
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, ScopeSelectors, Style, StyleModifier, Theme, ThemeSet, ThemeSettings},
    parsing::SyntaxSet,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};
use unicode_width::UnicodeWidthStr;

const RESET: &str = "\x1B[0m";
const BOLD: &str = "\x1B[1m";
const ITALIC: &str = "\x1B[3m";
const UNDERLINE: &str = "\x1B[4m";
const STRIKETHROUGH: &str = "\x1B[9m";
const FAINT: &str = "\x1b[2m";

struct AnsiContext {
    ps: SyntaxSet,
    theme: CustomTheme,
    hide_line_numbers: bool,
    line: AtomicUsize,
    output: String,
}
impl AnsiContext {
    fn write(&mut self, val: &str) {
        let fg = self.theme.foreground.fg.clone();
        let val = val.replace(RESET, &format!("{RESET}{fg}"));
        self.output.push_str(&val);
    }
    fn cr(&mut self) {
        self.output.push('\n');
    }
    fn sps(&mut self, sps: Sourcepos) {
        let current_line = self.line.load(Ordering::SeqCst);

        if sps.start.line > current_line {
            let offset = sps.start.line - current_line;
            self.output.push_str(&"\n".repeat(offset));
        }
        self.line.store(sps.end.line, Ordering::SeqCst);
    }
    fn collect<'a>(&self, node: &'a AstNode<'a>) -> String {
        let line = AtomicUsize::new(node.data.borrow().sourcepos.start.line);
        let mut ctx = AnsiContext {
            ps: self.ps.clone(),
            theme: self.theme.clone(),
            hide_line_numbers: self.hide_line_numbers,
            line,
            output: String::new(),
        };
        for child in node.children() {
            format_ast_node(child, &mut ctx);
        }
        ctx.output
    }
    fn collect_and_write<'a>(&mut self, node: &'a AstNode<'a>) {
        let text = self.collect(node);
        self.write(&text);
    }
}
pub fn md_to_ansi(md: &str, theme: Option<&str>, hide_line_numbers: bool) -> String {
    let arena = Arena::new();
    let opts = comrak_options();
    let root = comrak::parse_document(&arena, md, &opts);

    let ps = SyntaxSet::load_defaults_newlines();
    let theme = get_theme(theme);
    let mut ctx = AnsiContext {
        ps,
        theme,
        hide_line_numbers,
        output: String::new(),
        line: AtomicUsize::new(1),
    };
    ctx.write(&ctx.theme.foreground.fg.clone());
    format_ast_node(root, &mut ctx);

    let lines: Vec<String> =
        textwrap::wrap(&ctx.output, term_misc::get_wininfo().sc_width as usize)
            .into_iter()
            .map(|cow| cow.into_owned())
            .collect();
    lines.join("\n")
}

fn comrak_options<'a>() -> ComrakOptions<'a> {
    let mut options = ComrakOptions::default();
    // ➕ Enable extensions
    options.extension.strikethrough = true;
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

    // 🎯 Parsing options
    options.parse.smart = true; // fancy quotes, dashes, ellipses

    // 💄 Render options
    options.render.unsafe_ = true;

    options
}

fn get_theme(s: Option<&str>) -> CustomTheme {
    match s.unwrap_or("dark") {
        "catppuccin" => CustomTheme::catppuccin(),
        "nord" => CustomTheme::nord(),
        "monokai" => CustomTheme::monokai(),
        "dracula" => CustomTheme::dracula(),
        "gruvbox" => CustomTheme::gruvbox(),
        "one_dark" => CustomTheme::one_dark(),
        "solarized" => CustomTheme::solarized(),
        "tokyo_night" => CustomTheme::tokyo_night(),
        "makurai_healer" => CustomTheme::makurai_healer(),
        "makurai_mage" => CustomTheme::makurai_mage(),
        "ayu" => CustomTheme::ayu(),
        "ayu_mirage" => CustomTheme::ayu_mirage(),
        "github" => CustomTheme::github(),
        "synthwave" => CustomTheme::synthwave(),
        "material" => CustomTheme::material(),
        "rose_pine" => CustomTheme::rose_pine(),
        "kanagawa" => CustomTheme::kanagawa(),
        "vscode" => CustomTheme::vscode(),
        "everforest" => CustomTheme::everforest(),
        "autumn" => CustomTheme::autumn(),
        "spring" => CustomTheme::spring(),
        _ => CustomTheme::spring(),
    }
}

pub fn md_to_html(markdown: &str, style: Option<&str>) -> String {
    let options = comrak_options();

    let theme = get_theme(style);
    let mut theme_set = ThemeSet::load_defaults();
    let mut plugins = ComrakPlugins::default();
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

fn format_ast_node<'a>(node: &'a AstNode<'a>, ctx: &mut AnsiContext) {
    let data = node.data.borrow();
    let sps = data.sourcepos;
    ctx.sps(sps);

    match &data.value {
        NodeValue::Document => {
            for child in node.children() {
                format_ast_node(child, ctx);
            }
        }
        NodeValue::FrontMatter(str) => {
            // no idea what that is.
            ctx.write(str);
        }
        NodeValue::BlockQuote => {
            let block_content = ctx.collect(node);
            let color = ctx.theme.guide.fg.clone();
            let comment = ctx.theme.comment.fg.clone();

            for (i, line) in block_content.lines().enumerate() {
                if i != 0 {
                    ctx.cr();
                }
                ctx.write(&format!("{color}▌{RESET} {comment}{}{RESET}", line));
            }
        }
        NodeValue::List(node_list) => {
            let list_type = &node_list.list_type;
            let mut index: i32 = match list_type {
                comrak::nodes::ListType::Bullet => 0,
                comrak::nodes::ListType::Ordered => node_list.start as i32,
            };
            let bullet = if node_list.is_task_list { "" } else { "●" };
            let content = ctx.collect(node);

            let mut pre_offset = 0;
            for (i, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                if i != 0 {
                    ctx.cr();
                }

                let mut offset = 0;
                for c in line.chars() {
                    match c {
                        ' ' => offset += 1,
                        '\t' => offset += 2,
                        _ => break,
                    }
                }
                let is_nested = offset > pre_offset && i != 0;
                if is_nested {
                    index -= 1;
                } else {
                    pre_offset = offset;
                }
                let new_index = index + i as i32;
                let line = line.trim();
                let bullet = if is_nested {
                    ""
                } else {
                    match list_type {
                        comrak::nodes::ListType::Bullet => bullet,
                        comrak::nodes::ListType::Ordered => &format!("{new_index}."),
                    }
                };
                let offset = " ".repeat(offset);

                if bullet.is_empty() {
                    ctx.write(&format!("{offset}{line}"));
                } else {
                    let line = if line.contains("\0") {
                        let line = line.replace("\0", &format!("{bullet}{RESET}"));
                        line
                    } else {
                        format!("  {line}")
                    };
                    ctx.write(&format!("{offset}{line}"));
                }
            }

            let mut current = node.parent();
            let mut is_first = true;
            while let Some(parent) = current {
                match parent.data.borrow().value {
                    comrak::nodes::NodeValue::Item(_) => {
                        is_first = false;
                        break;
                    }
                    comrak::nodes::NodeValue::Document => break,
                    _ => current = parent.parent(),
                }
            }
            if is_first {
                ctx.cr();
            }
        }
        NodeValue::Item(_) => {
            let content = ctx.collect(node);
            let yellow = &ctx.theme.yellow.fg;
            ctx.write(&format!(
                "{}{yellow}\0 {content}",
                " ".repeat(data.sourcepos.start.column - 1)
            ));
        }
        NodeValue::CodeBlock(node_code_block) => {
            let code = &node_code_block.literal;
            let lang = &node_code_block.info;
            let lang = if lang.is_empty() {
                &"txt".to_string()
            } else {
                lang
            };

            let indent = data.sourcepos.start.column;
            if ctx.hide_line_numbers || code.lines().count() < 10 {
                format_code_simple(code, lang, ctx, indent);
            } else {
                format_code(code, lang, ctx, indent);
            }
        }
        NodeValue::HtmlBlock(node_html_block) => {
            let re = Regex::new(r#"<!--\s*S-TITLE:\s*(.*?)\s*-->"#).unwrap();
            if let Some(caps) = re.captures(&node_html_block.literal) {
                let title = caps.get(1).unwrap().as_str();
                let width = term_misc::get_wininfo().sc_width;
                let text_size = string_len(title);
                let padding = width as usize - text_size;
                let left_padding = padding / 2;
                let right_padding = padding - left_padding;
                let surface = ctx.theme.surface.bg.clone();
                let block = &format!("{surface}{}{RESET}\n", " ".repeat(width as usize));
                let fg_yellow = ctx.theme.yellow.fg.clone();
                ctx.write(&block);
                ctx.write(&format!(
                    "{surface}{}{fg_yellow}{BOLD}{title}{surface}{}{RESET}\n",
                    " ".repeat(left_padding),
                    " ".repeat(right_padding)
                ));
                ctx.write(&block);
                return;
            }

            let ts = ctx.theme.to_syntect_theme();
            let syntax = ctx
                .ps
                .find_syntax_by_token("html")
                .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
            let mut highlighter = HighlightLines::new(syntax, &ts);
            for line in LinesWithEndings::from(&node_html_block.literal) {
                let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
                let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
                ctx.write(&highlighted);
            }
            if ctx.output.ends_with('\n') {
                ctx.output.pop();
            }
        }
        NodeValue::Paragraph => {
            ctx.collect_and_write(node);
        }
        NodeValue::Heading(node_heading) => {
            let content = ctx.collect(node);
            let main_color = ctx.theme.keyword.fg.clone();
            match node_heading.level {
                0 | 1 => {
                    let l = string_len(&content);
                    let sep_len = (l + 6).min(term_misc::get_wininfo().sc_width as usize);
                    let sep = "-".repeat(sep_len);
                    ctx.write(&format!(
                        "{main_color}{BOLD}{sep}\n   {main_color}{content}\n{main_color}{sep}{RESET}"
                    ));
                }
                2 => {
                    let l = string_len(&content);
                    let sep_len = (l + 4).min(term_misc::get_wininfo().sc_width as usize);
                    let sep = "-".repeat(sep_len);
                    ctx.write(&format!(
                        "{BOLD}{main_color}  {content}\n{main_color}{sep}{RESET}"
                    ));
                }
                3 => {
                    ctx.write(&format!("{BOLD}{main_color}→ {content}{RESET}"));
                }
                4.. => {
                    ctx.write(&format!("▸ {main_color}{content}{RESET}"));
                }
            }
        }
        NodeValue::ThematicBreak => {
            let br = br();
            let border = ctx.theme.guide.fg.clone();
            ctx.write(&format!("{border}{br}{RESET}"));
        }
        NodeValue::FootnoteDefinition(_) => {}
        NodeValue::Table(table) => {
            let alignments = &table.alignments;
            let mut rows: Vec<Vec<String>> = Vec::new();

            for child in node.children() {
                let mut row_cells: Vec<String> = Vec::new();

                for cell_node in child.children() {
                    let cell_content = ctx.collect(cell_node);
                    row_cells.push(cell_content.to_string());
                }

                rows.push(row_cells);
            }

            // Find the maximum width for each column
            let mut column_widths: Vec<usize> = vec![0; alignments.len()];
            for row in &rows {
                for (i, cell) in row.iter().enumerate() {
                    let c = string_len(cell.trim());
                    if c > column_widths[i] {
                        column_widths[i] = c;
                    }
                }
            }

            let color = &ctx.theme.border.fg.clone();
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
                let middle_border = build_line("├", "┼", "┤", "─");
                let bottom_border = build_line("╰", "┴", "╯", "─");
                ctx.write(&top_border);
                ctx.cr();

                for (i, row) in rows.iter().enumerate() {
                    // Print the row content
                    ctx.write(&format!("{color}│{RESET}"));
                    for (j, cell) in row.iter().enumerate() {
                        let width = column_widths[j];
                        let padding = width - string_len(cell);
                        let (left_pad, right_pad) = match alignments[j] {
                            comrak::nodes::TableAlignment::Center => {
                                (padding / 2, padding - (padding / 2))
                            }
                            comrak::nodes::TableAlignment::Right => (padding, 0),
                            _ => (0, padding),
                        };
                        ctx.write(&format!(
                            " {}{}{} {color}│{RESET}",
                            " ".repeat(left_pad),
                            cell,
                            " ".repeat(right_pad)
                        ));
                    }
                    ctx.write("\n");

                    if i == 0 {
                        ctx.write(&middle_border);
                        ctx.cr();
                    }
                }
                ctx.write(&bottom_border);
            }
        }
        NodeValue::Text(literal) => ctx.write(literal),
        NodeValue::SoftBreak => ctx.write(" "),
        NodeValue::LineBreak => {} // already handles line breaks globally
        NodeValue::Math(NodeMath { literal, .. }) => ctx.write(literal),
        NodeValue::Strong => {
            let content = ctx.collect(node);
            ctx.write(&format!("{BOLD}{content}{RESET}"));
        }
        NodeValue::Emph => {
            let content = ctx.collect(node);
            ctx.write(&format!("{ITALIC}{content}{RESET}"));
        }
        NodeValue::Strikethrough => {
            let content = ctx.collect(node);
            ctx.write(&format!("{STRIKETHROUGH}{content}{RESET}"));
        }
        NodeValue::Link(_) => {
            let content = ctx.collect(node);
            let cyan = ctx.theme.cyan.fg.clone();
            ctx.write(&format!("{UNDERLINE}{cyan}\u{f0339} {}{RESET}", content));
        }
        NodeValue::Image(_) => {
            let content = ctx.collect(node);
            let cyan = ctx.theme.cyan.fg.clone();
            ctx.write(&format!("{UNDERLINE}{cyan}\u{f0976} {}{RESET}", content));
        }
        NodeValue::Code(node_code) => {
            let surface = ctx.theme.surface.bg.clone();
            let fg_surface = ctx.theme.surface.fg.clone();
            ctx.write(&format!(
                "{fg_surface}{RESET}{surface}{}{RESET}{fg_surface}{RESET}",
                node_code.literal
            ));
        }
        NodeValue::TaskItem(task) => {
            let offset = " ".repeat(data.sourcepos.start.column - 1);
            let checked = task.unwrap_or_default().to_lowercase().to_string() == "x";
            let green = ctx.theme.green.fg.clone();
            let red = ctx.theme.red.fg.clone();
            let checkbox = if checked {
                format!("{offset}{green}\u{f4a7}{RESET}  ")
            } else {
                format!("{offset}{red}\u{e640}{RESET}  ")
            };

            let content = ctx.collect(node);

            ctx.write(&format!("{}{}", checkbox, content));
        }
        NodeValue::HtmlInline(html) => {
            let string_color = ctx.theme.string.fg.clone();
            ctx.write(&format!("{string_color}{html}{RESET}"));
        }
        NodeValue::Raw(str) => {
            ctx.write(str);
        }
        NodeValue::Superscript => {
            ctx.collect_and_write(node);
        }
        NodeValue::MultilineBlockQuote(node_multi_line) => {
            let content = ctx.collect(node);
            let guide = ctx.theme.guide.fg.clone();
            let comment = ctx.theme.comment.fg.clone();
            for (i, line) in content.lines().enumerate() {
                if i != 0 {
                    ctx.cr();
                }
                let offset = " ".repeat(node_multi_line.fence_offset + 1);
                ctx.write(&format!("{guide}▌{offset}{comment}{line}{RESET}"));
            }
        }
        NodeValue::WikiLink(_) => {
            let content = ctx.collect(node);
            let cyan = ctx.theme.cyan.fg.clone();
            ctx.write(&format!("{cyan}\u{f15d6} {}{RESET}", content));
        }
        NodeValue::SpoileredText => {
            let content = ctx.collect(node);
            let comment = ctx.theme.comment.fg.clone();
            ctx.write(&format!("{FAINT}{comment}{content}{RESET}"));
        }
        NodeValue::Alert(node_alert) => {
            let kind = &node_alert.alert_type;
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

            ctx.write(&format!("{}▌ {BOLD}{}{RESET}", color, prefix));

            for child in node.children() {
                let alert_content = ctx.collect(child);

                for line in alert_content.lines() {
                    ctx.write(&format!("\n{}▌{RESET} {}", color, line));
                }
            }
        }
        NodeValue::TableRow(_) => {}          //handled at the table
        NodeValue::TableCell => {}            //handled at the table
        NodeValue::Escaped => {}              //disabled
        NodeValue::DescriptionList => {}      //disabled,
        NodeValue::DescriptionItem(_) => {}   //disabled,
        NodeValue::DescriptionTerm => {}      //disabled,
        NodeValue::DescriptionDetails => {}   //disabled,
        NodeValue::EscapedTag(_) => {}        //disabled
        NodeValue::Underline => {}            //disabled
        NodeValue::Subscript => {}            //disabled
        NodeValue::FootnoteReference(_) => {} // disabled
    }
}

fn string_len(str: &str) -> usize {
    strip_ansi_escapes::strip_str(&str).width()
}

pub fn get_lang_icon_and_color(lang: &str) -> Option<(&'static str, &'static str)> {
    let map: HashMap<&str, (&str, &str)> = [
        // code
        ("python", ("\u{e235}", "\x1b[38;5;214m")), // Python yellow-orange
        ("py", ("\u{e235}", "\x1b[38;5;214m")),
        ("rust", ("\u{e7a8}", "\x1b[38;5;166m")), // Rust orange
        ("rs", ("\u{e7a8}", "\x1b[38;5;166m")),
        ("javascript", ("\u{e74e}", "\x1b[38;5;227m")), // JS yellow
        ("js", ("\u{e74e}", "\x1b[38;5;227m")),
        ("typescript", ("\u{e628}", "\x1b[38;5;75m")), // TS blue
        ("ts", ("\u{e628}", "\x1b[38;5;75m")),
        ("go", ("\u{e627}", "\x1b[38;5;81m")), // Go cyan
        ("golang", ("\u{e627}", "\x1b[38;5;81m")),
        ("c", ("\u{e61e}", "\x1b[38;5;68m")),    // C blue
        ("cpp", ("\u{e61d}", "\x1b[38;5;204m")), // C++ pink-red
        ("c++", ("\u{e61d}", "\x1b[38;5;204m")),
        ("cc", ("\u{e61d}", "\x1b[38;5;204m")),
        ("cxx", ("\u{e61d}", "\x1b[38;5;204m")),
        ("java", ("\u{e738}", "\x1b[38;5;208m")), // Java orange
        ("csharp", ("\u{f81a}", "\x1b[38;5;129m")), // C# purple
        ("cs", ("\u{f81a}", "\x1b[38;5;129m")),
        ("ruby", ("\u{e21e}", "\x1b[38;5;196m")), // Ruby red
        ("rb", ("\u{e21e}", "\x1b[38;5;196m")),
        ("php", ("\u{e73d}", "\x1b[38;5;99m")), // PHP purple
        ("swift", ("\u{e755}", "\x1b[38;5;202m")), // Swift orange
        ("kotlin", ("\u{e634}", "\x1b[38;5;141m")), // Kotlin purple
        ("kt", ("\u{e634}", "\x1b[38;5;141m")),
        ("dart", ("\u{e798}", "\x1b[38;5;39m")), // Dart blue
        ("lua", ("\u{e620}", "\x1b[38;5;33m")),  // Lua blue
        ("sh", ("\u{f489}", "\x1b[38;5;34m")),   // Shell green
        ("bash", ("\u{f489}", "\x1b[38;5;34m")),
        ("zsh", ("\u{f489}", "\x1b[38;5;34m")),
        ("fish", ("\u{f489}", "\x1b[38;5;34m")),
        ("html", ("\u{e736}", "\x1b[38;5;202m")), // HTML orange
        ("htm", ("\u{e736}", "\x1b[38;5;202m")),
        ("css", ("\u{e749}", "\x1b[38;5;75m")),   // CSS blue
        ("scss", ("\u{e749}", "\x1b[38;5;199m")), // SCSS pink
        ("sass", ("\u{e74b}", "\x1b[38;5;199m")), // Sass pink
        ("less", ("\u{e758}", "\x1b[38;5;54m")),  // Less purple
        ("jsx", ("\u{e7ba}", "\x1b[38;5;81m")),   // React cyan
        ("tsx", ("\u{e7ba}", "\x1b[38;5;81m")),
        ("vue", ("\u{fd42}", "\x1b[38;5;83m")),   // Vue green
        ("json", ("\u{e60b}", "\x1b[38;5;185m")), // JSON yellow
        ("yaml", ("\u{f0c5}", "\x1b[38;5;167m")), // YAML orange-red
        ("yml", ("\u{f0c5}", "\x1b[38;5;167m")),
        ("toml", ("\u{e6b2}", "\x1b[38;5;131m")),
        ("svg", ("\u{f0721}", "\x1b[38;5;178m")),
        ("xml", ("\u{e619}", "\x1b[38;5;172m")), // XML orange
        ("md", ("\u{f48a}", "\x1b[38;5;255m")),  // Markdown white
        ("markdown", ("\u{f48a}", "\x1b[38;5;255m")),
        ("rst", ("\u{f15c}", "\x1b[38;5;248m")), // reStructuredText gray
        ("tex", ("\u{e600}", "\x1b[38;5;25m")),  // LaTeX blue
        ("latex", ("\u{e600}", "\x1b[38;5;25m")),
        ("txt", ("\u{f15c}", "\x1b[38;5;248m")), // Text gray
        ("text", ("\u{f15c}", "\x1b[38;5;248m")),
        ("log", ("\u{f18d}", "\x1b[38;5;242m")), // Log dark gray
        ("ini", ("\u{f17a}", "\x1b[38;5;172m")), // INI orange
        ("conf", ("\u{f0ad}", "\x1b[38;5;172m")), // Config orange
        ("config", ("\u{f0ad}", "\x1b[38;5;172m")),
        ("env", ("\u{f462}", "\x1b[38;5;227m")), // Environment yellow
        ("dockerfile", ("\u{f308}", "\x1b[38;5;39m")), // Docker cyan
        ("docker", ("\u{f308}", "\x1b[38;5;39m")),
        ("asm", ("\u{f471}", "\x1b[38;5;124m")), // Assembly dark red
        ("s", ("\u{f471}", "\x1b[38;5;124m")),
        ("haskell", ("\u{e777}", "\x1b[38;5;99m")), // Haskell purple
        ("hs", ("\u{e777}", "\x1b[38;5;99m")),
        ("elm", ("\u{e62c}", "\x1b[38;5;33m")),     // Elm blue
        ("clojure", ("\u{e768}", "\x1b[38;5;34m")), // Clojure green
        ("clj", ("\u{e768}", "\x1b[38;5;34m")),
        ("scala", ("\u{e737}", "\x1b[38;5;196m")), // Scala red
        ("erlang", ("\u{e7b1}", "\x1b[38;5;125m")), // Erlang magenta
        ("erl", ("\u{e7b1}", "\x1b[38;5;125m")),
        ("elixir", ("\u{e62d}", "\x1b[38;5;99m")), // Elixir purple
        ("ex", ("\u{e62d}", "\x1b[38;5;99m")),
        ("exs", ("\u{e62d}", "\x1b[38;5;99m")),
        ("perl", ("\u{e769}", "\x1b[38;5;33m")), // Perl blue
        ("pl", ("\u{e769}", "\x1b[38;5;33m")),
        ("r", ("\u{f25d}", "\x1b[38;5;33m")),       // R blue
        ("matlab", ("\u{f799}", "\x1b[38;5;202m")), // MATLAB orange
        ("m", ("\u{f799}", "\x1b[38;5;202m")),
        ("octave", ("\u{f799}", "\x1b[38;5;202m")), // Octave orange
        ("zig", ("\u{e6a9}", "\x1b[38;5;214m")),
        ("h", ("\u{e61e}", "\x1b[38;5;110m")),
        ("lock", ("\u{f023}", "\x1b[38;5;244m")),
        // images
        ("png", ("\u{f1c5}", "\x1b[38;5;117m")),
        ("jpg", ("\u{f1c5}", "\x1b[38;5;110m")),
        ("jpeg", ("\u{f1c5}", "\x1b[38;5;110m")),
        ("gif", ("\u{f1c5}", "\x1b[38;5;213m")),
        ("bmp", ("\u{f1c5}", "\x1b[38;5;103m")),
        ("webp", ("\u{f1c5}", "\x1b[38;5;149m")),
        ("tiff", ("\u{f1c5}", "\x1b[38;5;144m")),
        ("ico", ("\u{f1c5}", "\x1b[38;5;221m")),
        // videos
        ("mp4", ("\u{f03d}", "\x1b[38;5;203m")),
        ("mkv", ("\u{f03d}", "\x1b[38;5;132m")),
        ("webm", ("\u{f03d}", "\x1b[38;5;111m")),
        ("mov", ("\u{f03d}", "\x1b[38;5;173m")),
        ("avi", ("\u{f03d}", "\x1b[38;5;167m")),
        ("flv", ("\u{f03d}", "\x1b[38;5;131m")),
        // audio
        ("mp3", ("\u{f001}", "\x1b[38;5;215m")),
        ("ogg", ("\u{f001}", "\x1b[38;5;109m")),
        ("flac", ("\u{f001}", "\x1b[38;5;113m")),
        ("wav", ("\u{f001}", "\x1b[38;5;123m")),
        ("m4a", ("\u{f001}", "\x1b[38;5;174m")),
        // archive
        ("zip", ("\u{f410}", "\x1b[38;5;180m")),
        ("tar", ("\u{f410}", "\x1b[38;5;180m")),
        ("gz", ("\u{f410}", "\x1b[38;5;180m")),
        ("rar", ("\u{f410}", "\x1b[38;5;180m")),
        ("7z", ("\u{f410}", "\x1b[38;5;180m")),
        ("xz", ("\u{f410}", "\x1b[38;5;180m")),
        // documents
        ("pdf", ("\u{f1c1}", "\x1b[38;5;196m")),
        ("doc", ("\u{f1c2}", "\x1b[38;5;33m")),
        ("docx", ("\u{f1c2}", "\x1b[38;5;33m")),
        ("xls", ("\u{f1c3}", "\x1b[38;5;70m")),
        ("xlsx", ("\u{f1c3}", "\x1b[38;5;70m")),
        ("ppt", ("\u{f1c4}", "\x1b[38;5;166m")),
        ("pptx", ("\u{f1c4}", "\x1b[38;5;166m")),
        ("odt", ("\u{f1c2}", "\x1b[38;5;33m")),
        ("epub", ("\u{f02d}", "\x1b[38;5;135m")),
        ("csv", ("\u{f1c3}", "\x1b[38;5;190m")),
        // fonts
        ("ttf", ("\u{f031}", "\x1b[38;5;98m")),
        ("otf", ("\u{f031}", "\x1b[38;5;98m")),
        ("woff", ("\u{f031}", "\x1b[38;5;98m")),
        ("woff2", ("\u{f031}", "\x1b[38;5;98m")),
    ]
    .into();

    map.get(lang.to_lowercase().as_str()).copied()
}

fn wrap_highlighted_line(original: String, width: usize, sub_prefix: &str) -> String {
    if string_len(&original) <= width {
        return original;
    }
    let lines: Vec<String> = textwrap::wrap(&original, width)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect();
    let mut buf = String::new();
    for (i, line) in lines.iter().enumerate() {
        if i == 0 || line.trim().is_empty() {
            buf.push_str(line);
        } else {
            buf.push_str(&format!("\n{sub_prefix}{line}"));
        }
    }
    buf.push('\n');
    buf
}
fn format_code_simple(code: &str, lang: &str, ctx: &mut AnsiContext, indent: usize) {
    let (title, color) = match get_lang_icon_and_color(lang) {
        Some((icon, color)) => (format!("{color}{icon} {lang}"), color),
        None => (lang.to_owned(), ""),
    };

    let top = format!("{color}[ {} ]{RESET}\n", title);
    let surface = ctx.theme.surface.bg.clone();

    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ts);

    let mut buf = String::new();
    let twidth = term_misc::get_wininfo().sc_width - indent.saturating_sub(1) as u16;
    buf.push_str(&top);
    let count = code.lines().count();
    for (i, line) in LinesWithEndings::from(code).enumerate() {
        if i == count && line.trim().is_empty() {
            continue;
        }
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
        let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
        let highlighted = wrap_highlighted_line(highlighted, twidth as usize - 4, "  ");
        buf.push_str(&highlighted);
    }

    let mut bg_formatted_lines = String::new();
    for (i, line) in buf.lines().enumerate() {
        let left_space = (twidth as usize).saturating_sub(string_len(line));
        if i == 0 {
            let suffix = format!("{surface}{}", " ".repeat(left_space));
            bg_formatted_lines.push_str(&format!("{surface}{line}{suffix}{RESET}"));
        } else {
            let suffix = format!("{surface}{}", " ".repeat(left_space.saturating_sub(2)));
            bg_formatted_lines.push_str(&format!("\n{surface}  {line}{suffix}{RESET}"));
        }
    }
    ctx.write(&bg_formatted_lines);
}
fn format_code(code: &str, lang: &str, ctx: &mut AnsiContext, indent: usize) {
    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ts);

    let header = match get_lang_icon_and_color(lang) {
        Some((icon, color)) => &format!("{color}{icon} {lang}",),
        None => lang,
    };

    let max_lines = code.lines().count();
    let num_width = max_lines.to_string().chars().count() + 2;
    let term_width = term_misc::get_wininfo().sc_width - indent.saturating_sub(1) as u16;
    let text_size = term_width as usize - num_width;
    let color = ctx.theme.border.fg.clone();

    let top_header = format!(
        "{color}{}┬{}{RESET}",
        "─".repeat(num_width),
        "─".repeat(term_width as usize - num_width - 1)
    );
    let middle_header = format!("{color}{}│ {header}{RESET}", " ".repeat(num_width),);
    let bottom_header = format!(
        "{color}{}┼{}{RESET}",
        "─".repeat(num_width),
        "─".repeat(term_width as usize - num_width - 1)
    );
    ctx.write(&top_header);
    ctx.cr();
    ctx.write(&middle_header);
    ctx.cr();
    ctx.write(&bottom_header);
    ctx.cr();

    let mut num = 1;
    let prefix = format!("{}│  ", " ".repeat(num_width));
    for line in LinesWithEndings::from(code) {
        let left_space = num_width - num.to_string().chars().count();
        let left_offset = left_space / 2;
        let right_offset = left_space - left_offset;
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
        let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
        let highlighted = wrap_highlighted_line(highlighted, text_size - 2, &prefix);
        ctx.write(&format!(
            "{color}{}{num}{}│ {RESET}{}",
            " ".repeat(left_offset),
            " ".repeat(right_offset),
            highlighted
        ));
        num += 1;
    }

    let last_border = format!(
        "{color}{}┴{}{RESET}",
        "─".repeat(num_width),
        "─".repeat(term_width as usize - num_width - 1)
    );
    ctx.write(&last_border);
}

fn br() -> String {
    "━".repeat(term_misc::get_wininfo().sc_width as usize)
}

#[derive(Debug, Clone)]
pub struct ThemeColor {
    value: String,
    color: Color,
    bg: String,
    fg: String,
}

impl From<&str> for ThemeColor {
    fn from(hex_color: &str) -> Self {
        let color = hex_to_rgba(&hex_color);
        let (r, g, b) = (color.r, color.g, color.b);

        ThemeColor {
            value: hex_color.to_owned(),
            color,
            bg: format!("\x1b[48;2;{};{};{}m", r, g, b),
            fg: format!("\x1b[38;2;{};{};{}m", r, g, b),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CustomTheme {
    pub keyword: ThemeColor,
    pub function: ThemeColor,
    pub string: ThemeColor,
    pub module: ThemeColor,
    pub constant: ThemeColor,
    pub comment: ThemeColor,
    pub foreground: ThemeColor,
    pub guide: ThemeColor,
    pub background: ThemeColor,
    pub surface: ThemeColor,
    pub border: ThemeColor,

    red: ThemeColor,
    green: ThemeColor,
    blue: ThemeColor,
    cyan: ThemeColor,
    yellow: ThemeColor,

    #[allow(dead_code)]
    magenta: ThemeColor,
    #[allow(dead_code)]
    white: ThemeColor,
    #[allow(dead_code)]
    black: ThemeColor,
}

fn hex_to_rgba(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    Color { r, g, b, a: 255 }
}

impl CustomTheme {
    pub fn autumn() -> Self {
        CustomTheme {
            keyword: "#fc6501".into(),
            function: "#fac25a".into(),
            string: "#a1cd32".into(),
            module: "#fc4c4c".into(),
            constant: "#FF6B9D".into(),
            comment: "#5C6773".into(),
            foreground: "#FFFFFF".into(),
            guide: "#2D3640".into(),
            background: "#14161f".into(),
            surface: "#2a2a38".into(),
            border: "#5C6773".into(),

            red: "#fc4c4c".into(),
            green: "#a1cd32".into(),
            blue: "#5abffa".into(),
            cyan: "#5abffa".into(),
            magenta: "#FF6B9D".into(),
            yellow: "#fac25a".into(),
            white: "#FFFFFF".into(),
            black: "#2e3339".into(),
        }
    }

    pub fn spring() -> Self {
        CustomTheme {
            keyword: "#FFB347".into(),
            function: "#D4FF59".into(),
            string: "#8CEB3A".into(),
            module: "#66E6FF".into(),
            constant: "#D8A5FF".into(),
            comment: "#5C6773".into(),
            foreground: "#FFFFFF".into(),
            guide: "#2D3640".into(),
            background: "#14161f".into(),
            surface: "#2a2a38".into(),
            border: "#5C6773".into(),

            red: "#FF5555".into(),
            green: "#D4FF59".into(),
            blue: "#66E6FF".into(),
            cyan: "#66E6FF".into(),
            magenta: "#D2A6FF".into(),
            yellow: "#FFB347".into(),
            white: "#FFFFFF".into(),
            black: "#2e3339".into(),
        }
    }
    pub fn makurai_mage() -> Self {
        CustomTheme {
            keyword: "#FF7733".into(),
            function: "#FFEE99".into(),
            string: "#95FB79".into(),
            module: "#82AAFF".into(),
            constant: "#D2A6FF".into(),
            comment: "#5C6773".into(),
            foreground: "#FFFFFF".into(),
            guide: "#2D3640".into(),
            background: "#14161f".into(),
            surface: "#2a2a38".into(),
            border: "#5C6773".into(),

            red: "#FF5555".into(),
            green: "#95FB79".into(),
            blue: "#82AAFF".into(),
            cyan: "#66D9EF".into(),
            magenta: "#FF77FF".into(),
            yellow: "#FFEE99".into(),
            white: "#FFFFFF".into(),
            black: "#14161f".into(),
        }
    }
    pub fn makurai_healer() -> Self {
        CustomTheme {
            keyword: "#E35043".into(),
            function: "#3D76F3".into(),
            string: "#51A150".into(),
            module: "#AB31A9".into(),
            constant: "#976700".into(),
            comment: "#A0A1A7".into(),
            foreground: "#323640".into(),
            guide: "#D1D5DB".into(),
            background: "#f8f8fc".into(),
            surface: "#ebebf4".into(),
            border: "#7e8a9e".into(),

            red: "#E35043".into(),
            green: "#51A150".into(),
            blue: "#3D76F3".into(),
            cyan: "#00BFCF".into(),
            magenta: "#AB31A9".into(),
            yellow: "#FFCC00".into(),
            white: "#FFFFFF".into(),
            black: "#000000".into(),
        }
    }
    pub fn monokai() -> Self {
        CustomTheme {
            keyword: "#F92672".into(),
            function: "#A6E22E".into(),
            string: "#E6DB74".into(),
            module: "#66D9EF".into(),
            constant: "#AE81FF".into(),
            comment: "#75715E".into(),
            foreground: "#F8F8F2".into(),
            guide: "#3E3D32".into(),
            background: "#272822".into(),
            surface: "#3E3D32".into(),
            border: "#49483E".into(),

            red: "#F92672".into(),
            green: "#A6E22E".into(),
            blue: "#66D9EF".into(),
            cyan: "#66D9EF".into(),
            magenta: "#AE81FF".into(),
            yellow: "#E6DB74".into(),
            white: "#F8F8F2".into(),
            black: "#272822".into(),
        }
    }
    pub fn catppuccin() -> Self {
        CustomTheme {
            keyword: "#CBA6F7".into(),
            function: "#89B4FA".into(),
            string: "#A6E3A1".into(),
            module: "#89DCEB".into(),
            constant: "#F38BA8".into(),
            comment: "#7F849C".into(),
            foreground: "#CDD6F4".into(),
            guide: "#45475A".into(),
            background: "#1E1E2E".into(),
            surface: "#313244".into(),
            border: "#45475A".into(),

            red: "#F38BA8".into(),
            green: "#A6E3A1".into(),
            blue: "#89B4FA".into(),
            cyan: "#89DCEB".into(),
            magenta: "#CBA6F7".into(),
            yellow: "#F9E2AF".into(),
            white: "#CDD6F4".into(),
            black: "#1E1E2E".into(),
        }
    }
    pub fn tokyo_night() -> Self {
        CustomTheme {
            keyword: "#BB9AF7".into(),
            function: "#7AA2F7".into(),
            string: "#9ECE6A".into(),
            module: "#2AC3DE".into(),
            constant: "#FF9E64".into(),
            comment: "#565F89".into(),
            foreground: "#C0CAF5".into(),
            guide: "#3B4261".into(),
            background: "#1A1B26".into(),
            surface: "#24283B".into(),
            border: "#414868".into(),

            red: "#F7768E".into(),
            green: "#9ECE6A".into(),
            blue: "#7AA2F7".into(),
            cyan: "#2AC3DE".into(),
            magenta: "#BB9AF7".into(),
            yellow: "#E0AF68".into(),
            white: "#C0CAF5".into(),
            black: "#1A1B26".into(),
        }
    }
    pub fn dracula() -> Self {
        CustomTheme {
            keyword: "#FF79C6".into(),
            function: "#50FA7B".into(),
            string: "#F1FA8C".into(),
            module: "#8BE9FD".into(),
            constant: "#BD93F9".into(),
            comment: "#6272A4".into(),
            foreground: "#F8F8F2".into(),
            guide: "#44475A".into(),
            background: "#282A36".into(),
            surface: "#44475A".into(),
            border: "#44475A".into(),

            red: "#FF5555".into(),
            green: "#50FA7B".into(),
            blue: "#8BE9FD".into(),
            cyan: "#8BE9FD".into(),
            magenta: "#FF79C6".into(),
            yellow: "#F1FA8C".into(),
            white: "#F8F8F2".into(),
            black: "#282A36".into(),
        }
    }
    pub fn nord() -> Self {
        CustomTheme {
            keyword: "#81A1C1".into(),
            function: "#88C0D0".into(),
            string: "#A3BE8C".into(),
            module: "#8FBCBB".into(),
            constant: "#B48EAD".into(),
            comment: "#616E88".into(),
            foreground: "#D8DEE9".into(),
            guide: "#434C5E".into(),
            background: "#2E3440".into(),
            surface: "#3B4252".into(),
            border: "#434C5E".into(),

            red: "#BF616A".into(),
            green: "#A3BE8C".into(),
            blue: "#81A1C1".into(),
            cyan: "#88C0D0".into(),
            magenta: "#B48EAD".into(),
            yellow: "#EBCB8B".into(),
            white: "#D8DEE9".into(),
            black: "#2E3440".into(),
        }
    }
    pub fn gruvbox() -> Self {
        CustomTheme {
            keyword: "#FB4934".into(),
            function: "#FABD2F".into(),
            string: "#B8BB26".into(),
            module: "#83A598".into(),
            constant: "#D3869B".into(),
            comment: "#928374".into(),
            foreground: "#EBDBB2".into(),
            guide: "#504945".into(),
            background: "#282828".into(),
            surface: "#3C3836".into(),
            border: "#665C54".into(),

            red: "#FB4934".into(),
            green: "#B8BB26".into(),
            blue: "#83A598".into(),
            cyan: "#8EC07C".into(),
            magenta: "#D3869B".into(),
            yellow: "#FABD2F".into(),
            white: "#EBDBB2".into(),
            black: "#282828".into(),
        }
    }
    pub fn solarized() -> Self {
        CustomTheme {
            keyword: "#268BD2".into(),
            function: "#B58900".into(),
            string: "#2AA198".into(),
            module: "#859900".into(),
            constant: "#D33682".into(),
            comment: "#586E75".into(),
            foreground: "#839496".into(),
            guide: "#073642".into(),
            background: "#002B36".into(),
            surface: "#073642".into(),
            border: "#586E75".into(),

            red: "#DC322F".into(),
            green: "#859900".into(),
            blue: "#268BD2".into(),
            cyan: "#2AA198".into(),
            magenta: "#D33682".into(),
            yellow: "#B58900".into(),
            white: "#EEE8D5".into(),
            black: "#002B36".into(),
        }
    }
    pub fn one_dark() -> Self {
        CustomTheme {
            keyword: "#C678DD".into(),
            function: "#61AFEF".into(),
            string: "#98C379".into(),
            module: "#56B6C2".into(),
            constant: "#E06C75".into(),
            comment: "#5C6370".into(),
            foreground: "#ABB2BF".into(),
            guide: "#3E4451".into(),
            background: "#282C34".into(),
            surface: "#3E4451".into(),
            border: "#3E4451".into(),

            red: "#E06C75".into(),
            green: "#98C379".into(),
            blue: "#61AFEF".into(),
            cyan: "#56B6C2".into(),
            magenta: "#C678DD".into(),
            yellow: "#E5C07B".into(),
            white: "#ABB2BF".into(),
            black: "#282C34".into(),
        }
    }
    pub fn github() -> Self {
        CustomTheme {
            keyword: "#FF7B72".into(),
            function: "#D2A8FF".into(),
            string: "#A5D6FF".into(),
            module: "#FFA657".into(),
            constant: "#79C0FF".into(),
            comment: "#8B949E".into(),
            foreground: "#F0F6FC".into(),
            guide: "#30363D".into(),
            background: "#0D1117".into(),
            surface: "#161B22".into(),
            border: "#30363D".into(),

            red: "#F85149".into(),
            green: "#56D364".into(),
            blue: "#58A6FF".into(),
            cyan: "#39D0D6".into(),
            magenta: "#BC8CFF".into(),
            yellow: "#E3B341".into(),
            white: "#F0F6FC".into(),
            black: "#0D1117".into(),
        }
    }
    pub fn material() -> Self {
        CustomTheme {
            keyword: "#C792EA".into(),
            function: "#82AAFF".into(),
            string: "#C3E88D".into(),
            module: "#FFCB6B".into(),
            constant: "#F78C6C".into(),
            comment: "#546E7A".into(),
            foreground: "#EEFFFF".into(),
            guide: "#37474F".into(),
            background: "#263238".into(),
            surface: "#2E3C43".into(),
            border: "#37474F".into(),

            red: "#F07178".into(),
            green: "#C3E88D".into(),
            blue: "#82AAFF".into(),
            cyan: "#89DDFF".into(),
            magenta: "#C792EA".into(),
            yellow: "#FFCB6B".into(),
            white: "#EEFFFF".into(),
            black: "#263238".into(),
        }
    }
    pub fn ayu() -> Self {
        CustomTheme {
            keyword: "#FF8F40".into(),
            function: "#FFB454".into(),
            string: "#AAD94C".into(),
            module: "#59C2FF".into(),
            constant: "#D2A6FF".into(),
            comment: "#ACB6BF8C".into(),
            foreground: "#BFBDB6".into(),
            guide: "#1F2430".into(),
            background: "#0A0E14".into(),
            surface: "#11151C".into(),
            border: "#1F2430".into(),

            red: "#F28779".into(),
            green: "#AAD94C".into(),
            blue: "#59C2FF".into(),
            cyan: "#95E6CB".into(),
            magenta: "#D2A6FF".into(),
            yellow: "#FFB454".into(),
            white: "#BFBDB6".into(),
            black: "#0A0E14".into(),
        }
    }
    pub fn ayu_mirage() -> Self {
        CustomTheme {
            keyword: "#FFA759".into(),
            function: "#FFD580".into(),
            string: "#BAE67E".into(),
            module: "#73D0FF".into(),
            constant: "#D4BFFF".into(),
            comment: "#5C6773".into(),
            foreground: "#CBCCC6".into(),
            guide: "#242936".into(),
            background: "#1F2430".into(),
            surface: "#242936".into(),
            border: "#343B4C".into(),

            red: "#FF6666".into(),
            green: "#BAE67E".into(),
            blue: "#73D0FF".into(),
            cyan: "#95E6CB".into(),
            magenta: "#D4BFFF".into(),
            yellow: "#FFD580".into(),
            white: "#CBCCC6".into(),
            black: "#1F2430".into(),
        }
    }
    pub fn synthwave() -> Self {
        CustomTheme {
            keyword: "#FF7EDB".into(),
            function: "#36F9F6".into(),
            string: "#E6DB74".into(),
            module: "#FE4450".into(),
            constant: "#FF8CC8".into(),
            comment: "#848077".into(),
            foreground: "#F8F8F2".into(),
            guide: "#2A2139".into(),
            background: "#262335".into(),
            surface: "#34294F".into(),
            border: "#495495".into(),

            red: "#FE4450".into(),
            green: "#72F1B8".into(),
            blue: "#36F9F6".into(),
            cyan: "#36F9F6".into(),
            magenta: "#FF7EDB".into(),
            yellow: "#FEE715".into(),
            white: "#F8F8F2".into(),
            black: "#262335".into(),
        }
    }
    pub fn rose_pine() -> Self {
        CustomTheme {
            keyword: "#C4A7E7".into(),    // Iris (purple)
            function: "#9CCFD8".into(),   // Foam (cyan)
            string: "#F6C177".into(),     // Gold
            module: "#EBBCBA".into(),     // Rose
            constant: "#EB6F92".into(),   // Love (pink)
            comment: "#6E6A86".into(),    // Muted
            foreground: "#E0DEF4".into(), // Text
            guide: "#26233A".into(),      // Highlight low
            background: "#191724".into(), // Base
            surface: "#1F1D2E".into(),    // Surface
            border: "#403D52".into(),     // Highlight med

            red: "#EB6F92".into(),     // Love
            green: "#31748F".into(),   // Pine
            blue: "#9CCFD8".into(),    // Foam
            cyan: "#9CCFD8".into(),    // Foam
            magenta: "#C4A7E7".into(), // Iris
            yellow: "#F6C177".into(),  // Gold
            white: "#E0DEF4".into(),   // Text
            black: "#191724".into(),   // Base
        }
    }
    pub fn kanagawa() -> Self {
        CustomTheme {
            keyword: "#957FB8".into(),    // Oniviolet
            function: "#7AA89F".into(),   // Waveaqua1
            string: "#98BB6C".into(),     // Autumngreen
            module: "#7FB4CA".into(),     // Crystalblue
            constant: "#D27E99".into(),   // Sakurapink
            comment: "#727169".into(),    // Fujiwhite
            foreground: "#DCD7BA".into(), // Fujiwhite
            guide: "#2A2A37".into(),      // Waveblue1
            background: "#1F1F28".into(), // Sumiink0
            surface: "#16161D".into(),    // Sumiink1
            border: "#54546D".into(),     // Sumiink4

            red: "#C34043".into(),     // Peachred
            green: "#76946A".into(),   // Springgreen
            blue: "#7E9CD8".into(),    // Springblue
            cyan: "#6A9589".into(),    // Waveaqua2
            magenta: "#938AA9".into(), // Oniviolet2
            yellow: "#C0A36E".into(),  // Carpyellow
            white: "#DCD7BA".into(),   // Fujiwhite
            black: "#1F1F28".into(),   // Sumiink0
        }
    }
    pub fn everforest() -> Self {
        CustomTheme {
            keyword: "#E67E80".into(),    // Red
            function: "#A7C080".into(),   // Green
            string: "#DBBC7F".into(),     // Yellow
            module: "#7FBBB3".into(),     // Aqua
            constant: "#D699B6".into(),   // Purple
            comment: "#7A8478".into(),    // Grey1
            foreground: "#D3C6AA".into(), // Fg
            guide: "#3D484D".into(),      // Bg2
            background: "#2D353B".into(), // Bg0
            surface: "#343F44".into(),    // Bg1
            border: "#504945".into(),     // Grey0

            red: "#E67E80".into(),     // Red
            green: "#A7C080".into(),   // Green
            blue: "#7FBBB3".into(),    // Blue
            cyan: "#83C092".into(),    // Aqua
            magenta: "#D699B6".into(), // Purple
            yellow: "#DBBC7F".into(),  // Yellow
            white: "#D3C6AA".into(),   // Fg
            black: "#2D353B".into(),   // Bg0
        }
    }
    pub fn vscode() -> Self {
        CustomTheme {
            keyword: "#569CD6".into(),    // Keyword blue
            function: "#DCDCAA".into(),   // Function yellow
            string: "#CE9178".into(),     // String orange
            module: "#4EC9B0".into(),     // Type teal
            constant: "#B5CEA8".into(),   // Number green
            comment: "#6A9955".into(),    // Comment green
            foreground: "#D4D4D4".into(), // Editor foreground
            guide: "#404040".into(),      // Indent guide
            background: "#1E1E1E".into(), // Editor background
            surface: "#252526".into(),    // Side bar background
            border: "#3E3E42".into(),     // Panel border

            red: "#F44747".into(),     // Error red
            green: "#6A9955".into(),   // String green
            blue: "#569CD6".into(),    // Info blue
            cyan: "#4EC9B0".into(),    // Cyan
            magenta: "#C586C0".into(), // Keyword magenta
            yellow: "#DCDCAA".into(),  // Warning yellow
            white: "#D4D4D4".into(),   // White
            black: "#1E1E1E".into(),   // Black
        }
    }

    pub fn to_syntect_theme(&self) -> Theme {
        let mut settings = ThemeSettings::default();
        settings.foreground = Some(self.foreground.color);
        settings.background = Some(self.surface.color);
        settings.guide = Some(self.guide.color);

        let mut theme = Theme {
            name: None,
            author: None,
            settings,
            scopes: vec![],
        };

        fn create_selectors(selectors: &str) -> ScopeSelectors {
            ScopeSelectors::from_str(selectors).unwrap_or_default()
        }
        fn create_style(color: Color) -> StyleModifier {
            StyleModifier {
                foreground: Some(color),
                background: None,
                font_style: None,
            }
        }

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("keyword, storage.modifier, storage.type"),
            style: create_style(self.keyword.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("entity.name.function, support.function, variable.function"),
            style: create_style(self.function.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors(
                "module, struct, enum, generic, path, meta.path, entity.name.tag, support.type, meta.import-name",
            ),
            style: create_style(self.module.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors(
                "string, punctuation.string, constant.other.color, punctuation.definition.string",
            ),
            style: create_style(self.string.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("constant, keyword.other.unit, support.constant"),
            style: create_style(self.constant.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors("comment, punctuation.comment, punctuation.definition.comment"),
            style: create_style(self.comment.color),
        });

        theme.scopes.push(syntect::highlighting::ThemeItem {
            scope: create_selectors(
                "variable, operator, punctuation, block, support.type.property-name, punctuation.definition, keyword.operator",
            ),
            style: create_style(self.foreground.color),
        });

        theme
    }

    pub fn to_html_style(&self) -> String {
        let root_css = format!(
            r#"
:root {{
  --keyword: {};
  --function: {};
  --type: {};
  --constant: {};
  --comment: {};
  --foreground: {};
  
  /* UI Colors */
  --background: {};
  --surface: {};
  --border: {};
}}
"#,
            self.keyword.value,
            self.function.value,
            self.module.value,
            self.constant.value,
            self.comment.value,
            self.foreground.value,
            self.background.value,
            self.surface.value,
            self.border.value
        );
        let full_css = include_str!("../assets/style.css");
        format!("{full_css}\n\n{root_css}")
    }
}
