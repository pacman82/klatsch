use serde::{Deserialize, Serialize};

use super::{UserCreateOutcome, UserId, UserPersistence, password_hash};

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

/// Verifies credentials belong to a known user
#[cfg_attr(test, double_trait::dummies)]
pub trait AuthenticateUser {
    #[cfg(not(test))]
    fn authenticate(
        &mut self,
        name: String,
        password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send;

    #[cfg(test)]
    fn authenticate(
        &mut self,
        _name: String,
        _password: String,
    ) -> impl Future<Output = Result<UserId, UsersError>> + Send {
        async { Ok(UserId::nil()) }
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

    fn user_by_id(&mut self, id: UserId) -> impl Future<Output = Result<User, UsersError>> + Send;

    /// Change the password of an existing user.
    ///
    /// We demand the current password in order to limit the amount of damage a leaked session can
    /// do.
    fn change_password(
        &mut self,
        id: UserId,
        current_password: String,
        new_password: String,
    ) -> impl Future<Output = Result<(), UsersError>> + Send;

    /// Whether the system has no users at all yet.
    fn is_empty(&mut self) -> impl Future<Output = Result<bool, UsersError>> + Send;
}

impl<P> AuthenticateUser for UserStore<P>
where
    P: UserPersistence + Send,
{
    async fn authenticate(&mut self, name: String, password: String) -> Result<UserId, UsersError> {
        let maybe_user = self
            .persistence
            .id_and_hash_by_name(&name)
            .await
            .map_err(|_| UsersError::Internal)?;

        let (user_id, maybe_hash) = maybe_user.ok_or(UsersError::WrongCredentials)?;

        if let Some(hash) = maybe_hash
            && !password_hash::verify(&password, &hash)
        {
            return Err(UsersError::WrongCredentials);
        }

        // User existed already, but this is fine.
        Ok(user_id)
    }
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

        match outcome {
            UserCreateOutcome::Created => Ok(new_id),
            // Unlike login, signup never falls back to verifying a password against an existing
            // account — the name is simply unavailable, regardless of whether the given password
            // would have matched.
            UserCreateOutcome::Found { .. } => Err(UsersError::NameTaken),
        }
    }

    async fn user_by_id(&mut self, id: UserId) -> Result<User, UsersError> {
        self.persistence
            .user_by_id(id)
            .await
            .map_err(|_| UsersError::Internal)?
            .ok_or(UsersError::UnknownUser)
    }

    async fn change_password(
        &mut self,
        id: UserId,
        current_password: String,
        new_password: String,
    ) -> Result<(), UsersError> {
        let hash = self
            .persistence
            .password_hash_by_id(id)
            .await
            .map_err(|_| UsersError::Internal)?;

        if let Some(hash) = hash
            && !password_hash::verify(&current_password, &hash)
        {
            return Err(UsersError::WrongCredentials);
        }

        let new_hash = password_hash::generate(&new_password);
        self.persistence
            .set_password_hash(id, &new_hash)
            .await
            .map_err(|_| UsersError::Internal)?;

        Ok(())
    }

    async fn is_empty(&mut self) -> Result<bool, UsersError> {
        self.persistence
            .is_empty()
            .await
            .map_err(|_| UsersError::Internal)
    }
}

#[derive(Debug)]
pub enum UsersError {
    Internal,
    /// The user id does not belong to any user.
    UnknownUser,
    /// Either name or password is incorrect.
    WrongCredentials,
    /// A user with this name already exists.
    NameTaken,
}

#[cfg(test)]
mod tests {
    use std::{
        assert_matches,
        sync::{Arc, Mutex},
    };

    use anyhow::bail;

    use crate::user::{UserCreateOutcome, UserId, UserPersistence, UserStore, Users, UsersError};

    use super::{AuthenticateUser, User, password_hash};

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
    async fn signup_rejects_taken_name_even_when_password_would_match() {
        // Given

        // A user "Alice" already exists, with a password that happens to match what the caller
        // is about to supply. Signup must still reject this — unlike login, it must never
        // silently authenticate into an existing account.
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn create(
                &self,
                _name: &str,
                _new_id: UserId,
                _password_hash: Option<&str>,
            ) -> anyhow::Result<UserCreateOutcome> {
                Ok(UserCreateOutcome::Found)
            }
        }

        let mut users = UserStore::new(AliceStub);

        // When
        let result = users.signup("Alice".to_owned(), "secret".to_owned()).await;

        // Then
        assert_matches!(result, Err(UsersError::NameTaken));
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

        let result = users
            .authenticate("Alice".to_owned(), "secret".to_owned())
            .await;

        assert_matches!(result, Err(UsersError::WrongCredentials));
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
            .authenticate("Alice".to_owned(), "secret".to_owned())
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
            .authenticate("Alice".to_owned(), "wrong-secret".to_owned())
            .await;

        assert_matches!(result, Err(UsersError::WrongCredentials));
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
            .authenticate("Alice".to_owned(), "anything".to_owned())
            .await
            .unwrap();

        assert_eq!(id, UserId::ALICE);
    }

    #[tokio::test]
    async fn login_maps_persistence_error_to_internal() {
        let mut users = UserStore::new(Saboteur);

        let result = users
            .authenticate("Alice".to_owned(), "secret".to_owned())
            .await;

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

    #[tokio::test]
    async fn change_password_rejects_wrong_current_password() {
        // Given
        struct AliceStub;
        impl UserPersistence for AliceStub {
            async fn password_hash_by_id(&self, _id: UserId) -> anyhow::Result<Option<String>> {
                Ok(Some(password_hash::generate("secret")))
            }
        }
        let mut users = UserStore::new(AliceStub);

        // When
        let result = users
            .change_password(UserId::ALICE, "wrong-secret".to_owned(), "dummy".to_owned())
            .await;

        // Then
        assert_matches!(result, Err(UsersError::WrongCredentials));
    }

    #[tokio::test]
    async fn change_password_stores_hash_of_new_password() {
        // Given
        #[derive(Clone, Default)]
        struct PersistenceSpy {
            stored_hash: Arc<Mutex<Option<String>>>,
        }
        impl UserPersistence for PersistenceSpy {
            async fn password_hash_by_id(&self, _id: UserId) -> anyhow::Result<Option<String>> {
                Ok(Some(password_hash::generate("old-secret")))
            }

            async fn set_password_hash(
                &self,
                _id: UserId,
                password_hash: &str,
            ) -> anyhow::Result<()> {
                *self.stored_hash.lock().unwrap() = Some(password_hash.to_owned());
                Ok(())
            }
        }
        let persistence = PersistenceSpy::default();
        let mut users = UserStore::new(persistence.clone());

        // When
        users
            .change_password(
                UserId::ALICE,
                "old-secret".to_owned(),
                "new-secret".to_owned(),
            )
            .await
            .unwrap();

        // Then
        let stored_hash = persistence.stored_hash.lock().unwrap();
        let stored_hash = stored_hash
            .as_deref()
            .expect("new password hash must have been stored");
        assert!(password_hash::verify("new-secret", stored_hash));
    }

    #[tokio::test]
    async fn is_empty_forwards_persistence_result() {
        // Given
        struct EmptyStub;
        impl UserPersistence for EmptyStub {
            async fn is_empty(&self) -> anyhow::Result<bool> {
                Ok(true)
            }
        }
        let mut users = UserStore::new(EmptyStub);

        // When
        let is_empty = users.is_empty().await.unwrap();

        // Then
        assert!(is_empty);
    }

    #[tokio::test]
    async fn is_empty_maps_persistence_error_to_internal() {
        let mut users = UserStore::new(Saboteur);

        let result = users.is_empty().await;

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

        async fn is_empty(&self) -> anyhow::Result<bool> {
            bail!("Simulated persistence failure")
        }
    }
}
