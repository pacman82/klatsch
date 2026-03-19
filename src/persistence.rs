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

    pub async fn transaction<O>(
        &self,
        f: impl FnOnce(&rusqlite::Connection) -> Result<O, rusqlite::Error> + Send + 'static,
    ) -> Result<O, anyhow::Error>
    where
        O: Send + 'static,
    {
        self.conn
            .conn_mut(move |conn| {
                let transaction = conn.transaction()?;
                let out = f(&transaction)?;
                transaction.commit()?;
                Ok(out)
            })
            .await
            .inspect_err(|err| error!(target: "persistence", "Transaction failed: {err}"))
            .map_err(Into::into)
    }

    pub async fn row<O>(
        &self,
        query: &'static str,
        params: impl Params + Send + 'static,
        map: impl Fn(&Row<'_>) -> Result<O, rusqlite::Error> + Send + 'static,
    ) -> anyhow::Result<O>
    where
        O: Send + 'static,
    {
        let fetch_row = |conn: &rusqlite::Connection| {
            let mut stmt = conn
                .prepare_cached(query)
                .expect("hardcoded SQL must be valid");
            stmt.query_row(params, map)
        };

        self.conn
            .conn(move |conn| fetch_row(conn))
            .await
            .inspect_err(|err| error!(target: "persistence", "Failed to read row: {err}"))
            .map_err(Into::into)
    }

    pub async fn rows_vec<O>(
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
