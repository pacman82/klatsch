use std::path::Path;

use async_sqlite::{Client, ClientBuilder, JournalMode};
use tracing::error;

pub struct Persistence {
    pub conn: Client,
}

impl Persistence {
    pub async fn new(directory: Option<&Path>) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new();
        if let Some(dir) = directory {
            builder = builder
                .path(dir.join("klatsch.db"))
                .journal_mode(JournalMode::Wal);
        }
        let conn = builder
            .open()
            .await
            .inspect_err(|err| error!(target: "persistence", "Failed to open database: {err}"))?;

        let persistence = Persistence { conn };
        Ok(persistence)
    }
}
