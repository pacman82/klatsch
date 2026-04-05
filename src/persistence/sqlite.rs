use super::{ExecuteSql, FieldAccess, Parameter, Parameters, Persistence, PersistenceError};
use anyhow::{anyhow, bail};
use async_sqlite::{
    Client, ClientBuilder, JournalMode,
    rusqlite::{self, Params, Row, ToSql, ffi, params_from_iter, types::ToSqlOutput},
};
use fs2::{FileExt as _, lock_contended_error};
use std::{fs::File, path::Path};
use tokio::fs::create_dir_all;
use tracing::{error, info};

pub struct SqlitePersistence {
    conn: Client,
    /// Held for the lifetime of the struct to prevent concurrent instances on the same directory.
    _lock_file: Option<File>,
}

impl SqlitePersistence {
    pub async fn new(
        directory: Option<&Path>,
        create_schema: impl for<'any> FnOnce(&rusqlite::Connection) -> Result<(), rusqlite::Error>
        + Send
        + 'static,
    ) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new();
        let mut lock = None;
        if let Some(dir) = directory {
            create_dir_all(dir).await.inspect_err(
                |err| error!(target: "persistence", "Failed to create database directory: {err}"),
            )?;
            lock = Some(acquire_lock(dir)?);
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

        let persistence = SqlitePersistence {
            conn,
            _lock_file: lock,
        };
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
        params: impl Parameters + Send + Sync + 'static,
        map: impl Fn(&Row<'_>) -> Result<O, rusqlite::Error> + Send + 'static,
    ) -> anyhow::Result<O>
    where
        O: Send + 'static,
    {
        let fetch_row = move |conn: &rusqlite::Connection| {
            let mut stmt = conn
                .prepare_cached(query)
                .expect("hardcoded SQL must be valid");
            let params = to_rusqlite_params(&params);
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
        params: impl Parameters + Send + Sync + 'static,
        map: impl Fn(&Row<'_>) -> Result<O, rusqlite::Error> + Send + 'static,
    ) -> anyhow::Result<Vec<O>>
    where
        O: Send + 'static,
    {
        let fetch_rows = move |conn: &rusqlite::Connection| {
            let mut stmt = conn
                .prepare_cached(query)
                .expect("hardcoded SQL must be valid");
            let params = to_rusqlite_params(&params);
            stmt.query_map(params, map)?.collect()
        };

        self.conn
            .conn(move |conn| fetch_rows(conn))
            .await
            .inspect_err(|err| error!(target: "persistence", "Failed to read rows: {err}"))
            .map_err(Into::into)
    }
}

fn to_rusqlite_params<'a>(params: &'a impl Parameters) -> impl Params {
    let it = (0..params.len()).map(|index| params.get(index));
    params_from_iter(it)
}

impl FieldAccess for rusqlite::Row<'_> {
    fn get_blob(&self, index: usize) -> Vec<u8> {
        self.get(index).unwrap()
    }

    fn get_i64(&self, index: usize) -> i64 {
        self.get(index).unwrap()
    }

    fn get_i64_opt(&self, index: usize) -> Option<i64> {
        self.get(index).unwrap()
    }

    fn get_text(&self, index: usize) -> String {
        self.get(index).unwrap()
    }
}

impl ExecuteSql for rusqlite::Connection {
    type Row<'a> = rusqlite::Row<'a>;
    type Error = rusqlite::Error;

    fn execute(&self, query: &str, params: impl Parameters) -> Result<(), Self::Error> {
        let mut stmt = self.prepare_cached(query).expect("SQL must be valid");

        let params = to_rusqlite_params(&params);
        stmt.execute(params)?;
        Ok(())
    }

    fn row<O>(
        &self,
        query: &str,
        params: impl Parameters,
        map: impl Fn(&rusqlite::Row<'_>) -> Result<O, rusqlite::Error>,
    ) -> Result<O, rusqlite::Error> {
        let params = to_rusqlite_params(&params);
        self.prepare_cached(query)
            .expect("SQL must be valid")
            .query_row(params, map)
    }
}

impl PersistenceError for rusqlite::Error {
    fn is_unique_constraint_violation(&self) -> bool {
        matches!(
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

fn acquire_lock(dir: &Path) -> anyhow::Result<File> {
    let lock_file = File::create(dir.join("klatsch.lock"))?;
    match lock_file.try_lock_exclusive() {
        Ok(()) => Ok(lock_file),
        Err(err) if err.raw_os_error() == lock_contended_error().raw_os_error() => Err(anyhow!(
            "Another instance is already using this persistence directory"
        )),
        Err(err) => Err(err.into()),
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

impl ToSql for Parameter<'_> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Parameter::I64(i) => i.to_sql(),
            Parameter::Text(s) => s.to_sql(),
            Parameter::Blob(b) => b.to_sql(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::persistence::{FieldAccess, Persistence};

    use super::{ClientBuilder, JournalMode, SqlitePersistence, rusqlite};

    #[tokio::test]
    async fn creates_missing_persistence_directory() {
        // Given that the directory does not exist yet
        let parent = tempfile::tempdir().unwrap();
        let missing_dir = parent.path().join("does-not-exist");
        let dummy_migration = |_conn: &rusqlite::Connection| Ok(());

        // When a persistence instance is created with the missing directory
        SqlitePersistence::new(Some(&missing_dir), dummy_migration)
            .await
            .unwrap();

        // Then the directory is created and the database file exists
        assert!(missing_dir.join("klatsch.db").exists());
    }

    #[tokio::test]
    async fn second_instance_on_same_directory_is_rejected() {
        // Given a persistence instance backed by a directory in the file system
        let dir = tempfile::tempdir().unwrap();
        let dummy_migration = |_conn: &rusqlite::Connection| Ok(());
        let _first = SqlitePersistence::new(Some(dir.path()), dummy_migration)
            .await
            .unwrap();

        let result = SqlitePersistence::new(Some(dir.path()), dummy_migration).await;

        // When a second persistence instance is created in the same directory
        let Err(err) = result else {
            panic!("Must reject second instance on same directory");
        };

        // Then an error is returned indicating the directory is already in use
        assert_eq!(
            err.to_string(),
            "Another instance is already using the same persistence directory"
        );
    }

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

    #[tokio::test]
    async fn persistence() {
        // Given a directory
        let dir = tempfile::tempdir().unwrap();

        // When the directory is configured and database with data is created
        let create_schema = |connection: &rusqlite::Connection| {
            connection.execute(
                "CREATE TABLE my_table (id INTEGER PRIMARY KEY, data TEXT)",
                (),
            )?;
            Ok(())
        };
        let persistence = SqlitePersistence::new(Some(dir.path()), create_schema)
            .await
            .unwrap();
        persistence
            .transaction(|conn| {
                conn.execute(
                    "INSERT INTO my_table (id, data) VALUES (1, 'Hello, World!')",
                    (),
                )
            })
            .await
            .unwrap();
        drop(persistence);

        // Then reopening the database from the same directory the data previously inserted can be
        // queried.
        let persistence = SqlitePersistence::new(Some(dir.path()), create_schema)
            .await
            .unwrap();

        let after = persistence
            .rows_vec("SELECT id, data FROM my_table", (), |row| {
                Ok((row.get_i64(0), row.get_text(1)))
            })
            .await
            .unwrap();
        assert_eq!([(1i64, "Hello, World!".to_owned())].as_slice(), &after);
    }
}
