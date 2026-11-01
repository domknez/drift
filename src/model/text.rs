//! Message bodies: the HTML the service sends, reduced to blocks (paragraphs, tables,
//! quotes, code) of styled runs the UI can lay out. Anything it doesn't know keeps
//! its text and loses its markup.

/// How a run of text is drawn.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub code: bool,
    pub mention: bool,
    pub link: Option<String>,
    /// The text is an emoji the service has a picture of, by its key.
    pub emoji: Option<String>,
}

/// A run of text in one style. Line breaks are `\n` inside the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Inline {
    pub text: String,
    pub span: Span,
}

/// One block of a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Text(Vec<Inline>),
    /// Rows of cells; the first row is a header when `header` is set.
    Table {
        rows: Vec<Vec<Vec<Inline>>>,
        header: bool,
    },
    /// A quote, or the message a reply quotes (with that message's id).
    Quote(Vec<Inline>, Option<String>),
    /// Preformatted text, spaces kept.
    Code(String),
    /// A picture in the message: where it is, and its size if the message says.
    Image {
        url: String,
        size: Option<(u32, u32)>,
    },
}

/// Plain text, as it is.
pub fn plain(text: &str) -> Vec<Block> {
    if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![Block::Text(vec![Inline { text: text.trim().to_string(), span: Span::default() }])]
    }
}

/// The text alone, on one line (for previews and notifications).
pub fn flatten(body: &[Block]) -> String {
    let mut words = Vec::new();
    for block in body {
        match block {
            Block::Text(runs) | Block::Quote(runs, _) => words.extend(runs.iter().map(|r| r.text.as_str())),
            Block::Table { rows, .. } => words.extend(rows.iter().flatten().flatten().map(|r| r.text.as_str())),
            Block::Code(text) => words.push(text),
            Block::Image { .. } => words.push("[image]"),
        }
    }
    words.join(" ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A message as Markdown again, for editing it: what `markdown::to_html` reads.
pub fn to_markdown(body: &[Block]) -> String {
    let runs = |runs: &[Inline]| -> String {
        runs.iter()
            .map(|r| {
                let s = &r.span;
                let t = r.text.as_str();
                if s.mention || s.emoji.is_some() {
                    return if s.mention { format!("@{t}") } else { t.to_string() };
                }
                if s.code {
                    return format!("`{t}`");
                }
                let mut out = match &s.link {
                    Some(url) if url != t => format!("[{t}]({url})"),
                    _ => t.to_string(),
                };
                if s.strike {
                    out = format!("~~{out}~~");
                }
                if s.italic {
                    out = format!("*{out}*");
                }
                if s.bold {
                    out = format!("**{out}**");
                }
                out
            })
            .collect()
    };
    let mut parts = Vec::new();
    for block in body {
        parts.push(match block {
            Block::Text(r) => runs(r),
            Block::Quote(r, _) => runs(r).lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n"),
            Block::Code(code) => format!("```\n{code}\n```"),
            Block::Image { .. } => String::new(),
            Block::Table { rows, .. } => {
                rows.iter().map(|row| row.iter().map(|c| runs(c)).collect::<Vec<_>>().join(" | ")).collect::<Vec<_>>().join("\n")
            }
        });
    }
    parts.join("\n\n")
}

/// Blocks from a message's HTML.
pub fn from_html(html: &str) -> Vec<Block> {
    let mut b = Builder::default();
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            b.text(&decode(rest));
            break;
        };
        b.text(&decode(&rest[..lt]));
        rest = &rest[lt..];
        let Some(gt) = rest.find('>') else {
            b.text(&decode(rest));
            break;
        };
        let tag = Tag::parse(&rest[1..gt]);
        rest = &rest[gt + 1..];
        match tag {
            Tag::Open { name, attrs } => b.open(name, &attrs),
            Tag::Close(name) => b.close(&name),
            Tag::Other => {}
        }
    }
    b.finish()
}

struct Open {
    name: String,
    link: Option<String>,
}

#[derive(Default)]
struct Table {
    rows: Vec<Vec<Vec<Inline>>>,
    header: bool,
    /// A `<td>` or `<th>` is open.
    in_cell: bool,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    stack: Vec<Open>,
    /// The paragraph, quote or cell being written.
    runs: Runs,
    table: Option<Table>,
    /// Inside a `<blockquote>` (depth).
    quote: usize,
    /// The id of the message a reply's quote is of.
    quoted: Option<String>,
    /// Inside a `<pre>`: its text so far.
    pre: Option<String>,
}

impl Builder {
    fn span(&self) -> Span {
        let has = |names: &[&str]| self.stack.iter().any(|o| names.contains(&o.name.as_str()));
        Span {
            bold: has(&["b", "strong", "h1", "h2", "h3", "th"]),
            italic: has(&["i", "em"]),
            strike: has(&["s", "strike", "del"]),
            underline: has(&["u", "ins"]),
            code: has(&["code"]),
            mention: has(&["mention"]),
            link: self.stack.iter().rev().find_map(|o| o.link.clone()),
            emoji: None,
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(pre) = &mut self.pre {
            pre.push_str(text);
            return;
        }
        let span = self.span();
        if span.code {
            self.runs.text(text, span);
            return;
        }
        // Emoji Teams has pictures of become emoji runs (shown as the picture).
        let mut plain = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            match super::emoji::key_of(c) {
                Some(key) => {
                    if chars.peek() == Some(&'\u{fe0f}') {
                        chars.next();
                    }
                    self.runs.text(&std::mem::take(&mut plain), span.clone());
                    self.runs.text(&c.to_string(), Span { emoji: Some(key.to_string()), ..span.clone() });
                }
                None => plain.push(c),
            }
        }
        self.runs.text(&plain, span);
    }

    fn open(&mut self, name: String, attrs: &str) {
        match name.as_str() {
            "br" => {
                if let Some(pre) = &mut self.pre {
                    pre.push('\n');
                } else {
                    self.runs.text("\n", Span::default());
                }
                return;
            }
            "img"
                if !attr(attrs, "itemtype").is_some_and(|t| is_emoji(&t))
                    && attr(attrs, "src").is_some_and(|s| s.starts_with("https://") || s.starts_with("http://")) =>
            {
                // A picture someone pasted or attached, or a GIF they picked (anything but
                // an emoji).
                if let Some(url) = attr(attrs, "src") {
                    let number = |k: &str| attr(attrs, k).and_then(|v| v.trim_end_matches("px").parse::<u32>().ok()).filter(|n| *n > 0);
                    let size = number("width").zip(number("height"));
                    self.end_paragraph();
                    self.blocks.push(Block::Image { url: decode(&url), size });
                }
                return;
            }
            "img" => {
                // Emoji come as images: their key in itemid, the character in alt (an
                // organisation's own: its name, shown as :name: until its picture is in).
                if let Some(alt) = attr(attrs, "alt") {
                    let kind = attr(attrs, "itemtype").filter(|t| is_emoji(t));
                    let key = kind.as_ref().and(attr(attrs, "itemid"));
                    let alt = match kind {
                        Some(t) if t.ends_with("/CustomEmoji") => format!(":{}:", decode(&alt)),
                        _ => decode(&alt),
                    };
                    match key {
                        Some(key) if self.pre.is_none() => {
                            let span = Span { emoji: Some(key), ..self.span() };
                            self.runs.text(&alt, span);
                        }
                        _ => self.text(&alt),
                    }
                }
                return;
            }
            "hr" => {
                self.end_paragraph();
                return;
            }
            "pre" => {
                self.end_paragraph();
                self.pre = Some(String::new());
            }
            "table" => {
                self.end_paragraph();
                self.table.get_or_insert_default();
            }
            "tr" => {
                self.end_cell();
                if let Some(t) = &mut self.table {
                    t.rows.push(Vec::new());
                }
            }
            "td" | "th" => {
                self.end_cell();
                if let Some(t) = &mut self.table {
                    if t.rows.is_empty() {
                        t.rows.push(Vec::new());
                    }
                    if name == "th" && t.rows.len() == 1 {
                        t.header = true;
                    }
                    t.in_cell = true;
                }
            }
            "blockquote" => {
                self.end_paragraph();
                self.quote += 1;
                if attr(attrs, "itemtype").is_some_and(|t| t.ends_with("/Reply")) {
                    self.quoted = attr(attrs, "itemid").filter(|id| !id.is_empty());
                }
            }
            "li" => {
                self.runs.block();
                self.stack.push(Open { name, link: None });
                self.runs.text("• ", Span::default());
                return;
            }
            n if is_block(n) => self.runs.block(),
            _ => {}
        }
        let link = (name == "a").then(|| attr(attrs, "href").map(|h| decode(&h))).flatten();
        let mention = attr(attrs, "itemtype").is_some_and(|t| t.ends_with("/Mention"));
        self.stack.push(Open { name: if mention { "mention".into() } else { name }, link });
    }

    fn close(&mut self, name: &str) {
        if let Some(at) = self.stack.iter().rposition(|o| o.name == name || (name == "span" && o.name == "mention")) {
            self.stack.truncate(at);
        }
        match name {
            "pre" => {
                if let Some(code) = self.pre.take() {
                    let code = code.trim_matches('\n').to_string();
                    if !code.trim().is_empty() {
                        self.blocks.push(Block::Code(code));
                    }
                }
            }
            "td" | "th" => self.end_cell(),
            "table" => {
                self.end_cell();
                if let Some(t) = self.table.take() {
                    let rows: Vec<_> = t.rows.into_iter().filter(|r| !r.is_empty()).collect();
                    if !rows.is_empty() {
                        self.blocks.push(Block::Table { rows, header: t.header });
                    }
                }
            }
            "blockquote" => {
                self.end_paragraph();
                self.quote = self.quote.saturating_sub(1);
                if self.quote == 0 {
                    self.quoted = None;
                }
            }
            n if is_block(n) => self.runs.block(),
            _ => {}
        }
    }

    /// Put what was written so far into the open cell, if one is open.
    fn end_cell(&mut self) {
        let Some(t) = &mut self.table else { return };
        let runs = std::mem::take(&mut self.runs).finish();
        if !t.in_cell {
            return;
        }
        t.in_cell = false;
        match t.rows.last_mut() {
            Some(row) => row.push(runs),
            None => t.rows.push(vec![runs]),
        }
    }

    fn end_paragraph(&mut self) {
        if self.table.is_some() {
            return;
        }
        let runs = std::mem::take(&mut self.runs).finish();
        if runs.is_empty() {
            return;
        }
        self.blocks.push(if self.quote > 0 { Block::Quote(runs, self.quoted.clone()) } else { Block::Text(runs) });
    }

    fn finish(mut self) -> Vec<Block> {
        if let Some(code) = self.pre.take() {
            self.blocks.push(Block::Code(code));
        }
        self.end_cell();
        if let Some(t) = self.table.take()
            && !t.rows.is_empty()
        {
            self.blocks.push(Block::Table { rows: t.rows, header: t.header });
        }
        self.end_paragraph();
        self.blocks
    }
}

fn is_block(name: &str) -> bool {
    matches!(name, "p" | "div" | "ul" | "ol" | "li" | "h1" | "h2" | "h3" | "h4")
}

/// Runs being written, merging neighbours of one style.
#[derive(Default)]
struct Runs {
    runs: Vec<Inline>,
    /// A paragraph ended: the next text starts on a new line.
    pending_break: bool,
}

impl Runs {
    fn text(&mut self, text: &str, span: Span) {
        if text == "\n" {
            self.push("\n", Span::default());
            return;
        }
        let text = collapse(text);
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
        self.push(&text, span);
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
        while self.runs.first().is_some_and(|r| r.text.trim().is_empty()) {
            self.runs.remove(0);
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
/// An emoji's `itemtype`: Teams' own, or an organisation's.
fn is_emoji(itemtype: &str) -> bool {
    itemtype.ends_with("/Emoji") || itemtype.ends_with("/CustomEmoji")
}

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

    fn texts(blocks: &[Block]) -> Vec<Vec<&str>> {
        blocks
            .iter()
            .map(|b| match b {
                Block::Text(r) | Block::Quote(r, _) => r.iter().map(|r| r.text.as_str()).collect(),
                Block::Code(c) => vec![c.as_str()],
                Block::Table { .. } => vec!["<table>"],
                Block::Image { .. } => vec!["<image>"],
            })
            .collect()
    }

    fn runs(blocks: &[Block]) -> &[Inline] {
        match &blocks[0] {
            Block::Text(r) => r,
            b => panic!("not text: {b:?}"),
        }
    }

    #[test]
    fn paragraphs_become_lines() {
        let blocks = from_html("<p>First line</p><p>Second&nbsp;one &amp; more</p>");
        assert_eq!(flatten(&blocks), "First line Second one & more");
        assert_eq!(texts(&blocks), [["First line\nSecond one & more"]]);
    }

    #[test]
    fn keeps_styles_links_and_mentions() {
        let html = r#"<div>Hi <span itemtype="http://schema.skype.com/Mention" itemscope itemid="0">Maya</span>, see <a href="https://example.com/pr/1?a=1&amp;b=2">the PR</a> and <code>run()</code> <b>now</b> <s>not</s></div>"#;
        let blocks = from_html(html);
        let r = runs(&blocks);
        assert_eq!(
            r.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            ["Hi ", "Maya", ", see ", "the PR", " and ", "run()", " ", "now", " ", "not"]
        );
        assert!(r[1].span.mention && r[5].span.code && r[7].span.bold && r[9].span.strike);
        assert_eq!(r[3].span.link.as_deref(), Some("https://example.com/pr/1?a=1&b=2"));
    }

    #[test]
    fn emoji_images_become_their_character() {
        let blocks = from_html(r#"<p>ok <img alt="🙂" itemtype="http://schema.skype.com/Emoji" itemid="smile" src="x"></p>"#);
        assert_eq!(flatten(&blocks), "ok 🙂");
        assert_eq!(runs(&blocks)[1].span.emoji.as_deref(), Some("smile"));
    }

    #[test]
    fn custom_emoji_become_emoji() {
        let blocks = from_html(
            r#"<p><span><img alt="wave" itemid="wave;0-weu-d1-abc" itemtype="http://schema.skype.com/CustomEmoji" src="https://us-prod.asyncgw.teams.microsoft.com/v1/objects/0-weu-d1-abc/views/imgt1_anim" style="width:20px; height:20px"></span></p>"#,
        );
        let r = runs(&blocks);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].text.as_str(), r[0].span.emoji.as_deref()), (":wave:", Some("wave;0-weu-d1-abc")));
    }

    #[test]
    fn known_emoji_characters_become_emoji() {
        let blocks = from_html("<p>great 👍️ job</p>");
        let r = runs(&blocks);
        assert_eq!(r.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["great ", "👍", " job"]);
        assert_eq!(r[1].span.emoji.as_deref(), Some("yes"));
    }

    #[test]
    fn reply_quotes_know_their_message() {
        let html = r#"<blockquote itemscope="" itemtype="http://schema.skype.com/Reply" itemid="1698"><strong itemprop="mri">Maya</strong><p itemprop="preview">old</p></blockquote><p>new</p>"#;
        assert!(matches!(&from_html(html)[0], Block::Quote(_, Some(id)) if id == "1698"));
    }

    #[test]
    fn gifs_are_pictures() {
        let html = r#"<p><img itemtype="http://schema.skype.com/Giphy" src="https://media.example.com/a.gif" alt="A GIF" width="200" height="150"></p>"#;
        assert_eq!(from_html(html), vec![Block::Image { url: "https://media.example.com/a.gif".into(), size: Some((200, 150)) }]);
    }

    #[test]
    fn pictures_become_image_blocks() {
        let html = r#"<p>look</p><p><img itemscope="" itemtype="http://schema.skype.com/AMSImage" src="https://us-api.asm.skype.com/v1/objects/0-wus-d1-abc/views/imgo" width="640" height="480"></p>"#;
        let blocks = from_html(html);
        assert_eq!(
            blocks[1],
            Block::Image { url: "https://us-api.asm.skype.com/v1/objects/0-wus-d1-abc/views/imgo".into(), size: Some((640, 480)) }
        );
    }

    #[test]
    fn decodes_references() {
        assert_eq!(decode("a &lt;b&gt; &#39;c&#x27; &unknown; & d"), "a <b> 'c' &unknown; & d");
    }

    #[test]
    fn lists_get_bullets() {
        assert_eq!(texts(&from_html("<ul><li>one</li><li>two</li></ul>")), [["• one\n• two"]]);
    }

    #[test]
    fn back_to_markdown() {
        let blocks = from_html(
            "<p>Hi <b>all</b>, see <a href=\"https://x.io\">this</a> and <code>run()</code></p><blockquote><p>old</p></blockquote>",
        );
        assert_eq!(to_markdown(&blocks), "Hi **all**, see [this](https://x.io) and `run()`\n\n> old");
    }

    #[test]
    fn tables_keep_rows_and_cells() {
        let html = "<p>Tickets:</p><table><tbody><tr><th>Prio</th><th>Ticket</th></tr><tr><td>1</td><td><s>#1413</s></td></tr><tr><td>2</td><td>#8771</td></tr></tbody></table><p>Done</p>";
        let blocks = from_html(html);
        assert_eq!(texts(&blocks), [vec!["Tickets:"], vec!["<table>"], vec!["Done"]]);
        let Block::Table { rows, header } = &blocks[1] else { panic!() };
        assert!(*header);
        let cell = |r: usize, c: usize| rows[r][c].iter().map(|i| i.text.as_str()).collect::<String>();
        assert_eq!((rows.len(), rows[1].len()), (3, 2));
        assert_eq!((cell(0, 1), cell(1, 1), cell(2, 0)), ("Ticket".into(), "#1413".into(), "2".into()));
        assert!(rows[1][1][0].span.strike && rows[0][0][0].span.bold);
    }

    #[test]
    fn quotes_and_code_blocks() {
        let html = "<blockquote><p>original</p></blockquote><p>answer</p><pre><code>fn main() {\n    run();\n}</code></pre>";
        let blocks = from_html(html);
        assert!(matches!(&blocks[0], Block::Quote(r, None) if r[0].text == "original"));
        assert!(matches!(&blocks[1], Block::Text(r) if r[0].text == "answer"));
        assert_eq!(blocks[2], Block::Code("fn main() {\n    run();\n}".into()));
    }
}
