//! What the UI asks of a chat service. Teams is the one there is (`crate::teams`);
//! the UI only sees this trait and the `model` types.

use crate::model::markdown::Outgoing;
use crate::model::{Availability, ConversationId, CustomEmoji, Directory, Meeting, Message, Picture, Presence, SearchHit, Status};
use std::fmt;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The stored sign-in no longer works: the user has to sign in again.
    SignedOut,
    /// The service couldn't be reached.
    Network(String),
    /// The service answered with an error.
    Service(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::SignedOut => f.write_str("Signed out, please sign in again"),
            Error::Network(e) => write!(f, "Can't reach Teams: {e}"),
            Error::Service(e) => write!(f, "Teams answered with an error: {e}"),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// A change the service pushes as it happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Push {
    /// Something new or changed in a conversation.
    Conversation(ConversationId),
    /// Pushes may have been missed: fetch everything again.
    Everything,
    /// Whether pushes are coming (the socket is up).
    Live(bool),
}

/// A signed-in chat service. Calls block; the UI makes them on background threads.
pub trait Backend: Send + Sync {
    /// The signed-in user's name.
    fn me(&self) -> String;

    /// The signed-in user's id, as messages name their authors.
    fn me_id(&self) -> String;

    /// Teams with their channels, and the chats, newest first.
    fn directory(&self) -> Result<Directory>;

    /// The newest messages of a conversation, oldest first.
    fn messages(&self, conversation: &str) -> Result<Vec<Message>>;

    /// A picture's image file (JPEG or PNG).
    fn picture(&self, picture: &Picture) -> Result<Vec<u8>>;

    /// Send a message to a conversation. For a channel reply, `conversation` is
    /// `<channel id>;messageid=<post id>`; for a new post, the channel id.
    fn send(&self, conversation: &str, message: &Outgoing) -> Result<()>;

    /// Upload a picture for a message in `conversation`: the address to put in it.
    fn upload_image(&self, conversation: &str, name: &str, bytes: &[u8], mime: &str) -> Result<String>;

    /// Replace the text of one of the user's messages.
    fn edit(&self, conversation: &str, message: &str, with: &Outgoing) -> Result<()>;

    /// Delete one of the user's messages.
    fn delete(&self, conversation: &str, message: &str) -> Result<()>;

    /// Add (or with `add` false, take back) the user's reaction `key` to a message.
    fn react(&self, conversation: &str, message: &str, key: &str, add: bool) -> Result<()>;

    /// Tell the service the conversation is read up to `message` (Teams shows it read too).
    fn mark_read(&self, conversation: &str, message: &str) -> Result<()>;

    /// Up to which message each member of a chat has read: their id, and the id of
    /// the last message they've seen (message ids are times in ms).
    fn read_by(&self, conversation: &str) -> Result<Vec<(String, i64)>>;

    /// The user's meetings between two times (Unix ms), earliest first.
    fn calendar(&self, from: i64, to: i64) -> Result<Vec<Meeting>>;

    /// Messages matching `query` in all the user's chats and channels, best first.
    fn search(&self, query: &str) -> Result<Vec<SearchHit>>;

    /// Pin a conversation to the top of the list, or unpin it.
    fn pin(&self, conversation: &str, pinned: bool) -> Result<()>;

    /// The user's status.
    fn status(&self) -> Result<Status>;

    /// Set the user's status, everywhere they're signed in; `None` lets the service
    /// go by activity again.
    fn set_status(&self, to: Option<Availability>) -> Result<()>;

    /// How each of these people shows (by id). A service without presence says nothing.
    fn presence(&self, people: &[String]) -> Result<Vec<(String, Presence)>> {
        let _ = people;
        Ok(Vec::new())
    }

    /// The organisation's own emoji. A service without them has none.
    fn custom_emoji(&self) -> Result<Vec<CustomEmoji>> {
        Ok(Vec::new())
    }

    /// Tell the service drift is running, and whether the user is at the computer:
    /// this is what makes them show as available (or away when idle). A service
    /// without presence does nothing.
    fn report_activity(&self, active: bool) -> Result<()> {
        let _ = active;
        Ok(())
    }

    /// Have changes pushed to `to` as they happen, for as long as the backend lives.
    /// A service that can't push does nothing; the UI polls either way.
    fn listen(self: Arc<Self>, to: Box<dyn Fn(Push) + Send>) {
        let _ = to;
    }
}
