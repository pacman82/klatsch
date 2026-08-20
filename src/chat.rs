mod chat_http;
mod chat_persistence;
mod chat_runtime;
mod chat_store;
mod event;
mod message;
mod terminate_if;

use crate::persistence::ExecuteSqlAsync;

pub use self::{chat_persistence::migrate_chat_persistence, chat_runtime::ChatRuntime};

// Integrate chat store with chat runtime. We do it here, because we want the submodules to be
// independent from each other. Yet, the decision still belongs to the chat module.

use self::{
    chat_runtime::Chat,
    chat_store::{ChatError, PersistentChat},
    event::{Event, EventId},
    message::{Message, MessageId},
};

impl ChatRuntime {
    pub async fn new(
        persistence: impl ExecuteSqlAsync + Send + Sync + 'static,
    ) -> anyhow::Result<Self> {
        let chat_store = PersistentChat::new(persistence).await?;
        Ok(Self::with_chat_store(chat_store))
    }
}
