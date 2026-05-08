mod arguments;
mod sqlite;

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
    fn get_blob(&self, index: usize) -> Vec<u8>;
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
}

#[cfg_attr(test, double_trait::dummies)]
pub trait PersistenceError {
    fn is_unique_constraint_violation(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{Persistence as _, SqlitePersistence};
    use crate::chat::create_schema_chat;

    #[tokio::test]
    async fn create_scheam_from_scratch() {
        // Given an empty persistence directory
        let dir = tempdir().unwrap();

        // When starting persistence in this directory
        let persistence = SqlitePersistence::new(Some(dir.path()), create_schema_chat)
            .await
            .unwrap();

        // Then the schema should be created
        let expected_sql = vec![
            "CREATE TABLE events (\n                    \
            id INTEGER PRIMARY KEY,\n                    \
            message_id BLOB UNIQUE NOT NULL,\n                    \
            sender TEXT NOT NULL,\n                    \
            content TEXT NOT NULL,\n                    \
            timestamp_ms INTEGER NOT NULL\n                \
            )",
        ];

        let sql = persistence
            .rows_vec(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' ORDER BY name",
                (),
                |row| {
                    let sql: Option<String> = row.get(0).unwrap();
                    Ok(sql.unwrap())
                },
            )
            .await
            .unwrap();
        assert_eq!(expected_sql.as_slice(), &sql);
    }
}
