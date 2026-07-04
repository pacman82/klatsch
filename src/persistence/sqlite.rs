use crate::persistence::GetField;

use super::{Argument, Arguments, ExecuteSql, GetFieldNative, Persistence, PersistenceError};
use anyhow::{anyhow, bail};
use async_sqlite::{
    Client, ClientBuilder, JournalMode,
    rusqlite::{
        self, Params, Row, ToSql, ffi, params_from_iter,
        types::{ToSqlOutput, Value},
    },
};
use fs2::{FileExt as _, lock_contended_error};
use std::{fs::File, path::Path};
use tokio::fs::create_dir_all;
use tracing::{error, info};
use uuid::Uuid;

const CURRENT_SCHEMA_VERSION: u32 = 2;

pub struct SqlitePersistence {
    conn: Client,
    /// Held for the lifetime of the struct to prevent concurrent instances on the same directory.
    /// `None` for in-memory databases.
    _lock_file: Option<File>,
}

impl SqlitePersistence {
    pub async fn new(
        directory: Option<&Path>,
        migrate: impl for<'any> Fn(&rusqlite::Connection, u32) -> Result<(), rusqlite::Error>
        + Send
        + 'static,
    ) -> anyhow::Result<Self> {
        let mut builder = ClientBuilder::new();
        let mut lock = None;
        if let Some(dir) = directory {
            create_dir_all(dir).await.inspect_err(
                |err| error!(target: "persistence", error=%err, "Failed to create database directory"),
            )?;
            lock = Some(acquire_lock(dir)?);
            builder = builder
                .path(dir.join("klatsch.db"))
                .journal_mode(JournalMode::Wal);
        }
        let conn = builder.open().await.inspect_err(
            |err| error!(target: "persistence", error=%err, "Failed to open database"),
        )?;

        let outcome = conn
            .conn_mut(move |conn| migrate_to_current(conn, migrate))
            .await
            .inspect_err(
                |err| error!(target: "persistence", error=%err, "failed to migrate database"),
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
            .inspect_err(|err| error!(target: "persistence", error=%err, "Transaction failed"))
            .map_err(Into::into)
    }

    async fn row<O>(
        &self,
        query: &'static str,
        params: impl Arguments + Send + Sync + 'static,
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
            .conn(fetch_row)
            .await
            .inspect_err(|err| error!(target: "persistence", error=%err, "Failed to read row"))
            .map_err(Into::into)
    }

    async fn rows_vec<O>(
        &self,
        query: &'static str,
        params: impl Arguments + Send + Sync + 'static,
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
            .conn(fetch_rows)
            .await
            .inspect_err(|err| error!(target: "persistence", error=%err, "Failed to read rows"))
            .map_err(Into::into)
    }
}

/// Convert arguments as defined by the persistent trait, into `Params` as defined by rusqlite.
///
/// Both of these have the same responsibility as in being a set of input values to a query
/// which replace the values of the placeholders in the query with actual values. Yet `Arguments` is
/// its expression independent of and belongs to the persistence trait. `Params` is its sqlite
/// specific counterpart.
fn to_rusqlite_params(params: &impl Arguments) -> impl Params {
    let it = (0..params.len()).map(|index| params.get(index));
    params_from_iter(it)
}

impl GetFieldNative for rusqlite::Row<'_> {}

impl GetField<i64> for rusqlite::Row<'_> {
    fn get(&self, index: usize) -> i64 {
        self.get(index).unwrap()
    }
}

impl GetField<Option<i64>> for rusqlite::Row<'_> {
    fn get(&self, index: usize) -> Option<i64> {
        self.get(index).unwrap()
    }
}

impl GetField<String> for rusqlite::Row<'_> {
    fn get(&self, index: usize) -> String {
        self.get(index).unwrap()
    }
}

impl GetField<Option<String>> for rusqlite::Row<'_> {
    fn get(&self, index: usize) -> Option<String> {
        self.get(index).unwrap()
    }
}

impl GetField<Uuid> for rusqlite::Row<'_> {
    fn get(&self, index: usize) -> Uuid {
        let bytes = self.get(index).unwrap();
        Uuid::from_bytes(bytes)
    }
}

impl ExecuteSql for rusqlite::Connection {
    type Row<'a> = rusqlite::Row<'a>;
    type Error = rusqlite::Error;

    fn execute(&self, query: &str, params: impl Arguments) -> Result<(), Self::Error> {
        let mut stmt = self.prepare_cached(query).expect("SQL must be valid");

        let params = to_rusqlite_params(&params);
        stmt.execute(params)?;
        Ok(())
    }

    fn row<O>(
        &self,
        query: &str,
        args: impl Arguments,
        map: impl Fn(&rusqlite::Row<'_>) -> Result<O, rusqlite::Error>,
    ) -> Result<O, rusqlite::Error> {
        let params = to_rusqlite_params(&args);
        self.prepare_cached(query)
            .expect("SQL must be valid")
            .query_row(params, map)
    }

    fn rows_vec<O>(
        &self,
        query: &str,
        args: impl Arguments,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<Vec<O>, Self::Error> {
        let params = to_rusqlite_params(&args);
        let mut query = self.prepare_cached(query).expect("SQL must be valid");
        let it = query.query_map(params, map)?;
        it.collect()
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
    /// Found an old schema and migrated it to the current version.
    Migrated,
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
            MigrationOutcome::Migrated => {
                info!(target: "persistence", "Database migrated");
                Ok(())
            }
            MigrationOutcome::NoMigration => Ok(()),
            MigrationOutcome::Future { version } => {
                error!(
                    target: "persistence",
                    version = version,
                    "Database schema is newer than supported. Aborting to prevent data corruption.",
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
            "Another instance is already using the same persistence directory"
        )),
        Err(err) => Err(err.into()),
    }
}

/// Migration function running in the actor thread of async-sqlite
fn migrate_to_current(
    conn: &mut rusqlite::Connection,
    migrate: impl Fn(&rusqlite::Connection, u32) -> Result<(), rusqlite::Error>,
) -> Result<MigrationOutcome, rusqlite::Error> {
    let version: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    // Version 0 is the initial version of an empty database. We regard creating a new database as a
    // migration from version 0 to the current version.
    let outcome = match version {
        // New empty database. Create schema from scratch
        0 => {
            let tx = conn.transaction()?;
            migrate(&tx, 0)?;
            tx.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
            tx.commit()?;
            MigrationOutcome::Created
        }
        found_version @ (1..CURRENT_SCHEMA_VERSION) => {
            for from in found_version..CURRENT_SCHEMA_VERSION {
                let tx = conn.transaction()?;
                migrate(&tx, from)?;
                tx.pragma_update(None, "user_version", from + 1)?;
                tx.commit()?;
            }
            MigrationOutcome::Migrated
        }
        // Current version, do nothing.
        CURRENT_SCHEMA_VERSION => MigrationOutcome::NoMigration,
        // Future version. Abort and report error in order to prevent data loss.
        future_version => MigrationOutcome::Future {
            version: future_version,
        },
    };
    Ok(outcome)
}

impl ToSql for Argument<'_> {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        match self {
            Argument::I64(i) => i.to_sql(),
            Argument::Text(s) => s.to_sql(),
            Argument::Blob(b) => b.to_sql(),
            Argument::Null => Ok(ToSqlOutput::Owned(Value::Null)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::persistence::GetField;

    use super::{ClientBuilder, JournalMode, Persistence, SqlitePersistence, rusqlite};

    #[tokio::test]
    async fn creates_missing_persistence_directory() {
        // Given that the directory does not exist yet
        let parent = tempfile::tempdir().unwrap();
        let missing_dir = parent.path().join("does-not-exist");
        let dummy_migration = |_conn: &rusqlite::Connection, _from_version: u32| Ok(());

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
        let dummy_migration = |_conn: &rusqlite::Connection, _from_version: u32| Ok(());
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
        let dummy_migration = |_conn: &rusqlite::Connection, _from_version: u32| Ok(());

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
        let create_schema = |connection: &rusqlite::Connection, _from_version: u32| {
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
                // Avoid confusion with rusqlite::Row::get
                let id: i64 = <rusqlite::Row as GetField<i64>>::get(row, 0);
                let data: String = <rusqlite::Row as GetField<String>>::get(row, 1);
                Ok((id, data))
            })
            .await
            .unwrap();
        assert_eq!([(1i64, "Hello, World!".to_owned())].as_slice(), &after);
    }
}
