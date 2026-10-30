//! The types a backend fills and the UI reads: conversations, teams and their
//! channels, messages and the people who wrote them.

pub mod text;
pub mod time;

pub use text::Inline;

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
}

/// The newest message of a conversation, for the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub author: String,
    pub text: String,
    /// Unix milliseconds.
    pub at: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub channels: Vec<Channel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    /// Also the id of the channel's conversation.
    pub id: ConversationId,
    pub name: String,
    pub general: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub id: String,
    /// The post a channel reply answers; None for posts and chat messages.
    pub root: Option<String>,
    pub author: Author,
    /// Unix milliseconds.
    pub at: i64,
    pub body: Vec<Inline>,
    /// A card a bot or connector posted (a pull request, an alert, …).
    pub card: Option<Card>,
    pub edited: bool,
    pub reactions: Vec<Reaction>,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaction {
    /// The reaction's name (`like`, `heart`, …) as the service sends it.
    pub key: String,
    pub count: usize,
    /// The signed-in user is among those who reacted.
    pub mine: bool,
}

impl Reaction {
    /// The emoji drawn for a reaction key; unknown keys show their name.
    pub fn emoji(&self) -> &str {
        match self.key.as_str() {
            "like" | "yes" => "👍",
            "heart" => "❤️",
            "laugh" => "😆",
            "surprised" => "😮",
            "sad" => "😢",
            "angry" => "😠",
            "eyes" => "👀",
            "pray" => "🙏",
            "fire" => "🔥",
            "clap" => "👏",
            "tada" => "🎉",
            "100" => "💯",
            key => key,
        }
    }
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

    /// Chats with people, newest first.
    pub fn chats(&self) -> impl Iterator<Item = &Conversation> {
        self.conversations.iter().filter(|c| c.kind == Kind::Chat)
    }

    /// The team and channel a conversation belongs to, if it's a channel.
    pub fn channel(&self, id: &str) -> Option<(&Team, &Channel)> {
        self.teams.iter().find_map(|t| t.channels.iter().find(|c| c.id == id).map(|c| (t, c)))
    }
}
