use std::time::{Duration, SystemTime};

use crate::persistence::{ExecuteSqlAsync, ExecuteSqlSync, GetField as _};

use super::{Session, SessionHash};

#[cfg_attr(test, double_trait::dummies)]
pub trait SessionPersistence {
    /// All persisted sessions. Used after a restart to restore the state of [`super::SessionStore`].
    fn all_sessions(&self) -> impl Future<Output = anyhow::Result<Vec<Session>>> + Send;

    /// To insert a session after creation, so it is remembered after a reboot.
    fn insert(&mut self, session: Session) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Free memory used to remember the session after it has been revoked.
    fn remove(&mut self, session: SessionHash) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// Update activity timestamp in persistence to prolong timeout window
    fn update_activity(
        &mut self,
        session: SessionHash,
        last_activity: SystemTime,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

impl<P> SessionPersistence for P
where
    P: ExecuteSqlAsync + Send + Sync,
{
    async fn all_sessions(&self) -> anyhow::Result<Vec<Session>> {
        self.rows_vec(
            "SELECT hash, user_id, created_at_ms, last_activity_ms FROM sessions",
            (),
            |row| {
                let id = row.get(0);
                let user_id = row.get(1);
                let created_at_ms: i64 = row.get(2);
                let last_activity_ms: i64 = row.get(3);
                Ok(Session {
                    id,
                    user_id,
                    created_at: system_time_from_millis(created_at_ms),
                    last_activity: system_time_from_millis(last_activity_ms),
                })
            },
        )
        .await
    }

    async fn insert(&mut self, session: Session) -> anyhow::Result<()> {
        self.transaction(move |conn| {
            conn.execute(
                "INSERT INTO sessions (hash, user_id, created_at_ms, last_activity_ms) \
                    VALUES (?1, ?2, ?3, ?4)",
                (
                    session.id,
                    session.user_id,
                    millis_since_epoch(session.created_at),
                    millis_since_epoch(session.last_activity),
                ),
            )
        })
        .await
    }

    async fn remove(&mut self, session: SessionHash) -> anyhow::Result<()> {
        self.transaction(move |conn| conn.execute("DELETE FROM sessions WHERE hash = ?1", session))
            .await
    }

    async fn update_activity(
        &mut self,
        session: SessionHash,
        last_activity: SystemTime,
    ) -> anyhow::Result<()> {
        self.transaction(move |conn| {
            conn.execute(
                "UPDATE sessions SET last_activity_ms = ?1 WHERE hash = ?2",
                (millis_since_epoch(last_activity), session),
            )
        })
        .await
    }
}

fn millis_since_epoch(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .expect("session timestamps must not be before the unix epoch")
        .as_millis()
        .try_into()
        .expect("millisecond timestamp must fit in i64")
}

fn system_time_from_millis(millis: i64) -> SystemTime {
    let millis: u64 = millis
        .try_into()
        .expect("persisted timestamp must not be negative");
    SystemTime::UNIX_EPOCH + Duration::from_millis(millis)
}

pub fn migrate_session_persistence<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
where
    C: ExecuteSqlSync,
{
    match from_version {
        0 | 1 => create_schema_from_scratch(conn)?,
        _ => (),
    }
    Ok(())
}

fn create_schema_from_scratch<C>(conn: &C) -> Result<(), C::Error>
where
    C: ExecuteSqlSync,
{
    conn.execute(
        "CREATE TABLE sessions (
            hash BLOB PRIMARY KEY,
            user_id BLOB NOT NULL,
            created_at_ms INTEGER NOT NULL,
            last_activity_ms INTEGER NOT NULL
        )",
        (),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use async_sqlite::ClientBuilder;

    use crate::{sessions::SessionId, users::UserId};

    use super::{Session, SessionHash, SessionPersistence, migrate_session_persistence};

    #[tokio::test]
    async fn all_sessions_is_empty_when_none_persisted() {
        let persistence = persistence_fake().await;

        let sessions = persistence.all_sessions().await.unwrap();

        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn inserted_session_is_persisted() {
        // Given
        let mut persistence = persistence_fake().await;
        let session = Session {
            id: SessionHash::from_session_id(SessionId::ALICE),
            user_id: UserId::ALICE,
            created_at: SystemTime::UNIX_EPOCH + Duration::from_millis(1_000),
            last_activity: SystemTime::UNIX_EPOCH + Duration::from_millis(2_000),
        };

        // When
        persistence.insert(session.clone()).await.unwrap();

        // Then
        let sessions = persistence.all_sessions().await.unwrap();
        assert_eq!(sessions, vec![session]);
    }

    #[tokio::test]
    async fn removed_session_is_no_longer_persisted() {
        // Given two persisted sessions
        let mut persistence = persistence_fake().await;
        persistence
            .insert(Session {
                id: SessionHash::from_session_id(SessionId::ALICE),
                ..dummy_session()
            })
            .await
            .unwrap();
        persistence
            .insert(Session {
                id: SessionHash::from_session_id(SessionId::BOB),
                ..dummy_session()
            })
            .await
            .unwrap();

        // When removing one of them
        persistence
            .remove(SessionHash::from_session_id(SessionId::ALICE))
            .await
            .unwrap();

        // Then only the other remains
        let sessions = persistence.all_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, SessionHash::from_session_id(SessionId::BOB));
    }

    #[tokio::test]
    async fn activity_update_refreshes_last_activity() {
        // Given a persisted session
        let mut persistence = persistence_fake().await;
        let created_at = SystemTime::UNIX_EPOCH + Duration::from_millis(1_000);
        let session = Session {
            id: SessionHash::from_session_id(SessionId::ALICE),
            created_at,
            last_activity: created_at,
            ..dummy_session()
        };
        persistence.insert(session).await.unwrap();

        // When
        let new_last_activity = SystemTime::UNIX_EPOCH + Duration::from_millis(5_000);
        persistence
            .update_activity(
                SessionHash::from_session_id(SessionId::ALICE),
                new_last_activity,
            )
            .await
            .unwrap();

        // Then
        let sessions = persistence.all_sessions().await.unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].created_at, created_at);
        assert_eq!(sessions[0].last_activity, new_last_activity);
    }

    /// A session whose fields carry no meaning — only override the fields a test actually cares
    /// about via `..dummy_session()`.
    fn dummy_session() -> Session {
        Session {
            id: SessionHash::from_session_id(SessionId::nil()),
            user_id: UserId::nil(),
            created_at: SystemTime::UNIX_EPOCH,
            last_activity: SystemTime::UNIX_EPOCH,
        }
    }

    async fn persistence_fake() -> impl SessionPersistence {
        let client = ClientBuilder::new().open().await.unwrap();
        client
            .conn(|conn| migrate_session_persistence(conn, 0))
            .await
            .unwrap();
        client
    }
}
