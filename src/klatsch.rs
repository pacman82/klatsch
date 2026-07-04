use std::sync::Arc;

use crate::{
    chat::{ChatRuntime, PersistentChat},
    configuration::Configuration,
    persistence::{SqlitePersistence, migrate},
    server::Server,
    sessions::InMemorySessions,
    user::PersistedUsers,
};

pub struct Klatsch {
    chat: ChatRuntime,
    server: Server,
}

impl Klatsch {
    pub async fn new(cfg: &Configuration) -> anyhow::Result<Self> {
        let persistence = SqlitePersistence::new(cfg.persistence_dir(), migrate).await?;
        // users and history share the same persistence backend. This makes life easier for the
        // operators.
        let persistence = Arc::new(persistence);

        let users = PersistedUsers::new(persistence.clone());
        let history = PersistentChat::new(persistence).await?;

        // Forward messages between peers in the chat
        let chat = ChatRuntime::new(history);

        // Answer incoming HTTP requests
        let server = Server::new(
            cfg.socket_addr(),
            chat.client(),
            users,
            InMemorySessions::new(),
        )
        .await?;

        Ok(Self { chat, server })
    }

    pub async fn shutdown(self) {
        // Gracefully shutdown the http server.
        self.server.shutdown().await;

        // Let's shutdown the chat runtime as well. After the http interface, since the http interface
        // relies on it.
        self.chat.shutdown().await;
    }
}
