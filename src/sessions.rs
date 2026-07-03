use uuid::Uuid;

#[cfg_attr(test, double_trait::dummies)]
pub trait Sessions {
    fn create(&mut self, user_id: Uuid) -> Uuid;
    fn lookup(&mut self, session_id: Uuid) -> Option<Uuid>;
}

/// In-memory session store. Session ids are derived directly from the user id,
/// so no storage is needed and sessions never expire or accumulate.
#[derive(Clone)]
pub struct InMemorySessions;

impl Sessions for InMemorySessions {
    fn create(&mut self, user_id: Uuid) -> Uuid {
        user_id
    }

    fn lookup(&mut self, session_id: Uuid) -> Option<Uuid> {
        Some(session_id)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{InMemorySessions, Sessions as _};

    const ALICE_ID: Uuid = Uuid::from_bytes([
        0xab, 0x70, 0xb6, 0xca, 0x41, 0x39, 0x49, 0x9f, 0xa6, 0x6d, 0x15, 0xe8, 0x8f, 0x08, 0x1f,
        0xb1,
    ]);

    #[test]
    fn lookup_returns_user_id_session_was_created_for() {
        let mut sessions = InMemorySessions;
        let session_id = sessions.create(ALICE_ID);
        assert_eq!(sessions.lookup(session_id), Some(ALICE_ID));
    }
}
