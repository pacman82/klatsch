mod chat_http;
mod chat_persistence;
mod chat_runtime;
mod chat_store;
mod event;
mod message;
mod terminate_if;

use tokio::sync::watch;

use crate::{
    authentication::AuthenticateRequest, chat::chat_runtime::ChatClient,
    persistence::ExecuteSqlAsync, server::Routes,
};

pub use self::{chat_persistence::migrate_chat_persistence, chat_runtime::ChatRuntime};

// Integrate chat store with chat runtime. We do it here, because we want the submodules to be
// independent from each other. Yet, the decision still belongs to the chat module.

use self::{
    chat_http::chat_routes,
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

impl Routes for ChatClient {
    fn routes(
        self,
        auth: impl AuthenticateRequest + Send + Sync + Clone + 'static,
        shutting_down: watch::Receiver<bool>,
        _encrypted: bool,
    ) -> axum::Router<()> {
        chat_routes(self.clone(), auth, shutting_down)
    }
}
