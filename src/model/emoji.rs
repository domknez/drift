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

/// The key of an emoji character (a trailing variation selector is ignored).
pub fn key_of(ch: char) -> Option<&'static str> {
    PICKER.iter().find(|(_, c)| c.chars().next() == Some(ch) && c.chars().count() == 1).map(|(k, _)| *k)
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
    fn knows_its_characters() {
        assert_eq!(key_of('👍'), Some("yes"));
        assert_eq!(key_of('❤'), Some("heart"));
        assert_eq!(key_of('a'), None);
        assert!(html("smile", "🙂").contains("itemid=\"smile\""));
    }
}
