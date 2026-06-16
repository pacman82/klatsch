use uuid::Uuid;

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
pub trait Authenticate {
    fn user_id(&mut self, name: String) -> impl Future<Output = Result<Uuid, AuthenticationError>>;
}

impl Authenticate for UsersClient {
    async fn user_id(&mut self, name: String) -> Result<Uuid, AuthenticationError> {
        let uuid = Uuid::new_v4();
        Ok(uuid)
    }
}

#[derive(Debug)]
pub enum AuthenticationError {
    Internal,
}

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

#[cfg(test)]
mod tests {
    use super::{Authenticate, Users};

    #[tokio::test]
    async fn different_uuids_for_each_user() {
        let users = Users;
        let mut client = users.client();

        let alice_id = client.user_id("Alice".to_owned()).await.unwrap();
        let bob_id = client.user_id("Bob".to_owned()).await.unwrap();

        assert_ne!(alice_id, bob_id)
    }

    #[tokio::test]
    #[should_panic] // Not implemented yet
    async fn same_user_always_has_same_uuid() {
        let users = Users;
        let mut client = users.client();

        let alice_id_1 = client.user_id("Alice".to_owned()).await.unwrap();
        let alice_id_2 = client.user_id("Alice".to_owned()).await.unwrap();

        assert_eq!(alice_id_1, alice_id_2)
    }
}
