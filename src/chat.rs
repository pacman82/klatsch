mod chat_runtime;
mod chat_store;
mod event;
mod message;

use crate::persistence::Persistence;

pub use self::{
    chat_runtime::{Chat, ChatRuntime},
    chat_store::{ChatError, migrate_chat_persistence},
    event::{Event, EventId},
    message::{Message, MessageId},
};

// Integrate chat store with chat runtime. We do it here, because we want the submodules to be
// independent from each other. Yet, the decision still belongs to the chat module.

use self::chat_store::PersistentChat;

impl ChatRuntime {
    pub async fn new(
        persistence: impl Persistence + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let chat_store = PersistentChat::new(persistence).await?;
        Ok(Self::with_chat_store(chat_store))
    }
}
