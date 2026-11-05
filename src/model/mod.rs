//! The types a backend fills and the UI reads: conversations, teams and their
//! channels, messages and the people who wrote them.

pub mod emoji;
pub mod markdown;
pub mod text;
pub mod time;

pub use markdown::Mention;
pub use text::{Block, Inline};

/// A conversation's id as the service knows it (`19:…@thread.v2`, `48:mentions`, …).
pub type ConversationId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One to one, or a group chat.
    Chat,
    /// The chat of a meeting.
    Meeting,
    /// A channel of a team.
    Channel,
    /// One of the service's own feeds (mentions, saved, notifications, …).
    Feed,
}

/// A chat, meeting chat or channel, as listed in the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub id: ConversationId,
    pub kind: Kind,
    /// The topic, or for chats without one, the other members' names (filled later).
    pub title: String,
    pub last: Option<Preview>,
    /// Something arrived after what was last read.
    pub unread: bool,
    /// The id of the last message read (message ids are times in ms), if known.
    pub read_up_to: Option<i64>,
    /// Pinned to the top of the list (a favourite in Teams).
    pub pinned: bool,
    pub muted: bool,
    /// The meeting's join link, for meeting chats.
    pub join_url: Option<String>,
    pub picture: Option<Picture>,
    /// Who's in a chat (ids and names; the user left out). Empty for channels.
    pub members: Vec<Mention>,
}

/// A picture the service has: a person's photo, a team's, or a group chat's made
/// of its members'.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    Person {
        id: String,
        name: String,
    },
    Team {
        group: String,
        etag: String,
        name: String,
    },
    /// Members' ids and names, a few of them.
    Group(Vec<(String, String)>),
    /// An emoji by the service's key for it (`like`, `1f440_eyes`, or a tenant's own
    /// `<tenant>;<object id>`).
    Emoji(String),
    /// An image in a message, by its address.
    Image(String),
}

impl Picture {
    /// The same for the same picture: the cache's key.
    pub fn key(&self) -> String {
        match self {
            Picture::Person { id, .. } => format!("person:{id}"),
            Picture::Team { group, etag, .. } => format!("team:{group}:{etag}"),
            Picture::Group(members) => format!("group:{}", members.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>().join(",")),
            Picture::Emoji(key) => format!("emoji:{key}"),
            Picture::Image(url) => format!("image:{url}"),
        }
    }
}

/// The newest message of a conversation, for the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub author: String,
    /// Who wrote it, by id (empty if the service didn't say).
    pub author_id: String,
    pub text: String,
    /// Unix milliseconds.
    pub at: i64,
    /// A reply in a channel's thread, not a post.
    pub reply: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub picture: Option<Picture>,
    pub channels: Vec<Channel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// Also the id of the channel's conversation.
    pub id: ConversationId,
    pub name: String,
    pub general: bool,
    pub unread: bool,
    /// A favourite: listed first.
    pub pinned: bool,
    pub muted: bool,
    pub last: Option<Preview>,
    /// Followed in Teams: what in it notifies.
    pub follow: Follow,
}

/// What in a channel notifies, as the user set it in Teams (Channel notifications →
/// All activity).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Follow {
    #[default]
    Nothing,
    /// Every new post.
    Posts,
    /// Every new post, and every reply.
    Everything,
}

impl Follow {
    /// Whether the newest message in the channel is one that notifies.
    pub fn covers(self, last: &Preview) -> bool {
        match self {
            Follow::Nothing => false,
            Follow::Posts => !last.reply,
            Follow::Everything => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    /// The post a channel reply answers; None for posts and chat messages.
    pub root: Option<String>,
    pub author: Author,
    /// Unix milliseconds.
    pub at: i64,
    pub body: Vec<Block>,
    /// A card a bot or connector posted (a pull request, an alert, …).
    pub card: Option<Card>,
    /// Files shared with it (kept in SharePoint or OneDrive).
    pub files: Vec<File>,
    pub edited: bool,
    pub reactions: Vec<Reaction>,
    /// For items of the activity feed: where the mention or reply happened.
    pub source: Option<Source>,
    /// Written here and on its way: shown at once, replaced by what Teams has once it's there.
    pub sending: bool,
}

/// Where an activity happened: the conversation, its name, and what it was.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub conversation: ConversationId,
    /// The message itself, and in a channel the post whose thread it's in.
    pub message: Option<String>,
    pub root: Option<String>,
    pub title: String,
    /// `mention`, `reply`, `reaction`, … as the service names it.
    pub what: String,
    /// More about it: for a reaction, its emoji's key (`laugh`, `heart`, …).
    pub detail: String,
}

impl Source {
    /// What the person did, after their name ("mentioned you"); empty when it's
    /// not one of those.
    pub fn verb(&self) -> &'static str {
        match self.what.as_str() {
            "mention" | "mentionInChat" => "mentioned you",
            "reply" | "replyToReply" => "replied",
            "reaction" | "reactionInChat" => "reacted",
            "teamMembershipChange" => "added you",
            _ => "",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Author {
    /// The service's id for the person (`8:orgid:…`), or a bot's.
    pub id: String,
    pub name: String,
}

/// A card reduced to what the UI shows: a title, a line under it and where it leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Card {
    pub title: String,
    pub detail: String,
    pub link: Option<String>,
    /// Labelled values ("Target branch" → "develop"), as the card lists them.
    pub facts: Vec<(String, String)>,
}

/// A file put where Teams keeps a conversation's files (the sender's OneDrive for
/// a chat, the team's SharePoint for a channel), to go with a message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Upload {
    pub name: String,
    /// The file's page.
    pub url: String,
    /// The site it's in (ending in `/`), and SharePoint's ids for it.
    pub site: String,
    pub site_id: String,
    pub item_id: String,
    /// The chat's members' access to it (none for a channel's, the team has it).
    pub share_url: String,
    pub share_id: String,
}

/// A file shared in a message: its name, and its page in SharePoint or OneDrive
/// (opened in the browser, which signs in and offers the download).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub url: String,
}

impl File {
    /// What kind of file it is, from its name (`PDF`, `DOCX`, …).
    pub fn kind(&self) -> String {
        self.name.rsplit_once('.').map(|(_, ext)| ext.to_uppercase()).filter(|e| e.len() <= 5).unwrap_or_else(|| "FILE".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaction {
    /// The reaction's name (`like`, `heart`, …) as the service sends it.
    pub key: String,
    pub count: usize,
    /// The signed-in user is among those who reacted.
    pub mine: bool,
    /// Who reacted (their ids), as far as the service says.
    pub who: Vec<String>,
}

impl Reaction {
    /// The emoji drawn for a reaction key. Keys are names (`like`), or start with
    /// the emoji's code points (`1f440_eyes`); a `-tone<n>` suffix picks a skin tone.
    pub fn emoji(&self) -> String {
        // A tenant's own emoji has only a picture.
        if self.key.contains(';') {
            return "✦".into();
        }
        let (name, tone) = match self.key.rsplit_once("-tone") {
            Some((name, t)) => (name, t.parse::<u32>().ok().filter(|t| (1..=5).contains(t))),
            None => (self.key.as_str(), None),
        };
        let base = match name {
            "like" | "yes" | "thumbsup" | "plus1" => "👍".to_string(),
            "heart" | "love" | "heartbeat" => "❤".into(),
            "laugh" | "laughing" | "haha" => "😆".into(),
            "cwl" => "😂".into(),
            "surprised" | "wow" | "openmouth" => "😮".into(),
            "sad" | "cry" | "crying" => "😢".into(),
            "angry" | "rage" => "😠".into(),
            "eyes" | "looking" => "👀".into(),
            "pray" | "praying" | "prayer" => "🙏".into(),
            "fire" => "🔥".into(),
            "clap" => "👏".into(),
            "tada" => "🎉".into(),
            "100" => "💯".into(),
            "smile" => "🙂".into(),
            "wink" => "😉".into(),
            "rofl" => "🤣".into(),
            "thinking" => "🤔".into(),
            "nod" | "ok" => "👌".into(),
            name => code_points(name).or_else(|| emoji::char_of(name).map(str::to_string)).unwrap_or_else(|| name.to_string()),
        };
        // Skin tones: U+1F3FB (light) to U+1F3FF (dark).
        match tone.and_then(|t| char::from_u32(0x1F3FA + t)) {
            Some(modifier) => format!("{base}{modifier}"),
            None => base,
        }
    }
}

/// `1f440_eyes` → 👀; `2714_heavycheckmark` → ✔; `1f468-200d-1f4bb_…` → 👨‍💻.
fn code_points(key: &str) -> Option<String> {
    let hex = key.split('_').next()?;
    hex.split('-').map(|h| u32::from_str_radix(h, 16).ok().filter(|c| *c >= 0x20).and_then(char::from_u32)).collect()
}

/// A calendar event (one occurrence of a series).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meeting {
    pub id: String,
    pub subject: String,
    /// Unix milliseconds.
    pub start: i64,
    pub end: i64,
    pub organizer: String,
    pub location: String,
    /// The Teams link to join it, for online meetings.
    pub join_url: Option<String>,
    /// The meeting's chat, if it has one.
    pub chat: Option<ConversationId>,
    pub cancelled: bool,
    /// Accepted, tentative, …, as the service says.
    pub response: String,
}

/// A message found by search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub conversation: ConversationId,
    pub message: String,
    pub author: String,
    /// Unix milliseconds.
    pub at: i64,
    pub text: String,
    /// Where it was written: a team's channel, or a chat's name.
    pub place: String,
}

/// How available someone shows as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    Busy,
    DoNotDisturb,
    BeRightBack,
    Away,
    Offline,
    /// The service doesn't know (or didn't say).
    Unknown,
}

impl Availability {
    /// What the user can pick, in the order Teams lists them.
    pub const PICKS: [Availability; 6] = [
        Availability::Available,
        Availability::Busy,
        Availability::DoNotDisturb,
        Availability::BeRightBack,
        Availability::Away,
        Availability::Offline,
    ];

    /// The service's name for it.
    pub fn key(self) -> &'static str {
        match self {
            Availability::Available => "Available",
            Availability::Busy => "Busy",
            Availability::DoNotDisturb => "DoNotDisturb",
            Availability::BeRightBack => "BeRightBack",
            Availability::Away => "Away",
            Availability::Offline => "Offline",
            Availability::Unknown => "PresenceUnknown",
        }
    }

    pub fn from_key(key: &str) -> Availability {
        match key {
            "Available" | "AvailableIdle" => Availability::Available,
            "Busy" | "BusyIdle" => Availability::Busy,
            "DoNotDisturb" => Availability::DoNotDisturb,
            "BeRightBack" => Availability::BeRightBack,
            "Away" => Availability::Away,
            "Offline" => Availability::Offline,
            _ => Availability::Unknown,
        }
    }

    /// How it reads as a status.
    pub fn label(self) -> &'static str {
        match self {
            Availability::Available => "Available",
            Availability::Busy => "Busy",
            Availability::DoNotDisturb => "Do not disturb",
            Availability::BeRightBack => "Be right back",
            Availability::Away => "Away",
            Availability::Offline => "Offline",
            Availability::Unknown => "Status unknown",
        }
    }

    /// How it reads in the menu that sets it.
    pub fn pick_label(self) -> &'static str {
        match self {
            Availability::Away => "Appear away",
            Availability::Offline => "Appear offline",
            other => other.label(),
        }
    }
}

/// The signed-in user's status: what others see, and what the user set (if they
/// did; otherwise Teams goes by activity).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub shown: Availability,
    pub set: Option<Availability>,
}

/// How someone else shows: their availability, and whether they're out of office.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presence {
    pub availability: Availability,
    pub out_of_office: bool,
}

impl Presence {
    /// How it reads (on hover, and to screen readers).
    pub fn label(self) -> &'static str {
        if self.out_of_office { "Out of office" } else { self.availability.label() }
    }
}

/// An emoji of the user's organisation (a tenant's own, added in Teams).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomEmoji {
    /// The service's key: `<shortcut>;<object id>` (in reactions and messages).
    pub key: String,
    /// Its shortcut (`mbai` for `:mbai:`).
    pub name: String,
    /// Words it's found by, lowercase.
    pub keywords: Vec<String>,
    /// Its picture's address, for the message markup.
    pub src: String,
}

/// Everything listed in the sidebar.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Directory {
    pub teams: Vec<Team>,
    pub conversations: Vec<Conversation>,
}

impl Directory {
    pub fn conversation(&self, id: &str) -> Option<&Conversation> {
        self.conversations.iter().find(|c| c.id == id)
    }

    /// The feeds Activity is made of: mentions, and the notifications feed (reactions,
    /// replies and the rest); any feed the service lists if it names neither.
    pub fn activity_feeds(&self) -> Vec<&Conversation> {
        let feeds = || self.conversations.iter().filter(|c| c.kind == Kind::Feed);
        let named: Vec<&Conversation> = feeds().filter(|c| c.id.contains("mention") || c.id.contains("notification")).collect();
        if named.is_empty() { feeds().take(1).collect() } else { named }
    }

    /// Whether a conversation is one of Activity's feeds.
    pub fn is_activity_feed(&self, id: &str) -> bool {
        self.activity_feeds().iter().any(|c| c.id == id)
    }

    /// What in a channel notifies (nothing for anything else).
    pub fn follow(&self, id: &str) -> Follow {
        self.channel(id).map_or(Follow::Nothing, |(_, c)| c.follow)
    }

    /// The team and channel a conversation belongs to, if it's a channel.
    pub fn channel(&self, id: &str) -> Option<(&Team, &Channel)> {
        self.teams.iter().find_map(|t| t.channels.iter().find(|c| c.id == id).map(|c| (t, c)))
    }
}

#[cfg(test)]
mod tests {
    use super::Reaction;

    fn emoji(key: &str) -> String {
        Reaction { key: key.into(), count: 1, mine: false, who: Vec::new() }.emoji()
    }

    #[test]
    fn draws_reaction_keys_as_emoji() {
        assert_eq!(emoji("like"), "👍");
        assert_eq!(emoji("1f440_eyes"), "👀");
        assert_eq!(emoji("2714_heavycheckmark"), "✔");
        assert_eq!(emoji("yes-tone1"), "👍🏻");
        assert_eq!(emoji("cwl"), "😂");
        assert_eq!(emoji("somethingnew"), "somethingnew");
    }
}
