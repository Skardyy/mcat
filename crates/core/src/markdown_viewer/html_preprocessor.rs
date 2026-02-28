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
            } else if let Some(text) = child.value().as_text() {
                Some(text.to_string())
            } else {
                None
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

                if let Some(align) = element.value().attr("align") {
                    if align.trim().to_lowercase() == "center" {
                        return format!("<!--CENTER_ON-->\n\n{content}\n\n<!--CENTER_OFF-->");
                    }
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

pub fn process(markdown: &str) -> String {
    let ctx = ProcessingContext::new();

    let escaped_markdown = ctx.escape_unknown_elements(markdown);
    let document = Html::parse_fragment(&escaped_markdown);

    let content = collect(document.root_element(), &ctx, "\n\n");

    content
}
