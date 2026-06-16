use uuid::Uuid;

use crate::persistence::{ExecuteSql, FieldAccess as _, Persistence};

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
    use crate::{
        persistence::{Persistence, SqlitePersistence},
        user::migrate_users_persistence,
    };

    use super::{Authenticate, Users};

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

    async fn persistence_fake() -> impl Persistence {
        SqlitePersistence::new(None, migrate_users_persistence)
            .await
            .unwrap()
    }
}
