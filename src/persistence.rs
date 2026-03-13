use std::path::Path;

use async_sqlite::{
    Client, ClientBuilder, JournalMode,
    rusqlite::{self, Params, Row},
};
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

    pub async fn rows<O>(
        &self,
        query: &'static str,
        params: impl Params + Send + 'static,
        map: impl Fn(&Row<'_>) -> Result<O, rusqlite::Error> + Send + 'static,
    ) -> anyhow::Result<Vec<O>>
    where
        O: Send + 'static,
    {
        let fetch_rows = |conn: &rusqlite::Connection| {
            let mut stmt = conn
                .prepare_cached(query)
                .expect("hardcoded SQL must be valid");
            stmt.query_map(params, map)?.collect()
        };

        self.conn
            .conn(move |conn| fetch_rows(conn))
            .await
            .inspect_err(|err| error!(target: "persistence", "Failed to read rows: {err}"))
            .map_err(Into::into)
    }
}
