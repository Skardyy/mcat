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
        ctx.add_quote_rules();
        ctx.add_heading_rules();
        ctx.add_formatting_rules();
        ctx.add_link_rules();
        ctx.add_img_rules();
        ctx.add_code_rules();
        ctx.add_block_rules();
        ctx.add_empty_rules();

        ctx
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

        self.rules.insert("code".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("`{}`", content)
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

        self.rules.insert("var".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("*{}*", content.trim())
        });

        self.rules.insert("i".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("*{}*", content.trim())
        });

        self.rules
            .insert("hr".to_string(), |_element, _ctx| "---".to_string());

        self.rules.insert("b".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("**{}**", content.trim())
        });

        self.rules.insert("strong".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("**{}**", content.trim())
        });

        self.rules.insert("em".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("*{}*", content.trim())
        });

        self.rules.insert("del".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("~~{}~~", content.trim())
        });

        self.rules.insert("s".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
            format!("~~{}~~", content.trim())
        });

        self.rules.insert("strike".to_string(), |element, ctx| {
            let content = collect(element, ctx, "");
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
            let content = collect(element, ctx, "");
            format!("\"{}\"", content)
        });
    }

    fn add_div_rules(&mut self) {
        for item in ["div", "p"] {
            self.rules.insert(item.to_string(), |element, ctx| {
                let content = collect(element, ctx, "\n\n");

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

        tag_regex
            .replace_all(&escaped, |caps: &regex::Captures| {
                caps.get(0)
                    .unwrap()
                    .as_str()
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
            })
            .to_string()
    }
}

/// Replace backtick-enclosed code spans with placeholders.
/// Handles both single and multiple backtick delimiters.
fn protect_code_spans(input: &str) -> (String, Vec<String>) {
    let mut result = String::with_capacity(input.len());
    let mut placeholders: Vec<String> = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '`' {
            // Count consecutive backticks
            let start = i;
            let mut tick_count = 0;
            while i < chars.len() && chars[i] == '`' {
                tick_count += 1;
                i += 1;
            }
            // Find the matching closing backticks
            let mut found_end = false;
            let mut j = i;
            while j <= chars.len().saturating_sub(tick_count) {
                let mut matches = true;
                for k in 0..tick_count {
                    if chars[j + k] != '`' {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    // Verify the closing delimiter is exactly
                    // tick_count backticks (not more)
                    let after = j + tick_count;
                    if after >= chars.len() || chars[after] != '`' {
                        let end = j + tick_count;
                        let span: String = chars[start..end]
                            .iter().collect();
                        let idx = placeholders.len();
                        placeholders.push(span);
                        result.push_str(
                            &format!("MCAT_CODE_PLACEHOLDER_{}_END", idx),
                        );
                        i = end;
                        found_end = true;
                        break;
                    }
                }
                j += 1;
            }
            if !found_end {
                // No matching close; emit the backticks literally
                for _ in 0..tick_count {
                    result.push('`');
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    (result, placeholders)
}

pub fn process(markdown: &str) -> String {
    let ctx = ProcessingContext::new();

    // Protect backtick-enclosed content from HTML processing.
    // Replace inline code spans with placeholders so the HTML
    // parser does not interpret tags inside backticks.
    let (protected, placeholders) = protect_code_spans(markdown);

    let escaped_markdown = ctx.escape_unknown_elements(&protected);
    let document = Html::parse_fragment(&escaped_markdown);
    let mut content = collect(document.root_element(), &ctx, "\n\n");

    // Restore backtick-enclosed content
    for (i, original) in placeholders.iter().enumerate() {
        let placeholder = format!("MCAT_CODE_PLACEHOLDER_{}_END", i);
        content = content.replace(&placeholder, original);
    }

    content
}
