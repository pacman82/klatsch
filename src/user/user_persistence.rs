use crate::persistence::{ExecuteSql, GetField as _, Persistence, PersistenceError as _};

use super::{User, UserId};

/// Outcome of [`UserPersistence::create`].
pub enum UserCreateOutcome {
    /// No user of that name existed yet. It has been created with the id passed to `create`.
    Created,
    /// A user of that name already existed. Its id and password hash (if any) are returned so the
    /// caller can authenticate against it.
    Found {
        id: UserId,
        password_hash: Option<String>,
    },
}

/// Persistence operations required by the `users` domain
#[cfg_attr(test, double_trait::dummies)]
pub trait UserPersistence {
    fn id_and_hash_by_name(
        &self,
        name: &str,
    ) -> impl Future<Output = anyhow::Result<Option<(UserId, Option<String>)>>> + Send;

    /// Creates a user if it does not exist yet. I.e. ensures a user with these credentials exists.
    fn create(
        &self,
        name: &str,
        new_id: UserId,
        password_hash: Option<&str>,
    ) -> impl Future<Output = anyhow::Result<UserCreateOutcome>> + Send;

    fn user_by_id(&self, id: UserId) -> impl Future<Output = anyhow::Result<Option<User>>> + Send;
}

impl<P> UserPersistence for P
where
    P: Persistence + Send + Sync,
{
    async fn id_and_hash_by_name(
        &self,
        name: &str,
    ) -> anyhow::Result<Option<(UserId, Option<String>)>> {
        let name = name.to_owned();
        self.transaction(move |conn| fetch_id_and_hash(conn, &name))
            .await
    }

    async fn create(
        &self,
        name: &str,
        new_id: UserId,
        password_hash: Option<&str>,
    ) -> anyhow::Result<UserCreateOutcome> {
        let name = name.to_owned();
        let password_hash = password_hash.map(str::to_owned);
        self.transaction(move |conn| create_user(conn, &name, new_id, password_hash.as_deref()))
            .await
    }

    async fn user_by_id(&self, id: UserId) -> anyhow::Result<Option<User>> {
        let mut users = self
            .rows_vec("SELECT name FROM users WHERE id = ?1", id, |row| {
                let user = User { name: row.get(0) };
                Ok(user)
            })
            .await?;
        Ok(users.pop())
    }
}

fn fetch_id_and_hash<C>(conn: &C, name: &str) -> Result<Option<(UserId, Option<String>)>, C::Error>
where
    C: ExecuteSql,
{
    conn.rows_vec(
        "SELECT id, password_hash FROM users WHERE name = ?1",
        name,
        |row| {
            let user_id = row.get(0);
            let password_hash = row.get(1);
            Ok((user_id, password_hash))
        },
    )
    .map(|mut rows| rows.pop())
}

fn insert_user<C>(
    conn: &C,
    id: UserId,
    name: &str,
    password_hash: Option<&str>,
) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "INSERT INTO users (id, name, password_hash) VALUES (?1, ?2, ?3)",
        (id, name, password_hash),
    )
}

fn create_user<C>(
    conn: &C,
    name: &str,
    new_id: UserId,
    password_hash: Option<&str>,
) -> Result<UserCreateOutcome, C::Error>
where
    C: ExecuteSql,
{
    let Err(err) = insert_user(conn, new_id, name, password_hash) else {
        return Ok(UserCreateOutcome::Created);
    };

    // Insertion failed, but is it due to the name already being taken, or something else?
    if !err.is_unique_constraint_violation() {
        return Err(err);
    }

    // Someone else won the race for this name. Report it as found instead.
    let (id, password_hash) = fetch_id_and_hash(conn, name)?
        .expect("row must exist, we just failed to insert due to its unique constraint");
    Ok(UserCreateOutcome::Found { id, password_hash })
}

fn create_schema_from_scratch<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSql,
{
    conn.execute(
        "CREATE TABLE users (
            id BLOB PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
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
    use crate::{
        persistence::SqlitePersistence,
        user::{User, UserId},
    };

    use super::{UserCreateOutcome, UserPersistence, migrate_users_persistence};

    #[tokio::test]
    async fn create_new_user() {
        let persistence = persistence_fake().await;
        let id = UserId::new();

        let outcome = persistence.create("Alice", id, None).await.unwrap();

        assert!(matches!(outcome, UserCreateOutcome::Created));
    }

    #[tokio::test]
    async fn create_for_existing_user() {
        // Given
        let persistence = persistence_fake().await;
        persistence
            .create("Alice", UserId::ALICE, Some("alice-hash"))
            .await
            .unwrap();

        // When
        let new_alice_id = UserId::new();
        let outcome = persistence
            .create("Alice", new_alice_id, Some("other-hash"))
            .await
            .unwrap();

        // Then
        let UserCreateOutcome::Found { id, password_hash } = outcome else {
            panic!("Expected an already existing user to be found, not created");
        };
        assert_eq!(id, UserId::ALICE);
        assert_eq!(password_hash.as_deref(), Some("alice-hash"));
    }

    #[tokio::test]
    async fn lookup_existing_user() {
        // Given
        let persistence = persistence_fake().await;
        persistence
            .create("Alice", UserId::ALICE, Some("hash"))
            .await
            .unwrap();

        // When
        let found = persistence.id_and_hash_by_name("Alice").await.unwrap();

        // Then
        assert_eq!(found, Some((UserId::ALICE, Some("hash".to_owned()))));
    }

    #[tokio::test]
    async fn lookup_unknown_user_by_name() {
        // Given
        let persistence = persistence_fake().await;

        // When
        let found = persistence.id_and_hash_by_name("Alice").await.unwrap();

        // Then
        assert_eq!(found, None);
    }

    #[tokio::test]
    async fn lookup_existing_user_by_id() {
        // Given
        let persistence = persistence_fake().await;
        let id = UserId::new();
        persistence.create("Alice", id, None).await.unwrap();

        // When
        let user = persistence.user_by_id(id).await.unwrap();

        // Then
        assert_eq!(
            user,
            Some(User {
                name: "Alice".to_owned()
            })
        );
    }

    #[tokio::test]
    async fn lookup_unknown_user_by_id() {
        // Given
        let persistence = persistence_fake().await;

        // When
        let user = persistence.user_by_id(UserId::new()).await.unwrap();

        // Then
        assert_eq!(user, None);
    }

    async fn persistence_fake() -> impl UserPersistence {
        SqlitePersistence::new(None, migrate_users_persistence)
            .await
            .unwrap()
    }
}
