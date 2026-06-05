use crate::{
    chat::{ChatRuntime, PersistentChat},
    configuration::Configuration,
    persistence::{SqlitePersistence, migrate},
    server::Server,
};

pub struct Klatsch {
    chat: ChatRuntime,
    server: Server,
}

impl Klatsch {
    pub async fn new(cfg: &Configuration) -> anyhow::Result<Self> {
        // Initialize persistence for chat
        let persistence = SqlitePersistence::new(cfg.persistence_dir(), migrate).await?;

        let history = PersistentChat::new(persistence).await?;

        // Forward messages between peers in the chat
        let chat = ChatRuntime::new(history);

        // Answer incoming HTTP requests
        let server = Server::new(cfg.socket_addr(), chat.client()).await?;

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
