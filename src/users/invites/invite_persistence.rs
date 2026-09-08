use std::time::SystemTime;

use crate::persistence::{ExecuteSqlAsync, ExecuteSqlSync, GetField as _};

use super::InviteToken;

/// Raw persistence operations required by the `invites` domain.
#[cfg_attr(test, double_trait::dummies)]
pub trait InvitePersistence {
    /// Persists a newly created invite.
    fn insert(
        &self,
        token: InviteToken,
        created_at: SystemTime,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// The invite's creation time, if it still exists.
    fn created_at(
        &self,
        token: InviteToken,
    ) -> impl Future<Output = anyhow::Result<Option<SystemTime>>> + Send;

    /// Deletes the invite. Deleting an invite that no longer exists is not an error.
    fn delete(&self, token: InviteToken) -> impl Future<Output = anyhow::Result<()>> + Send;

    /// The creation time of the oldest outstanding invite (i.e. the one that will expire
    /// soonest), if any invites exist.
    fn earliest_created_at(&self) -> impl Future<Output = anyhow::Result<Option<SystemTime>>> + Send;

    /// Deletes every invite created at or before `cutoff`.
    fn delete_created_before(
        &self,
        cutoff: SystemTime,
    ) -> impl Future<Output = anyhow::Result<()>> + Send;
}

impl<P> InvitePersistence for P
where
    P: ExecuteSqlAsync + Send + Sync,
{
    async fn insert(&self, token: InviteToken, created_at: SystemTime) -> anyhow::Result<()> {
        self.transaction(move |conn| {
            conn.execute(
                "INSERT INTO invites (token, created_at_ms) VALUES (?1, ?2)",
                (token, millis_since_epoch(created_at)),
            )
        })
        .await
    }

    async fn created_at(&self, token: InviteToken) -> anyhow::Result<Option<SystemTime>> {
        let mut rows = self
            .rows_vec(
                "SELECT created_at_ms FROM invites WHERE token = ?1",
                token,
                |row| {
                    let created_at_ms: i64 = row.get(0);
                    Ok(created_at_ms)
                },
            )
            .await?;
        Ok(rows.pop().map(system_time_from_millis))
    }

    async fn delete(&self, token: InviteToken) -> anyhow::Result<()> {
        self.transaction(move |conn| conn.execute("DELETE FROM invites WHERE token = ?1", token))
            .await
    }

    async fn earliest_created_at(&self) -> anyhow::Result<Option<SystemTime>> {
        let earliest_created_at_ms: Option<i64> = self
            .row("SELECT MIN(created_at_ms) FROM invites", (), |row| {
                Ok(row.get(0))
            })
            .await?;
        Ok(earliest_created_at_ms.map(system_time_from_millis))
    }

    async fn delete_created_before(&self, cutoff: SystemTime) -> anyhow::Result<()> {
        self.transaction(move |conn| {
            conn.execute(
                "DELETE FROM invites WHERE created_at_ms <= ?1",
                millis_since_epoch(cutoff),
            )
        })
        .await
    }
}

fn millis_since_epoch(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .expect("invite timestamps must not be before the unix epoch")
        .as_millis()
        .try_into()
        .expect("millisecond timestamp must fit in i64")
}

fn system_time_from_millis(millis: i64) -> SystemTime {
    let millis: u64 = millis
        .try_into()
        .expect("persisted timestamp must not be negative");
    SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(millis)
}

pub fn migrate_invite_persistence<C>(conn: &C, from_version: u32) -> Result<(), C::Error>
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
        "CREATE TABLE invites (
            token BLOB PRIMARY KEY,
            created_at_ms INTEGER NOT NULL
        )",
        (),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use async_sqlite::ClientBuilder;

    use super::{InvitePersistence, migrate_invite_persistence};
    use crate::users::invites::InviteToken;

    #[tokio::test]
    async fn created_at_is_none_for_unknown_token() {
        let persistence = persistence_fake().await;

        let created_at = persistence.created_at(InviteToken::new()).await.unwrap();

        assert_eq!(created_at, None);
    }

    #[tokio::test]
    async fn inserted_invite_can_be_found_by_token() {
        // Given
        let persistence = persistence_fake().await;
        let token = InviteToken::new();
        // Millisecond-aligned, since persistence only has millisecond resolution.
        let created_at = SystemTime::UNIX_EPOCH + Duration::from_millis(1_000);

        // When
        persistence.insert(token, created_at).await.unwrap();

        // Then
        assert_eq!(persistence.created_at(token).await.unwrap(), Some(created_at));
    }

    #[tokio::test]
    async fn deleted_invite_can_no_longer_be_found() {
        // Given
        let persistence = persistence_fake().await;
        let token = InviteToken::new();
        persistence.insert(token, SystemTime::now()).await.unwrap();

        // When
        persistence.delete(token).await.unwrap();

        // Then
        assert_eq!(persistence.created_at(token).await.unwrap(), None);
    }

    #[tokio::test]
    async fn deleting_an_unknown_invite_is_not_an_error() {
        let persistence = persistence_fake().await;

        persistence.delete(InviteToken::new()).await.unwrap();
    }

    #[tokio::test]
    async fn earliest_created_at_is_none_without_invites() {
        let persistence = persistence_fake().await;

        let earliest = persistence.earliest_created_at().await.unwrap();

        assert_eq!(earliest, None);
    }

    #[tokio::test]
    async fn earliest_created_at_reflects_oldest_invite() {
        // Given two invites, created at different times
        let persistence = persistence_fake().await;
        let older = SystemTime::UNIX_EPOCH + Duration::from_millis(1_000);
        let newer = SystemTime::UNIX_EPOCH + Duration::from_millis(61_000);
        persistence.insert(InviteToken::new(), newer).await.unwrap();
        persistence.insert(InviteToken::new(), older).await.unwrap();

        // When
        let earliest = persistence.earliest_created_at().await.unwrap();

        // Then
        assert_eq!(earliest, Some(older));
    }

    #[tokio::test]
    async fn delete_created_before_only_removes_older_invites() {
        // Given an old and a recent invite
        let persistence = persistence_fake().await;
        let cutoff = SystemTime::now();
        let old = InviteToken::new();
        let recent = InviteToken::new();
        persistence
            .insert(old, cutoff - Duration::from_secs(1))
            .await
            .unwrap();
        persistence
            .insert(recent, cutoff + Duration::from_secs(1))
            .await
            .unwrap();

        // When
        persistence.delete_created_before(cutoff).await.unwrap();

        // Then
        assert_eq!(persistence.created_at(old).await.unwrap(), None);
        assert!(persistence.created_at(recent).await.unwrap().is_some());
    }

    async fn persistence_fake() -> impl InvitePersistence {
        let client = ClientBuilder::new().open().await.unwrap();
        client
            .conn(|conn| migrate_invite_persistence(conn, 0))
            .await
            .unwrap();
        client
    }
}
