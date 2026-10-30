//! What the user types, as the HTML Teams shows: Markdown (`**bold**`, `*italic*`,
//! `~~strike~~`, `` `code` ``, fenced code, lists, `>` quotes, links) and mentions
//! (`@Name` of someone picked from the list) become their Teams markup.

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
}

/// The HTML for `text`, with `@Name` turned into mentions of `people` it names.
pub fn to_html(text: &str, people: &[Mention]) -> Outgoing {
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
                        html.push_str(&format!("<li>{}</li>", inline(item, people, &mut out.mentions)));
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
                quoted.push(inline(q, people, &mut out.mentions));
                i += 1;
            }
            html.push_str(&format!("<blockquote><p>{}</p></blockquote>", quoted.join("<br>")));
            continue;
        }
        paragraph.push(inline(line, people, &mut out.mentions));
        i += 1;
    }
    flush(&mut paragraph, &mut html);
    out.html = html;
    out
}

/// A list item: whether it's numbered, and its text.
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
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// One line's inline markup: code, links, mentions, then emphasis on the plain text.
fn inline(line: &str, people: &[Mention], mentions: &mut Vec<Mention>) -> String {
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
