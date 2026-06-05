mod arguments;
mod sqlite;

use uuid::Uuid;

use crate::{chat::migrate_chat_persistence, user::migrate_users_persistence};

pub use self::{
    arguments::{Argument, Arguments},
    sqlite::SqlitePersistence,
};

#[cfg_attr(test, double_trait::dummies)]
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
        args: impl Arguments + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<O>> + Send
    where
        O: Send + 'static;

    fn rows_vec<O>(
        &self,
        query: &'static str,
        args: impl Arguments + Send + Sync + 'static,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error> + Send + 'static,
    ) -> impl Future<Output = anyhow::Result<Vec<O>>> + Send
    where
        O: Send + 'static;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait FieldAccess {
    fn get_uuid(&self, index: usize) -> Uuid;
    fn get_i64(&self, index: usize) -> i64;
    fn get_i64_opt(&self, index: usize) -> Option<i64>;
    fn get_text(&self, index: usize) -> String;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait ExecuteSql {
    type Row<'a>: FieldAccess;
    type Error: PersistenceError;

    fn execute(&self, query: &str, args: impl Arguments) -> Result<(), Self::Error>;

    fn row<O>(
        &self,
        query: &'static str,
        args: impl Arguments,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<O, Self::Error>;

    fn rows_vec<O>(
        &self,
        query: &str,
        args: impl Arguments,
        map: impl Fn(&Self::Row<'_>) -> Result<O, Self::Error>,
    ) -> Result<Vec<O>, Self::Error>;
}

#[cfg_attr(test, double_trait::dummies)]
pub trait PersistenceError {
    fn is_unique_constraint_violation(&self) -> bool;
}

/// Migrates the schema for the entire klatsch application
pub fn migrate<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    // we do so by migrating the schemas of our individual modules
    migrate_users_persistence(conn, from_version)?;
    migrate_chat_persistence(conn, from_version)?;
    Ok(())
}

#[cfg(test)]
mod tests {

    use tempfile::tempdir;
    use tokio::fs;

    use crate::persistence::{Persistence as _, SqlitePersistence};

    use super::migrate;

    #[tokio::test]
    async fn schema_from_v1() {
        // Given an persistence directory with an existing v1 database
        let dir = tempdir().unwrap();
        fs::copy("tests/v1.db", dir.path().join("klatsch.db"))
            .await
            .unwrap();

        // When starting persistence in this directory
        let persistence = SqlitePersistence::new(Some(dir.path()), migrate)
            .await
            .unwrap();

        // Then
        assert_eq!(sql_schema_from_scratch().await, schema(&persistence).await)
    }

    async fn schema(persistence: &SqlitePersistence) -> Vec<String> {
        persistence
            .rows_vec(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' ORDER BY name",
                (),
                |row| {
                    let sql: Option<String> = row.get(0).unwrap();
                    Ok(sql.unwrap())
                },
            )
            .await
            .unwrap()
    }

    /// SQL creating the the persistence schema from scratch, then no migration takes place.
    async fn sql_schema_from_scratch() -> Vec<String> {
        let persistence = SqlitePersistence::new(None, migrate).await.unwrap();

        schema(&persistence).await
    }
}
