use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier as _,
    password_hash::{SaltString, rand_core::OsRng},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persistence::{ExecuteSql, FieldAccess as _, Persistence};

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
    fn login(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<Uuid, UsersError>> + Send;

    fn user_by_id(&mut self, id: Uuid) -> impl Future<Output = Result<User, UsersError>> + Send;

    fn authenticate(&mut self, id: Uuid) -> impl Future<Output = Result<(), UsersError>> + Send;
}

impl<P> Users for PersistedUsers<P>
where
    P: Persistence + Send,
{
    async fn login(&mut self, name: String, password: String) -> Result<Uuid, UsersError> {
        let name_clone = name.clone();
        let maybe_user = self
            .persistence
            .transaction(move |conn| fetch_user_id_and_hash(conn, &name_clone))
            .await
            .map_err(|_| UsersError::Internal)?;

        if let Some((user_id, maybe_password_hash)) = maybe_user {
            if let Some(password_hash) = maybe_password_hash {
                // Verify the password if the user has set one
                let password_hash = PasswordHash::new(&password_hash)
                    .expect("Persisted password hash must be valid, utf-8 encoded PHC hash");
                Argon2::default()
                    .verify_password(password.as_bytes(), &password_hash)
                    .map_err(|_| UsersError::Unauthenticated)?;
            }

            // User already exists, nothing more to do
            return Ok(user_id);
        }

        // User does not exist, create a new one
        let user_id = Uuid::new_v4();
        let password_hash = (!password.is_empty()).then(|| {
            // let salt = SaltString::generate(rng());
            let salt = SaltString::generate(OsRng);
            Argon2::default()
                .hash_password(password.as_bytes(), salt.as_salt())
                .unwrap()
                .to_string()
        });
        self.persistence
            .transaction(move |conn| create_user(conn, user_id, &name, password_hash.as_deref()))
            .await
            .map_err(|_| UsersError::Internal)?;

        Ok(user_id)
    }

    async fn authenticate(&mut self, id: Uuid) -> Result<(), UsersError> {
        self.persistence
            .rows_vec("SELECT 1 FROM users WHERE id = ?1", id, |_| Ok(()))
            .await
            .map_err(|_| UsersError::Internal)?
            .pop()
            .ok_or(UsersError::UnknownUser)
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
    /// Either name or password is incorrect.
    Unauthenticated,
}

fn fetch_user_id_and_hash<C>(
    conn: &C,
    name: &str,
) -> Result<Option<(Uuid, Option<String>)>, C::Error>
where
    C: ExecuteSql,
{
    let maybe_user_auth = conn
        .rows_vec(
            "SELECT id, password_hash FROM users WHERE name = ?1",
            name,
            |row| Ok((row.get_uuid(0), row.get_text_opt(1))),
        )?
        .pop();
    Ok(maybe_user_auth)
}

fn create_user<C>(
    conn: &C,
    user_id: Uuid,
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

    use uuid::Uuid;

    use crate::{
        persistence::{Persistence, SqlitePersistence},
        user::{
            UsersError::{self, Unauthenticated},
            migrate_users_persistence,
        },
    };

    use super::{PersistedUsers, User, Users};

    #[tokio::test]
    async fn different_uuids_for_each_user() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let alice_id = users
            .login("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();
        let bob_id = users
            .login("Bob".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

        assert_ne!(alice_id, bob_id)
    }

    #[tokio::test]
    async fn same_user_always_has_same_uuid() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let alice_id_1 = users
            .login("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();
        let alice_id_2 = users
            .login("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

        assert_eq!(alice_id_1, alice_id_2)
    }

    #[tokio::test]
    async fn reject_login_with_wrong_password() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let _alice_id = users
            .login("Alice".to_owned(), "secret".to_owned())
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
            .login("Alice".to_owned(), "dummy".to_owned())
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
    async fn authenticate_known_user() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);
        let alice_id = users
            .login("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

        let result = users.authenticate(alice_id).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn authenticate_unknown_user() {
        let persistence = persistence_fake().await;
        let mut users = PersistedUsers::new(persistence);

        let result = users.authenticate(Uuid::new_v4()).await;

        assert_matches!(result, Err(UsersError::UnknownUser));
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
