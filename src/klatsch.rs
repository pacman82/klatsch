use crate::{
    chat::ChatRuntime,
    configuration::Configuration,
    persistence::{SqlitePersistence, migrate},
    server::Server,
    sessions::SessionsRuntime,
    user::UserStore,
};

pub struct Klatsch {
    chat: ChatRuntime,
    sessions: SessionsRuntime,
    server: Server,
    /// Held for the lifetime of the application to keep the persistence directory locked.
    _persistence: SqlitePersistence,
}

impl Klatsch {
    pub async fn new(cfg: &Configuration) -> anyhow::Result<Self> {
        let persistence = SqlitePersistence::new(cfg.persistence_dir(), migrate).await?;

        // users and history share the same persistence backend. This makes life easier for the
        // operators.
        let users = UserStore::new(persistence.client().await?);

        // Chat and sessions are independent of each other, so start them concurrently.
        let (chat, sessions) = tokio::try_join!(
            async { ChatRuntime::new(persistence.client().await?).await },
            async { SessionsRuntime::new(cfg.session_expiry(), persistence.client().await?).await },
        )?;

        // Answer incoming HTTP requests
        let server = Server::new(cfg.server(), chat.client(), users, sessions.client()).await?;

        Ok(Self {
            chat,
            server,
            sessions,
            _persistence: persistence,
        })
    }

    pub async fn shutdown(self) {
        // Gracefully shutdown the http server.
        self.server.shutdown().await;

        // Shutdown the chat and sessions runtimes after the http interface, since the http
        // interface relies on them.
        tokio::join!(self.chat.shutdown(), self.sessions.shutdown());
    }
}
