//! Message bodies: the HTML the service sends, reduced to runs of styled text the UI
//! can lay out. Anything it doesn't know keeps its text and loses its markup.

/// How a run of text is drawn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub mention: bool,
    pub link: Option<String>,
}

/// A run of text in one style. Line breaks are `\n` inside the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub span: Span,
}

/// Plain text, as it is.
pub fn plain(text: &str) -> Vec<Inline> {
    if text.is_empty() { Vec::new() } else { vec![Inline { text: text.to_string(), span: Span::default() }] }
}

/// The text alone, on one line (for previews and notifications).
pub fn flatten(body: &[Inline]) -> String {
    let joined: String = body.iter().map(|i| i.text.as_str()).collect();
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Styled runs from a message's HTML.
pub fn from_html(html: &str) -> Vec<Inline> {
    let mut out = Out::default();
    let mut stack: Vec<Open> = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.text(&decode(rest), &stack);
            break;
        };
        out.text(&decode(&rest[..lt]), &stack);
        rest = &rest[lt..];
        let Some(gt) = rest.find('>') else {
            out.text(&decode(rest), &stack);
            break;
        };
        let tag = Tag::parse(&rest[1..gt]);
        rest = &rest[gt + 1..];
        match tag {
            Tag::Open { name, attrs } => {
                if is_block(&name) {
                    out.block();
                }
                match name.as_str() {
                    "br" => out.text("\n", &stack),
                    "img" => {
                        // Emoji come as images with the character in alt.
                        if let Some(alt) = attr(&attrs, "alt") {
                            out.text(&decode(&alt), &stack);
                        }
                    }
                    "li" => {
                        stack.push(Open { name, link: None });
                        out.text("• ", &stack);
                    }
                    "hr" => out.block(),
                    _ => {
                        let link = (name == "a").then(|| attr(&attrs, "href").map(|h| decode(&h))).flatten();
                        let mention = attr(&attrs, "itemtype").is_some_and(|t| t.ends_with("/Mention"));
                        stack.push(Open { name: if mention { "mention".into() } else { name }, link });
                    }
                }
            }
            Tag::Close(name) => {
                if let Some(at) = stack.iter().rposition(|o| o.name == name || (name == "span" && o.name == "mention")) {
                    stack.truncate(at);
                }
                if is_block(&name) {
                    out.block();
                }
            }
            Tag::Other => {}
        }
    }
    out.finish()
}

struct Open {
    name: String,
    link: Option<String>,
}

fn span_of(stack: &[Open]) -> Span {
    let has = |names: &[&str]| stack.iter().any(|o| names.contains(&o.name.as_str()));
    Span {
        bold: has(&["b", "strong", "h1", "h2", "h3"]),
        italic: has(&["i", "em"]),
        code: has(&["code", "pre"]),
        mention: has(&["mention"]),
        link: stack.iter().rev().find_map(|o| o.link.clone()),
    }
}

fn is_block(name: &str) -> bool {
    matches!(name, "p" | "div" | "ul" | "ol" | "li" | "pre" | "blockquote" | "h1" | "h2" | "h3" | "table" | "tr")
}

#[derive(Default)]
struct Out {
    runs: Vec<Inline>,
    /// A paragraph ended: the next text starts on a new line.
    pending_break: bool,
}

impl Out {
    fn text(&mut self, text: &str, stack: &[Open]) {
        let in_pre = stack.iter().any(|o| o.name == "pre");
        let text = if in_pre { text.to_string() } else { collapse(text) };
        if text.is_empty() || (text == " " && self.at_line_start()) {
            return;
        }
        if self.pending_break {
            self.pending_break = false;
            if !self.runs.is_empty() {
                self.push("\n", Span::default());
            }
        }
        let text = if self.at_line_start() { text.trim_start().to_string() } else { text };
        self.push(&text, span_of(stack));
    }

    fn push(&mut self, text: &str, span: Span) {
        if text.is_empty() {
            return;
        }
        match self.runs.last_mut() {
            Some(last) if last.span == span => last.text.push_str(text),
            _ => self.runs.push(Inline { text: text.to_string(), span }),
        }
    }

    fn at_line_start(&self) -> bool {
        self.runs.last().is_none_or(|r| r.text.ends_with('\n'))
    }

    fn block(&mut self) {
        self.pending_break = true;
    }

    fn finish(mut self) -> Vec<Inline> {
        while let Some(last) = self.runs.last_mut() {
            let trimmed = last.text.trim_end().len();
            last.text.truncate(trimmed);
            if last.text.is_empty() {
                self.runs.pop();
            } else {
                break;
            }
        }
        self.runs
    }
}

/// Runs of whitespace as one space, as a browser shows them.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            space = true;
        } else {
            if space {
                out.push(' ');
                space = false;
            }
            out.push(c);
        }
    }
    if space {
        out.push(' ');
    }
    out
}

enum Tag {
    Open {
        name: String,
        attrs: String,
    },
    Close(String),
    /// Comments, doctypes and the like.
    Other,
}

impl Tag {
    fn parse(inner: &str) -> Tag {
        let inner = inner.trim();
        if let Some(name) = inner.strip_prefix('/') {
            return Tag::Close(name.trim().to_ascii_lowercase());
        }
        if inner.starts_with('!') || inner.starts_with('?') {
            return Tag::Other;
        }
        let inner = inner.trim_end_matches('/');
        let (name, attrs) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        Tag::Open { name: name.to_ascii_lowercase(), attrs: attrs.to_string() }
    }
}

/// An attribute's value from a tag's attribute text.
fn attr(attrs: &str, key: &str) -> Option<String> {
    let mut rest = attrs;
    while let Some(eq) = rest.find('=') {
        let name = rest[..eq].trim().rsplit(char::is_whitespace).next().unwrap_or("").to_ascii_lowercase();
        let after = rest[eq + 1..].trim_start();
        let (value, next) = match after.chars().next() {
            Some(q @ ('"' | '\'')) => {
                let end = after[1..].find(q).map_or(after.len(), |e| e + 1);
                (&after[1..end], after.get(end + 1..).unwrap_or(""))
            }
            _ => {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                (&after[..end], &after[end..])
            }
        };
        if name == key {
            return Some(value.to_string());
        }
        rest = next;
    }
    None
}

/// HTML character references.
pub fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let end = rest[1..].find(|c: char| c == ';' || c == '&' || c.is_whitespace()).map(|e| e + 1);
        let entity = end.filter(|&e| rest.as_bytes()[e] == b';').map(|e| &rest[1..e]);
        let ch = entity.and_then(|e| match e {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => {
                let n = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")).map(|h| u32::from_str_radix(h, 16));
                let n = n.or_else(|| e.strip_prefix('#').map(str::parse::<u32>));
                n.and_then(Result::ok).and_then(char::from_u32)
            }
        });
        match (ch, entity) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = &rest[e.len() + 2..];
            }
            _ => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(runs: &[Inline]) -> Vec<&str> {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn paragraphs_become_lines() {
        let runs = from_html("<p>First line</p><p>Second&nbsp;one &amp; more</p>");
        assert_eq!(flatten(&runs), "First line Second one & more");
        assert_eq!(texts(&runs), ["First line\nSecond one & more"]);
    }

    #[test]
    fn keeps_styles_links_and_mentions() {
        let html = r#"<div>Hi <span itemtype="http://schema.skype.com/Mention" itemscope itemid="0">Maya</span>, see <a href="https://example.com/pr/1?a=1&amp;b=2">the PR</a> and <code>run()</code> <b>now</b></div>"#;
        let runs = from_html(html);
        assert_eq!(texts(&runs), ["Hi ", "Maya", ", see ", "the PR", " and ", "run()", " ", "now"]);
        assert!(runs[1].span.mention);
        assert_eq!(runs[3].span.link.as_deref(), Some("https://example.com/pr/1?a=1&b=2"));
        assert!(runs[5].span.code && runs[7].span.bold);
    }

    #[test]
    fn emoji_images_become_their_character() {
        let runs = from_html(r#"<p>ok <img alt="🙂" itemtype="http://schema.skype.com/Emoji" src="x"></p>"#);
        assert_eq!(flatten(&runs), "ok 🙂");
    }

    #[test]
    fn decodes_references() {
        assert_eq!(decode("a &lt;b&gt; &#39;c&#x27; &unknown; & d"), "a <b> 'c' &unknown; & d");
    }

    #[test]
    fn lists_get_bullets() {
        assert_eq!(texts(&from_html("<ul><li>one</li><li>two</li></ul>")), ["• one\n• two"]);
    }
}
