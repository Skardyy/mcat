use std::{borrow::Cow, collections::HashMap, sync::LazyLock};

use itertools::Itertools;
use regex::Regex;
use strip_ansi_escapes::strip_str;
use syntect::{
    easy::HighlightLines,
    highlighting::Style,
    util::{LinesWithEndings, as_24_bit_terminal_escaped},
};
use unicode_width::UnicodeWidthStr;

use super::render::{AnsiContext, BOLD, RESET};

static ANSI_ESCAPE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap());

static COLOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)color\s*:\s*([a-z]+|#[0-9a-f]{3,8})"#).unwrap());

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
    strip_ansi_escapes::strip_str(str).width()
}

fn find_last_format(text: &str) -> Option<String> {
    let mut fg: Option<String> = None;
    let mut bold = false;
    let mut faint = false;
    let mut italic = false;
    let mut underline = false;
    let mut strikethrough = false;
    let mut ever_set = false;

    for m in ANSI_ESCAPE_REGEX.find_iter(text) {
        let seq = m.as_str();
        let codes_str = &seq[2..seq.len() - 1];
        ever_set = true;

        if codes_str.is_empty() || codes_str == "0" {
            fg = None;
            bold = false;
            faint = false;
            italic = false;
            underline = false;
            strikethrough = false;
            continue;
        }

        let parts: Vec<&str> = codes_str.split(';').collect();
        let mut i = 0;
        while i < parts.len() {
            match parts[i].parse::<u32>().unwrap_or(999) {
                1 => bold = true,
                2 => faint = true,
                3 => italic = true,
                4 => underline = true,
                9 => strikethrough = true,
                22 => {
                    bold = false;
                    faint = false;
                }
                23 => italic = false,
                24 => underline = false,
                29 => strikethrough = false,
                39 => fg = None,
                38 => {
                    let rest = parts[i..].join(";");
                    fg = Some(rest);
                    break;
                }
                n if (30..=37).contains(&n) || (90..=97).contains(&n) => {
                    fg = Some(n.to_string());
                }
                _ => {}
            }
            i += 1;
        }
    }

    if !ever_set {
        return None;
    }

    let mut codes: Vec<String> = vec![];
    if bold {
        codes.push("1".into());
    }
    if faint {
        codes.push("2".into());
    }
    if italic {
        codes.push("3".into());
    }
    if underline {
        codes.push("4".into());
    }
    if strikethrough {
        codes.push("9".into());
    }
    if let Some(ref f) = fg {
        codes.push(f.clone());
    }

    if codes.is_empty() {
        Some(String::new())
    } else {
        Some(format!("\x1b[{}m", codes.join(";")))
    }
}

pub fn wrap_char_based(
    ctx: &AnsiContext,
    original: &str,
    char: char,
    indent: usize,
    prefix: &str,
    sub_prefix: &str,
) -> String {
    let (space, sub_space, indent, sub_indent) = info_for_wrapping(ctx, indent, prefix, sub_prefix);
    let suffix = if original.ends_with("\n") { "\n" } else { "" };

    original
        .lines()
        .map(|line| {
            let char_index = line.rfind(char).map(|v| v + char.len_utf8()).unwrap_or(0);
            let str_to_char = line.get(..char_index).unwrap_or("");
            let line = format!("{indent}{line}");
            // adding RESET, since it is only used for block quote and alerts
            let sub_prefix = format!("{sub_indent}{str_to_char}{RESET} ");
            let sub_space = sub_space.saturating_sub(string_len(&sub_prefix));
            wrap_highlighted_line(line, space, sub_space, &sub_prefix, false)
                .trim_matches('\n')
                .to_owned()
        })
        .join("\n")
        + suffix
}

fn info_for_wrapping(
    ctx: &AnsiContext,
    indent: usize,
    prefix: &str,
    sub_prefix: &str,
) -> (usize, usize, String, String) {
    let space = (ctx.wininfo.sc_width as usize).saturating_sub(indent * 2);
    let sub_space = space.saturating_sub(string_len(sub_prefix));
    let space = space.saturating_sub(string_len(prefix));

    let indent = " ".repeat(indent);
    let sub_indent = format!("{indent}{sub_prefix}");
    let indent = format!("{indent}{prefix}");

    (space, sub_space, indent, sub_indent)
}

/// for braindead indenting any element.
pub fn wrap_lines(
    ctx: &AnsiContext,
    original: &str,
    multi_line: bool,
    indent: usize,
    prefix: &str,
    sub_prefix: &str,
    auto_indent: bool,
) -> String {
    let (space, sub_space, indent, sub_indent) = info_for_wrapping(ctx, indent, prefix, sub_prefix);
    let suffix = if original.ends_with("\n") { "\n" } else { "" };

    if multi_line {
        original
            .lines()
            .map(|line| {
                let line = format!("{indent}{line}");
                wrap_highlighted_line(line, space, sub_space, &sub_indent, auto_indent)
                    .trim_matches('\n')
                    .to_owned()
            })
            .join("\n")
            + suffix
    } else {
        let line = format!("{indent}{original}");
        wrap_highlighted_line(line, space, sub_space, &indent, auto_indent)
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
    let mut pre_format = "".to_owned();

    // add prefix and lost colors
    for (i, line) in lines.iter().enumerate() {
        if i == 0 || line.trim().is_empty() {
            buf.push_str(line);
        } else {
            buf.push_str(&format!("\n{sub_prefix}{padding}{pre_format}{line}"));
        }
        // clear links..
        if line.contains("\x1b]8;;") {
            buf.push_str("\x1b]8;;\x1b\\");
        }
        // carry on formatting
        if let Some(ansi) = find_last_format(&buf) {
            pre_format = ansi
        }
        buf.push_str(RESET);
    }
    buf.push_str(suffix);

    buf
}

pub fn format_code_simple(code: &str, lang: &str, ctx: &AnsiContext, indent: usize) -> String {
    let header = match get_lang_icon_and_color(lang) {
        Some((icon, color)) => &format!("{color}{icon} {lang}{RESET}",),
        None => lang,
    };

    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_extension(lang)
        .or_else(|| ctx.ps.find_syntax_by_token(lang))
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
    let (space, sub_space, indent, sub_indent) = info_for_wrapping(ctx, indent, "", &sub_indent);
    let content = content
        .lines()
        .map(|line| {
            let line = format!("{indent}{line}");
            wrap_highlighted_line(line, space, sub_space, &sub_indent, true)
                .trim_matches('\n')
                .to_owned()
        })
        .join("\n");

    format!("{indent}{header}\n{content}{RESET}")
}

pub fn format_code_full(code: &str, lang: &str, ctx: &AnsiContext) -> String {
    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_extension(lang)
        .or_else(|| ctx.ps.find_syntax_by_token(lang))
        .unwrap_or_else(|| ctx.ps.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &ts);

    let header = match get_lang_icon_and_color(lang) {
        Some((icon, color)) => &format!("{color}{icon} {lang}",),
        None => lang,
    };

    let max_lines = code.lines().count();
    let num_width = max_lines.to_string().chars().count() + 2;
    // -1 because the indent is 1 based
    let term_width = ctx.wininfo.sc_width;
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
    buffer
}

pub fn format_code_box(code: &str, lang: &str, title: &str, ctx: &AnsiContext) -> String {
    let term_width = ctx.wininfo.sc_width as usize;
    let color = &ctx.theme.border.fg;
    let content = code.trim();

    let ts = ctx.theme.to_syntect_theme();
    let syntax = ctx
        .ps
        .find_syntax_by_extension(lang)
        .or_else(|| ctx.ps.find_syntax_by_token(lang))
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

    buffer
}

pub fn format_tb(ctx: &AnsiContext, offset: usize) -> String {
    let w = ctx.wininfo.sc_width as usize;
    let br = "━".repeat(w.saturating_sub(offset.saturating_sub(1)));
    let border = &ctx.theme.guide.fg;
    format!("{border}{br}{RESET}")
}

#[rustfmt::skip]
pub fn to_superscript(ch: char) -> Option<char> {
    Some(match ch {
        // nums
        '0' => '⁰', '1' => '¹', '2' => '²', '3' => '³', '4' => '⁴',
        '5' => '⁵', '6' => '⁶', '7' => '⁷', '8' => '⁸', '9' => '⁹',

        // symbols
        '+' => '⁺', '-' => '⁻', '=' => '⁼', '(' => '⁽', ')' => '⁾',

        // lowercase letters (no q)
        'a' => 'ᵃ', 'b' => 'ᵇ', 'c' => 'ᶜ', 'd' => 'ᵈ', 'e' => 'ᵉ',
        'f' => 'ᶠ', 'g' => 'ᵍ', 'h' => 'ʰ', 'i' => 'ⁱ', 'j' => 'ʲ',
        'k' => 'ᵏ', 'l' => 'ˡ', 'm' => 'ᵐ', 'n' => 'ⁿ', 'o' => 'ᵒ',
        'p' => 'ᵖ', 'r' => 'ʳ', 's' => 'ˢ', 't' => 'ᵗ', 'u' => 'ᵘ',
        'v' => 'ᵛ', 'w' => 'ʷ', 'x' => 'ˣ', 'y' => 'ʸ', 'z' => 'ᶻ',

        // uppercase letters (no C, F, Q, S, X, Y, Z)
        'A' => 'ᴬ', 'B' => 'ᴮ', 'D' => 'ᴰ', 'E' => 'ᴱ', 'G' => 'ᴳ',
        'H' => 'ᴴ', 'I' => 'ᴵ', 'J' => 'ᴶ', 'K' => 'ᴷ', 'L' => 'ᴸ',
        'M' => 'ᴹ', 'N' => 'ᴺ', 'O' => 'ᴼ', 'P' => 'ᴾ', 'R' => 'ᴿ',
        'T' => 'ᵀ', 'U' => 'ᵁ', 'V' => 'ⱽ', 'W' => 'ᵂ',

        ' ' => ' ',

        _ => return None,
    })
}

pub fn extract_span_color<'a>(lit: &str, ctx: &'a AnsiContext) -> Option<Cow<'a, str>> {
    let caps = COLOR_RE.captures(lit)?;
    let color = caps.get(1)?.as_str().to_lowercase();
    let theme = &ctx.theme;

    if let Some(hex) = color.strip_prefix('#') {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        return Some(Cow::Owned(format!("\x1b[38;2;{r};{g};{b}m")));
    }

    let name = match color.as_str() {
        "red" => &theme.red.fg,
        "green" => &theme.green.fg,
        "blue" => &theme.blue.fg,
        "yellow" => &theme.yellow.fg,
        "magenta" | "purple" | "pink" => &theme.magenta.fg,
        "cyan" => &theme.cyan.fg,
        "black" => &theme.black.fg,
        "white" | "gray" | "grey" => &theme.foreground.fg,
        _ => return None,
    };
    Some(Cow::Borrowed(name))
}

// we only test core wrapping logic..
#[cfg(test)]
mod tests {
    use rasteroid::{RasterEncoder, term_misc::Wininfo};

    use crate::{
        config::McatConfig, markdown_viewer::image_preprocessor::ImagePreprocessor,
        themes::CustomTheme,
    };

    use super::*;

    fn make_ctx() -> AnsiContext {
        let arena = comrak::Arena::new();
        let root = comrak::parse_document(&arena, "", &comrak::Options::default());
        let mut conf = McatConfig::default();
        conf.encoder = Some(RasterEncoder::Kitty);
        conf.wininfo = Some(Wininfo {
            sc_width: 50,
            sc_height: 20,
            spx_width: 1920,
            spx_height: 1080,
            is_tmux: false,
            needs_inline: true,
        });
        AnsiContext {
            ps: two_face::syntax::extra_newlines(),
            theme: CustomTheme::github(),
            wininfo: conf.wininfo.clone().unwrap(),
            hide_line_numbers: false,
            show_frontmatter: false,
            center: false,
            image_preprocessor: ImagePreprocessor::new(root, &conf, None).unwrap(),
            blockquote_fenced_offset: None,
            is_multi_block_quote: false,
            paragraph_collecting_line: None,
            collecting_depth: 0,
            under_header: false,
            force_simple_code_block: 0,
            list_depth: 0,
        }
    }

    #[test]
    fn test_trim_ansi_string_trims() {
        assert_eq!(trim_ansi_string("  hello  ".into()), "hello");
        assert_eq!(trim_ansi_string("hello".into()), "hello");
        assert_eq!(trim_ansi_string("   ".into()), "");
        assert_eq!(
            trim_ansi_string("\x1b[31m red text \x1b[0m".into()),
            "\x1b[31mred text\x1b[0m"
        );
    }

    #[test]
    fn test_string_len_ignores_ansi() {
        assert_eq!(string_len("hello"), 5);
        assert_eq!(string_len("\x1b[31mhello\x1b[0m"), 5);
        assert_eq!(string_len(""), 0);
    }

    #[test]
    fn test_find_last_format_tracks_state() {
        assert_eq!(find_last_format("hello"), None);
        assert_eq!(
            find_last_format("\x1b[31mhello\x1b[0m"),
            Some(String::new())
        );
        assert_eq!(find_last_format("\x1b[1mhello"), Some("\x1b[1m".into()));
        assert_eq!(
            find_last_format("\x1b[31mhi\x1b[32mbye"),
            Some("\x1b[32m".into())
        );
        assert_eq!(
            find_last_format("\x1b[1m\x1b[31mhi"),
            Some("\x1b[1;31m".into())
        );
    }

    #[test]
    fn test_info_for_wrapping_basic() {
        let ctx = make_ctx();
        // sc_width=50, indent=2 -> space=50-(2*2)=46
        let (space, sub_space, indent, sub_indent) = info_for_wrapping(&ctx, 2, "", "");
        assert_eq!(space, 46);
        assert_eq!(sub_space, 46);
        assert_eq!(indent, "  ");
        assert_eq!(sub_indent, "  ");

        // prefix and sub_prefix affect space/sub_space
        let (space, sub_space, indent, sub_indent) = info_for_wrapping(&ctx, 0, "> ", "  ");
        assert_eq!(space, 48); // 50 - len("> ")
        assert_eq!(sub_space, 48); // 50 - len("  ")
        assert_eq!(indent, "> ");
        assert_eq!(sub_indent, "  ");

        let (space, sub_space, indent, sub_indent) = info_for_wrapping(&ctx, 2, "> ", "  ");
        assert_eq!(space, 44); // 50 - (2*2) - len("> ")
        assert_eq!(sub_space, 44); // 50 - (2*2) - len("  ")
        assert_eq!(indent, "  > ");
        assert_eq!(sub_indent, "    ");
    }

    #[test]
    fn test_wrap_highlighted_line_basic() {
        let text = "the quick brown fox jumps over the lazy dog";

        // fits in width -> returned as-is
        assert_eq!(
            wrap_highlighted_line(text.to_string(), 50, 50, "", false),
            text
        );

        // wraps at first_width, sub lines get sub_prefix
        let result = wrap_highlighted_line(text.to_string(), 10, 30, ">> ", false);
        for line in result.lines().skip(1) {
            assert!(
                line.starts_with(">> "),
                "sub line should start with '>> ', got: {:?}",
                line
            );
        }

        // exact width -> no wrap
        let ten = "0123456789".to_string();
        assert_eq!(
            wrap_highlighted_line(ten.clone(), 10, 10, ">> ", false),
            ten
        );

        // trailing newline preserved
        assert!(
            wrap_highlighted_line(format!("{text}\n"), 10, 30, "", false).ends_with("\n"),
            "trailing newline should be preserved"
        );

        // no trailing newline -> none added
        assert!(
            !wrap_highlighted_line(text.to_string(), 10, 30, "", false).ends_with("\n"),
            "no trailing newline should not be added"
        );

        // auto_indent copies leading spaces from first line onto all sub lines
        let result = wrap_highlighted_line(format!("    {text}"), 12, 40, "", true);
        for line in result.lines().skip(1) {
            assert!(
                line.starts_with("    "),
                "auto_indent sub line should start with 4 spaces, got: {:?}",
                line
            );
        }

        // ansi color from first line carries into sub lines
        let result = wrap_highlighted_line(format!("\x1b[31m{text}\x1b[0m"), 10, 30, "  ", false);
        for line in result.lines().skip(1) {
            assert!(
                line.starts_with("  \x1b[31m"),
                "ansi color should carry into sub lines, got: {:?}",
                line
            );
        }

        // sub_prefix is empty -> sub lines start directly with text
        let result = wrap_highlighted_line(text.to_string(), 10, 30, "", false);
        let sub = result.lines().nth(1).unwrap_or("");
        assert!(
            !sub.starts_with(" "),
            "empty sub_prefix should not add spaces, got: {:?}",
            sub
        );

        // reset before wrap -> no color carries into sub lines
        let result =
            wrap_highlighted_line(format!("\x1b[31mhi\x1b[0m {text}"), 10, 30, "  ", false);
        for line in result.lines().skip(1) {
            assert!(
                !line.contains("\x1b[31m"),
                "reset color should not carry into sub lines, got: {:?}",
                line
            );
        }
    }

    #[test]
    fn test_wrap_lines_basic() {
        let ctx = make_ctx();
        let text = "the quick brown fox jumps over the lazy dog";

        // single line -> same result as wrap_highlighted_line with indent prepended
        let result = wrap_lines(&ctx, text, false, 0, "", "", false);
        let expected = wrap_highlighted_line(text.to_string(), 50, 50, "", false);
        assert_eq!(result, expected);

        // multi_line -> each line wrapped independently
        let multi = format!("{text}\n{text}");
        let result = wrap_lines(&ctx, &multi, true, 0, "", "", false);
        let expected = [text, text]
            .iter()
            .map(|line| {
                wrap_highlighted_line(line.to_string(), 50, 50, "", false)
                    .trim_matches('\n')
                    .to_owned()
            })
            .join("\n");
        assert_eq!(result, expected);

        // indent shifts space available and prepends to lines
        let result = wrap_lines(&ctx, text, false, 2, "", "", false);
        let expected = wrap_highlighted_line(format!("  {text}"), 46, 46, "  ", false);
        assert_eq!(result, expected);
    }
}
