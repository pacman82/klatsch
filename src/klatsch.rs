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
        let users = UserStore::new(persistence.client());

        // Forward messages between peers in the chat
        let chat = ChatRuntime::new(persistence.client()).await?;

        let sessions = SessionsRuntime::new();

        // Answer incoming HTTP requests
        let server =
            Server::new(cfg.socket_addr(), chat.client(), users, sessions.client()).await?;

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
