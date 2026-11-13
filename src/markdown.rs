//! Lays out markdown (or plain text) as lines for a fixed-width receipt
//! printer.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// The printer has 48 columns, but filling a line completely makes it wrap
/// on its own, and the newline that follows then prints a blank line.
pub const COLUMNS: usize = 47;

/// Indentation is capped so that deeply nested content keeps room for text.
const MIN_TEXT_COLUMNS: usize = 8;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub bold: bool,
    pub underline: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub centered: bool,
    pub double_size: bool,
    pub spans: Vec<(Style, String)>,
}

type StyledChar = (char, Style);

/// Drops control characters, which the printer would interpret as commands.
fn sanitize(text: &str) -> String {
    text.replace('\t', "    ")
        .chars()
        .filter(|c| *c == '\n' || !c.is_control())
        .collect()
}

fn plain(text: &str) -> impl Iterator<Item = StyledChar> + '_ {
    text.chars().map(|c| (c, Style::default()))
}

fn to_spans(chars: &[StyledChar]) -> Vec<(Style, String)> {
    let mut spans: Vec<(Style, String)> = Vec::new();
    for &(c, style) in chars {
        match spans.last_mut() {
            Some((last_style, text)) if *last_style == style => text.push(c),
            _ => spans.push((style, c.to_string())),
        }
    }
    spans
}

fn clamp_prefix(prefix: &str, width: usize) -> Vec<StyledChar> {
    plain(prefix)
        .take(width.saturating_sub(MIN_TEXT_COLUMNS))
        .collect()
}

/// Word wraps `chars` to `width` columns. A `\n` forces a line break.
fn wrap(
    chars: &[StyledChar],
    width: usize,
    first_prefix: &str,
    cont_prefix: &str,
) -> Vec<Vec<StyledChar>> {
    let cont_prefix = clamp_prefix(cont_prefix, width);
    let mut lines = Vec::new();
    let mut line = clamp_prefix(first_prefix, width);

    for segment in chars.split(|(c, _)| *c == '\n') {
        let mut has_word = false;
        for mut word in segment
            .split(|(c, _)| c.is_whitespace())
            .filter(|w| !w.is_empty())
        {
            loop {
                if line.len() + word.len() + usize::from(has_word) <= width {
                    if has_word {
                        // Keep styling continuous across a run of words.
                        let prev = line.last().map(|(_, s)| *s);
                        let style = prev.filter(|s| *s == word[0].1).unwrap_or_default();
                        line.push((' ', style));
                    }
                    line.extend_from_slice(word);
                    has_word = true;
                    break;
                }
                if !has_word {
                    // The word is longer than a whole line: split it.
                    let (head, tail) = word.split_at(width - line.len());
                    line.extend_from_slice(head);
                    word = tail;
                }
                lines.push(std::mem::replace(&mut line, cont_prefix.clone()));
                has_word = false;
            }
        }
        if has_word {
            lines.push(std::mem::replace(&mut line, cont_prefix.clone()));
        }
    }
    lines
}

/// Lays out text without interpreting it, keeping line breaks and leading
/// indentation.
pub fn render_plain(text: &str) -> Vec<Line> {
    let text = sanitize(text);
    let mut lines = Vec::new();
    for raw in text.trim_end().lines() {
        let body = raw.trim_start();
        let indent = &raw[..raw.len() - body.len()];
        let chars: Vec<StyledChar> = plain(body).collect();
        let wrapped = wrap(&chars, COLUMNS, indent, indent);
        if wrapped.is_empty() {
            lines.push(Line::default());
        }
        lines.extend(wrapped.iter().map(|l| Line {
            spans: to_spans(l),
            ..Line::default()
        }));
    }
    lines
}

#[derive(Default)]
struct Renderer {
    lines: Vec<Line>,
    /// Inline content of the block currently being read.
    buf: Vec<StyledChar>,
    bold: u32,
    italic: u32,
    heading: Option<HeadingLevel>,
    /// Indentation contributed by each enclosing list item or block quote.
    indent: Vec<String>,
    /// List marker waiting to be printed on the next line.
    marker: Option<String>,
    /// Next number for each enclosing list, `None` for bulleted lists.
    lists: Vec<Option<u64>>,
    code_block: Option<String>,
    links: Vec<String>,
}

impl Renderer {
    fn style(&self) -> Style {
        Style {
            bold: self.bold > 0 || matches!(self.heading, Some(l) if l != HeadingLevel::H1),
            underline: self.italic > 0 || self.heading == Some(HeadingLevel::H2),
        }
    }

    fn push_str(&mut self, text: &str) {
        let style = self.style();
        self.buf.extend(text.chars().map(|c| (c, style)));
    }

    fn blank(&mut self) {
        if self.lines.last().is_some_and(|l| !l.spans.is_empty()) {
            self.lines.push(Line::default());
        }
    }

    /// Emits the buffered inline content as wrapped lines.
    fn flush(&mut self) {
        if self.buf.is_empty() {
            return;
        }
        // The top level heading is printed double size, so half as many
        // characters fit.
        let title = self.heading == Some(HeadingLevel::H1);
        let width = if title { COLUMNS / 2 } else { COLUMNS };

        let cont_prefix = self.indent.concat();
        let first_prefix = match self.marker.take() {
            Some(marker) => {
                let outer = &self.indent[..self.indent.len().saturating_sub(1)];
                outer.concat() + &marker
            }
            None => cont_prefix.clone(),
        };
        let buf = std::mem::take(&mut self.buf);
        for line in wrap(&buf, width, &first_prefix, &cont_prefix) {
            self.lines.push(Line {
                centered: title,
                double_size: title,
                spans: to_spans(&line),
            });
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Link { dest_url, .. } => self.links.push(dest_url.into_string()),
            Tag::Image { .. } => self.push_str("[image: "),
            tag => {
                // Everything else starts a new block.
                self.flush();
                match tag {
                    Tag::Heading { level, .. } => {
                        self.blank();
                        self.heading = Some(level);
                    }
                    Tag::BlockQuote(_) => self.indent.push("| ".to_string()),
                    Tag::CodeBlock(_) => self.code_block = Some(String::new()),
                    Tag::List(start) => self.lists.push(start),
                    Tag::Item => {
                        let marker = match self.lists.last_mut() {
                            Some(Some(n)) => {
                                *n += 1;
                                format!("{}. ", *n - 1)
                            }
                            _ => "- ".to_string(),
                        };
                        self.indent.push(" ".repeat(marker.len()));
                        self.marker = Some(marker);
                    }
                    _ => {}
                }
            }
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Emphasis => self.italic -= 1,
            TagEnd::Strong => self.bold -= 1,
            TagEnd::Link => {
                let url = self.links.pop().unwrap_or_default();
                let shown: String = self.buf.iter().map(|(c, _)| c).collect();
                // Autolinks already show their destination.
                if !url.is_empty() && !shown.ends_with(&url) {
                    self.push_str(&format!(" ({url})"));
                }
            }
            TagEnd::Image => self.push_str("]"),
            tag => {
                self.flush();
                match tag {
                    TagEnd::Heading(_) => {
                        self.heading = None;
                        self.blank();
                    }
                    TagEnd::BlockQuote(_) => {
                        self.indent.pop();
                        self.blank();
                    }
                    TagEnd::CodeBlock => self.end_code_block(),
                    TagEnd::List(_) => {
                        self.lists.pop();
                        if self.lists.is_empty() {
                            self.blank();
                        }
                    }
                    TagEnd::Item => {
                        self.indent.pop();
                        self.marker = None;
                    }
                    TagEnd::Paragraph if self.lists.is_empty() => self.blank(),
                    _ => {}
                }
            }
        }
    }

    /// Code is printed verbatim: over-long lines are split, not word wrapped.
    fn end_code_block(&mut self) {
        let code = self.code_block.take().unwrap_or_default();
        let prefix = clamp_prefix(&self.indent.concat(), COLUMNS);
        for code_line in code.lines() {
            let chars: Vec<StyledChar> = plain(code_line).collect();
            let mut chunks = chars.chunks(COLUMNS - prefix.len()).peekable();
            if chunks.peek().is_none() {
                self.lines.push(Line::default());
            }
            for chunk in chunks {
                let line: Vec<StyledChar> = prefix.iter().chain(chunk).copied().collect();
                self.lines.push(Line {
                    spans: to_spans(&line),
                    ..Line::default()
                });
            }
        }
        if self.lists.is_empty() {
            self.blank();
        }
    }

    fn event(&mut self, event: Event) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => match &mut self.code_block {
                Some(code) => code.push_str(&text),
                None => self.push_str(&text),
            },
            Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => self.push_str(&text),
            Event::SoftBreak => self.push_str(" "),
            Event::HardBreak => self.push_str("\n"),
            Event::Rule => {
                self.flush();
                self.blank();
                self.lines.push(Line {
                    spans: vec![(Style::default(), "-".repeat(COLUMNS))],
                    ..Line::default()
                });
                self.blank();
            }
            Event::TaskListMarker(done) => self.push_str(if done { "[x] " } else { "[ ] " }),
            _ => {}
        }
    }
}

/// Lays out markdown: `#` headings are large and centered, lower headings
/// and `**strong**` are bold, `*emphasis*` is underlined.
pub fn render_markdown(text: &str) -> Vec<Line> {
    let text = sanitize(text);
    let mut renderer = Renderer::default();
    for event in Parser::new_ext(&text, Options::ENABLE_TASKLISTS) {
        renderer.event(event);
    }
    renderer.flush();
    while renderer.lines.last().is_some_and(|l| l.spans.is_empty()) {
        renderer.lines.pop();
    }
    renderer.lines
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOLD: Style = Style {
        bold: true,
        underline: false,
    };
    const UNDERLINE: Style = Style {
        bold: false,
        underline: true,
    };

    fn texts(lines: &[Line]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|(_, t)| t.as_str()).collect())
            .collect()
    }

    #[test]
    fn inline_styles() {
        let lines = render_markdown("plain **bold text** and *it*");
        assert_eq!(
            lines[0].spans,
            vec![
                (Style::default(), "plain ".to_string()),
                (BOLD, "bold text".to_string()),
                (Style::default(), " and ".to_string()),
                (UNDERLINE, "it".to_string()),
            ]
        );
    }

    #[test]
    fn headings() {
        let lines = render_markdown("# Title\n\n## Section\n\nbody");
        assert_eq!(texts(&lines), ["Title", "", "Section", "", "body"]);
        assert!(lines[0].centered && lines[0].double_size);
        assert_eq!(lines[0].spans[0].0, Style::default());
        assert_eq!(
            lines[2].spans[0].0,
            Style {
                bold: true,
                underline: true
            }
        );
        assert!(!lines[2].double_size);
    }

    #[test]
    fn long_title_wraps_at_half_width() {
        let lines = render_markdown("# one two three four five six seven");
        assert!(lines.len() > 1);
        assert!(
            texts(&lines)
                .iter()
                .all(|l| l.chars().count() <= COLUMNS / 2)
        );
    }

    #[test]
    fn wraps_words() {
        let lines = render_markdown(&"word ".repeat(30));
        let texts = texts(&lines);
        assert_eq!(texts.len(), 4);
        assert!(texts.iter().all(|l| l.len() <= COLUMNS));
        assert!(
            texts
                .iter()
                .all(|l| !l.starts_with(' ') && !l.ends_with(' '))
        );
    }

    #[test]
    fn splits_long_words() {
        let lines = render_markdown(&"x".repeat(100));
        assert_eq!(
            texts(&lines),
            ["x".repeat(47), "x".repeat(47), "x".repeat(6)]
        );
    }

    #[test]
    fn lists() {
        let md = "- one\n- two\n  1. nested\n  2. again\n- [ ] todo\n- [x] done\n\nafter";
        assert_eq!(
            texts(&render_markdown(md)),
            [
                "- one",
                "- two",
                "  1. nested",
                "  2. again",
                "- [ ] todo",
                "- [x] done",
                "",
                "after"
            ]
        );
    }

    #[test]
    fn list_items_wrap_with_hanging_indent() {
        let lines = render_markdown(&format!("9. {}", "word ".repeat(12)));
        let texts = texts(&lines);
        assert!(texts[0].starts_with("9. word"));
        assert!(texts[1].starts_with("   word"));
    }

    #[test]
    fn code_block_and_quote() {
        let md = "```\nfn main() {\n    hi();\n}\n```\n\n> quoted\n> text";
        assert_eq!(
            texts(&render_markdown(md)),
            ["fn main() {", "    hi();", "}", "", "| quoted text"]
        );
    }

    #[test]
    fn links_and_rules() {
        let md = "[site](https://example.com) <https://example.org>\n\n---\n\nend";
        assert_eq!(
            texts(&render_markdown(md)),
            [
                "site (https://example.com) https://example.org",
                "",
                &"-".repeat(COLUMNS),
                "",
                "end"
            ]
        );
    }

    #[test]
    fn strips_control_characters() {
        let lines = render_markdown("a\x1b@b\x1dVc");
        assert_eq!(texts(&lines), ["a@bVc"]);
        let lines = render_plain("a\x1b@b\r\n\x07c");
        assert_eq!(texts(&lines), ["a@b", "c"]);
    }

    #[test]
    fn plain_text_is_not_interpreted() {
        let lines = render_plain("# not *a* title\n\n  indented line\n");
        assert_eq!(texts(&lines), ["# not *a* title", "", "  indented line"]);
        assert!(lines.iter().all(|l| !l.double_size && !l.centered));
        assert!(
            lines
                .iter()
                .flat_map(|l| &l.spans)
                .all(|(s, _)| *s == Style::default())
        );
    }
}
