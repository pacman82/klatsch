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
