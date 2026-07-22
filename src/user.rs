mod password_hash;
mod user_http;
mod user_id;
mod user_persistence;

use serde::{Deserialize, Serialize};

pub use self::{
    user_http::user_routes,
    user_id::UserId,
    user_persistence::{UserPersistence, migrate_users_persistence},
};

use self::user_persistence::UserCreateOutcome;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub name: String,
}

#[derive(Clone)]
pub struct UserStore<P> {
    persistence: P,
}

impl<P> UserStore<P> {
    pub fn new(persistence: P) -> Self {
        UserStore { persistence }
    }
}

#[cfg_attr(test, double_trait::dummies)]
pub trait Users {
    #[cfg(not(test))]
    fn signup(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send;

    #[cfg(test)]
    fn signup(
        &mut self,
        _name: String,
        _password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send {
        async { Ok(UserId::nil()) }
    }

    #[cfg(not(test))]
    fn login(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send;

    #[cfg(test)]
    fn login(
        &mut self,
        _name: String,
        _password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send {
        async { Ok(UserId::nil()) }
    }

    fn user_by_id(&mut self, id: UserId) -> impl Future<Output = Result<User, UsersError>> + Send;
}

impl<P> Users for UserStore<P>
where
    P: UserPersistence + Send,
{
    async fn signup(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
        let new_id = UserId::new();
        let password_hash = (!password.is_empty()).then(|| password_hash::generate(&password));
        let outcome = self
            .persistence
            .create(&name, new_id, password_hash.as_deref())
            .await
            .map_err(|_| UsersError::Internal)?;

        let (user_id, maybe_hash) = match outcome {
            // New user created, nothing more to do, but to return.
            UserCreateOutcome::Created => return Ok(new_id),
            UserCreateOutcome::Found { id, password_hash } => (id, password_hash),
        };

        // Existing user found, do the passwords match?
        if let Some(hash) = maybe_hash
            && !password_hash::verify(&password, &hash)
        {
            // Password does not match, we can not create a user with this password
            return Err(UsersError::Unauthenticated);
        }

        Ok(user_id)
    }

    async fn login(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
        let maybe_user = self
            .persistence
            .id_and_hash_by_name(&name)
            .await
            .map_err(|_| UsersError::Internal)?;

        let (user_id, maybe_hash) = maybe_user.ok_or(UsersError::Unauthenticated)?;

        if let Some(hash) = maybe_hash
            && !password_hash::verify(&password, &hash)
        {
            return Err(UsersError::Unauthenticated);
        }

        // User existed already, but this is fine.
        Ok(user_id)
    }

    async fn user_by_id(&mut self, id: UserId) -> Result<User, UsersError> {
        self.persistence
            .user_by_id(id)
            .await
            .map_err(|_| UsersError::Internal)?
            .ok_or(UsersError::UnknownUser)
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

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use anyhow::bail;

    use crate::user::{UserCreateOutcome, UserId, UserPersistence, UserStore, Users, UsersError};

    use super::{User, password_hash};

    #[tokio::test]
    async fn create_new_user() {
        struct CreateMock;
        impl UserPersistence for CreateMock {
            async fn create(
                &self,
                name: &str,
                _new_id: UserId,
                hash: Option<&str>,
            ) -> anyhow::Result<UserCreateOutcome> {
                assert_eq!(name, "Alice");
                assert!(hash.is_some_and(|hash| password_hash::verify("secret", hash)));
                Ok(UserCreateOutcome::Created)
            }
        }
        let mut users = UserStore::new(CreateMock);

        users
            .signup("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn signup_generates_distinct_ids() {
        struct CreateStub;
        impl UserPersistence for CreateStub {
            async fn create(
                &self,
                _name: &str,
                _new_id: UserId,
                _password_hash: Option<&str>,
            ) -> anyhow::Result<UserCreateOutcome> {
                Ok(UserCreateOutcome::Created)
            }
        }
        let mut users = UserStore::new(CreateStub);

        let bob_id = users
            .signup("Bob".to_owned(), "dummy".to_owned())
            .await
            .unwrap();
        let alice_id = users
            .signup("Alice".to_owned(), "dummy".to_owned())
            .await
            .unwrap();

        assert_ne!(bob_id, alice_id);
    }

    #[tokio::test]
    async fn signup_returns_id_when_found_user_is_passwordless() {
        struct AliceWithoutPasswordStub;
        impl UserPersistence for AliceWithoutPasswordStub {
            async fn create(
                &self,
                _name: &str,
                _new_id: UserId,
                _password_hash: Option<&str>,
            ) -> anyhow::Result<UserCreateOutcome> {
                Ok(UserCreateOutcome::Found {
                    id: UserId::ALICE,
                    password_hash: None,
                })
            }
        }
        let mut users = UserStore::new(AliceWithoutPasswordStub);

        let id = users
            .signup("Alice".to_owned(), "anything".to_owned())
            .await
            .unwrap();

        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn signup_is_idempotent() {
        // Given

        // User persistence containing the user Alice
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn create(
                &self,
                _name: &str,
                _new_id: UserId,
                _password_hash: Option<&str>,
            ) -> anyhow::Result<UserCreateOutcome> {
                Ok(UserCreateOutcome::Found {
                    id: UserId::ALICE,
                    password_hash: Some(password_hash::generate("secret")),
                })
            }
        }

        // When
        let mut users = UserStore::new(AliceStub);

        let id = users
            .signup("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();

        // Then
        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn signup_must_not_create_same_user_with_different_password() {
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn create(
                &self,
                _name: &str,
                _new_id: UserId,
                _password_hash: Option<&str>,
            ) -> anyhow::Result<UserCreateOutcome> {
                Ok(UserCreateOutcome::Found {
                    id: UserId::ALICE,
                    password_hash: Some(password_hash::generate("original-secret")),
                })
            }
        }

        let mut users = UserStore::new(AliceStub);

        let result = users
            .signup("Alice".to_owned(), "new-secret".to_owned())
            .await;

        assert_matches!(result, Err(UsersError::Unauthenticated));
    }

    #[tokio::test]
    async fn signup_maps_persistence_error_to_internal() {
        let mut users = UserStore::new(Saboteur);

        let result = users.signup("Alice".to_owned(), "secret".to_owned()).await;

        assert_matches!(result, Err(UsersError::Internal));
    }

    #[tokio::test]
    async fn login_rejects_unknown_user() {
        struct UnknownUserStub;
        impl UserPersistence for UnknownUserStub {
            async fn id_and_hash_by_name(
                &self,
                _name: &str,
            ) -> anyhow::Result<Option<(UserId, Option<String>)>> {
                Ok(None)
            }
        }
        let mut users = UserStore::new(UnknownUserStub);

        let result = users.login("Alice".to_owned(), "secret".to_owned()).await;

        assert_matches!(result, Err(UsersError::Unauthenticated));
    }

    #[tokio::test]
    async fn login_accepts_correct_password() {
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn id_and_hash_by_name(
                &self,
                _name: &str,
            ) -> anyhow::Result<Option<(UserId, Option<String>)>> {
                Ok(Some((
                    UserId::ALICE,
                    Some(password_hash::generate("secret")),
                )))
            }
        }

        let mut users = UserStore::new(AliceStub);

        let id = users
            .login("Alice".to_owned(), "secret".to_owned())
            .await
            .unwrap();

        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn login_rejects_wrong_password() {
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn id_and_hash_by_name(
                &self,
                _name: &str,
            ) -> anyhow::Result<Option<(UserId, Option<String>)>> {
                Ok(Some((
                    UserId::ALICE,
                    Some(password_hash::generate("secret")),
                )))
            }
        }
        let mut users = UserStore::new(AliceStub);

        let result = users
            .login("Alice".to_owned(), "wrong-secret".to_owned())
            .await;

        assert_matches!(result, Err(UsersError::Unauthenticated));
    }

    #[tokio::test]
    async fn login_accepts_any_password_if_user_did_not_set_one() {
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn id_and_hash_by_name(
                &self,
                _name: &str,
            ) -> anyhow::Result<Option<(UserId, Option<String>)>> {
                Ok(Some((UserId::ALICE, None)))
            }
        }
        let mut users = UserStore::new(AliceStub);

        let id = users
            .login("Alice".to_owned(), "anything".to_owned())
            .await
            .unwrap();

        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn login_maps_persistence_error_to_internal() {
        let mut users = UserStore::new(Saboteur);

        let result = users.login("Alice".to_owned(), "secret".to_owned()).await;

        assert_matches!(result, Err(UsersError::Internal));
    }

    #[tokio::test]
    async fn user_by_id_returns_user_when_found() {
        struct AliceMock;
        impl UserPersistence for AliceMock {
            async fn user_by_id(&self, id: UserId) -> anyhow::Result<Option<User>> {
                assert_eq!(id, UserId::ALICE);
                Ok(Some(User {
                    name: "Alice".to_owned(),
                }))
            }
        }
        let mut users = UserStore::new(AliceMock);

        let user = users.user_by_id(UserId::ALICE).await.unwrap();

        assert_eq!(
            user,
            User {
                name: "Alice".to_owned()
            }
        );
    }

    #[tokio::test]
    async fn user_by_id_rejects_unknown_id() {
        struct UnknownIdStub;
        impl UserPersistence for UnknownIdStub {
            async fn user_by_id(&self, _id: UserId) -> anyhow::Result<Option<User>> {
                Ok(None)
            }
        }
        let mut users = UserStore::new(UnknownIdStub);

        let result = users.user_by_id(UserId::ALICE).await;

        assert_matches!(result, Err(UsersError::UnknownUser));
    }

    #[tokio::test]
    async fn user_by_id_maps_persistence_error_to_internal() {
        let mut users = UserStore::new(Saboteur);

        let result = users.user_by_id(UserId::ALICE).await;

        assert_matches!(result, Err(UsersError::Internal));
    }

    /// Fails every persistence operation, to test error mapping to `UsersError::Internal`.
    struct Saboteur;
    impl UserPersistence for Saboteur {
        async fn id_and_hash_by_name(
            &self,
            _name: &str,
        ) -> anyhow::Result<Option<(UserId, Option<String>)>> {
            bail!("Simulated persistence failure")
        }

        async fn create(
            &self,
            _name: &str,
            _new_id: UserId,
            _password_hash: Option<&str>,
        ) -> anyhow::Result<UserCreateOutcome> {
            bail!("Simulated persistence failure")
        }

        async fn user_by_id(&self, _id: UserId) -> anyhow::Result<Option<User>> {
            bail!("Simulated persistence failure")
        }
    }
}
