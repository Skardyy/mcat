use std::{borrow::Cow, collections::HashMap, sync::OnceLock, usize};

use itertools::Itertools;
use rasteroid::term_misc;
use regex::Regex;
use strip_ansi_escapes::strip_str;
use syntect::{
    easy::HighlightLines,
    highlighting::Style,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};
use unicode_width::UnicodeWidthStr;

use super::render::{AnsiContext, BOLD, RESET};

static NEWLINE_REGEX: OnceLock<Regex> = OnceLock::new();
static ANSI_ESCAPE_REGEX: OnceLock<Regex> = OnceLock::new();

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
        ("sh", ("\u{ebca}", "\x1b[38;5;34m")),   // Shell green
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

pub fn trim_ansi_string(mut str: String) -> String {
    let stripped = strip_str(&str);
    let mut leading = stripped.chars().take_while(|c| c.is_whitespace()).count();
    let mut trailing = stripped
        .chars()
        .rev()
        .take_while(|c| c.is_whitespace())
        .count();

    if leading == 0 && trailing == 0 {
        return str;
    }

    // Remove first N spaces
    str.retain(|c| {
        if c == ' ' && leading > 0 {
            leading -= 1;
            false
        } else {
            true
        }
    });

    // Remove last N spaces
    let mut i = str.len();
    while i > 0 && trailing > 0 {
        i -= 1;
        if str.as_bytes()[i] == b' ' {
            str.remove(i);
            trailing -= 1;
        }
    }

    str
}

pub fn string_len(str: &str) -> usize {
    strip_ansi_escapes::strip_str(&str).width()
}

fn find_last_fg_color_sequence(text: &str) -> Option<String> {
    let re = ANSI_ESCAPE_REGEX.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());
    let mut last_fg_color = None;

    for m in re.find_iter(text) {
        let seq = m.as_str();
        let codes_str = &seq[2..seq.len() - 1];

        if codes_str.is_empty() || codes_str == "0" {
            last_fg_color = None;
        } else {
            for code in codes_str.split(';') {
                if let Ok(num) = code.parse::<u32>() {
                    if (30..=37).contains(&num) || (90..=97).contains(&num) || num == 38 {
                        last_fg_color = Some(seq.to_string());
                        break;
                    }
                }
            }
        }
    }

    last_fg_color
}

pub fn wrap_char_based(
    original: &str,
    char: char,
    indent: usize,
    prefix: &str,
    sub_prefix: &str,
) -> String {
    let (space, sub_space, indent, sub_indent) = info_for_wrapping(indent, prefix, sub_prefix);
    let suffix = if original.ends_with("\n") { "\n" } else { "" };

    original
        .lines()
        .map(|line| {
            let char_index = line.rfind(char).map(|v| v + char.len_utf8()).unwrap_or(0);
            let str_to_char = line.get(..char_index).unwrap_or("");
            let line = format!("{indent}{line}");
            let sub_prefix = format!("{sub_indent}{str_to_char} ");
            let sub_space = sub_space.saturating_sub(string_len(&sub_prefix));
            wrap_highlighted_line(line, space, sub_space, &sub_prefix, false)
                .trim_matches('\n')
                .to_owned()
        })
        .join("\n")
        + suffix
}

fn info_for_wrapping(
    indent: usize,
    prefix: &str,
    sub_prefix: &str,
) -> (usize, usize, String, String) {
    let space = (term_misc::get_wininfo().sc_width as usize).saturating_sub(indent * 2);
    let sub_space = space.saturating_sub(string_len(sub_prefix));
    let space = space.saturating_sub(string_len(prefix));

    let indent = " ".repeat(indent);
    let sub_indent = format!("{indent}{sub_prefix}");
    let indent = format!("{indent}{prefix}");

    (space, sub_space, indent, sub_indent)
}

/// for braindead indenting any element.
pub fn wrap_lines(
    original: &str,
    multi_line: bool,
    indent: usize,
    prefix: &str,
    sub_prefix: &str,
) -> String {
    let (space, sub_space, indent, sub_indent) = info_for_wrapping(indent, prefix, sub_prefix);
    let suffix = if original.ends_with("\n") { "\n" } else { "" };

    if multi_line {
        original
            .lines()
            .map(|line| {
                let line = format!("{indent}{line}");
                wrap_highlighted_line(line, space, sub_space, &sub_indent, false)
                    .trim_matches('\n')
                    .to_owned()
            })
            .join("\n")
            + suffix
    } else {
        let line = format!("{indent}{original}");
        wrap_highlighted_line(line, space, sub_space, &indent, false)
    }
}

fn wrap_with_sub(original: String, first_width: usize, sub_width: usize) -> Vec<String> {
    let lines: Vec<String> = textwrap::wrap(&original, first_width)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect();

    let first_line = match lines.first() {
        Some(v) => v.clone(),
        None => return vec![original],
    };
    let sub_lines = lines.into_iter().skip(1).join(" ");

    let sub_width = sub_width;
    let lines: Vec<String> = textwrap::wrap(&sub_lines, sub_width)
        .into_iter()
        .map(|cow| cow.into_owned())
        .collect();

    let mut res = vec![first_line];
    res.extend_from_slice(&lines);

    res
}

/// first_width: the space for text on the first line.
/// sub_width:   the space for left for sub lines. doesn't factor in sub_prefix width, calc yourself.
/// auto_indent: add the firstline indent to the sub lines.
pub fn wrap_highlighted_line(
    original: String,
    first_width: usize,
    sub_width: usize,
    sub_prefix: &str,
    auto_indent: bool,
) -> String {
    if string_len(&original) <= first_width {
        return original;
    }

    let suffix = if original.ends_with("\n") { "\n" } else { "" };

    // wrap lines
    let pre_padding = if auto_indent {
        strip_str(&original)
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(0)
    } else {
        0
    };
    let lines = wrap_with_sub(original, first_width, sub_width.saturating_sub(pre_padding));

    let padding = " ".repeat(pre_padding);
    let mut buf = String::new();
    let mut pre_last_color = "".to_owned();

    // add prefix and lost colors
    for (i, line) in lines.iter().enumerate() {
        if i == 0 || line.trim().is_empty() {
            buf.push_str(line);
        } else {
            buf.push_str(&format!("\n{sub_prefix}{padding}{pre_last_color}{line}"));
        }
        match find_last_fg_color_sequence(line) {
            Some(color) => pre_last_color = color,
            None => {}
        }
    }
    buf.push_str(suffix);

    buf
}

pub fn format_code_simple<'a>(code: &str, lang: &str, ctx: &AnsiContext, indent: usize) -> String {
    let header = match get_lang_icon_and_color(lang) {
        Some((icon, color)) => &format!("{color}{icon} {lang}{RESET}",),
        None => lang,
    };

    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ts);

    let line_count = code.lines().count().saturating_sub(1);
    let content = LinesWithEndings::from(code)
        .enumerate()
        .filter_map(|(i, line)| {
            if line_count == i && line.trim().is_empty() {
                return None;
            }
            let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
            let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
            Some(format!("  {}", highlighted.trim_matches('\n')))
        })
        .join("\n");

    let sub_indent = 4usize;
    let sub_indent = " ".repeat(sub_indent.saturating_sub(indent));
    let (space, sub_space, indent, sub_indent) = info_for_wrapping(indent, "", &sub_indent);
    let content = content
        .lines()
        .map(|line| {
            let line = format!("{indent}{line}");
            wrap_highlighted_line(line, space, sub_space, &sub_indent, true)
                .trim_matches('\n')
                .to_owned()
        })
        .join("\n");

    format!("\n\n{indent}{header}\n{content}{RESET}\n\n")
}

pub fn format_code_full<'a>(code: &str, lang: &str, ctx: &AnsiContext) -> String {
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
    // -1 because the indent is 1 based
    let term_width = term_misc::get_wininfo().sc_width;
    let text_size = (term_width as usize)
        .saturating_sub(num_width)
        .saturating_sub(3); // -2 for spacing both ways, -1 for the | char after line num
    let color = ctx.theme.border.fg.clone();
    let mut buffer = String::new();

    let after_num_width = (term_width as usize)
        .saturating_sub(num_width)
        .saturating_sub(1); // because the connected char ┬
    let top_header = format!(
        "{color}{}┬{}{RESET}",
        "─".repeat(num_width),
        "─".repeat(after_num_width)
    );
    let middle_header = format!("{color}{}│ {header}{RESET}", " ".repeat(num_width),);
    let bottom_header = format!(
        "{color}{}┼{}{RESET}",
        "─".repeat(num_width),
        "─".repeat(after_num_width)
    );
    buffer.push_str(&format!("{top_header}\n{middle_header}\n{bottom_header}\n"));

    let mut num = 1;
    let prefix = format!("{}{color}│{RESET}     ", " ".repeat(num_width));
    let sub_text_size = text_size.saturating_sub(4); // 4 extra space for visual indent.
    for line in LinesWithEndings::from(code) {
        let left_space = num_width - num.to_string().chars().count();
        let left_offset = left_space / 2;
        let right_offset = left_space - left_offset;
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
        let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
        let highlighted =
            wrap_highlighted_line(highlighted, text_size, sub_text_size, &prefix, true);
        buffer.push_str(&format!(
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
    buffer.push_str(&last_border);
    format!("\n\n{buffer}\n\n")
}

pub fn format_code_box<'a>(code: &str, lang: &str, title: &str, ctx: &AnsiContext) -> String {
    let term_width = ctx.term_width;
    let color = &ctx.theme.border.fg;
    let content = code.trim();

    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ts);

    let max_line_width = content
        .lines()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let box_width = (max_line_width + 4).min(term_width.saturating_sub(4));

    let bg = &ctx.theme.keyword.bg;
    let fg = &ctx.theme.black.fg;
    let header_text = format!(" {} ", title);
    let header_padding = box_width.saturating_sub(string_len(&header_text) + 2); // -2 for ╭╮
    let styled_header = format!("{bg}{fg}{BOLD}{header_text}{RESET}{color}");
    let left_pad = header_padding / 2;
    let right_pad = header_padding - left_pad;

    let mut buffer = String::new();

    // Top border with title
    buffer.push_str(&format!(
        "{color}╭{}{}{}╮{RESET}\n",
        "─".repeat(left_pad),
        styled_header,
        "─".repeat(right_pad)
    ));

    let prefix = format!("{color}│{RESET}     ");
    let content_width = box_width.saturating_sub(4); // -4 for "│ " on each side
    let sub_content_width = content_width.saturating_sub(4); // 4 spaces for visual indent
    for line in LinesWithEndings::from(content) {
        let ranges: Vec<(Style, &str)> = highlighter.highlight_line(line, &ctx.ps).unwrap();
        let highlighted = as_24_bit_terminal_escaped(&ranges[..], false);
        let wrapped = wrap_highlighted_line(
            highlighted,
            content_width,
            sub_content_width,
            &prefix,
            false,
        );

        buffer.push_str(&format!("{color}│{RESET} "));
        for (i, wrapped_line) in wrapped.lines().enumerate() {
            let visible_len = string_len(wrapped_line);
            let av_space = if i == 0 {
                content_width
            } else {
                content_width + 2 // +2 for reversing the indent to "| " " |" spaces and indent
            };
            let padding = av_space.saturating_sub(visible_len);

            buffer.push_str(&format!(
                "{}{} {color}│{RESET}\n",
                wrapped_line,
                " ".repeat(padding)
            ));
        }
    }

    // Bottom border
    buffer.push_str(&format!(
        "{color}╰{}╯{RESET}\n",
        "─".repeat(box_width.saturating_sub(2))
    ));

    format!("\n{buffer}\n")
}

pub fn format_tb(ctx: &AnsiContext, offset: usize) -> String {
    let w = term_misc::get_wininfo().sc_width as usize;
    let br = "━".repeat(w.saturating_sub(offset.saturating_sub(1)));
    let border = &ctx.theme.guide.fg;
    format!("{border}{br}{RESET}")
}

pub fn limit_newlines<'a>(original: &'a str) -> Cow<'a, str> {
    let re = NEWLINE_REGEX.get_or_init(|| Regex::new(r"\n([ \t]*\n){2,}").unwrap());
    re.replace_all(&original, "\n\n")
}
