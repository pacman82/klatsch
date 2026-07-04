mod password_hash;
mod user_id;

use serde::{Deserialize, Serialize};

use crate::persistence::{ExecuteSql, FieldAccess as _, Persistence};

pub use self::user_id::UserId;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    fn signup(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send;

    fn login(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send;

    fn user_by_id(&mut self, id: UserId) -> impl Future<Output = Result<User, UsersError>> + Send;
}

impl<P> Users for PersistedUsers<P>
where
    P: Persistence + Send,
{
    async fn signup(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
        let name_clone = name.clone();
        let maybe_user = self
            .persistence
            .transaction(move |conn| fetch_user_id_and_hash(conn, &name_clone))
            .await
            .map_err(|_| UsersError::Internal)?;

        if let Some((user_id, maybe_hash)) = maybe_user {
            if let Some(hash) = maybe_hash
                && !password_hash::verify(&password, &hash)
            {
                return Err(UsersError::Unauthenticated);
            }

            // User already exists, nothing more to do
            return Ok(user_id);
        }

        // User does not exist, create a new one
        let user_id = UserId::new();
        let password_hash = (!password.is_empty()).then(|| password_hash::generate(&password));
        self.persistence
            .transaction(move |conn| create_user(conn, user_id, &name, password_hash.as_deref()))
            .await
            .map_err(|_| UsersError::Internal)?;

        Ok(user_id)
    }

    async fn login(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
        let maybe_user = self
            .persistence
            .transaction(move |conn| fetch_user_id_and_hash(conn, &name))
            .await
            .map_err(|_| UsersError::Internal)?;

        let (user_id, maybe_hash) = maybe_user.ok_or(UsersError::Unauthenticated)?;

        if let Some(hash) = maybe_hash
            && !password_hash::verify(&password, &hash)
        {
            return Err(UsersError::Unauthenticated);
        }

        Ok(user_id)
    }

    async fn user_by_id(&mut self, id: UserId) -> Result<User, UsersError> {
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
    /// Either name or password is incorrect.
    Unauthenticated,
}

fn fetch_user_id_and_hash<C>(
    conn: &C,
    name: &str,
) -> Result<Option<(UserId, Option<String>)>, C::Error>
where
    C: ExecuteSql,
{
    let maybe_user_auth = conn
        .rows_vec(
            "SELECT id, password_hash FROM users WHERE name = ?1",
            name,
            |row| Ok((UserId::from_uuid(row.get_uuid(0)), row.get_text_opt(1))),
        )?
        .pop();
    Ok(maybe_user_auth)
}

fn create_user<C>(
    conn: &C,
    user_id: UserId,
    name: &str,
    password_hash: Option<&str>,
) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "INSERT INTO users (id, name, password_hash) VALUES (?1, ?2, ?3)",
        (user_id, name, password_hash),
    )?;
    Ok(())
}

fn create_schema_from_scratch<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "CREATE TABLE users (
            id BLOB PRIMARY KEY,
            name TEXT NOT NULL,
            password_hash TEXT
        )",
        (),
    )?;
    Ok(())
}

pub fn migrate_users_persistence<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    if from_version == 0 {
        create_schema_from_scratch(conn)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use crate::{
        persistence::{Persistence, SqlitePersistence},
        user::{
            UsersError::{self, Unauthenticated},
            migrate_users_persistence,
        },
    };

    use super::{PersistedUsers, User, UserId, Users};

    #[tokio::test]
    async fn different_uuids_for_each_user() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let alice_id = users
            .signup("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();
        let bob_id = users
            .signup("Bob".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

        assert_ne!(alice_id, bob_id)
    }

    #[tokio::test]
    async fn same_user_always_has_same_uuid() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let alice_id_1 = users
            .signup("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();
        let alice_id_2 = users
            .signup("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

        assert_eq!(alice_id_1, alice_id_2)
    }

    #[tokio::test]
    async fn reject_login_with_wrong_password() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let _alice_id = users
            .signup("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();

        let result = users
            .signup("Alice".to_owned(), "wrong-secret".to_owned())
            .await;

        assert_matches!(result, Err(Unauthenticated))
    }

    #[tokio::test]
    async fn login_returns_same_uuid_as_signup() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let signup_id = users
            .signup("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();

        let login_id = users
            .login("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();

        assert_eq!(signup_id, login_id)
    }

    #[tokio::test]
    async fn login_rejects_unknown_user() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let result = users.login("Alice".to_owned(), "secret".to_owned()).await;

        assert_matches!(result, Err(Unauthenticated))
    }

    #[tokio::test]
    async fn login_rejects_wrong_password() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let _alice_id = users
            .signup("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();

        let result = users
            .login("Alice".to_owned(), "wrong-secret".to_owned())
            .await;

        assert_matches!(result, Err(Unauthenticated))
    }

    #[tokio::test]
    async fn fetch_user_attributes_by_id() {
        // Given
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let alice_id = users
            .signup("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

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
        let result = users.user_by_id(UserId::new()).await;

        // Then
        assert_matches!(result, Err(UsersError::UnknownUser));
    }

    async fn persistence_fake() -> impl Persistence {
        SqlitePersistence::new(None, migrate_users_persistence)
            .await
            .unwrap()
    }
}
