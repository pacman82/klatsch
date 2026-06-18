use serde::Serialize;
use uuid::Uuid;

use crate::persistence::{ExecuteSql, FieldAccess as _, Persistence};

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct User {
    pub name: String,
}

#[derive(Clone)]
pub struct PersistedUsers<P> {
    persistence: P,
}

impl<P> PersistedUsers<P> {
    pub fn new(persistence: P) -> Self {
        PersistedUsers { persistence }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Users {
    fn user_id(&mut self, name: String) -> impl Future<Output = Result<Uuid, UsersError>> + Send;

    fn user_by_id(&mut self, id: Uuid) -> impl Future<Output = Result<User, UsersError>> + Send;
}

impl<P> Users for PersistedUsers<P>
where
    P: Persistence + Send,
{
    async fn user_id(&mut self, name: String) -> Result<Uuid, UsersError> {
        let uuid = self
            .persistence
            .transaction(|conn| fetch_user_id(conn, name))
            .await
            .map_err(|_| UsersError::Internal)?;
        Ok(uuid)
    }

    async fn user_by_id(&mut self, id: Uuid) -> Result<User, UsersError> {
        let mut users = self
            .persistence
            .rows_vec("SELECT name FROM users WHERE id = ?1", id, |row| {
                let user = User {
                    name: row.get_text(0),
                };
                Ok(user)
            })
            .await
            .map_err(|_| UsersError::Internal)?;
        users.pop().ok_or(UsersError::UnknownUser)
    }
}

#[derive(Debug)]
pub enum UsersError {
    Internal,
    /// The user id does not belong to any user.
    UnknownUser,
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
    use std::assert_matches;

    use uuid::Uuid;

    use crate::{
        persistence::{Persistence, SqlitePersistence},
        user::{UsersError, migrate_users_persistence},
    };

    use super::{PersistedUsers, User, Users};

    #[tokio::test]
    async fn different_uuids_for_each_user() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let alice_id = users.user_id("Alice".to_owned()).await.unwrap();
        let bob_id = users.user_id("Bob".to_owned()).await.unwrap();

        assert_ne!(alice_id, bob_id)
    }

    #[tokio::test]
    async fn same_user_always_has_same_uuid() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let alice_id_1 = users.user_id("Alice".to_owned()).await.unwrap();
        let alice_id_2 = users.user_id("Alice".to_owned()).await.unwrap();

        assert_eq!(alice_id_1, alice_id_2)
    }

    #[tokio::test]
    async fn fetch_user_by_id() {
        // Given
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let alice_id = users.user_id("Alice".to_owned()).await.unwrap();

        // When
        let user = users.user_by_id(alice_id).await.unwrap();

        // Then
        let expected = User {
            name: "Alice".to_owned(),
        };
        assert_eq!(expected, user);
    }

    #[tokio::test]
    async fn fetch_unknown_user_by_id() {
        // Given
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        // When
        let result = users.user_by_id(Uuid::new_v4()).await;

        // Then
        assert_matches!(result, Err(UsersError::UnknownUser));
    }

    async fn persistence_fake() -> impl Persistence {
        SqlitePersistence::new(None, migrate_users_persistence)
            .await
            .unwrap()
    }
}
