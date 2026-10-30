//! What the UI asks of a chat service. Teams is the one there is (`crate::teams`);
//! the UI only sees this trait and the `model` types.

use crate::model::{Directory, Message, Picture};
use std::fmt;

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
}
