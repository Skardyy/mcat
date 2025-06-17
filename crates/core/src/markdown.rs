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

const RESET: &str = "\x1B[0m";
const BOLD: &str = "\x1B[1m";
const ITALIC: &str = "\x1B[3m";
const UNDERLINE: &str = "\x1B[4m";
const STRIKETHROUGH: &str = "\x1B[9m";
const FAINT: &str = "\x1b[2m";

const FG_RED: &str = "\x1B[31m";
const FG_GREEN: &str = "\x1B[32m";
const FG_YELLOW: &str = "\x1B[33m";
const FG_BLUE: &str = "\x1B[34m";
const FG_CYAN: &str = "\x1B[36m";
const FG_BRIGHT_BLACK: &str = "\x1B[90m";
const FG_BRIGHT_RED: &str = "\x1B[91m";
const FG_BRIGHT_GREEN: &str = "\x1B[92m";
const FG_BRIGHT_YELLOW: &str = "\x1B[93m";
const FG_BRIGHT_BLUE: &str = "\x1B[94m";
const FG_BRIGHT_CYAN: &str = "\x1B[96m";

struct AnsiContext {
    ps: SyntaxSet,
    theme: CustomTheme,
    hide_line_numbers: bool,
    line: AtomicUsize,
    output: String,
}
impl AnsiContext {
    fn write(&mut self, val: &str) {
        self.output.push_str(val);
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
        "light" => CustomTheme::light(),
        _ => CustomTheme::dark(),
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

            for (i, line) in block_content.lines().enumerate() {
                if i != 0 {
                    ctx.cr();
                }
                ctx.write(&format!("{FG_YELLOW}▌ {RESET}{}", line));
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
            ctx.write(&format!(
                "{}{FG_YELLOW}\0 {content}",
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
            if ctx.hide_line_numbers {
                format_code_simple(code, lang, ctx);
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
                ctx.write(&block);
                ctx.write(&format!(
                    "{surface}{}{FG_YELLOW}{BOLD}{title}{surface}{}{RESET}\n",
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
        }
        NodeValue::Paragraph => {
            ctx.collect_and_write(node);
        }
        NodeValue::Heading(node_heading) => {
            let prefix = "∙".repeat(node_heading.level as usize);
            let content = ctx.collect(node);
            ctx.write(&format!("{BOLD}{FG_BLUE}{prefix} {content}{RESET}"));
        }
        NodeValue::ThematicBreak => {
            let br = br();
            ctx.write(&format!("{FG_BRIGHT_BLACK}{br}{RESET}"));
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

            let color = FG_BLUE;
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
        NodeValue::Link(node_link) => {
            let content = ctx.collect(node);
            ctx.write(&format!(
                "{UNDERLINE}{FG_CYAN}\u{eb01} {}{RESET} {FG_BRIGHT_BLACK}({}){RESET}",
                content, node_link.url
            ));
        }
        NodeValue::Image(node_link) => {
            let content = ctx.collect(node);
            ctx.write(&format!(
                "{UNDERLINE}{FG_CYAN}\u{f03e} {}{RESET} {FG_BRIGHT_BLACK}({}){RESET}",
                content, node_link.url
            ));
        }
        NodeValue::Code(node_code) => {
            let surface = ctx.theme.surface.bg.clone();
            let fg_surface = ctx.theme.surface.fg.clone();
            ctx.write(&format!(
                "{fg_surface}{FG_GREEN}{surface}{}{RESET}{fg_surface}{RESET}",
                node_code.literal
            ));
        }
        NodeValue::TaskItem(task) => {
            let offset = " ".repeat(data.sourcepos.start.column - 1);
            let checked = task.unwrap_or_default().to_lowercase().to_string() == "x";
            let checkbox = if checked {
                format!("{offset}{FG_GREEN}\u{f4a7}{RESET}  ")
            } else {
                format!("{offset}{FG_RED}\u{e640}{RESET}  ")
            };

            let content = ctx.collect(node);

            ctx.write(&format!("{}{}", checkbox, content));
        }
        NodeValue::HtmlInline(html) => {
            ctx.write(&format!("{FG_BLUE}{html}{RESET}"));
        }
        NodeValue::Raw(str) => {
            ctx.write(str);
        }
        NodeValue::Superscript => {
            ctx.collect_and_write(node);
        }
        NodeValue::MultilineBlockQuote(node_multi_line) => {
            let content = ctx.collect(node);
            for (i, line) in content.lines().enumerate() {
                if i != 0 {
                    ctx.cr();
                }
                let offset = " ".repeat(node_multi_line.fence_offset + 1);
                ctx.write(&format!("{FG_GREEN}▌{offset}{line}"));
            }
        }
        NodeValue::WikiLink(node_wiki_link) => {
            let content = ctx.collect(node);
            ctx.write(&format!(
                "{FG_CYAN}\u{f15d6} {}{RESET} {FG_BRIGHT_BLACK}({}){RESET}",
                content, node_wiki_link.url
            ));
        }
        NodeValue::SpoileredText => {
            let content = ctx.collect(node);
            ctx.write(&format!("{FAINT}{FG_BRIGHT_BLACK}{content}{RESET}"));
        }
        NodeValue::Alert(node_alert) => {
            let kind = &node_alert.alert_type;

            let (prefix, color) = match kind {
                comrak::nodes::AlertType::Note => ("\u{f05d6} NOTE", FG_BRIGHT_BLUE),
                comrak::nodes::AlertType::Tip => ("\u{f400} TIP", FG_BRIGHT_GREEN),
                comrak::nodes::AlertType::Important => ("\u{f017e} INFO", FG_BRIGHT_CYAN),
                comrak::nodes::AlertType::Warning => ("\u{ea6c} WARNING", FG_BRIGHT_YELLOW),
                comrak::nodes::AlertType::Caution => ("\u{f0ce6} DANGER", FG_BRIGHT_RED),
            };

            ctx.write(&format!("{}│ {BOLD}{}{RESET}\n", color, prefix));

            for child in node.children() {
                let alert_content = ctx.collect(child);

                for line in alert_content.lines() {
                    ctx.write(&format!("{}│ {}", color, line));
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
    strip_ansi_escapes::strip_str(&str).chars().count()
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

fn format_code_simple(code: &str, lang: &str, ctx: &mut AnsiContext) {
    let full_width = term_misc::get_wininfo().sc_width as usize;
    let box_width = (full_width as f32 / 2.5) as usize;

    let (mut title, color) = match get_lang_icon_and_color(lang) {
        Some((icon, color)) => (format!("{color}{icon} {lang}"), color),
        None => (lang.to_owned(), ""),
    };

    if string_len(&title) > box_width.saturating_sub(2) {
        if let Some((icon, color)) = get_lang_icon_and_color(lang) {
            title = format!("{color}{icon}");
        }
    }

    let visible_len = string_len(&title);
    let padding = box_width.saturating_sub(visible_len) - 2;
    let left_pad = padding / 2;
    let right_pad = padding - left_pad;

    let top = format!(
        "{color}{}{RESET} {}{RESET} {color}{}{RESET}\n",
        "─".repeat(left_pad),
        title,
        "─".repeat(right_pad)
    );
    let bottom = format!("{color}{}{RESET}", "─".repeat(box_width));

    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ts);

    ctx.write(&top);
    for line in LinesWithEndings::from(code) {
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
        let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
        ctx.write(&highlighted);
    }
    ctx.write(&bottom);
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
    let color = FG_BRIGHT_BLACK;

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
    for line in LinesWithEndings::from(code) {
        let left_space = num_width - num.to_string().chars().count();
        let left_offset = left_space / 2;
        let right_offset = left_space - left_offset;
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
        let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
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
    "─".repeat(term_misc::get_wininfo().sc_width as usize)
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
}

fn hex_to_rgba(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    Color { r, g, b, a: 255 }
}

impl CustomTheme {
    pub fn dark() -> Self {
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
            surface: "#20202b".into(),
            border: "#2D3640".into(),
        }
    }
    pub fn light() -> Self {
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
        }
    }
    pub fn monokai() -> Self {
        CustomTheme {
            keyword: "#F92672".into(),    // Magenta
            function: "#A6E22E".into(),   // Green
            string: "#E6DB74".into(),     // Yellow
            module: "#66D9EF".into(),     // Cyan
            constant: "#AE81FF".into(),   // Purple
            comment: "#75715E".into(),    // Brown/Gray
            foreground: "#F8F8F2".into(), // Off-white
            guide: "#3E3D32".into(),      // Dark gray
            background: "#272822".into(), // Dark green-gray
            surface: "#3E3D32".into(),    // Slightly lighter
            border: "#49483E".into(),     // Border gray
        }
    }
    pub fn catppuccin() -> Self {
        CustomTheme {
            keyword: "#CBA6F7".into(),    // Mauve
            function: "#89B4FA".into(),   // Blue
            string: "#A6E3A1".into(),     // Green
            module: "#89DCEB".into(),     // Sky
            constant: "#F38BA8".into(),   // Pink
            comment: "#6C7086".into(),    // Overlay0
            foreground: "#CDD6F4".into(), // Text
            guide: "#45475A".into(),      // Surface1
            background: "#1E1E2E".into(), // Base
            surface: "#313244".into(),    // Surface0
            border: "#45475A".into(),     // Surface1
        }
    }
    pub fn tokyo_night() -> Self {
        CustomTheme {
            keyword: "#BB9AF7".into(),    // Purple
            function: "#7AA2F7".into(),   // Blue
            string: "#9ECE6A".into(),     // Green
            module: "#2AC3DE".into(),     // Cyan
            constant: "#FF9E64".into(),   // Orange
            comment: "#565F89".into(),    // Comment
            foreground: "#C0CAF5".into(), // Foreground
            guide: "#3B4261".into(),      // Line highlight
            background: "#1A1B26".into(), // Background
            surface: "#24283B".into(),    // Background highlight
            border: "#414868".into(),     // Border
        }
    }
    pub fn dracula() -> Self {
        CustomTheme {
            keyword: "#FF79C6".into(),    // Pink
            function: "#50FA7B".into(),   // Green
            string: "#F1FA8C".into(),     // Yellow
            module: "#8BE9FD".into(),     // Cyan
            constant: "#BD93F9".into(),   // Purple
            comment: "#6272A4".into(),    // Comment
            foreground: "#F8F8F2".into(), // Foreground
            guide: "#44475A".into(),      // Current line
            background: "#282A36".into(), // Background
            surface: "#44475A".into(),    // Selection
            border: "#6272A4".into(),     // Comment (used as border)
        }
    }
    pub fn nord() -> Self {
        CustomTheme {
            keyword: "#81A1C1".into(),    // Nord9 (blue)
            function: "#88C0D0".into(),   // Nord8 (cyan)
            string: "#A3BE8C".into(),     // Nord14 (green)
            module: "#8FBCBB".into(),     // Nord7 (cyan)
            constant: "#B48EAD".into(),   // Nord15 (purple)
            comment: "#616E88".into(),    // Nord3 (bright black)
            foreground: "#D8DEE9".into(), // Nord4 (white)
            guide: "#434C5E".into(),      // Nord1 (dark gray)
            background: "#2E3440".into(), // Nord0 (black)
            surface: "#3B4252".into(),    // Nord1 (dark gray)
            border: "#434C5E".into(),     // Nord2 (gray)
        }
    }
    pub fn gruvbox() -> Self {
        CustomTheme {
            keyword: "#FB4934".into(),    // Red
            function: "#FABD2F".into(),   // Yellow
            string: "#B8BB26".into(),     // Green
            module: "#83A598".into(),     // Blue
            constant: "#D3869B".into(),   // Purple
            comment: "#928374".into(),    // Gray
            foreground: "#EBDBB2".into(), // Light cream
            guide: "#504945".into(),      // Dark gray
            background: "#282828".into(), // Dark background
            surface: "#3C3836".into(),    // Dark gray
            border: "#665C54".into(),     // Medium gray
        }
    }
    pub fn solarized() -> Self {
        CustomTheme {
            keyword: "#268BD2".into(),    // Blue
            function: "#B58900".into(),   // Yellow
            string: "#2AA198".into(),     // Cyan
            module: "#859900".into(),     // Green
            constant: "#D33682".into(),   // Magenta
            comment: "#586E75".into(),    // Base01
            foreground: "#839496".into(), // Base0
            guide: "#073642".into(),      // Base02
            background: "#002B36".into(), // Base03
            surface: "#073642".into(),    // Base02
            border: "#586E75".into(),     // Base01
        }
    }
    pub fn one_dark() -> Self {
        CustomTheme {
            keyword: "#C678DD".into(),    // Purple
            function: "#61AFEF".into(),   // Blue
            string: "#98C379".into(),     // Green
            module: "#56B6C2".into(),     // Cyan
            constant: "#E06C75".into(),   // Red
            comment: "#5C6370".into(),    // Gray
            foreground: "#ABB2BF".into(), // Light gray
            guide: "#3E4451".into(),      // Dark gray
            background: "#282C34".into(), // Dark background
            surface: "#21252B".into(),    // Darker background
            border: "#3E4451".into(),     // Border gray
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
        let full_css = include_str!("../assets//style.css");
        format!("{full_css}\n\n{root_css}")
    }
}
