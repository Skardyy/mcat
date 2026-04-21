use itertools::Itertools;
use regex::Regex;
use scraper::{ElementRef, Html};
use std::collections::HashMap;

macro_rules! heading_processor {
    ($level:expr) => {
        |element: ElementRef, ctx: &ProcessingContext| -> String {
            let prefix = "#".repeat($level);
            let content = collect(element, ctx, "");
            format!("{} {}", prefix, content.trim())
        }
    };
}

pub struct ProcessingContext {
    rules: HashMap<String, ProcessorFn>,
}

type ProcessorFn = fn(ElementRef, &ProcessingContext) -> String;

fn collect(element: ElementRef, ctx: &ProcessingContext, sep: &str) -> String {
    element
        .children()
        .filter_map(|child| {
            if let Some(el) = ElementRef::wrap(child) {
                let tag = el.value().name();
                if let Some(rule) = ctx.rules.get(tag) {
                    Some(rule(el, ctx))
                } else {
                    Some(collect(el, ctx, ""))
                }
            } else {
                child.value().as_text().map(|v| v.to_string())
            }
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .join(sep)
}

impl ProcessingContext {
    fn new() -> Self {
        let mut ctx = Self {
            rules: HashMap::new(),
        };

        ctx.add_div_rules();
        ctx.add_details_rules();
        ctx.add_heading_rules();
        ctx.add_formatting_rules();
        ctx.add_link_rules();
        ctx.add_img_rules();
        ctx.add_code_rules();
        ctx.add_block_rules();
        ctx.add_empty_rules();
        ctx.add_table_rules();

        ctx
    }

    fn add_table_rules(&mut self) {
        self.rules.insert("table".to_string(), |element, ctx| {
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

            if let Some(align) = element.value().attr("align")
                && align.trim().to_lowercase() == "center"
            {
                return format!("<!--CENTER_ON-->\n\n{out}\n\n<!--CENTER_OFF-->");
            }

            out
        });

        self.rules
            .insert("td".to_string(), |element, ctx| collect(element, ctx, "\n"));
        self.rules
            .insert("th".to_string(), |element, ctx| collect(element, ctx, "\n"));

        self.rules.insert("tr".to_string(), |_, _| String::new());
        self.rules.insert("thead".to_string(), |_, _| String::new());
        self.rules.insert("tbody".to_string(), |_, _| String::new());
        self.rules.insert("tfoot".to_string(), |_, _| String::new());
    }

    fn add_img_rules(&mut self) {
        self.rules.insert("img".to_string(), |element, _ctx| {
            let src = element.value().attr("src").unwrap_or("");
            let alt = element.value().attr("alt").unwrap_or("IMG");
            let width = element.value().attr("width");
            let height = element.value().attr("height");

            let enhanced_src = match (width, height) {
                (Some(w), Some(h)) => format!("{}#{}x{}", src, w, h),
                (Some(w), None) => format!("{}#{}x", src, w),
                (None, Some(h)) => format!("{}#x{}", src, h),
                (None, None) => src.to_string(),
            };

            format!("![{}]({})", alt, enhanced_src)
        });
    }

    fn add_code_rules(&mut self) {
        self.rules.insert("pre".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");

            format!("```\n{}\n```", content)
        });
    }

    fn add_link_rules(&mut self) {
        self.rules.insert("a".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");

            if let Some(href) = element.value().attr("href") {
                format!("[{}]({})", content.trim(), href.trim())
            } else {
                content
            }
        });
    }

    fn add_block_rules(&mut self) {
        self.rules.insert("blockquote".to_string(), |element, ctx| {
            let content = collect(element, ctx, "\n\n");

            content.lines().map(|line| format!("> {line}")).join("\n")
        });
    }

    fn add_empty_rules(&mut self) {
        self.rules.insert("figure".to_string(), |element, ctx| {
            collect(element, ctx, "\n\n")
        });
        self.rules.insert("figcaption".to_string(), |element, ctx| {
            collect(element, ctx, "")
        });
    }

    fn add_formatting_rules(&mut self) {
        self.rules
            .insert("br".to_string(), |_element, _ctx| "\n".to_string());

        self.rules
            .insert("hr".to_string(), |_element, _ctx| "---".to_string());
    }

    fn add_heading_rules(&mut self) {
        self.rules.insert("h1".to_string(), heading_processor!(1));
        self.rules.insert("h2".to_string(), heading_processor!(2));
        self.rules.insert("h3".to_string(), heading_processor!(3));
        self.rules.insert("h4".to_string(), heading_processor!(4));
        self.rules.insert("h5".to_string(), heading_processor!(5));
        self.rules.insert("h6".to_string(), heading_processor!(6));
    }

    fn add_div_rules(&mut self) {
        for item in ["div", "p"] {
            self.rules.insert(item.to_string(), |element, ctx| {
                let sep = if element.value().name() == "p" {
                    " "
                } else {
                    "\n\n"
                };
                let content = collect(element, ctx, sep);

                if content.is_empty() {
                    return content;
                }

                if let Some(align) = element.value().attr("align")
                    && align.trim().to_lowercase() == "center"
                {
                    return format!("<!--CENTER_ON-->\n\n{content}\n\n<!--CENTER_OFF-->");
                }

                content
            });
        }
    }

    fn add_details_rules(&mut self) {
        self.rules.insert("details".to_string(), |element, ctx| {
            let content = collect(element, ctx, "\n\n");

            content.lines().map(|line| format!("> {line}")).join("\n")
        });

        self.rules.insert("summary".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("▼ {}", content.trim())
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
        let tag_regex = Regex::new(&format!(r"&lt;(/?(?:{}))\b[^&]*&gt;", known_tags)).unwrap();

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

// only add blocky elements logic, inline ones are rendered at the markdown viewer.
// only exceptions is <a> and <img>, which in some cases will be treated as blocky sadly.
pub fn process(markdown: &str) -> String {
    let ctx = ProcessingContext::new();

    let escaped_markdown = ctx.escape_unknown_elements(markdown);
    let document = Html::parse_fragment(&escaped_markdown);

    collect(document.root_element(), &ctx, "\n\n")
}
