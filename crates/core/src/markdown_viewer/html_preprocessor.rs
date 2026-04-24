use itertools::Itertools;
use regex::Regex;
use scraper::{ElementRef, Html};
use std::{cell::Cell, collections::HashMap, sync::LazyLock};

macro_rules! heading_processor {
    ($level:expr) => {
        |element: ElementRef, ctx: &ProcessingContext| -> String {
            let prefix = "#".repeat($level);
            let content = collect(element, ctx);
            format!("\n\n{} {}\n\n", prefix, content.trim())
        }
    };
}

pub struct ProcessingContext {
    rules: HashMap<String, ProcessorFn>,
    in_table: Cell<bool>,
}

type ProcessorFn = fn(ElementRef, &ProcessingContext) -> String;

static BLANKS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t]*(\n[ \t]*){3,}").unwrap());

fn collapse_blanks(s: &str) -> String {
    BLANKS_RE.replace_all(s, "\n\n").to_string()
}

fn collect(element: ElementRef, ctx: &ProcessingContext) -> String {
    let mut result = String::new();
    for child in element.children() {
        if let Some(el) = ElementRef::wrap(child) {
            let tag = el.value().name();
            let rendered = if let Some(rule) = ctx.rules.get(tag) {
                rule(el, ctx)
            } else {
                collect(el, ctx)
            };
            result.push_str(&rendered);
        } else if let Some(text) = child.value().as_text() {
            result.push_str(text);
        }
    }
    result
}

impl ProcessingContext {
    fn new() -> Self {
        let mut ctx = Self {
            rules: HashMap::new(),
            in_table: Cell::new(false),
        };

        ctx.add_div_rules();
        ctx.add_details_rules();
        ctx.add_quote_rules();
        ctx.add_heading_rules();
        ctx.add_formatting_rules();
        ctx.add_link_rules();
        ctx.add_img_rules();
        ctx.add_code_rules();
        ctx.add_block_rules();
        ctx.add_list_rules();
        ctx.add_empty_rules();
        ctx.add_table_rules();

        ctx
    }

    fn add_table_rules(&mut self) {
        self.rules.insert("table".to_string(), |element, ctx| {
            let prev = ctx.in_table.replace(true);
            let rows: Vec<Vec<String>> = element
                .descendants()
                .filter_map(ElementRef::wrap)
                .filter(|e| e.value().name() == "tr")
                .map(|tr| {
                    tr.children()
                        .filter_map(ElementRef::wrap)
                        .filter(|e| matches!(e.value().name(), "td" | "th"))
                        .map(|cell| {
                            let rule = ctx.rules.get(cell.value().name()).unwrap();
                            let raw = rule(cell, ctx);

                            // if there is going to be > 1 con \n, this will be better then replace,
                            // though not sure its a thing?
                            raw.trim()
                                .replace('|', r"\|")
                                .split('\n')
                                .map(str::trim)
                                .filter(|l| !l.is_empty())
                                .join("<br>")
                        })
                        .collect::<Vec<_>>()
                })
                .filter(|r| !r.is_empty())
                .collect();

            if rows.is_empty() {
                return String::new();
            }

            // our markdown render doesn't care if table is only headers
            let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            let header = rows[0].clone();
            let body = &rows[1..];

            let render_row = |cells: &[String]| -> String {
                let mut padded = cells.to_vec();
                while padded.len() < col_count {
                    padded.push(String::new());
                }
                format!("| {} |", padded.join(" | "))
            };

            // if 1 td/th in the col contains align, they all get. not sure i can do it in another
            // way, given its markdown.
            let mut col_alignments: Vec<Option<&str>> = vec![None; col_count];
            for tr in element
                .descendants()
                .filter_map(ElementRef::wrap)
                .filter(|e| e.value().name() == "tr")
            {
                for (col_idx, cell) in tr
                    .children()
                    .filter_map(ElementRef::wrap)
                    .filter(|e| matches!(e.value().name(), "td" | "th"))
                    .enumerate()
                {
                    if col_idx >= col_count {
                        break;
                    }
                    if col_alignments[col_idx].is_none()
                        && let Some(a) = cell.value().attr("align")
                    {
                        let a = a.trim();
                        if matches!(a, "left" | "center" | "right") {
                            col_alignments[col_idx] = Some(a);
                        }
                    }
                }
            }

            let separator = {
                let cells: Vec<&str> = col_alignments
                    .iter()
                    .map(|a| match *a {
                        Some("center") => ":---:",
                        Some("right") => "---:",
                        Some("left") => ":---",
                        _ => "---",
                    })
                    .collect();
                format!("| {} |", cells.join(" | "))
            };

            let mut out = render_row(&header);
            out.push('\n');
            out.push_str(&separator);
            for row in body {
                out.push('\n');
                out.push_str(&render_row(row));
            }

            let out = if let Some(align) = element.value().attr("align")
                && align.trim().to_lowercase() == "center"
            {
                format!("<!--CENTER_ON-->\n\n{out}\n\n<!--CENTER_OFF-->")
            } else {
                out
            };

            ctx.in_table.set(prev);
            format!("\n\n{}\n\n", out)
        });

        self.rules.insert("td".to_string(), |element, ctx| {
            collect(element, ctx).trim().to_string()
        });
        self.rules.insert("th".to_string(), |element, ctx| {
            collect(element, ctx).trim().to_string()
        });

        self.rules.insert("tr".to_string(), |_, _| String::new());
        self.rules.insert("thead".to_string(), |_, _| String::new());
        self.rules.insert("tbody".to_string(), |_, _| String::new());
        self.rules.insert("tfoot".to_string(), |_, _| String::new());
    }

    fn add_img_rules(&mut self) {
        self.rules.insert("img".to_string(), |element, _ctx| {
            let src = element.value().attr("src").unwrap_or("");
            let alt = element.value().attr("alt").unwrap_or("IMG");

            let clean = |s: &str| -> String {
                s.chars()
                    .take_while(|c| c.is_ascii_digit() || c.is_ascii_alphabetic() || *c == '%')
                    .collect()
            };
            let width = element.value().attr("width").map(clean);
            let height = element.value().attr("height").map(clean);

            let enhanced_src = match (width, height) {
                (Some(w), Some(h)) => format!("{}#{}x{}", src, w, h),
                (Some(w), None) => format!("{}#{}x", src, w),
                (None, Some(h)) => format!("{}#x{}", src, h),
                (None, None) => src.to_string(),
            };

            format!("![{}]({})", alt, enhanced_src)
        });

        // not really doing dark / light mode, so just ignore
        self.rules.insert("picture".to_string(), collect);
        self.rules
            .insert("source".to_string(), |_, _| String::new());
    }

    fn add_code_rules(&mut self) {
        self.rules.insert("pre".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("\n\n```\n{}\n```\n\n", content.trim_end())
        });

        self.rules.insert("code".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("`{}`", content)
        });
    }

    fn add_list_rules(&mut self) {
        self.rules.insert("ul".to_string(), |element, ctx| {
            let items: Vec<String> = element
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|e| e.value().name() == "li")
                .map(|li| {
                    let rule = ctx.rules.get("li").unwrap();
                    let raw = rule(li, ctx);
                    let indented = raw
                        .lines()
                        .enumerate()
                        .map(|(i, line)| {
                            if i == 0 {
                                line.to_string()
                            } else if line.is_empty() {
                                String::new()
                            } else {
                                format!("    {}", line)
                            }
                        })
                        .join("\n");
                    format!("- {}", indented)
                })
                .collect();
            format!("\n\n{}\n\n", items.join("\n"))
        });

        self.rules.insert("ol".to_string(), |element, ctx| {
            let start: usize = element
                .value()
                .attr("start")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let items: Vec<String> = element
                .children()
                .filter_map(ElementRef::wrap)
                .filter(|e| e.value().name() == "li")
                .enumerate()
                .map(|(i, li)| {
                    let rule = ctx.rules.get("li").unwrap();
                    let raw = rule(li, ctx);
                    let indented = raw
                        .lines()
                        .enumerate()
                        .map(|(j, line)| {
                            if j == 0 {
                                line.to_string()
                            } else if line.is_empty() {
                                String::new()
                            } else {
                                format!("    {}", line)
                            }
                        })
                        .join("\n");
                    let marker = format!("{}. ", start + i);
                    format!("{}{}", marker, indented)
                })
                .collect();
            format!("\n\n{}\n\n", items.join("\n"))
        });

        self.rules.insert("li".to_string(), |element, ctx| {
            let raw = collect(element, ctx);
            collapse_blanks(raw.trim()).to_string()
        });
    }

    fn add_link_rules(&mut self) {
        self.rules.insert("a".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            if let Some(href) = element.value().attr("href") {
                format!("[{}]({})", content.trim(), href.trim())
            } else {
                content
            }
        });
    }

    fn add_block_rules(&mut self) {
        self.rules.insert("blockquote".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            let content = collapse_blanks(content.trim());
            let body = content.lines().map(|line| format!("> {line}")).join("\n");
            format!("\n\n{}\n\n", body)
        });
    }

    fn add_empty_rules(&mut self) {
        self.rules.insert("figure".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("\n\n{}\n\n", content.trim())
        });
        self.rules.insert("figcaption".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("\n\n{}\n\n", content.trim())
        });
    }

    fn add_formatting_rules(&mut self) {
        self.rules
            .insert("hr".to_string(), |_element, _ctx| "\n\n---\n\n".to_string());

        self.rules.insert("sup".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("^{}^", content.trim())
        });

        self.rules.insert("mark".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("=={}==", content.trim())
        });

        self.rules.insert("kbd".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("`[{}]`", content.trim())
        });

        self.rules.insert("var".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("*{}*", content.trim())
        });

        self.rules.insert("i".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("*{}*", content.trim())
        });

        self.rules.insert("b".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("**{}**", content.trim())
        });

        self.rules.insert("strong".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("**{}**", content.trim())
        });

        self.rules.insert("em".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("*{}*", content.trim())
        });

        self.rules.insert("del".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("~~{}~~", content.trim())
        });

        self.rules.insert("s".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("~~{}~~", content.trim())
        });

        self.rules.insert("strike".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("~~{}~~", content.trim())
        });
    }

    fn add_heading_rules(&mut self) {
        self.rules.insert("h1".to_string(), heading_processor!(1));
        self.rules.insert("h2".to_string(), heading_processor!(2));
        self.rules.insert("h3".to_string(), heading_processor!(3));
        self.rules.insert("h4".to_string(), heading_processor!(4));
        self.rules.insert("h5".to_string(), heading_processor!(5));
        self.rules.insert("h6".to_string(), heading_processor!(6));
    }

    fn add_quote_rules(&mut self) {
        self.rules.insert("q".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("\"{}\"", content)
        });
    }

    fn add_div_rules(&mut self) {
        for item in ["div", "p"] {
            self.rules.insert(item.to_string(), |element, ctx| {
                let content = collect(element, ctx);

                if content.trim().is_empty() {
                    return String::new();
                }

                if !ctx.in_table.get()
                    && let Some(align) = element.value().attr("align")
                    && align.trim().to_lowercase() == "center"
                {
                    return format!("\n\n<!--CENTER_ON-->\n\n{content}\n\n<!--CENTER_OFF-->\n\n");
                }

                format!("\n\n{}\n\n", content)
            });
        }
    }

    fn add_details_rules(&mut self) {
        self.rules.insert("details".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            let content = collapse_blanks(content.trim());
            let body = content.lines().map(|line| format!("> {line}")).join("\n");
            format!("\n\n{}\n\n", body)
        });

        self.rules.insert("summary".to_string(), |element, ctx| {
            let content = collect(element, ctx);
            format!("\n\n▼ {}\n\n", content.trim())
        });
    }

    fn escape_unknown_elements(&self, markdown: &str) -> String {
        let escaped = markdown.replace('<', "&lt;").replace('>', "&gt;");

        // unescape known tags
        let known_tags = self
            .rules
            .keys()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join("|");
        let tag_regex = Regex::new(&format!(r"(?s)&lt;(/?(?:{}))\b.*?&gt;", known_tags)).unwrap();

        let markdown = tag_regex.replace_all(&escaped, |caps: &regex::Captures| {
            caps.get(0)
                .unwrap()
                .as_str()
                .replace("&lt;", "<")
                .replace("&gt;", ">")
        });

        let triple_regex = Regex::new(r"```([\s\S]*?)```").unwrap();
        let single_regex = Regex::new(r"`([^`\n]+)`").unwrap();

        let markdown = triple_regex.replace_all(&markdown, |caps: &regex::Captures| {
            let inner = caps[1].replace('<', "&lt;").replace('>', "&gt;");
            format!("```{}```", inner)
        });
        let markdown = single_regex.replace_all(&markdown, |caps: &regex::Captures| {
            let inner = caps[1].replace('<', "&lt;").replace('>', "&gt;");
            format!("`{}`", inner)
        });
        markdown.to_string()
    }
}

pub fn process(markdown: &str) -> String {
    let ctx = ProcessingContext::new();

    let escaped_markdown = ctx.escape_unknown_elements(markdown);
    let document = Html::parse_fragment(&escaped_markdown);

    let result = collect(document.root_element(), &ctx);
    collapse_blanks(result.trim()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_inline_formatters_convert_to_markdown() {
        assert_eq!(process("<b>bold</b>"), "**bold**");
        assert_eq!(process("<strong>bold</strong>"), "**bold**");
        assert_eq!(process("<i>italic</i>"), "*italic*");
        assert_eq!(process("<em>italic</em>"), "*italic*");
        assert_eq!(process("<var>var</var>"), "*var*");
        assert_eq!(process("<s>strike</s>"), "~~strike~~");
        assert_eq!(process("<del>strike</del>"), "~~strike~~");
        assert_eq!(process("<strike>strike</strike>"), "~~strike~~");
        assert_eq!(process("<mark>highlighted</mark>"), "==highlighted==");
        assert_eq!(process("<sup>2</sup>"), "^2^");
        assert_eq!(process("<kbd>Ctrl</kbd>"), "`[Ctrl]`");
        assert_eq!(process("<q>quoted</q>"), "\"quoted\"");
    }

    #[test]
    fn test_nested_inline_formatters() {
        assert_eq!(
            process("<b>bold with <i>italic</i> inside</b>"),
            "**bold with *italic* inside**"
        );
        assert_eq!(
            process("<p>Text with <b>bold <mark>and mark</mark></b>.</p>"),
            "Text with **bold ==and mark==**."
        );
    }

    #[test]
    fn test_img_with_dimensions_appends_hash() {
        assert_eq!(
            process(r#"<img src="foo.png" alt="x" width="10" height="20"/>"#),
            "![x](foo.png#10x20)"
        );
        assert_eq!(
            process(r#"<img src="foo.png" alt="x" width="10"/>"#),
            "![x](foo.png#10x)"
        );
        assert_eq!(
            process(r#"<img src="foo.png" alt="x" height="20"/>"#),
            "![x](foo.png#x20)"
        );
        assert_eq!(process(r#"<img src="foo.png"/>"#), "![IMG](foo.png)");
        assert_eq!(
            process(r#"<img src="foo.png" alt="x" width="10px;" height="20px;"/>"#),
            "![x](foo.png#10pxx20px)"
        );
    }

    #[test]
    fn test_link_with_nested_image() {
        let result = process(r#"<a href="http://e.com"><img src="i.png" alt="icon"/></a>"#);
        assert_eq!(result, "[![icon](i.png)](http://e.com)");
    }

    #[test]
    fn test_link_with_amp_in_url() {
        let result = process(r#"<a href="http://e.com?a=1&b=2">text</a>"#);
        assert_eq!(result, "[text](http://e.com?a=1&b=2)");
    }

    #[test]
    fn test_link_without_href_returns_content() {
        assert_eq!(process("<a>text</a>"), "text");
    }

    #[test]
    fn test_img_with_amp_in_src() {
        let result = process(r#"<img src="http://e.com?a=1&b=2" alt="x"/>"#);
        assert_eq!(result, "![x](http://e.com?a=1&b=2)");
    }

    #[test]
    fn test_headings_convert() {
        assert_eq!(process("<h1>Title</h1>"), "# Title");
        assert_eq!(process("<h2>Title</h2>"), "## Title");
        assert_eq!(process("<h6>Title</h6>"), "###### Title");
    }

    #[test]
    fn test_hr_emits_thematic_break() {
        let expected = indoc! {"
            a

            ---

            b
        "};
        assert_eq!(process("a<hr/>b"), expected.trim_end());
    }

    #[test]
    fn test_nested_blockquote() {
        let input = indoc! {"
            <blockquote>outer
                <blockquote>inner</blockquote>
            </blockquote>
        "};

        let expected = indoc! {"
            > outer
            > 
            > > inner
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_center_alignment_becomes_comment() {
        let input = indoc! {r#"
            <div align="center">hello</div>
        "#};

        let expected = indoc! {"
            <!--CENTER_ON-->

            hello

            <!--CENTER_OFF-->
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_unknown_tag_passes_through() {
        assert_eq!(
            process("<YouDontKnowMe>unknown</YouDontKnowMe>"),
            "<YouDontKnowMe>unknown</YouDontKnowMe>"
        );
    }

    #[test]
    fn test_code_block_contents_not_touched() {
        let input = indoc! {"
            ```
            <b>not bold</b>
            ```
        "};

        let expected = indoc! {"
            ```
            <b>not bold</b>
            ```
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_inline_code_contents_not_touched() {
        assert_eq!(process("`<b>text</b>`"), "`<b>text</b>`");
    }

    #[test]
    fn test_table_renders_with_alignment() {
        let input = indoc! {r#"
            <table>
                <tr>
                    <th align="left">L</th>
                    <th align="center">C</th>
                    <th align="right">R</th>
                </tr>
                <tr>
                    <td>1</td>
                    <td>2</td>
                    <td>3</td>
                </tr>
            </table>
        "#};

        let expected = indoc! {"
            | L | C | R |
            | :--- | :---: | ---: |
            | 1 | 2 | 3 |
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_table_cell_pipe_escaped() {
        let input = indoc! {"
            <table><tr><td>a|b</td></tr></table>
        "};

        let expected = indoc! {r"
            | a\|b |
            | --- |
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_ul_with_nested_and_multi_paragraph() {
        let input = indoc! {"
            <ul>
                <li>
                    <p>First paragraph.</p>
                    <p>Second paragraph.</p>
                </li>
                <li>outer
                    <ul>
                        <li>nested one</li>
                        <li>nested two</li>
                    </ul>
                </li>
                <li>simple</li>
            </ul>
        "};

        let expected = indoc! {"
            - First paragraph.

                Second paragraph.
            - outer

                - nested one
                - nested two
            - simple
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_details_summary() {
        let input = indoc! {"
            <details>
                <summary>Summary here</summary>
                Body content.
            </details>
        "};

        let expected = indoc! {"
            > ▼ Summary here
            > 
            > Body content.
        "};

        assert_eq!(process(input), expected.trim_end());
    }

    #[test]
    fn test_collapse_blanks_limits_con_newlines() {
        assert_eq!(collapse_blanks("a\n\n\n\nb"), "a\n\nb");
        assert_eq!(collapse_blanks("a\n\nb"), "a\n\nb");
        assert_eq!(collapse_blanks("a\nb"), "a\nb");
        assert_eq!(collapse_blanks("a\n  \n  \n  \nb"), "a\n\nb");
    }
}
