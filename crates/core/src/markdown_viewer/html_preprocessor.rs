use itertools::Itertools;
use regex::Regex;
use scraper::{ElementRef, Html};
use std::collections::HashMap;

pub struct ProcessingResult {
    pub content: String,
    pub centered_lines: Vec<usize>,
}

pub struct ProcessingContext {
    output: String,
    collect_stack: Vec<String>,
    rules: HashMap<String, ProcessorFn>,
    centered_lines: Vec<usize>,
    ensure_space_flag: bool,
}

type ProcessorFn = fn(ElementRef, &mut ProcessingContext);

// to avoid doing a move
macro_rules! heading_processor {
    ($level:expr) => {
        |element, ctx| {
            let prefix = "#".repeat($level);
            let content = ctx.collect(element).replace("\n", " ");
            let result = format!("{} {}", prefix, content.trim());
            ctx.ensure_empty_line();

            if let Some(align) = element.value().attr("align") {
                if align.trim().to_lowercase() == "center" {
                    let line_num = ctx.output.lines().count() + 1;
                    ctx.centered_lines.push(line_num);
                }
            }

            ctx.write(&result);
            ctx.ensure_space();
        }
    };
}

impl ProcessingContext {
    fn new() -> Self {
        let mut ctx = Self {
            output: String::new(),
            collect_stack: Vec::new(),
            rules: HashMap::new(),
            centered_lines: Vec::new(),
            ensure_space_flag: false,
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

    fn write(&mut self, text: &str) {
        if self.ensure_space_flag && !text.starts_with("\n") {
            self.ensure_empty_line();
        }

        // spaces and tabs after a block element only cause issues
        let text = if self.ensure_space_flag && text.trim_matches([' ', '\t']).is_empty() {
            ""
        } else {
            text
        };
        if let Some(buffer) = self.collect_stack.last_mut() {
            buffer.push_str(&text);
        } else {
            self.output.push_str(&text);
        }

        self.ensure_space_flag = false;
    }

    fn ensure_empty_line(&mut self) {
        let mut target_index = None;
        for i in (0..self.collect_stack.len()).rev() {
            if !self.collect_stack[i].is_empty() {
                target_index = Some(i);
                break;
            }
        }

        if let Some(index) = target_index {
            // Found a non-empty buffer, check if it needs a newline
            if !self.collect_stack[index].ends_with('\n') {
                self.collect_stack[index].push('\n');
            }
        } else {
            // All buffers are empty (or no buffers), check main output
            if !self.output.ends_with('\n') && !self.output.is_empty() {
                self.output.push('\n');
            }
        }
    }

    fn ensure_space(&mut self) {
        self.ensure_space_flag = true;
    }

    fn collect(&mut self, element: ElementRef) -> String {
        self.collect_stack.push(String::new());

        self.process_children(element);

        self.collect_stack.pop().unwrap()
    }

    fn add_img_rules(&mut self) {
        self.rules.insert("img".to_string(), |element, ctx| {
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

            ctx.write(&format!("![{}]({})", alt, enhanced_src));
        });
    }

    fn add_code_rules(&mut self) {
        self.rules.insert("pre".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            let content = content.trim();
            ctx.ensure_empty_line();
            ctx.write(&format!("```\n{}\n```", content));
            ctx.ensure_space();
        });

        self.rules.insert("code".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("`{}`", content));
        });
    }

    fn add_link_rules(&mut self) {
        self.rules.insert("a".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            if let Some(href) = element.value().attr("href") {
                ctx.write(&format!("[{}]({})", content.trim(), href.trim()));
            } else {
                ctx.write(&content);
            }
        });
    }

    fn add_block_rules(&mut self) {
        self.rules.insert("blockquote".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            let content = content.trim();
            ctx.ensure_empty_line();

            let content = content.lines().map(|line| format!("> {line}")).join("\n");
            ctx.write(&content);

            ctx.ensure_space();
        });
    }

    fn add_empty_rules(&mut self) {
        for item in ["figure", "figcaption"] {
            self.rules.insert(item.to_string(), |element, ctx| {
                let content = ctx.collect(element);
                let content = content.trim();

                if content.is_empty() {
                    return;
                }

                ctx.ensure_empty_line();
                ctx.write(&content);
                ctx.ensure_space();
            });
        }
    }

    fn add_formatting_rules(&mut self) {
        self.rules.insert("br".to_string(), |_element, ctx| {
            ctx.ensure_space();
        });

        self.rules.insert("var".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("*{}*", content.trim()));
        });

        self.rules.insert("i".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("*{}*", content.trim()));
        });

        self.rules.insert("hr".to_string(), |_element, ctx| {
            ctx.ensure_empty_line();
            ctx.write("<!--HR-->");
            ctx.ensure_space();
        });

        self.rules.insert("b".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("**{}**", content.trim()));
        });

        self.rules.insert("strong".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("**{}**", content.trim()));
        });

        self.rules.insert("em".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("*{}*", content.trim()));
        });

        self.rules.insert("del".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("~~{}~~", content.trim()));
        });

        self.rules.insert("s".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("~~{}~~", content.trim()));
        });

        self.rules.insert("strike".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("~~{}~~", content.trim()));
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
            let content = ctx.collect(element);
            ctx.write(&format!("\"{}\"", content));
        });
    }

    fn add_div_rules(&mut self) {
        for item in ["div", "p"] {
            self.rules.insert(item.to_string(), |element, ctx| {
                let content = ctx.collect(element);
                let content = content.trim();

                if content.is_empty() {
                    return;
                }

                ctx.ensure_empty_line();

                if let Some(align) = element.value().attr("align") {
                    if align.trim().to_lowercase() == "center" {
                        // Mark all lines of this section as centered
                        let start_line = ctx.output.lines().count() + 1;
                        ctx.write(&content);
                        let end_line = ctx.output.lines().count();

                        for line_num in start_line..=end_line {
                            ctx.centered_lines.push(line_num);
                        }
                        ctx.ensure_space();
                        return;
                    }
                }
                ctx.write(&content);
                ctx.ensure_space();
            });
        }
    }

    fn add_details_rules(&mut self) {
        self.rules.insert("details".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.ensure_empty_line();

            let content = content
                .trim()
                .lines()
                .map(|line| format!("> {line}"))
                .join("\n");
            ctx.write(&content);

            ctx.ensure_space();
        });

        self.rules.insert("summary".to_string(), |element, ctx| {
            let content = ctx.collect(element);
            ctx.write(&format!("▼ {}", content.trim()));
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
        let tag_regex = Regex::new(&format!(r"&lt;(/?(?:{}))[^&]*&gt;", known_tags)).unwrap();

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

    fn process_children(&mut self, element: ElementRef) {
        for child in element.children() {
            match child.value() {
                scraper::node::Node::Element(_) => {
                    let child_element = ElementRef::wrap(child).unwrap();
                    let tag_name = child_element.value().name();

                    if let Some(rule) = self.rules.get(tag_name) {
                        rule(child_element, self);
                    } else {
                        // For unknown elements, process their children
                        self.process_children(child_element);
                    }
                }
                scraper::node::Node::Text(text) => {
                    self.write(text.text.as_ref());
                }
                _ => {}
            }
        }
    }
}

/// # the tags handled:
/// img, pre, code, a, blockquote, br, var, i, hr, b, strong, em, del, s, strike, h1-6, q, div, p, deatils, summary
/// elements not included will remain the same.
///
/// # for attributes:
/// * img: i do src, alt, width, height: should be normal markdown, but the width and height are encoded into the url like url#{width}x{height} but can also be url#x{height} or url#{width}x
/// * div,p,headings: align=center -- doesn't reflect in the markdown itself.
/// * a: href, should be same as markdown.
///
/// <inline> -- just writes into the buffer
/// <block>  -- calls ensure_empty_line before the write and ensure_space after the write
/// <v>      -- the content (can be nested, elements but after formatting)
///
/// # elements formatting:
/// img:        <inline>,  as mentioned in the atrributes section
/// pre:        <block>,   ```{v}```
/// code:       <inline>,  `{v}`
/// a:          <inline>,  with href: [{v}]({href}) without href {v}
/// blockquote: <block>,   maps each line to "> {v}"
/// br:         <N>,       calls ensure_space
/// var:        <inline>,  *{v}*
/// i:          <inline>,  *{v}*
/// hr:         <block>,   <!--HR-->
/// b:          <inline>,  **{v}**
/// strong:     <inline>,  **{v}**
/// em:         <inline>,  *{v}*
/// del:        <inline>,  ~~{v}~~
/// s:          <inline>,  ~~{v}~~
/// strike:     <inline>,  ~~{v}~~
/// h1-6:       <?>,       #*N{v} (partial block, just calls ensure_empty_line)
/// p,div:      <block>,   {v}
/// q:          <block>,   "{v}" (quoted)
/// details:    <block>,   maps each line to "> {v}"
/// summary:    <inline>,  "▼ {v}"
///
/// # NOTE
/// paragraphs don't enforce a \n\n like it should in markdown spec.
pub fn process(markdown: &str) -> ProcessingResult {
    let mut ctx = ProcessingContext::new();

    let escaped_markdown = ctx.escape_unknown_elements(markdown);
    let document = Html::parse_fragment(&escaped_markdown);

    ctx.process_children(document.root_element());

    ProcessingResult {
        content: ctx.output,
        centered_lines: ctx.centered_lines,
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown_viewer::html_preprocessor::process;

    #[test]
    fn converts_complex_html_to_markdown_correctly() {
        let html = r#"
<h1>Big&nbsp;Title</h1>Text before.  
<h3>
multi line
title
</h3>

<p>
Some <b>bold <em>and *nested*</em></b> text
plus an <a href="https://example.com">inline link</a>
and one without href: <a>bare link</a>.
</p>

<div>
<h1>Big&nbsp;Title</h1>
<h2>Subtitle</h2>
<blockquote>
<p>A quoted<br>paragraph.</p>
</blockquote>

<details>
<summary>Details&nbsp;title</summary>
<p>Hidden <var>code</var> goes here.</p>
</details>
</div>

<p>hello world</p><p>this sure is a good day</p>

<pre>&lt;raw code block&gt;</pre>

<code>inline_code()</code>

<b>hello</b> <em>world</em>

<img src="pic.png" alt="alt" width="640" height="480"> <img src="half.png" alt="h" height="120">
<img src="half.png" alt="h" height="120">
<img src="wide.png" alt="w" width="320"> <hr>

<hr> <hr>

<q>Inline quotation</q>

<s>strike</s>, <del>delete</del>, <strike>old‑strike</strike>

Text after.
"#;

        let expected = r#"
# Big Title
Text before.  
### multi line title

Some **bold *and *nested**** text
plus an [inline link](https://example.com)
and one without href: bare link.

# Big Title
## Subtitle
> A quoted
> paragraph.

> ▼ Details title
> Hidden *code* goes here.

hello world
this sure is a good day

```
<raw code block>
```

`inline_code()`

**hello** *world*

![alt](pic.png#640x480) ![h](half.png#x120)
![h](half.png#x120)
![w](wide.png#320x) 
<!--HR-->

<!--HR-->
<!--HR-->

"Inline quotation"

~~strike~~, ~~delete~~, ~~old‑strike~~

Text after.
"#;

        let res = process(html);

        assert_eq!(res.content, expected);
    }
}
