//! The emoji Teams has pictures of, by their key and their character: the picker
//! offers them, sent messages carry them as Teams emoji, and the same characters in
//! received text are shown as their pictures.

/// Key and character, in the picker's order.
pub const PICKER: [(&str, &str); 40] = [
    ("smile", "🙂"),
    ("laugh", "😄"),
    ("sweatgrinning", "😅"),
    ("cwl", "😂"),
    ("rofl", "🤣"),
    ("wink", "😉"),
    ("blush", "😊"),
    ("upsidedownface", "🙃"),
    ("cool", "😎"),
    ("inlove", "😍"),
    ("kiss", "😘"),
    ("tongueout", "😛"),
    ("giggle", "🤭"),
    ("hug", "🤗"),
    ("nerdy", "🤓"),
    ("surprised", "😮"),
    ("sad", "🙁"),
    ("cry", "😢"),
    ("sweat", "😓"),
    ("sleepy", "😪"),
    ("angry", "😠"),
    ("facepalm", "🤦"),
    ("yes", "👍"),
    ("ok", "👌"),
    ("clap", "👏"),
    ("praying", "🙏"),
    ("muscle", "💪"),
    ("1f44b_wavinghand", "👋"),
    ("1f440_eyes", "👀"),
    ("heart", "❤"),
    ("fire", "🔥"),
    ("star", "⭐"),
    ("2705_whiteheavycheckmark", "✅"),
    ("1f389_partypopper", "🎉"),
    ("party", "🥳"),
    ("cake", "🎂"),
    ("coffee", "☕"),
    ("beer", "🍺"),
    ("sun", "☀"),
    ("bug", "🐛"),
];

/// Text emoticons Teams turns into emoji as they're typed.
const EMOTICONS: [(&str, &str); 20] = [
    (":)", "🙂"),
    (":-)", "🙂"),
    ("(:", "🙂"),
    (":D", "😄"),
    (":-D", "😄"),
    ("xD", "😆"),
    ("XD", "😆"),
    (";)", "😉"),
    (";-)", "😉"),
    (":(", "🙁"),
    (":-(", "🙁"),
    (":'(", "😢"),
    (":P", "😛"),
    (":p", "😛"),
    (":-P", "😛"),
    (":O", "😮"),
    (":o", "😮"),
    (":|", "😐"),
    ("<3", "❤"),
    ("(y)", "👍"),
];

/// The emoji a word stands for, if it's an emoticon.
pub fn emoticon(word: &str) -> Option<&'static str> {
    EMOTICONS.iter().find(|(e, _)| *e == word).map(|(_, ch)| *ch)
}

/// `text` with its emoticons as emoji: whole words only (a link's `:/` or a time's
/// `8:30` stay), and nothing in `code`.
pub fn emoticons(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_code = false;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            out.push_str(line);
            continue;
        }
        let mut ticks = 0;
        let mut first = true;
        for word in line.split(' ') {
            if !first {
                out.push(' ');
            }
            first = false;
            match emoticon(word).filter(|_| !in_code && ticks % 2 == 0) {
                Some(ch) => out.push_str(ch),
                None => out.push_str(word),
            }
            ticks += word.matches('`').count();
        }
    }
    out
}

/// The key of an emoji character (a trailing variation selector is ignored): the
/// picker's own first, then Teams' catalog (emoji proper, not the letter-like
/// symbols it also has, such as © or ™).
pub fn key_of(ch: char) -> Option<&'static str> {
    if let Some((key, _)) = PICKER.iter().find(|(_, c)| c.starts_with(ch) && c.chars().count() == 1) {
        return Some(key);
    }
    if (ch as u32) < 0x2600 {
        return None;
    }
    catalog().by_char.get(&ch).map(|&i| catalog().emoji[i].key)
}

/// The character of an emoji by its key, from Teams' catalog.
pub fn char_of(key: &str) -> Option<&'static str> {
    by_key(key).map(|e| e.ch)
}

/// An emoji of Teams' catalog by its key.
pub fn by_key(key: &str) -> Option<&'static Emoji> {
    catalog().by_key.get(key).map(|&i| &catalog().emoji[i])
}

/// An emoji of Teams' catalog.
#[derive(Debug)]
pub struct Emoji {
    pub key: &'static str,
    /// The character(s).
    pub ch: &'static str,
    pub name: &'static str,
    /// Words it's found by, lowercase, space-separated.
    pub keywords: &'static str,
    /// Its category, an index into [`Catalog::categories`].
    pub category: usize,
    /// How often it's used, 1 the most.
    pub rank: u32,
}

/// Teams' emoji, by category, as `scripts/emoji-catalog.py` writes them.
pub struct Catalog {
    pub categories: Vec<&'static str>,
    /// In the catalog's order (by category); one per character.
    pub emoji: Vec<Emoji>,
    by_char: std::collections::HashMap<char, usize>,
    /// Every key the catalog has (characters listed twice, too).
    by_key: std::collections::HashMap<&'static str, usize>,
}

pub fn catalog() -> &'static Catalog {
    static CATALOG: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| parse_catalog(include_str!("../../assets/emoji/catalog.tsv")))
}

fn parse_catalog(text: &'static str) -> Catalog {
    let mut catalog = Catalog {
        categories: Vec::new(),
        emoji: Vec::new(),
        by_char: std::collections::HashMap::new(),
        by_key: std::collections::HashMap::new(),
    };
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some(category) = line.strip_prefix('#') {
            catalog.categories.push(category.split_once('\t').map_or("", |(_, title)| title));
            continue;
        }
        let mut fields = line.split('\t');
        let (Some(category), Some(key), Some(ch), Some(name), Some(keywords), Some(rank)) =
            (fields.next(), fields.next(), fields.next(), fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        // The catalog has some characters more than once (an animation of its own,
        // an old name): the plain emoji is the one keyed by its code point.
        let plain = ch.chars().next().is_some_and(|c| key.starts_with(&format!("{:x}_", c as u32)));
        catalog.by_key.insert(key, seen.get(ch).copied().unwrap_or(catalog.emoji.len()));
        if let Some(&at) = seen.get(ch) {
            if plain && !catalog.emoji[at].key.starts_with(&format!("{:x}_", ch.chars().next().map_or(0, |c| c as u32))) {
                catalog.emoji[at].key = key;
                catalog.emoji[at].name = name;
            }
            continue;
        }
        seen.insert(ch, catalog.emoji.len());
        let mut chars = ch.chars().filter(|c| *c != '\u{fe0f}');
        if let (Some(single), None) = (chars.next(), chars.next()) {
            catalog.by_char.insert(single, catalog.emoji.len());
        }
        catalog.emoji.push(Emoji { key, ch, name, keywords, category: category.parse().unwrap_or(0), rank: rank.parse().unwrap_or(9999) });
    }
    catalog
}

/// The emoji whose name or keywords match `query`, the best first: a name starting
/// with it, then a word of the name or a keyword starting with it, then anywhere;
/// the more used first within each.
pub fn search(query: &str) -> Vec<&'static Emoji> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut found: Vec<(u8, u32, &'static Emoji)> = catalog()
        .emoji
        .iter()
        .filter_map(|e| {
            let name = e.name.to_lowercase();
            let starts_word = |text: &str| text.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&query));
            let score = if name.starts_with(&query) {
                0
            } else if starts_word(&name) || starts_word(e.keywords) {
                1
            } else if name.contains(&query) || e.keywords.contains(&query) {
                2
            } else {
                return None;
            };
            Some((score, e.rank, e))
        })
        .collect();
    found.sort_by_key(|(score, rank, _)| (*score, *rank));
    found.into_iter().map(|(_, _, e)| e).collect()
}

/// The Teams markup of an emoji, as its composer sends it.
pub fn html(key: &str, ch: &str) -> String {
    format!(
        "<img itemscope=\"\" itemtype=\"http://schema.skype.com/Emoji\" itemid=\"{key}\" alt=\"{ch}\" \
         src=\"https://statics.teams.cdn.office.net/evergreen-assets/personal-expressions/v2/assets/emoticons/{key}/default/20_f.png\" \
         width=\"20\" height=\"20\" style=\"width:20px;height:20px;\">"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoticons_become_emoji() {
        assert_eq!(emoticons("ok :) see you <3"), "ok 🙂 see you ❤");
        assert_eq!(emoticons(":D"), "😄");
        assert_eq!(emoticons("at 8:30, https://x.io/a:)b"), "at 8:30, https://x.io/a:)b");
        assert_eq!(emoticons("`:)` and :)"), "`:)` and 🙂");
        assert_eq!(emoticons("```\n:)\n```\n:("), "```\n:)\n```\n🙁");
        assert_eq!(emoticon("(y)"), Some("👍"));
        // A list's "B)" or "8)" isn't one.
        assert_eq!(emoticons("option B) or 8) maybe"), "option B) or 8) maybe");
    }

    #[test]
    fn knows_its_characters() {
        assert_eq!(key_of('👍'), Some("yes"));
        assert_eq!(key_of('❤'), Some("heart"));
        assert_eq!(key_of('a'), None);
        assert!(html("smile", "🙂").contains("itemid=\"smile\""));
        // From the catalog, not the picker.
        assert_eq!(key_of('🍕'), Some("pizzaslice"));
        assert_eq!(key_of('😃'), Some("1f603_grinningfacewithbigeyes"));
        assert_eq!(key_of('©'), None);
    }

    #[test]
    fn reads_the_catalog() {
        let c = catalog();
        assert_eq!(c.categories.first().copied(), Some("Smilies"));
        assert!(c.emoji.len() > 1000);
        assert!(c.emoji.iter().all(|e| e.category < c.categories.len()));
    }

    #[test]
    fn searches_by_name_and_keyword() {
        assert_eq!(search("fox").first().map(|e| e.ch), Some("🦊"));
        assert!(search("pizza").iter().any(|e| e.ch == "🍕"));
        assert!(search("  ").is_empty());
        assert_eq!(char_of("pizzaslice"), Some("🍕"));
        assert_eq!(char_of("happyface"), Some("😃"));
    }
}
