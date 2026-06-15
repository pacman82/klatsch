use crate::persistence::ExecuteSql;

pub struct Users;

impl Users {
    pub fn client(&self) -> UsersClient {
        UsersClient
    }
}

#[derive(Clone)]
pub struct UsersClient;

#[cfg_attr(test, double_trait::dummies)]
pub trait Authenticate {}

impl Authenticate for UsersClient {}

pub fn migrate_users_persistence<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    match from_version {
        // No prior database found create current schema from scratch
        0 => {
            create_schema_from_scratch(conn)?;
        }
        _ => (),
    }
    Ok(())
}

fn create_schema_from_scratch<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "CREATE TABLE users (
            id BLOB PRIMARY KEY,
            name TEXT NOT NULL
        )",
        (),
    )?;
    Ok(())
}
