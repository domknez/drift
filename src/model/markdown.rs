//! What the user types, as the HTML Teams shows: Markdown (`**bold**`, `*italic*`,
//! `~~strike~~`, `` `code` ``, fenced code, lists, `>` quotes, links) and mentions
//! (`@Name` of someone picked from the list) become their Teams markup.

use super::CustomEmoji;

/// Someone mentioned in a message: the service's id and the name shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mention {
    pub id: String,
    pub name: String,
}

/// A message ready to send: its HTML and who it mentions, in the order of their
/// `itemid`s in the HTML.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outgoing {
    pub html: String,
    pub mentions: Vec<Mention>,
    /// Files going with it, already uploaded.
    pub files: Vec<super::Upload>,
}

/// The HTML for `text`, with `@Name` turned into mentions of `people` it names.
#[cfg(test)]
pub fn to_html(text: &str, people: &[Mention]) -> Outgoing {
    to_html_with(text, people, &[])
}

/// The HTML for `text`, with `@Name` turned into mentions of `people` it names and
/// `:name:` of the organisation's own emoji (`custom`) into those emoji.
pub fn to_html_with(text: &str, people: &[Mention], custom: &[CustomEmoji]) -> Outgoing {
    let mut out = Outgoing::default();
    let mut html = String::new();
    let lines: Vec<&str> = text.trim_matches('\n').lines().collect();
    let mut i = 0;
    let mut paragraph: Vec<String> = Vec::new();
    let flush = |paragraph: &mut Vec<String>, html: &mut String| {
        if !paragraph.is_empty() {
            html.push_str(&format!("<p>{}</p>", paragraph.join("<br>")));
            paragraph.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush(&mut paragraph, &mut html);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with("```") {
                code.push(escape(lines[i]));
                i += 1;
            }
            html.push_str(&format!("<pre><code>{}</code></pre>", code.join("\n")));
            i += 1;
            continue;
        }
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut html);
            i += 1;
            continue;
        }
        if let Some(kind) = list_item(trimmed).map(|(ordered, _)| ordered) {
            flush(&mut paragraph, &mut html);
            let tag = if kind { "ol" } else { "ul" };
            html.push_str(&format!("<{tag}>"));
            while i < lines.len() {
                match list_item(lines[i].trim_start()) {
                    Some((ordered, item)) if ordered == kind => {
                        html.push_str(&format!("<li>{}</li>", inline(item, people, custom, &mut out.mentions)));
                        i += 1;
                    }
                    _ => break,
                }
            }
            html.push_str(&format!("</{tag}>"));
            continue;
        }
        if trimmed.starts_with('>') {
            flush(&mut paragraph, &mut html);
            let mut quoted = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let q = lines[i].trim_start()[1..].trim_start();
                quoted.push(inline(q, people, custom, &mut out.mentions));
                i += 1;
            }
            html.push_str(&format!("<blockquote><p>{}</p></blockquote>", quoted.join("<br>")));
            continue;
        }
        paragraph.push(inline(line, people, custom, &mut out.mentions));
        i += 1;
    }
    flush(&mut paragraph, &mut html);
    out.html = html;
    out
}

/// A list item: whether it's numbered, and its text.
/// Text pasted from a page, with the links its HTML has written as `[text](url)`:
/// the clipboard's plain text loses them. Each link's text is found in `plain` after
/// the one before; one not found (or whose text is its address) stays as it is.
pub fn with_pasted_links(plain: &str, html: &str) -> String {
    let blocks = super::text::from_html(html);
    let mut out = String::with_capacity(plain.len());
    let mut rest = plain;
    let links = blocks.iter().flat_map(|b| match b {
        super::text::Block::Text(runs) | super::text::Block::Quote(runs, _) => runs.as_slice(),
        _ => &[],
    });
    for run in links {
        let Some(url) = run.span.link.as_deref().filter(|u| is_url(u)) else { continue };
        let label = run.text.trim();
        if label.is_empty() || label == url || label.contains(['[', ']']) {
            continue;
        }
        let Some(at) = rest.find(label) else { continue };
        out.push_str(&rest[..at]);
        // Parentheses in the address would end the link early.
        out.push_str(&format!("[{label}]({})", url.replace('(', "%28").replace(')', "%29")));
        rest = &rest[at + label.len()..];
    }
    out.push_str(rest);
    out
}

fn list_item(line: &str) -> Option<(bool, &str)> {
    if let Some(rest) = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).or_else(|| line.strip_prefix("• ")) {
        return Some((false, rest));
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some((true, &line[digits + 2..]));
    }
    None
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// One line's inline markup: code, links, mentions, then emphasis on the plain text.
fn inline(line: &str, people: &[Mention], custom: &[CustomEmoji], mentions: &mut Vec<Mention>) -> String {
    let mut out = String::new();
    let mut text = String::new();
    let mut rest = line;
    let flush = |text: &mut String, out: &mut String| {
        out.push_str(&emphasis(&escape(text)));
        text.clear();
    };
    while let Some(c) = rest.chars().next() {
        // `code`
        if c == '`'
            && let Some(end) = rest[1..].find('`')
        {
            flush(&mut text, &mut out);
            out.push_str(&format!("<code>{}</code>", escape(&rest[1..1 + end])));
            rest = &rest[end + 2..];
            continue;
        }
        // [text](url)
        if c == '['
            && let Some(close) = rest.find("](")
            && let Some(end) = rest[close + 2..].find(')')
        {
            let (label, url) = (&rest[1..close], &rest[close + 2..close + 2 + end]);
            if !label.is_empty() && is_url(url) {
                flush(&mut text, &mut out);
                out.push_str(&format!("<a href=\"{}\">{}</a>", escape(url), escape(label)));
                rest = &rest[close + 3 + end..];
                continue;
            }
        }
        // A bare link.
        if (rest.starts_with("http://") || rest.starts_with("https://")) && text.chars().last().is_none_or(char::is_whitespace) {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let url = rest[..end].trim_end_matches(['.', ',', ')', '!', '?']);
            flush(&mut text, &mut out);
            out.push_str(&format!("<a href=\"{0}\">{0}</a>", escape(url)));
            rest = &rest[url.len()..];
            continue;
        }
        // @Name of someone picked.
        if c == '@'
            && let Some(person) = people.iter().filter(|p| rest[1..].starts_with(p.name.as_str())).max_by_key(|p| p.name.len())
        {
            flush(&mut text, &mut out);
            let index = mentions.iter().position(|m| m.id == person.id && m.name == person.name).unwrap_or_else(|| {
                mentions.push(person.clone());
                mentions.len() - 1
            });
            out.push_str(&format!(
                "<span itemscope=\"\" itemtype=\"http://schema.skype.com/Mention\" itemid=\"{index}\">{}</span>",
                escape(&person.name)
            ));
            rest = &rest[1 + person.name.len()..];
            continue;
        }
        // :name: of an organisation's emoji.
        if c == ':'
            && let Some(e) = custom
                .iter()
                .filter(|e| rest[1..].strip_prefix(e.name.as_str()).is_some_and(|after| after.starts_with(':')))
                .max_by_key(|e| e.name.len())
        {
            flush(&mut text, &mut out);
            out.push_str(&format!(
                "<span><img alt=\"{0}\" itemid=\"{1}\" itemtype=\"http://schema.skype.com/CustomEmoji\" src=\"{2}\" style=\"width:20px; height:20px\"></span>",
                escape(&e.name).replace('"', "&quot;"),
                escape(&e.key).replace('"', "&quot;"),
                escape(&e.src).replace('"', "&quot;"),
            ));
            rest = &rest[e.name.len() + 2..];
            continue;
        }
        // An emoji Teams has a picture of goes as its Teams emoji.
        if let Some(key) = super::emoji::key_of(c) {
            flush(&mut text, &mut out);
            out.push_str(&super::emoji::html(key, &c.to_string()));
            rest = rest[c.len_utf8()..].strip_prefix('\u{fe0f}').unwrap_or(&rest[c.len_utf8()..]);
            continue;
        }
        text.push(c);
        rest = &rest[c.len_utf8()..];
    }
    flush(&mut text, &mut out);
    out
}

fn is_url(s: &str) -> bool {
    (s.starts_with("http://") || s.starts_with("https://") || s.starts_with("mailto:")) && !s.contains(char::is_whitespace)
}

/// `**bold**`, `__bold__`, `~~strike~~`, `*italic*`, `_italic_` in escaped text.
fn emphasis(text: &str) -> String {
    let mut s = text.to_string();
    for (marker, tag) in [("**", "b"), ("__", "b"), ("~~", "s"), ("*", "i"), ("_", "i")] {
        s = pair(&s, marker, tag);
    }
    s
}

/// Wrap text between pairs of `marker` in `<tag>`. A marker opens only before a
/// non-space and closes only after one; `_` also needs a word boundary outside, so
/// snake_case stays as it is.
fn pair(s: &str, marker: &str, tag: &str) -> String {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find(marker) {
        let before = rest[..open].chars().last();
        let inner_start = open + marker.len();
        let opens = rest[inner_start..].chars().next().is_some_and(|c| !c.is_whitespace()) && !(marker == "_" && word(before));
        let close = opens
            .then(|| {
                let mut from = inner_start;
                while let Some(at) = rest[from..].find(marker).map(|a| a + from) {
                    let last = rest[..at].chars().last();
                    let after = rest[at + marker.len()..].chars().next();
                    // Not across a pair an earlier marker made (`**a *b** c*`): the tags would cross.
                    if at > inner_start
                        && last.is_some_and(|c| !c.is_whitespace())
                        && !(marker == "_" && word(after))
                        && balanced(&rest[inner_start..at])
                    {
                        return Some(at);
                    }
                    from = at + marker.len();
                }
                None
            })
            .flatten();
        match close {
            Some(close) => {
                out.push_str(&rest[..open]);
                out.push_str(&format!("<{tag}>{}</{tag}>", &rest[inner_start..close]));
                rest = &rest[close + marker.len()..];
            }
            None => {
                out.push_str(&rest[..inner_start]);
                rest = &rest[inner_start..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether every tag an earlier pass made in `s` is closed in it too (the text
/// itself is escaped, so any `<b>` is one of them).
fn balanced(s: &str) -> bool {
    ["b", "s", "i"].iter().all(|t| s.matches(&format!("<{t}>")).count() == s.matches(&format!("</{t}>")).count())
}

/// How a piece of typed text looks while it's typed: its Markdown shown as it will be
/// sent, the marks themselves faint.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Look {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    /// A Markdown mark (`**`, `` ` ``, `>`…).
    pub mark: bool,
    pub quote: bool,
}

/// The look of each byte of `text`, by the same rules [`to_html`] sends it with.
pub fn looks(text: &str) -> Vec<Look> {
    let mut out = vec![Look::default(); text.len()];
    let mut fenced = false;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let start = at;
        at += line.len();
        let trimmed = body.trim_start();
        let indent = body.len() - trimmed.len();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            out[start..start + body.len()].iter_mut().for_each(|l| *l = Look { mark: true, code: true, ..Look::default() });
            continue;
        }
        if fenced {
            out[start..start + body.len()].iter_mut().for_each(|l| l.code = true);
            continue;
        }
        let mut from = start + indent;
        if trimmed.starts_with('>') {
            out[from].mark = true;
            out[from + 1..start + body.len()].iter_mut().for_each(|l| l.quote = true);
            from += 1;
        } else if let Some((_, item)) = list_item(trimmed) {
            let mark = trimmed.len() - item.len();
            out[from..from + mark].iter_mut().for_each(|l| l.mark = true);
            from += mark;
        }
        inline_looks(&text[from..start + body.len()], &mut out[from..start + body.len()]);
    }
    out
}

/// Code spans and emphasis in one line.
fn inline_looks(line: &str, out: &mut [Look]) {
    // Code first: what's in it isn't emphasis. Matched marks become \x01 in `work`,
    // so they aren't matched again.
    let mut work: Vec<u8> = line.as_bytes().to_vec();
    let mut i = 0;
    while let Some(open) = line[i..].find('`').map(|o| o + i) {
        let Some(close) = line[open + 1..].find('`').map(|c| c + open + 1) else { break };
        out[open].mark = true;
        out[close].mark = true;
        for (j, l) in out.iter_mut().enumerate().take(close).skip(open + 1) {
            l.code = true;
            work[j] = 1;
        }
        work[open] = 1;
        work[close] = 1;
        i = close + 1;
    }
    for (marker, set) in [("**", 0), ("__", 0), ("~~", 1), ("*", 2), ("_", 2)] {
        let text = String::from_utf8_lossy(&work).into_owned();
        for (open, close) in pairs(&text, marker) {
            for j in open..open + marker.len() {
                out[j].mark = true;
                work[j] = 1;
            }
            for j in close..close + marker.len() {
                out[j].mark = true;
                work[j] = 1;
            }
            for l in &mut out[open + marker.len()..close] {
                match set {
                    0 => l.bold = true,
                    1 => l.strike = true,
                    _ => l.italic = true,
                }
            }
        }
    }
}

/// Where pairs of `marker` open and close in `s` (byte offsets of each mark), by
/// the rules of [`pair`].
fn pairs(s: &str, marker: &str) -> Vec<(usize, usize)> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
    let mut found = Vec::new();
    let mut base = 0;
    let mut rest = s;
    while let Some(open) = rest.find(marker) {
        let before = rest[..open].chars().last();
        let inner_start = open + marker.len();
        let opens =
            rest[inner_start..].chars().next().is_some_and(|c| !c.is_whitespace() && c != '\x01') && !(marker == "_" && word(before));
        let close = opens
            .then(|| {
                let mut from = inner_start;
                while let Some(at) = rest[from..].find(marker).map(|a| a + from) {
                    let last = rest[..at].chars().last();
                    let after = rest[at + marker.len()..].chars().next();
                    if at > inner_start && last.is_some_and(|c| !c.is_whitespace()) && !(marker == "_" && word(after)) {
                        return Some(at);
                    }
                    from = at + marker.len();
                }
                None
            })
            .flatten();
        match close {
            Some(close) => {
                found.push((base + open, base + close));
                base += close + marker.len();
                rest = &rest[close + marker.len()..];
            }
            None => {
                base += inner_start;
                rest = &rest[inner_start..];
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(text: &str) -> String {
        to_html(text, &[]).html
    }

    #[test]
    fn plain_text_becomes_paragraphs() {
        assert_eq!(html("hello"), "<p>hello</p>");
        assert_eq!(html("one\ntwo\n\nthree"), "<p>one<br>two</p><p>three</p>");
        assert_eq!(html("a < b & c"), "<p>a &lt; b &amp; c</p>");
    }

    #[test]
    fn emphasis_and_code() {
        assert_eq!(html("**bold** and *it* and ~~gone~~"), "<p><b>bold</b> and <i>it</i> and <s>gone</s></p>");
        assert_eq!(html("keep snake_case_names and 2 * 3 * 4"), "<p>keep snake_case_names and 2 * 3 * 4</p>");
        assert_eq!(html("_soft_ and `**not bold**`"), "<p><i>soft</i> and <code>**not bold**</code></p>");
    }

    #[test]
    fn any_text_survives() {
        // Markdown's own characters, HTML's, and a few wide ones, in random order.
        let alphabet: Vec<char> = "*_~`[]()>#-.:@!<&\"' \nab1é😀".chars().collect();
        let mut rng = fastrand::Rng::with_seed(7);
        for _ in 0..3000 {
            let text: String = (0..rng.usize(0..40)).map(|_| alphabet[rng.usize(..alphabet.len())]).collect();
            let out = html(&text);
            assert_eq!(looks(&text).len(), text.len(), "{text:?}");
            assert!(balanced(&out), "{text:?} → {out}");
            let _ = crate::model::text::from_html(&out);
            let _ = crate::model::text::from_html(&text);
        }
    }

    #[test]
    fn emphasis_never_crosses() {
        assert_eq!(html("**a *b** c*"), "<p><b>a *b</b> c*</p>");
        assert_eq!(html("*a **b** c*"), "<p><i>a <b>b</b> c</i></p>");
    }

    #[test]
    fn quotes_cant_add_attributes() {
        assert_eq!(html("[x](https://a/\"onmouseover=\"x)"), "<p><a href=\"https://a/&quot;onmouseover=&quot;x\">x</a></p>");
    }

    #[test]
    fn pasted_links_come_back() {
        let html = r#"<meta charset="utf-8"><a href="https://dev.azure.com/org/Proj%20X/_git/ui/pullrequest/10552">Pull Request 10552</a><span>: feat(trace): toggle (AB#6908)</span>"#;
        assert_eq!(
            with_pasted_links("Pull Request 10552: feat(trace): toggle (AB#6908)", html),
            "[Pull Request 10552](https://dev.azure.com/org/Proj%20X/_git/ui/pullrequest/10552): feat(trace): toggle (AB#6908)"
        );
        assert_eq!(with_pasted_links("just text", "<p>just text</p>"), "just text");
    }

    #[test]
    fn links() {
        assert_eq!(
            html("see https://example.com/a?b=1&c=2."),
            "<p>see <a href=\"https://example.com/a?b=1&amp;c=2\">https://example.com/a?b=1&amp;c=2</a>.</p>"
        );
        assert_eq!(html("[the PR](https://example.com/pr/1)"), "<p><a href=\"https://example.com/pr/1\">the PR</a></p>");
    }

    #[test]
    fn blocks() {
        assert_eq!(html("- one\n- two\n\n1. first\n2. second"), "<ul><li>one</li><li>two</li></ul><ol><li>first</li><li>second</li></ol>");
        assert_eq!(html("> quoted\nanswer"), "<blockquote><p>quoted</p></blockquote><p>answer</p>");
        assert_eq!(html("```\nfn a() {}\n  <x>\n```"), "<pre><code>fn a() {}\n  &lt;x&gt;</code></pre>");
    }

    #[test]
    fn looks_while_typed() {
        let text = "**hi** `x*y*` ~~no~~ and *it*";
        let l = looks(text);
        let at = |s: &str| text.find(s).unwrap();
        assert!(l[0].mark && l[1].mark && l[at("hi")].bold && !l[at("hi")].mark);
        assert!(l[at("x*")].code && !l[at("*y") + 1].italic);
        assert!(l[at("no")].strike && l[at("it")].italic);
        assert!(!l[at("and")].bold && !l[at("and")].mark);
        let quoted = looks("> said\n- item");
        assert!(quoted[0].mark && quoted[2].quote && quoted[7].mark && !quoted[9].mark);
        let fenced = looks("```\nfn a()\n```\nafter");
        assert!(fenced[0].mark && fenced[4].code && !fenced[15].code);
    }

    #[test]
    fn custom_emoji_go_by_name() {
        let wave =
            CustomEmoji { key: "wave;0-weu-d1-abc".into(), name: "wave".into(), keywords: vec![], src: "https://ams.example/x".into() };
        let out = to_html_with("hi :wave: and :nope:", &[], &[wave]);
        assert!(
            out.html
                .starts_with("<p>hi <span><img alt=\"wave\" itemid=\"wave;0-weu-d1-abc\" itemtype=\"http://schema.skype.com/CustomEmoji\"")
        );
        assert!(out.html.ends_with("</span> and :nope:</p>"));
    }

    #[test]
    fn emoji_go_as_teams_emoji() {
        let out = html("nice 👍");
        assert!(out.starts_with("<p>nice <img itemscope=\"\" itemtype=\"http://schema.skype.com/Emoji\" itemid=\"yes\" alt=\"👍\""));
        assert!(html("❤️ it").contains("itemid=\"heart\""));
        assert!(!html("❤️ it").contains('\u{fe0f}'));
    }

    #[test]
    fn mentions_of_people_picked() {
        let people =
            [Mention { id: "8:orgid:m".into(), name: "Maya Lindqvist".into() }, Mention { id: "8:orgid:t".into(), name: "Tomás".into() }];
        let out = to_html("@Maya Lindqvist and @Tomás, and @nobody", &people);
        assert_eq!(
            out.html,
            "<p><span itemscope=\"\" itemtype=\"http://schema.skype.com/Mention\" itemid=\"0\">Maya Lindqvist</span> and \
             <span itemscope=\"\" itemtype=\"http://schema.skype.com/Mention\" itemid=\"1\">Tomás</span>, and @nobody</p>"
        );
        assert_eq!(out.mentions, people);
    }
}
