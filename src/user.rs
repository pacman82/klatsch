use uuid::Uuid;

use crate::persistence::{ExecuteSql, FieldAccess as _, Persistence};

#[derive(Debug, PartialEq, Eq)]
pub struct User {
    name: String,
}

#[derive(Clone)]
pub struct Users<P> {
    persistence: P,
}

impl<P> Users<P> {
    pub fn new(persistence: P) -> Self {
        Users { persistence }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Authenticate {
    fn user_id(
        &mut self,
        name: String,
    ) -> impl Future<Output = Result<Uuid, AuthenticationError>> + Send;

    fn user_by_id(
        &mut self,
        id: Uuid,
    ) -> impl Future<Output = Result<User, AuthenticationError>> + Send;
}

impl<P> Authenticate for Users<P>
where
    P: Persistence + Send,
{
    async fn user_id(&mut self, name: String) -> Result<Uuid, AuthenticationError> {
        let uuid = self
            .persistence
            .transaction(|conn| fetch_user_id(conn, name))
            .await
            .map_err(|_| AuthenticationError::Internal)?;
        Ok(uuid)
    }

    async fn user_by_id(&mut self, id: Uuid) -> Result<User, AuthenticationError> {
        let mut users = self
            .persistence
            .rows_vec("SELECT name FROM users WHERE id = ?1", id, |row| {
                let user = User {
                    name: row.get_text(0),
                };
                Ok(user)
            })
            .await
            .map_err(|_| AuthenticationError::Internal)?;
        let user = users.pop().expect("TODO: HANDLE UNKNOWN USER ID");
        Ok(user)
    }
}

#[derive(Debug)]
pub enum AuthenticationError {
    Internal,
}

fn fetch_user_id<C>(conn: &C, name: String) -> Result<Uuid, C::Error>
where
    C: ExecuteSql,
{
    let maybe_user_id = conn
        .rows_vec("SELECT id FROM users WHERE name = ?1", &name, |row| {
            Ok(row.get_uuid(0))
        })?
        .pop();
    let user_id = match maybe_user_id {
        Some(user_id) => user_id,
        None => {
            let user_id = Uuid::new_v4();
            conn.execute(
                "INSERT INTO users (id, name) VALUES (?1, ?2)",
                (&user_id, &name),
            )?;
            user_id
        }
    };

    Ok(user_id)
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

#[cfg(test)]
mod tests {
    use crate::{
        persistence::{Persistence, SqlitePersistence},
        user::migrate_users_persistence,
    };

    use super::{Authenticate, User, Users};

    #[tokio::test]
    async fn different_uuids_for_each_user() {
        let persistence = persistence_fake().await;
        let mut users = Users::new(persistence);

        let alice_id = users.user_id("Alice".to_owned()).await.unwrap();
        let bob_id = users.user_id("Bob".to_owned()).await.unwrap();

        assert_ne!(alice_id, bob_id)
    }

    #[tokio::test]
    async fn same_user_always_has_same_uuid() {
        let persistence = persistence_fake().await;
        let mut users = Users::new(persistence);

        let alice_id_1 = users.user_id("Alice".to_owned()).await.unwrap();
        let alice_id_2 = users.user_id("Alice".to_owned()).await.unwrap();

        assert_eq!(alice_id_1, alice_id_2)
    }

    #[tokio::test]
    async fn fetch_user_by_id() {
        // Given
        let persistence = persistence_fake().await;
        let mut users = Users::new(persistence);
        let alice_id = users.user_id("Alice".to_owned()).await.unwrap();

        // When
        let user = users.user_by_id(alice_id).await.unwrap();

        // Then
        let expected = User {
            name: "Alice".to_owned(),
        };
        assert_eq!(expected, user);
    }

    async fn persistence_fake() -> impl Persistence {
        SqlitePersistence::new(None, migrate_users_persistence)
            .await
            .unwrap()
    }
}
