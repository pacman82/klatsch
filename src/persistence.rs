use std::path::Path;

use anyhow::bail;
use async_sqlite::{
    Client, ClientBuilder, JournalMode,
    rusqlite::{self, Params, Row, ffi},
};
use tracing::{error, info};

pub trait Persistence {
    type Row<'a>: FieldAccess;
    type Error: PersistenceError;
    type Connection: ExecuteSql<Error = Self::Error>;

    fn transaction<O>(
        &self,
        f: impl FnOnce(&Self::Connection) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = Result<O, anyhow::Error>> + Send
    where
        O: Send + 'static;

    fn row<O>(
        &self,
        query: &'static str,
        params: impl Params + Send + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<O>> + Send
    where
        O: Send + 'static;

    fn rows_vec<O>(
        &self,
        query: &'static str,
        params: impl Params + Send + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<Vec<O>>> + Send
    where
        O: Send + 'static;
}

pub trait FieldAccess {
    fn get_i64_opt(&self, index: usize) -> Option<i64>;
    fn get_string(&self, index: usize) -> String;
}

pub trait ExecuteSql {
    type Row<'a>: FieldAccess;
    type Error: PersistenceError;

    fn execute(&self, query: &str, params: impl Params) -> Result<(), Self::Error>;

    fn row<O>(
        &self,
        query: &'static str,
        params: impl Params,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<O, Self::Error>;
}

pub trait PersistenceError {
    fn is_unique_constraint_violation(&self) -> bool;
}

pub struct SqlitePersistence {
    conn: Client,
}

impl SqlitePersistence {
    pub async fn new(
        directory: Option<&Path>,
        create_schema: impl for<'any> FnOnce(&rusqlite::Connection) -> Result<(), rusqlite::Error>
        + Send
        + 'static,
    ) -> anyhow::Result<Self> {
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

        let outcome = conn
            .conn_mut(move |conn| migrate(conn, create_schema))
            .await
            .inspect_err(
                |err| error!(target: "persistence", "failed to migrate database: {err}"),
            )?;
        outcome.report_migration_status()?;

        let persistence = SqlitePersistence { conn };
        Ok(persistence)
    }
}

impl Persistence for SqlitePersistence {
    type Row<'a> = rusqlite::Row<'a>;
    type Connection = rusqlite::Connection;
    type Error = rusqlite::Error;

    async fn transaction<O>(
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

    async fn row<O>(
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

    async fn rows_vec<O>(
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

impl FieldAccess for rusqlite::Row<'_> {
    fn get_i64_opt(&self, index: usize) -> Option<i64> {
        self.get(index).unwrap()
    }

    fn get_string(&self, index: usize) -> String {
        self.get(index).unwrap()
    }
}

impl ExecuteSql for rusqlite::Connection {
    type Row<'a> = rusqlite::Row<'a>;
    type Error = rusqlite::Error;

    fn execute(&self, query: &str, params: impl Params) -> Result<(), Self::Error> {
        let mut stmt = self.prepare_cached(query).expect("SQL must be valid");
        stmt.execute(params)?;
        Ok(())
    }

    fn row<O>(
        &self,
        query: &str,
        params: impl Params,
        map: impl Fn(&rusqlite::Row<'_>) -> Result<O, rusqlite::Error>,
    ) -> Result<O, rusqlite::Error> {
        self.prepare_cached(query)
            .expect("SQL must be valid")
            .query_row(params, map)
    }
}

impl PersistenceError for rusqlite::Error {
    fn is_unique_constraint_violation(&self) -> bool {
        !matches!(
            self,
            rusqlite::Error::SqliteFailure(
                ffi::Error {
                    code: ffi::ErrorCode::ConstraintViolation,
                    extended_code: ffi::SQLITE_CONSTRAINT_UNIQUE,
                },
                _,
            )
        )
    }
}

enum MigrationOutcome {
    /// Found an empty database and created the schema from scratch.
    Created,
    /// Found a recent schema. No migration was necessary.
    NoMigration,
    /// Found a future schema version. Aborted to prevent data loss.
    Future { version: u32 },
}

impl MigrationOutcome {
    fn report_migration_status(self) -> anyhow::Result<()> {
        match self {
            MigrationOutcome::Created => {
                info!(target: "persistence", "New database created");
                Ok(())
            }
            MigrationOutcome::NoMigration => Ok(()),
            MigrationOutcome::Future { version } => {
                error!(
                    "Database schema version ({version}) is newer than supported. Aborting to \
                    prevent data corruption."
                );
                bail!(
                    "Found Database created by a newer version. Update to a newer version to load \
                    it."
                )
            }
        }
    }
}

fn migrate(
    conn: &mut rusqlite::Connection,
    create_schema: impl FnOnce(&rusqlite::Connection) -> Result<(), rusqlite::Error>,
) -> Result<MigrationOutcome, rusqlite::Error> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    // Version 0 is the initial version of an empty database. We regard creating a new database as a
    // migration from version 0 to the current version.
    let outcome = match version {
        // New empty database. Create schema from scratch
        0 => {
            let tx = conn.transaction()?;
            create_schema(&tx)?;
            tx.pragma_update(None, "user_version", 1)?;
            tx.commit()?;
            MigrationOutcome::Created
        }
        // Current version, do nothing.
        1 => MigrationOutcome::NoMigration,
        // Future version. Abort and report error in order to prevent data loss.
        future_version => MigrationOutcome::Future {
            version: future_version,
        },
    };
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::{ClientBuilder, JournalMode, SqlitePersistence, rusqlite};

    #[tokio::test]
    async fn rejects_database_from_newer_version() {
        // Given a database with a schema version newer than supported
        let dir = tempfile::tempdir().unwrap();
        ClientBuilder::new()
            .path(dir.path().join("klatsch.db"))
            .journal_mode(JournalMode::Wal)
            .open()
            .await
            .unwrap()
            .conn_mut(|conn| conn.pragma_update(None, "user_version", 1_000))
            .await
            .unwrap();
        let dummy_migration = |_conn: &rusqlite::Connection| Ok(());

        // When trying to open the database
        let result = SqlitePersistence::new(Some(dir.path()), dummy_migration).await;

        // Then it fails with a clear error
        let Err(err) = result else {
            panic!("Must reject newer schema version");
        };
        assert_eq!(
            err.to_string(),
            "Found Database created by a newer version. Update to a newer version to load it."
        );
    }
}
