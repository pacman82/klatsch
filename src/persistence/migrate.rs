use crate::{
    chat::migrate_chat_persistence, persistence::ExecuteSql, user::migrate_users_persistence,
};

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
